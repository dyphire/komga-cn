use super::*;
use axum::extract::State;
use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use std::io::Read;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn oauth2_authorization(
    State(app): State<Arc<HttpAppState>>,
    Path(registration_id): Path<String>,
    headers: HeaderMap,
) -> Response {
    let state = &app.operational;
    let Some(client) = state
        .oauth2_clients
        .iter()
        .find(|client| client.registration_id == registration_id)
    else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let Ok(auth_url) = AuthUrl::new(client.authorization_uri.clone()) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let base_url = request_base_url(&headers);
    let context_base_url = format!("{base_url}{}", request_context_path(&headers));
    let Ok(redirect_url) = RedirectUrl::new(format!(
        "{context_base_url}/login/oauth2/code/{}",
        client.registration_id
    )) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let oauth_client = BasicClient::new(ClientId::new(client.client_id.clone()))
        .set_client_secret(ClientSecret::new(client.client_secret.clone()))
        .set_auth_uri(auth_url)
        .set_redirect_uri(redirect_url);

    let authorization_request = client.scopes.iter().cloned().fold(
        oauth_client.authorize_url(CsrfToken::new_random),
        |request, scope| request.add_scope(Scope::new(scope)),
    );
    let (url, csrf_state) = authorization_request.url();

    let existing_session_token = oauth2_session_cookie_token(&headers);
    let session_token = existing_session_token
        .clone()
        .unwrap_or_else(issue_oauth2_session_token);
    app.services
        .runtime_identity
        .store_oauth2_authorization_state(
            state.remember_me_runtime_key.clone(),
            session_token.clone(),
            client.registration_id.clone(),
            csrf_state.secret().to_string(),
        );

    let mut response = (
        StatusCode::FOUND,
        [(
            header::LOCATION,
            HeaderValue::from_str(url.as_str()).unwrap_or_else(|_| {
                HeaderValue::from_static("/login?server_redirect=Y&error=oauth2_invalid_redirect")
            }),
        )],
    )
        .into_response();

    if existing_session_token.is_none() {
        response.headers_mut().append(
            header::SET_COOKIE,
            HeaderValue::from_str(oauth2_session_cookie(session_token.as_str()).as_str())
                .unwrap_or_else(|_| {
                    HeaderValue::from_static("KOMGA-SESSION=; Path=/; HttpOnly; SameSite=Lax")
                }),
        );
    }

    response
}

