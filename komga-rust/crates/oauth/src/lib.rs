use oauth2::basic::BasicClient;
use oauth2::reqwest;
use oauth2::{
    AuthType, AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet,
    EndpointNotSet, EndpointSet, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
    TokenResponse, TokenUrl,
};
use openidconnect::core::{
    CoreAuthenticationFlow, CoreClient, CoreJsonWebKeySet, CoreJwsSigningAlgorithm,
    CoreProviderMetadata, CoreResponseType, CoreSubjectIdentifierType,
};
use openidconnect::{
    AccessTokenHash, EmptyAdditionalProviderMetadata, IssuerUrl, JsonWebKeySetUrl, Nonce,
    ResponseTypes, SubjectIdentifier, TokenResponse as _, UserInfoUrl,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

type ReadyOAuthClient =
    BasicClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointSet>;
type ReadyOidcClient = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OAuthClientConfig {
    pub registration_id: String,
    pub client_name: String,
    pub client_id: String,
    pub client_secret: String,
    pub authorization_uri: Option<String>,
    pub token_uri: Option<String>,
    pub user_info_uri: Option<String>,
    pub issuer_uri: Option<String>,
    pub jwk_set_uri: Option<String>,
    pub redirect_uri: Option<String>,
    pub client_authentication_method: Option<String>,
    pub scopes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AuthorizationSession {
    pub state: String,
    pub pkce_verifier: String,
    pub nonce: Option<String>,
    pub redirect_uri: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorizationStart {
    pub authorization_url: String,
    pub session: AuthorizationSession,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalOAuthIdentity {
    pub email: String,
    pub email_verified: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OAuthLoginError {
    InvalidClientConfig,
    StateMismatch,
    TokenExchangeFailed,
    TokenInvalidResponse,
    MissingAccessToken,
    MissingEmail,
    OidcMissingEmail,
    OidcVerificationFailed,
}

impl OAuthLoginError {
    pub fn redirect_error_code(&self) -> &'static str {
        match self {
            OAuthLoginError::InvalidClientConfig => "oauth2_invalid_client_config",
            OAuthLoginError::StateMismatch => "oauth2_state_mismatch",
            OAuthLoginError::TokenExchangeFailed => "oauth2_token_exchange_failed",
            OAuthLoginError::TokenInvalidResponse => "oauth2_token_invalid_response",
            OAuthLoginError::MissingAccessToken => "oauth2_missing_access_token",
            OAuthLoginError::MissingEmail => "ERR_1024",
            OAuthLoginError::OidcMissingEmail => "ERR_1028",
            OAuthLoginError::OidcVerificationFailed => "oauth2_oidc_verification_failed",
        }
    }
}

pub fn issue_pre_auth_session_token() -> String {
    format!("komga-session-oauth-{}", CsrfToken::new_random().secret())
}

pub async fn prepare_authorization(
    config: &OAuthClientConfig,
    context_base_url: &str,
) -> Result<AuthorizationStart, OAuthLoginError> {
    let redirect_uri = redirect_uri_for_client(config, context_base_url)?;
    if config.uses_oidc() && (config.authorization_uri.is_none() || config.token_uri.is_none()) {
        return prepare_oidc_authorization(config, redirect_uri).await;
    }

    let mut client = oauth2_client(config, redirect_uri.as_str())?;
    client = client.set_auth_type(auth_type(config));

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let nonce = config
        .uses_oidc()
        .then(|| Nonce::new_random().secret().to_string());

    let request = config.scopes.iter().cloned().fold(
        client.authorize_url(CsrfToken::new_random),
        |request, scope| request.add_scope(Scope::new(scope)),
    );
    let request = request.set_pkce_challenge(pkce_challenge);
    let request = if let Some(nonce) = nonce.as_ref() {
        request.add_extra_param("nonce", nonce.clone())
    } else {
        request
    };
    let (authorization_url, state) = request.url();

    Ok(AuthorizationStart {
        authorization_url: authorization_url.to_string(),
        session: AuthorizationSession {
            state: state.secret().to_string(),
            pkce_verifier: pkce_verifier.secret().to_string(),
            nonce,
            redirect_uri,
        },
    })
}

async fn prepare_oidc_authorization(
    config: &OAuthClientConfig,
    redirect_uri: String,
) -> Result<AuthorizationStart, OAuthLoginError> {
    let client = oidc_client(config, redirect_uri.as_str()).await?;
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let request = config
        .scopes
        .iter()
        .filter(|scope| !scope.eq_ignore_ascii_case("openid"))
        .cloned()
        .fold(
            client.authorize_url(
                CoreAuthenticationFlow::AuthorizationCode,
                CsrfToken::new_random,
                Nonce::new_random,
            ),
            |request, scope| request.add_scope(Scope::new(scope)),
        )
        .set_pkce_challenge(pkce_challenge);
    let (authorization_url, state, nonce) = request.url();

    Ok(AuthorizationStart {
        authorization_url: authorization_url.to_string(),
        session: AuthorizationSession {
            state: state.secret().to_string(),
            pkce_verifier: pkce_verifier.secret().to_string(),
            nonce: Some(nonce.secret().to_string()),
            redirect_uri,
        },
    })
}

pub async fn complete_callback(
    config: &OAuthClientConfig,
    session: &AuthorizationSession,
    code: &str,
    received_state: &str,
) -> Result<ExternalOAuthIdentity, OAuthLoginError> {
    if !secrets_equal(session.state.as_str(), received_state) {
        return Err(OAuthLoginError::StateMismatch);
    }

    if config.uses_oidc() {
        complete_oidc_callback(config, session, code).await
    } else {
        complete_oauth2_callback(config, session, code).await
    }
}

async fn complete_oauth2_callback(
    config: &OAuthClientConfig,
    session: &AuthorizationSession,
    code: &str,
) -> Result<ExternalOAuthIdentity, OAuthLoginError> {
    let mut client = oauth2_client(config, session.redirect_uri.as_str())?;
    client = client.set_auth_type(auth_type(config));
    let http_client = http_client()?;
    let token = client
        .exchange_code(AuthorizationCode::new(code.to_string()))
        .set_pkce_verifier(PkceCodeVerifier::new(session.pkce_verifier.clone()))
        .request_async(&http_client)
        .await
        .map_err(|_| OAuthLoginError::TokenExchangeFailed)?;
    let access_token = token.access_token().secret();
    if access_token.trim().is_empty() {
        return Err(OAuthLoginError::MissingAccessToken);
    }

    let email = resolve_oauth2_email(config, access_token).await;
    let Some(email) = email else {
        return Err(OAuthLoginError::MissingEmail);
    };

    Ok(ExternalOAuthIdentity {
        email,
        email_verified: None,
    })
}

async fn complete_oidc_callback(
    config: &OAuthClientConfig,
    session: &AuthorizationSession,
    code: &str,
) -> Result<ExternalOAuthIdentity, OAuthLoginError> {
    let nonce = session
        .nonce
        .as_ref()
        .ok_or(OAuthLoginError::OidcVerificationFailed)?;
    let client = oidc_client(config, session.redirect_uri.as_str()).await?;
    let http_client = http_client()?;
    let token = client
        .exchange_code(AuthorizationCode::new(code.to_string()))
        .map_err(|_| OAuthLoginError::TokenExchangeFailed)?
        .set_pkce_verifier(PkceCodeVerifier::new(session.pkce_verifier.clone()))
        .request_async(&http_client)
        .await
        .map_err(|_| OAuthLoginError::TokenExchangeFailed)?;
    let id_token = token
        .id_token()
        .ok_or(OAuthLoginError::OidcVerificationFailed)?;
    let verifier = client.id_token_verifier();
    let claims = id_token
        .claims(&verifier, &Nonce::new(nonce.clone()))
        .map_err(|_| OAuthLoginError::OidcVerificationFailed)?;

    if let Some(expected_access_token_hash) = claims.access_token_hash() {
        let actual_access_token_hash = AccessTokenHash::from_token(
            token.access_token(),
            id_token
                .signing_alg()
                .map_err(|_| OAuthLoginError::OidcVerificationFailed)?,
            id_token
                .signing_key(&verifier)
                .map_err(|_| OAuthLoginError::OidcVerificationFailed)?,
        )
        .map_err(|_| OAuthLoginError::OidcVerificationFailed)?;
        if actual_access_token_hash != *expected_access_token_hash {
            return Err(OAuthLoginError::OidcVerificationFailed);
        }
    }

    let subject = claims.subject().clone();
    let mut email = claims.email().map(|value| value.as_str().to_string());
    let mut email_verified = claims.email_verified();

    if let Some(userinfo_claims) = resolve_oidc_userinfo(
        &client,
        &http_client,
        token.access_token().to_owned(),
        subject,
    )
    .await
    {
        if let Some(value) = userinfo_claims.email() {
            email = Some(value.as_str().to_string());
        }
        if userinfo_claims.email_verified().is_some() {
            email_verified = userinfo_claims.email_verified();
        }
    }

    let Some(email) = email else {
        return Err(OAuthLoginError::OidcMissingEmail);
    };

    Ok(ExternalOAuthIdentity {
        email,
        email_verified,
    })
}

async fn resolve_oidc_userinfo(
    client: &ReadyOidcClient,
    http_client: &reqwest::Client,
    access_token: openidconnect::AccessToken,
    subject: SubjectIdentifier,
) -> Option<openidconnect::core::CoreUserInfoClaims> {
    let Ok(request) = client.user_info(access_token, Some(subject)) else {
        return None;
    };
    request.request_async(http_client).await.ok()
}

async fn oidc_client(
    config: &OAuthClientConfig,
    redirect_uri: &str,
) -> Result<ReadyOidcClient, OAuthLoginError> {
    let http_client = http_client()?;
    let issuer_uri = config
        .issuer_uri
        .as_ref()
        .ok_or(OAuthLoginError::OidcVerificationFailed)?;

    let metadata = if let Some(jwk_set_uri) = config.jwk_set_uri.as_ref() {
        explicit_oidc_metadata(config, issuer_uri, jwk_set_uri, &http_client).await?
    } else {
        CoreProviderMetadata::discover_async(
            IssuerUrl::new(issuer_uri.clone())
                .map_err(|_| OAuthLoginError::OidcVerificationFailed)?,
            &http_client,
        )
        .await
        .map_err(|_| OAuthLoginError::OidcVerificationFailed)?
    };

    Ok(CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(config.client_id.clone()),
        Some(ClientSecret::new(config.client_secret.clone())),
    )
    .set_auth_type(auth_type(config))
    .set_redirect_uri(
        RedirectUrl::new(redirect_uri.to_string())
            .map_err(|_| OAuthLoginError::InvalidClientConfig)?,
    ))
}

async fn explicit_oidc_metadata(
    config: &OAuthClientConfig,
    issuer_uri: &str,
    jwk_set_uri: &str,
    http_client: &reqwest::Client,
) -> Result<CoreProviderMetadata, OAuthLoginError> {
    let authorization_uri = config
        .authorization_uri
        .as_ref()
        .ok_or(OAuthLoginError::OidcVerificationFailed)?;
    let token_uri = config
        .token_uri
        .as_ref()
        .ok_or(OAuthLoginError::OidcVerificationFailed)?;
    let jwk_set_uri = JsonWebKeySetUrl::new(jwk_set_uri.to_string())
        .map_err(|_| OAuthLoginError::OidcVerificationFailed)?;
    let jwks = CoreJsonWebKeySet::fetch_async(&jwk_set_uri, http_client)
        .await
        .map_err(|_| OAuthLoginError::OidcVerificationFailed)?;
    let mut metadata = CoreProviderMetadata::new(
        IssuerUrl::new(issuer_uri.to_string())
            .map_err(|_| OAuthLoginError::OidcVerificationFailed)?,
        openidconnect::AuthUrl::new(authorization_uri.clone())
            .map_err(|_| OAuthLoginError::OidcVerificationFailed)?,
        jwk_set_uri,
        vec![ResponseTypes::new(vec![CoreResponseType::Code])],
        vec![CoreSubjectIdentifierType::Public],
        secure_oidc_signing_algorithms(),
        EmptyAdditionalProviderMetadata {},
    )
    .set_jwks(jwks)
    .set_token_endpoint(Some(
        openidconnect::TokenUrl::new(token_uri.clone())
            .map_err(|_| OAuthLoginError::OidcVerificationFailed)?,
    ));
    if let Some(user_info_uri) = config.user_info_uri.as_ref() {
        metadata = metadata.set_userinfo_endpoint(Some(
            UserInfoUrl::new(user_info_uri.clone())
                .map_err(|_| OAuthLoginError::OidcVerificationFailed)?,
        ));
    }
    Ok(metadata)
}

fn secure_oidc_signing_algorithms() -> Vec<CoreJwsSigningAlgorithm> {
    vec![
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256,
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha384,
        CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha512,
        CoreJwsSigningAlgorithm::RsaSsaPssSha256,
        CoreJwsSigningAlgorithm::RsaSsaPssSha384,
        CoreJwsSigningAlgorithm::RsaSsaPssSha512,
        CoreJwsSigningAlgorithm::EcdsaP256Sha256,
        CoreJwsSigningAlgorithm::EcdsaP384Sha384,
        CoreJwsSigningAlgorithm::EcdsaP521Sha512,
        CoreJwsSigningAlgorithm::HmacSha256,
        CoreJwsSigningAlgorithm::HmacSha384,
        CoreJwsSigningAlgorithm::HmacSha512,
    ]
}

fn oauth2_client(
    config: &OAuthClientConfig,
    redirect_uri: &str,
) -> Result<ReadyOAuthClient, OAuthLoginError> {
    let authorization_uri = config
        .authorization_uri
        .as_ref()
        .ok_or(OAuthLoginError::InvalidClientConfig)?;
    let token_uri = config
        .token_uri
        .as_ref()
        .ok_or(OAuthLoginError::InvalidClientConfig)?;

    Ok(BasicClient::new(ClientId::new(config.client_id.clone()))
        .set_client_secret(ClientSecret::new(config.client_secret.clone()))
        .set_auth_uri(
            AuthUrl::new(authorization_uri.clone())
                .map_err(|_| OAuthLoginError::InvalidClientConfig)?,
        )
        .set_token_uri(
            TokenUrl::new(token_uri.clone()).map_err(|_| OAuthLoginError::InvalidClientConfig)?,
        )
        .set_redirect_uri(
            RedirectUrl::new(redirect_uri.to_string())
                .map_err(|_| OAuthLoginError::InvalidClientConfig)?,
        ))
}

fn auth_type(config: &OAuthClientConfig) -> AuthType {
    match config
        .client_authentication_method
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("client_secret_post") | Some("post") => AuthType::RequestBody,
        Some("client_secret_basic") | Some("basic") => AuthType::BasicAuth,
        _ => AuthType::BasicAuth,
    }
}

fn redirect_uri_for_client(
    config: &OAuthClientConfig,
    context_base_url: &str,
) -> Result<String, OAuthLoginError> {
    let value = config
        .redirect_uri
        .clone()
        .unwrap_or_else(|| "{baseUrl}/login/oauth2/code/{registrationId}".to_string())
        .replace("{baseUrl}", context_base_url.trim_end_matches('/'))
        .replace("{registrationId}", config.registration_id.as_str());
    RedirectUrl::new(value.clone()).map_err(|_| OAuthLoginError::InvalidClientConfig)?;
    Ok(value)
}

async fn resolve_oauth2_email(config: &OAuthClientConfig, access_token: &str) -> Option<String> {
    let http_client = http_client().ok()?;
    for candidate in userinfo_candidates(config) {
        let Ok(response) = http_client
            .get(candidate.endpoint.as_str())
            .bearer_auth(access_token)
            .header("User-Agent", "komga-rust/runtime")
            .header("Accept", "application/json")
            .send()
            .await
        else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        let Ok(body) = response.bytes().await else {
            continue;
        };
        let Ok(payload) = serde_json::from_slice::<Value>(&body) else {
            continue;
        };
        let email = match candidate.kind {
            UserinfoKind::Standard => extract_standard_email(&payload),
            UserinfoKind::GithubEmails => extract_github_email(&payload),
        };
        if email.is_some() {
            return email;
        }
    }
    None
}

#[derive(Clone, Copy)]
enum UserinfoKind {
    Standard,
    GithubEmails,
}

struct UserinfoCandidate {
    endpoint: String,
    kind: UserinfoKind,
}

fn userinfo_candidates(config: &OAuthClientConfig) -> Vec<UserinfoCandidate> {
    let mut candidates = Vec::new();
    if let Some(user_info_uri) = config.user_info_uri.as_ref() {
        push_userinfo_candidate(
            &mut candidates,
            user_info_uri.clone(),
            UserinfoKind::Standard,
        );
        if config.supports_github_email_lookup() {
            push_userinfo_candidate(
                &mut candidates,
                format!("{}/emails", user_info_uri.trim_end_matches('/')),
                UserinfoKind::GithubEmails,
            );
        }
    }
    candidates
}

fn push_userinfo_candidate(
    candidates: &mut Vec<UserinfoCandidate>,
    endpoint: String,
    kind: UserinfoKind,
) {
    if !candidates
        .iter()
        .any(|candidate| candidate.endpoint == endpoint)
    {
        candidates.push(UserinfoCandidate { endpoint, kind });
    }
}

fn extract_standard_email(payload: &Value) -> Option<String> {
    payload
        .get("email")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn extract_github_email(payload: &Value) -> Option<String> {
    payload.as_array().and_then(|entries| {
        entries
            .iter()
            .find(|entry| {
                entry
                    .get("email")
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty())
                    && entry
                        .get("primary")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                    && entry
                        .get("verified")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
            })
            .and_then(|entry| entry.get("email"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn http_client() -> Result<reqwest::Client, OAuthLoginError> {
    reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|_| OAuthLoginError::TokenExchangeFailed)
}

fn secrets_equal(left: &str, right: &str) -> bool {
    use sha2::{Digest, Sha256};

    Sha256::digest(left.as_bytes()) == Sha256::digest(right.as_bytes())
}

impl OAuthClientConfig {
    fn uses_oidc(&self) -> bool {
        self.scopes
            .iter()
            .any(|scope| scope.eq_ignore_ascii_case("openid"))
    }

    fn supports_github_email_lookup(&self) -> bool {
        self.registration_id.eq_ignore_ascii_case("github")
            && self.scopes.iter().any(|scope| {
                scope.eq_ignore_ascii_case("user") || scope.eq_ignore_ascii_case("user:email")
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client() -> OAuthClientConfig {
        OAuthClientConfig {
            registration_id: "github".to_string(),
            client_name: "GitHub".to_string(),
            client_id: "client-id".to_string(),
            client_secret: "client-secret".to_string(),
            authorization_uri: Some("https://github.example/login/oauth/authorize".to_string()),
            token_uri: Some("https://github.example/login/oauth/access_token".to_string()),
            user_info_uri: Some("https://github.example/user".to_string()),
            issuer_uri: None,
            jwk_set_uri: None,
            redirect_uri: None,
            client_authentication_method: None,
            scopes: vec!["user:email".to_string()],
        }
    }

    #[tokio::test]
    async fn authorization_uses_pkce_and_state() {
        let start = prepare_authorization(&client(), "https://komga.example")
            .await
            .expect("authorization should build");

        assert!(start.authorization_url.contains("state="));
        assert!(start.authorization_url.contains("code_challenge="));
        assert!(
            start
                .authorization_url
                .contains("code_challenge_method=S256")
        );
        assert!(!start.session.pkce_verifier.is_empty());
        assert_eq!(
            start.session.redirect_uri,
            "https://komga.example/login/oauth2/code/github"
        );
    }

    #[tokio::test]
    async fn oidc_authorization_includes_nonce() {
        let mut config = client();
        config.registration_id = "oidc".to_string();
        config.scopes = vec!["openid".to_string(), "email".to_string()];

        let start = prepare_authorization(&config, "https://komga.example")
            .await
            .expect("authorization should build");

        assert!(start.authorization_url.contains("nonce="));
        assert!(start.session.nonce.is_some());
    }

    #[tokio::test]
    async fn callback_rejects_state_mismatch_before_network_exchange() {
        let start = prepare_authorization(&client(), "https://komga.example")
            .await
            .expect("authorization should build");

        let error = complete_callback(&client(), &start.session, "code", "wrong-state")
            .await
            .expect_err("state mismatch should fail");

        assert_eq!(error, OAuthLoginError::StateMismatch);
    }
}
