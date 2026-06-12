use std::sync::Arc;

use super::{
    AuthActivityPort, AuthOutcome, AuthTokenSource, AuthUser, AuthenticationActivityApiKey,
    AuthenticationPort, PersistedApiKeyMetadata, ResolvedAuthToken, SessionLifecyclePort,
    SessionResolverPort,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuthSessionActivityContext {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BasicAuthCredentials {
    pub username: String,
    pub password: String,
}

impl BasicAuthCredentials {
    pub fn parse_basic_payload(credentials: &str) -> Option<Self> {
        credentials
            .split_once(':')
            .map(|(username, password)| Self {
                username: username.to_string(),
                password: password.to_string(),
            })
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuthTokenRequest {
    pub session_token: Option<String>,
    pub remember_me_token: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthSessionRequest {
    pub api_key: Option<String>,
    pub basic: Option<BasicAuthCredentials>,
    pub session_token: Option<String>,
    pub remember_me_token: Option<String>,
    pub empty_auth_token_supplied: bool,
    pub remember_me_requested: bool,
    pub session_runtime_key: String,
    pub remember_me_runtime_key: String,
    pub activity: AuthSessionActivityContext,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthSessionSuccess {
    pub user: AuthUser,
    pub session_token: String,
    pub response_mode: AuthSessionResponseMode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthSessionResponseMode {
    BodyOnly,
    SessionCookie,
    SessionHeaderAndCookie,
    RememberMeCookies {
        remember_me_token: String,
        remember_me_max_age_seconds: u64,
    },
    RememberMeHeader {
        remember_me_token: String,
        remember_me_max_age_seconds: u64,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthSessionError {
    InvalidApiKey,
    Unauthorized,
    RememberMeUnavailable,
    StorageFailure,
}

pub struct AuthSessionService {
    authentication: Arc<dyn AuthenticationPort>,
    session_resolver: Arc<dyn SessionResolverPort>,
    session_lifecycle: Arc<dyn SessionLifecyclePort>,
    auth_activity: Arc<dyn AuthActivityPort>,
}

impl AuthSessionService {
    pub fn new(
        authentication: Arc<dyn AuthenticationPort>,
        session_resolver: Arc<dyn SessionResolverPort>,
        session_lifecycle: Arc<dyn SessionLifecyclePort>,
        auth_activity: Arc<dyn AuthActivityPort>,
    ) -> Self {
        Self {
            authentication,
            session_resolver,
            session_lifecycle,
            auth_activity,
        }
    }

    pub async fn authenticate(
        &self,
        request: AuthSessionRequest,
    ) -> Result<AuthSessionSuccess, AuthSessionError> {
        if let Some(api_key) = request.api_key.as_deref() {
            match self
                .authentication
                .authenticate_api_key(api_key)
                .await
                .map_err(|_| AuthSessionError::StorageFailure)?
            {
                AuthOutcome::Valid(user) => {
                    let user = *user;
                    let metadata = self
                        .authentication
                        .api_key_metadata_by_token(api_key)
                        .await
                        .map_err(|_| AuthSessionError::StorageFailure)?;
                    self.record_success(&user, "ApiKey", metadata.as_ref(), &request.activity)
                        .await;
                    return Ok(self.session_success(
                        user,
                        request.session_runtime_key.as_str(),
                        AuthSessionResponseMode::SessionCookie,
                    ));
                }
                AuthOutcome::Invalid => return Err(AuthSessionError::InvalidApiKey),
                AuthOutcome::Missing => {}
            }
        }

        if let Some(resolved) = self
            .resolve_auth_token(&request)
            .map_err(|_| AuthSessionError::StorageFailure)?
        {
            return self.authenticated_token_success(resolved, &request).await;
        }

        let Some(basic) = request.basic.as_ref() else {
            return Err(AuthSessionError::Unauthorized);
        };
        match self
            .authentication
            .authenticate_basic(basic.username.as_str(), basic.password.as_str())
            .await
            .map_err(|_| AuthSessionError::StorageFailure)?
        {
            AuthOutcome::Valid(user) => self.basic_success(*user, &request).await,
            AuthOutcome::Invalid | AuthOutcome::Missing => Err(AuthSessionError::Unauthorized),
        }
    }

    pub fn login_cookie_session_token(
        &self,
        request: AuthTokenRequest,
    ) -> Result<Option<String>, AuthSessionError> {
        let Some(session_token) = request
            .session_token
            .filter(|token| !token.trim().is_empty())
        else {
            return Ok(None);
        };
        let request = AuthTokenRequest {
            session_token: Some(session_token.clone()),
            remember_me_token: None,
        };
        let Some(_) = self
            .resolve_session_user(&request)
            .map_err(|_| AuthSessionError::StorageFailure)?
        else {
            return Ok(None);
        };
        Ok(Some(session_token))
    }

    pub fn logout(&self, request: AuthTokenRequest) -> Result<bool, AuthSessionError> {
        if self
            .resolve_session_user(&request)
            .map_err(|_| AuthSessionError::StorageFailure)?
            .is_none()
        {
            return Ok(false);
        }

        if let Some(token) = request.session_token.as_deref() {
            self.session_lifecycle.invalidate_session_token(token);
        }
        Ok(true)
    }

    fn resolve_session_user(&self, request: &AuthTokenRequest) -> Result<Option<AuthUser>, String> {
        self.session_resolver.resolve_session_user(
            request.session_token.as_deref(),
            request.remember_me_token.as_deref(),
        )
    }

    fn resolve_auth_token(
        &self,
        request: &AuthSessionRequest,
    ) -> Result<Option<ResolvedAuthToken>, String> {
        self.session_resolver.resolve_auth_token(
            request.session_token.as_deref(),
            request.remember_me_token.as_deref(),
        )
    }

    async fn authenticated_token_success(
        &self,
        resolved: ResolvedAuthToken,
        request: &AuthSessionRequest,
    ) -> Result<AuthSessionSuccess, AuthSessionError> {
        match resolved.source {
            AuthTokenSource::Session => Ok(AuthSessionSuccess {
                user: resolved.user,
                session_token: request.session_token.clone().unwrap_or_default(),
                response_mode: AuthSessionResponseMode::BodyOnly,
            }),
            AuthTokenSource::RememberMe => {
                self.record_success(&resolved.user, "RememberMe", None, &request.activity)
                    .await;
                Ok(self.session_success(
                    resolved.user,
                    request.session_runtime_key.as_str(),
                    AuthSessionResponseMode::SessionHeaderAndCookie,
                ))
            }
        }
    }

    async fn basic_success(
        &self,
        user: AuthUser,
        request: &AuthSessionRequest,
    ) -> Result<AuthSessionSuccess, AuthSessionError> {
        self.record_success(&user, "Password", None, &request.activity)
            .await;

        if request.remember_me_requested {
            let Some(remember_me_token) = self
                .session_lifecycle
                .remember_me_token_for_user(&user, request.remember_me_runtime_key.as_str())
            else {
                return Err(AuthSessionError::RememberMeUnavailable);
            };
            let remember_me_max_age_seconds = self
                .session_lifecycle
                .remember_me_max_age_seconds(request.remember_me_runtime_key.as_str());
            let response_mode = if request.empty_auth_token_supplied {
                AuthSessionResponseMode::RememberMeHeader {
                    remember_me_token,
                    remember_me_max_age_seconds,
                }
            } else {
                AuthSessionResponseMode::RememberMeCookies {
                    remember_me_token,
                    remember_me_max_age_seconds,
                }
            };
            return Ok(self.session_success(
                user,
                request.session_runtime_key.as_str(),
                response_mode,
            ));
        }

        let response_mode = if request.empty_auth_token_supplied {
            AuthSessionResponseMode::SessionHeaderAndCookie
        } else {
            AuthSessionResponseMode::SessionCookie
        };
        Ok(self.session_success(user, request.session_runtime_key.as_str(), response_mode))
    }

    fn session_success(
        &self,
        user: AuthUser,
        session_runtime_key: &str,
        response_mode: AuthSessionResponseMode,
    ) -> AuthSessionSuccess {
        let session_token = self
            .session_lifecycle
            .session_token_for_user(&user, session_runtime_key);
        AuthSessionSuccess {
            user,
            session_token,
            response_mode,
        }
    }

    async fn record_success(
        &self,
        user: &AuthUser,
        source: &str,
        api_key_metadata: Option<&PersistedApiKeyMetadata>,
        activity: &AuthSessionActivityContext,
    ) {
        let _ = self
            .auth_activity
            .persisted_record_successful_authentication_activity(
                user,
                source,
                AuthenticationActivityApiKey::from_persisted(api_key_metadata),
                activity.ip.as_deref(),
                activity.user_agent.as_deref(),
            )
            .await;
    }
}