pub async fn oauth2_login_code(
    State(app): State<Arc<HttpAppState>>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    Path(registration_id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<OAuth2CallbackQuery>,
) -> Response {
    let auth_db = &app.auth_db;
    let state = &app.operational;
    let Some(client_config) = state
        .oauth2_clients
        .iter()
        .find(|client| client.registration_id == registration_id)
    else {
        return oauth2_login_error_redirect("oauth2_provider_not_found");
    };
    let client_name = client_config.client_name.as_str();

    if let Some(error) = query.error.as_deref() {
        return oauth2_login_error_response(
            &app,
            &headers,
            &connection_info,
            client_name,
            error,
            None,
        )
        .await;
    }

    let Some(code) = query.code.as_deref() else {
        return oauth2_login_error_response(
            &app,
            &headers,
            &connection_info,
            client_name,
            "oauth2_missing_code",
            None,
        )
        .await;
    };

    let Some(received_state) = query.state.as_deref() else {
        return oauth2_login_error_response(
            &app,
            &headers,
            &connection_info,
            client_name,
            "oauth2_state_missing",
            None,
        )
        .await;
    };
    let Some(session_token) = oauth2_session_cookie_token(&headers) else {
        return oauth2_login_error_response(
            &app,
            &headers,
            &connection_info,
            client_name,
            "oauth2_state_missing",
            None,
        )
        .await;
    };
    let Some(expected_state) = app
        .services
        .runtime_identity
        .take_oauth2_authorization_state(
            auth_db.session_runtime_key.clone(),
            session_token.clone(),
            registration_id.clone(),
        )
    else {
        return oauth2_login_error_response(
            &app,
            &headers,
            &connection_info,
            client_name,
            "oauth2_state_missing",
            None,
        )
        .await;
    };
    if received_state != expected_state {
        return oauth2_login_error_response(
            &app,
            &headers,
            &connection_info,
            client_name,
            "oauth2_state_mismatch",
            None,
        )
        .await;
    }

    let base_url = request_base_url(&headers);
    let context_base_url = format!("{base_url}{}", request_context_path(&headers));
    let redirect_uri = format!("{context_base_url}/login/oauth2/code/{registration_id}");

    let token_payload = match exchange_oauth2_token(client_config, code, &redirect_uri).await {
        Ok(payload) => payload,
        Err(error) => {
            return oauth2_login_error_response(
                &app,
                &headers,
                &connection_info,
                client_name,
                error.as_str(),
                None,
            )
            .await;
        }
    };

    let access_token = token_payload
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());

    let Some(access_token) = access_token else {
        return oauth2_login_error_response(
            &app,
            &headers,
            &connection_info,
            client_name,
            "oauth2_missing_access_token",
            None,
        )
        .await;
    };

    let email = if oauth2_client_uses_oidc(client_config) {
        let claims = resolve_oidc_claims(client_config, &token_payload, access_token).await;
        let Some(email) = claims.email else {
            return oauth2_login_error_response(
                &app,
                &headers,
                &connection_info,
                client_name,
                "ERR_1028",
                None,
            )
            .await;
        };
        if state.oidc_email_verification {
            match claims.email_verified {
                Some(true) => email,
                Some(false) => {
                    return oauth2_login_error_response(
                        &app,
                        &headers,
                        &connection_info,
                        client_name,
                        "ERR_1026",
                        Some(email.as_str()),
                    )
                    .await;
                }
                None => {
                    return oauth2_login_error_response(
                        &app,
                        &headers,
                        &connection_info,
                        client_name,
                        "ERR_1027",
                        Some(email.as_str()),
                    )
                    .await;
                }
            }
        } else {
            email
        }
    } else {
        let email = resolve_oauth2_email(client_config, access_token).await;
        let Some(email) = email else {
            return oauth2_login_error_response(
                &app,
                &headers,
                &connection_info,
                client_name,
                "ERR_1024",
                None,
            )
            .await;
        };
        email
    };

    let allow_create = oauth2_account_creation_enabled(&app).await;
    let user = match app
        .services
        .runtime_identity
        .ensure_oauth_user(email.clone(), allow_create)
        .await
    {
        Ok(Some(user)) => user,
        Ok(None) => {
            return oauth2_login_error_response(
                &app,
                &headers,
                &connection_info,
                client_name,
                "ERR_1025",
                Some(email.as_str()),
            )
            .await;
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let source = format!("OAuth2:{client_name}");
    let _ = persisted_record_successful_authentication_activity(
        &*app.services.runtime_identity,
        &user,
        authentication_activity_write_input(
            &authentication_activity_headers_metadata_with_remote_addr(
                &headers,
                connection_info.remote_addr(),
            ),
            source.as_str(),
            None,
            None,
        ),
    )
    .await;

    let session_token = session_token_for_user_with_runtime_key(
        &*app.services.runtime_identity,
        &user,
        auth_db.session_runtime_key.as_str(),
    );
    oauth2_login_success_redirect(session_token.as_str())
}

fn oauth2_login_error_redirect(error: &str) -> Response {
    let redirect = format!("/login?server_redirect=Y&error={error}");
    (
        StatusCode::FOUND,
        [(
            header::LOCATION,
            HeaderValue::from_str(&redirect).unwrap_or_else(|_| {
                HeaderValue::from_static("/login?server_redirect=Y&error=oauth2_invalid_redirect")
            }),
        )],
    )
        .into_response()
}

fn oauth2_login_success_redirect(session_token: &str) -> Response {
    let session_cookie = oauth2_session_cookie(session_token);

    (
        StatusCode::FOUND,
        [
            (
                header::LOCATION,
                HeaderValue::from_static("/?server_redirect=Y"),
            ),
            (
                header::SET_COOKIE,
                HeaderValue::from_str(&session_cookie).unwrap_or_else(|_| {
                    HeaderValue::from_static("KOMGA-SESSION=; Path=/; HttpOnly; SameSite=Lax")
                }),
            ),
        ],
    )
        .into_response()
}

fn oauth2_client_uses_oidc(client: &crate::state::OAuth2ClientConfig) -> bool {
    client
        .scopes
        .iter()
        .any(|scope| scope.eq_ignore_ascii_case("openid"))
}

#[derive(Default)]
struct OidcIdentityClaims {
    email: Option<String>,
    email_verified: Option<bool>,
}

fn oauth2_session_cookie(session_token: &str) -> String {
    Cookie::build(("KOMGA-SESSION", session_token.to_string()))
        .path("/")
        .http_only(true)
        .same_site(SameSite::Lax)
        .build()
        .to_string()
}

fn oauth2_session_cookie_token(headers: &HeaderMap) -> Option<String> {
    let jar = CookieJar::from_headers(headers);
    jar.get("KOMGA-SESSION")
        .map(|cookie| cookie.value().to_string())
        .filter(|value| !value.trim().is_empty())
}

fn issue_oauth2_session_token() -> String {
    format!("komga-session-oauth-{}", random_hex_token(24))
}

fn random_hex_token(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let _ = file.read_exact(&mut bytes);
    } else {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = ((seed >> ((index % 8) * 8)) as u8) ^ (index as u8).wrapping_mul(31);
        }
    }

    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

async fn exchange_oauth2_token(
    client: &crate::state::OAuth2ClientConfig,
    code: &str,
    redirect_uri: &str,
) -> Result<Value, String> {
    let http = Client::new();
    let form = reqwest::Url::parse_with_params(
        client.token_uri.as_str(),
        [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", client.client_id.as_str()),
            ("client_secret", client.client_secret.as_str()),
        ],
    )
    .ok()
    .and_then(|url| url.query().map(str::to_string))
    .ok_or_else(|| "oauth2_token_exchange_failed".to_string())?;

    let response = http
        .post(client.token_uri.as_str())
        .header(header::ACCEPT.as_str(), "application/json")
        .header(
            header::CONTENT_TYPE.as_str(),
            "application/x-www-form-urlencoded",
        )
        .body(form)
        .send()
        .await
        .map_err(|_| "oauth2_token_exchange_failed".to_string())?;

    let status = response.status();
    let payload = response
        .json::<Value>()
        .await
        .map_err(|_| "oauth2_token_invalid_response".to_string())?;

    if !status.is_success() {
        let error_code = payload
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("oauth2_token_exchange_failed");
        return Err(error_code.to_string());
    }

    Ok(payload)
}

async fn resolve_oauth2_email(
    client: &crate::state::OAuth2ClientConfig,
    access_token: &str,
) -> Option<String> {
    let http = Client::new();
    let candidates = oauth2_userinfo_candidates(client);
    for endpoint in candidates {
        let request = http
            .get(endpoint.endpoint.as_str())
            .bearer_auth(access_token)
            .header("User-Agent", "komga-rust/runtime")
            .header(header::ACCEPT.as_str(), "application/json");

        let Ok(response) = request.send().await else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }

        let Ok(payload) = response.json::<Value>().await else {
            continue;
        };

        let email = match endpoint.kind {
            OAuth2UserinfoKind::Standard => extract_standard_email_from_userinfo_payload(&payload),
            OAuth2UserinfoKind::GithubEmails => extract_github_email_from_payload(&payload),
        };
        if let Some(email) = email {
            return Some(email);
        }
    }

    None
}

async fn resolve_oidc_claims(
    client: &crate::state::OAuth2ClientConfig,
    token_payload: &Value,
    access_token: &str,
) -> OidcIdentityClaims {
    let mut claims = extract_oidc_claims_from_id_token(token_payload).unwrap_or_default();
    if client.user_info_uri.is_none() {
        return claims;
    }

    let http = Client::new();
    for endpoint in oauth2_userinfo_candidates(client) {
        let request = http
            .get(endpoint.endpoint.as_str())
            .bearer_auth(access_token)
            .header("User-Agent", "komga-rust/runtime")
            .header(header::ACCEPT.as_str(), "application/json");

        let Ok(response) = request.send().await else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }

        let Ok(payload) = response.json::<Value>().await else {
            continue;
        };
        let userinfo_claims = extract_oidc_claims(&payload);
        if userinfo_claims.email.is_some() {
            claims.email = userinfo_claims.email;
        }
        if userinfo_claims.email_verified.is_some() {
            claims.email_verified = userinfo_claims.email_verified;
        }
    }

    claims
}

fn extract_oidc_claims_from_id_token(token_payload: &Value) -> Option<OidcIdentityClaims> {
    let id_token = token_payload.get("id_token")?.as_str()?.trim();
    if id_token.is_empty() {
        return None;
    }
    let payload_segment = id_token.split('.').nth(1)?;
    let decoded = URL_SAFE_NO_PAD.decode(payload_segment).ok()?;
    let payload = serde_json::from_slice::<Value>(&decoded).ok()?;
    Some(extract_oidc_claims(&payload))
}

fn extract_oidc_claims(payload: &Value) -> OidcIdentityClaims {
    OidcIdentityClaims {
        email: payload
            .get("email")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        email_verified: payload.get("email_verified").and_then(Value::as_bool),
    }
}

fn extract_standard_email_from_userinfo_payload(payload: &Value) -> Option<String> {
    if let Some(email) = payload
        .get("email")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Some(email.to_string());
    }

    None
}

fn extract_github_email_from_payload(payload: &Value) -> Option<String> {
    if let Some(array) = payload.as_array() {
        let selected = array.iter().find(|entry| {
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
        });
        if let Some(email) = selected
            .and_then(|entry| entry.get("email"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            return Some(email.to_string());
        }
    }

    None
}

#[derive(Clone, Copy)]
enum OAuth2UserinfoKind {
    Standard,
    GithubEmails,
}

struct OAuth2UserinfoCandidate {
    endpoint: String,
    kind: OAuth2UserinfoKind,
}

fn oauth2_userinfo_candidates(
    client: &crate::state::OAuth2ClientConfig,
) -> Vec<OAuth2UserinfoCandidate> {
    let mut candidates = Vec::new();
    if let Some(user_info_uri) = client.user_info_uri.as_ref() {
        push_userinfo_candidate(
            &mut candidates,
            user_info_uri.clone(),
            OAuth2UserinfoKind::Standard,
        );

        if oauth2_client_supports_github_email_lookup(client) {
            push_userinfo_candidate(
                &mut candidates,
                format!("{}/emails", user_info_uri.trim_end_matches('/')),
                OAuth2UserinfoKind::GithubEmails,
            );
        }

        return candidates;
    }
    candidates
}

async fn oauth2_login_error_response(
    app: &HttpAppState,
    headers: &HeaderMap,
    connection_info: &RequestConnectionInfo,
    client_name: &str,
    error: &str,
    email: Option<&str>,
) -> Response {
    let source = format!("OAuth2:{client_name}");
    let _ = app
        .services
        .runtime_identity
        .persisted_record_failed_authentication_activity(
            email.map(str::to_string),
            authentication_activity_write_input(
                &authentication_activity_headers_metadata_with_remote_addr(
                    headers,
                    connection_info.remote_addr(),
                ),
                source.as_str(),
                None,
                None,
            ),
            error.to_string(),
        )
        .await;

    oauth2_login_error_redirect(error)
}

fn push_userinfo_candidate(
    candidates: &mut Vec<OAuth2UserinfoCandidate>,
    endpoint: String,
    kind: OAuth2UserinfoKind,
) {
    if !candidates
        .iter()
        .any(|candidate| candidate.endpoint == endpoint)
    {
        candidates.push(OAuth2UserinfoCandidate { endpoint, kind });
    }
}

fn oauth2_client_supports_github_email_lookup(client: &crate::state::OAuth2ClientConfig) -> bool {
    client.registration_id.eq_ignore_ascii_case("github")
        && client.scopes.iter().any(|scope| {
            scope.eq_ignore_ascii_case("user") || scope.eq_ignore_ascii_case("user:email")
        })
}

async fn oauth2_account_creation_enabled(app: &HttpAppState) -> bool {
    let state = &app.operational;
    if state.oauth2_account_creation {
        return true;
    }
    if std::env::var("KOMGA_OAUTH2_ACCOUNT_CREATION")
        .ok()
        .is_some_and(|value| value.eq_ignore_ascii_case("true") || value == "1")
    {
        return true;
    }
    if std::env::var("KOMGA_OAUTH2_ACCOUNT_CREATION_ENABLED")
        .ok()
        .is_some_and(|value| value.eq_ignore_ascii_case("true") || value == "1")
    {
        return true;
    }

    let Ok(settings) = app.services.server_settings.load_map().await else {
        return false;
    };
    [
        "OAUTH2_ACCOUNT_CREATION",
        "oauth2AccountCreation",
        "oauth2.account.creation",
    ]
    .iter()
    .find_map(|key| settings.get(*key))
    .and_then(|value| value.as_ref())
    .is_some_and(|value| {
        value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes") || value == "1"
    })
}
