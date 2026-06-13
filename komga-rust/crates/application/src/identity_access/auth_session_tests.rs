use std::sync::{Arc, Mutex};

use super::{
    AuthActivityPort, AuthOutcome, AuthSessionActivityContext, AuthSessionError,
    AuthSessionRequest, AuthSessionResponseMode, AuthSessionService, AuthTokenRequest,
    AuthTokenSource, AuthUser, AuthenticationActivityApiKey, AuthenticationPort,
    BasicAuthCredentials, PersistedApiKeyMetadata, PersistedAuthenticationActivity,
    ResolvedAuthToken, SessionLifecyclePort, SessionResolverPort,
};

#[tokio::test]
async fn session_token_returns_existing_session_without_recording_activity() {
    let ports = Arc::new(AuthSessionPorts::with_resolved_token(ResolvedAuthToken {
        user: sample_user("session-user"),
        source: AuthTokenSource::Session,
    }));
    let service =
        AuthSessionService::new(ports.clone(), ports.clone(), ports.clone(), ports.clone());

    let outcome = service
        .authenticate(AuthSessionRequest {
            session_token: Some("session-token".to_string()),
            ..request_defaults()
        })
        .await
        .expect("session token should authenticate");

    assert_eq!(outcome.user.id, "session-user");
    assert_eq!(outcome.session_token, "session-token");
    assert_eq!(outcome.response_mode, AuthSessionResponseMode::BodyOnly);
    assert!(ports.activity_calls().is_empty());
    assert!(ports.issued_tokens().is_empty());
}

#[tokio::test]
async fn remember_me_token_issues_session_and_records_activity() {
    let ports = Arc::new(AuthSessionPorts::with_resolved_token(ResolvedAuthToken {
        user: sample_user("remember-user"),
        source: AuthTokenSource::RememberMe,
    }));
    let service =
        AuthSessionService::new(ports.clone(), ports.clone(), ports.clone(), ports.clone());

    let outcome = service
        .authenticate(AuthSessionRequest {
            remember_me_token: Some("remember-token".to_string()),
            activity: sample_activity(),
            ..request_defaults()
        })
        .await
        .expect("remember-me token should authenticate");

    assert_eq!(outcome.user.id, "remember-user");
    assert_eq!(
        outcome.session_token,
        "session:session-runtime:remember-user"
    );
    assert_eq!(
        outcome.response_mode,
        AuthSessionResponseMode::SessionHeaderAndCookie
    );
    assert_eq!(
        ports.activity_calls(),
        vec!["success:remember-user:RememberMe::".to_string()]
    );
}

#[tokio::test]
async fn api_key_login_records_metadata_and_issues_session_cookie() {
    let ports = Arc::new(AuthSessionPorts {
        api_key_outcome: Some(AuthOutcome::Valid(Box::new(sample_user("api-user")))),
        api_key_metadata: Some(PersistedApiKeyMetadata {
            id: "api-key-1".to_string(),
            comment: "Reader".to_string(),
        }),
        ..AuthSessionPorts::default()
    });
    let service =
        AuthSessionService::new(ports.clone(), ports.clone(), ports.clone(), ports.clone());

    let outcome = service
        .authenticate(AuthSessionRequest {
            api_key: Some("api-token".to_string()),
            activity: sample_activity(),
            ..request_defaults()
        })
        .await
        .expect("api key should authenticate");

    assert_eq!(outcome.user.id, "api-user");
    assert_eq!(outcome.session_token, "session:session-runtime:api-user");
    assert_eq!(
        outcome.response_mode,
        AuthSessionResponseMode::SessionCookie
    );
    assert_eq!(
        ports.activity_calls(),
        vec!["success:api-user:ApiKey:api-key-1:Reader".to_string()]
    );
}

#[tokio::test]
async fn api_key_storage_failure_stops_authentication() {
    let ports = Arc::new(AuthSessionPorts {
        api_key_outcome: None,
        basic_outcome: Some(AuthOutcome::Valid(Box::new(sample_user("basic-user")))),
        ..AuthSessionPorts::default()
    });
    let service =
        AuthSessionService::new(ports.clone(), ports.clone(), ports.clone(), ports.clone());

    let error = service
        .authenticate(AuthSessionRequest {
            api_key: Some("api-token".to_string()),
            basic: Some(BasicAuthCredentials {
                username: "reader@example.org".to_string(),
                password: "secret".to_string(),
            }),
            ..request_defaults()
        })
        .await
        .expect_err("api key storage failure should not fall back to basic auth");

    assert_eq!(error, AuthSessionError::StorageFailure);
}

#[tokio::test]
async fn basic_storage_failure_is_not_unauthorized() {
    let ports = Arc::new(AuthSessionPorts {
        basic_outcome: None,
        ..AuthSessionPorts::default()
    });
    let service =
        AuthSessionService::new(ports.clone(), ports.clone(), ports.clone(), ports.clone());

    let error = service
        .authenticate(AuthSessionRequest {
            basic: Some(BasicAuthCredentials {
                username: "reader@example.org".to_string(),
                password: "secret".to_string(),
            }),
            ..request_defaults()
        })
        .await
        .expect_err("basic auth storage failure should not become unauthorized");

    assert_eq!(error, AuthSessionError::StorageFailure);
}

#[tokio::test]
async fn token_resolution_storage_failure_stops_authentication() {
    let ports = Arc::new(AuthSessionPorts {
        resolved_token_error: true,
        basic_outcome: Some(AuthOutcome::Valid(Box::new(sample_user("basic-user")))),
        ..AuthSessionPorts::default()
    });
    let service =
        AuthSessionService::new(ports.clone(), ports.clone(), ports.clone(), ports.clone());

    let error = service
        .authenticate(AuthSessionRequest {
            session_token: Some("session-token".to_string()),
            basic: Some(BasicAuthCredentials {
                username: "reader@example.org".to_string(),
                password: "secret".to_string(),
            }),
            ..request_defaults()
        })
        .await
        .expect_err("token storage failure should not fall back to basic auth");

    assert_eq!(error, AuthSessionError::StorageFailure);
}

#[tokio::test]
async fn basic_remember_me_with_empty_auth_token_returns_header_mode() {
    let ports = Arc::new(AuthSessionPorts {
        basic_outcome: Some(AuthOutcome::Valid(Box::new(sample_user("password-user")))),
        remember_me_max_age_seconds: 345_600,
        ..AuthSessionPorts::default()
    });
    let service =
        AuthSessionService::new(ports.clone(), ports.clone(), ports.clone(), ports.clone());

    let outcome = service
        .authenticate(AuthSessionRequest {
            basic: Some(BasicAuthCredentials {
                username: "reader@example.org".to_string(),
                password: "secret".to_string(),
            }),
            remember_me_requested: true,
            empty_auth_token_supplied: true,
            activity: sample_activity(),
            ..request_defaults()
        })
        .await
        .expect("basic credentials should authenticate");

    assert_eq!(outcome.user.id, "password-user");
    assert_eq!(
        outcome.session_token,
        "session:session-runtime:password-user"
    );
    assert_eq!(
        outcome.response_mode,
        AuthSessionResponseMode::RememberMeHeader {
            remember_me_token: "remember:remember-runtime:password-user".to_string(),
            remember_me_max_age_seconds: 345_600,
        }
    );
    assert_eq!(
        ports.issued_tokens(),
        vec![
            "remember:remember-runtime:password-user".to_string(),
            "session:session-runtime:password-user".to_string(),
        ]
    );
}

#[test]
fn login_cookie_uses_only_present_session_token_after_authentication() {
    let ports = Arc::new(AuthSessionPorts::with_resolved_user(sample_user(
        "session-user",
    )));
    let service =
        AuthSessionService::new(ports.clone(), ports.clone(), ports.clone(), ports.clone());

    assert_eq!(
        service.login_cookie_session_token(AuthTokenRequest {
            session_token: Some("session-token".to_string()),
            remember_me_token: None,
        }),
        Ok(Some("session-token".to_string()))
    );
    assert_eq!(
        service.login_cookie_session_token(AuthTokenRequest {
            session_token: None,
            remember_me_token: Some("remember-token".to_string()),
        }),
        Ok(None)
    );
}

#[test]
fn logout_invalidates_only_present_session_token_after_authentication() {
    let ports = Arc::new(AuthSessionPorts::with_resolved_user(sample_user(
        "session-user",
    )));
    let service =
        AuthSessionService::new(ports.clone(), ports.clone(), ports.clone(), ports.clone());

    assert!(
        service
            .logout(AuthTokenRequest {
                session_token: Some("session-token".to_string()),
                remember_me_token: Some("remember-token".to_string()),
            })
            .expect("logout token resolution should not fail"),
        "logout should succeed for an authenticated request"
    );
    assert_eq!(
        ports.invalidated_session_tokens(),
        vec!["session-token".to_string()]
    );
}

#[test]
fn token_resolution_storage_failure_is_not_unauthorized() {
    let ports = Arc::new(AuthSessionPorts {
        resolved_user_error: true,
        ..AuthSessionPorts::default()
    });
    let service =
        AuthSessionService::new(ports.clone(), ports.clone(), ports.clone(), ports.clone());

    assert_eq!(
        service.login_cookie_session_token(AuthTokenRequest {
            session_token: Some("session-token".to_string()),
            remember_me_token: Some("remember-token".to_string()),
        }),
        Err(AuthSessionError::StorageFailure)
    );
    assert_eq!(
        service.logout(AuthTokenRequest {
            session_token: Some("session-token".to_string()),
            remember_me_token: Some("remember-token".to_string()),
        }),
        Err(AuthSessionError::StorageFailure)
    );
}

#[derive(Default)]
struct AuthSessionPorts {
    basic_outcome: Option<AuthOutcome>,
    api_key_outcome: Option<AuthOutcome>,
    api_key_metadata: Option<PersistedApiKeyMetadata>,
    resolved_user: Option<AuthUser>,
    resolved_token: Option<ResolvedAuthToken>,
    resolved_user_error: bool,
    resolved_token_error: bool,
    remember_me_max_age_seconds: u64,
    issued_tokens: Mutex<Vec<String>>,
    invalidated_session_tokens: Mutex<Vec<String>>,
    activity_calls: Mutex<Vec<String>>,
}

impl AuthSessionPorts {
    fn with_resolved_user(user: AuthUser) -> Self {
        Self {
            resolved_user: Some(user),
            ..Self::default()
        }
    }

    fn with_resolved_token(resolved_token: ResolvedAuthToken) -> Self {
        Self {
            resolved_token: Some(resolved_token),
            ..Self::default()
        }
    }

    fn issued_tokens(&self) -> Vec<String> {
        self.issued_tokens
            .lock()
            .expect("issued tokens lock should not be poisoned")
            .clone()
    }

    fn invalidated_session_tokens(&self) -> Vec<String> {
        self.invalidated_session_tokens
            .lock()
            .expect("invalidated session tokens lock should not be poisoned")
            .clone()
    }

    fn activity_calls(&self) -> Vec<String> {
        self.activity_calls
            .lock()
            .expect("activity calls lock should not be poisoned")
            .clone()
    }
}

#[async_trait::async_trait]
impl AuthenticationPort for AuthSessionPorts {
    async fn authenticate_basic(
        &self,
        _username: &str,
        _password: &str,
    ) -> Result<AuthOutcome, String> {
        self.basic_outcome
            .clone()
            .ok_or_else(|| "storage failure".to_string())
    }

    async fn authenticate_api_key(&self, _api_key: &str) -> Result<AuthOutcome, String> {
        self.api_key_outcome
            .clone()
            .ok_or_else(|| "storage failure".to_string())
    }

    async fn api_key_metadata_by_token(
        &self,
        _api_key: &str,
    ) -> Result<Option<PersistedApiKeyMetadata>, String> {
        Ok(self.api_key_metadata.clone())
    }
}

impl SessionResolverPort for AuthSessionPorts {
    fn resolve_session_user(
        &self,
        _session_token: Option<&str>,
        _remember_me_token: Option<&str>,
    ) -> Result<Option<AuthUser>, String> {
        if self.resolved_user_error {
            return Err("token storage failure".to_string());
        }
        Ok(self.resolved_user.clone())
    }

    fn resolve_auth_token(
        &self,
        _session_token: Option<&str>,
        _remember_me_token: Option<&str>,
    ) -> Result<Option<ResolvedAuthToken>, String> {
        if self.resolved_token_error {
            return Err("token storage failure".to_string());
        }
        Ok(self.resolved_token.clone())
    }
}

impl SessionLifecyclePort for AuthSessionPorts {
    fn session_token_for_user(&self, user: &AuthUser, runtime_key: &str) -> String {
        let token = format!("session:{runtime_key}:{}", user.id);
        self.issued_tokens
            .lock()
            .expect("issued tokens lock should not be poisoned")
            .push(token.clone());
        token
    }

    fn remember_me_token_for_user(&self, user: &AuthUser, runtime_key: &str) -> Option<String> {
        let token = format!("remember:{runtime_key}:{}", user.id);
        self.issued_tokens
            .lock()
            .expect("issued tokens lock should not be poisoned")
            .push(token.clone());
        Some(token)
    }

    fn sync_session_runtime_settings(&self, _runtime_key: &str, _max_inactive_seconds: u64) {}

    fn sync_remember_me_runtime_database_file(&self, _runtime_key: &str) {}

    fn sync_remember_me_runtime_settings(
        &self,
        _runtime_key: &str,
        _key: &str,
        _duration_days: u64,
    ) {
    }

    fn remember_me_max_age_seconds(&self, _runtime_key: &str) -> u64 {
        self.remember_me_max_age_seconds
    }

    fn invalidate_user_sessions(&self, _user_id: &str) {}

    fn invalidate_user_sessions_with_runtime_key(&self, _user_id: &str, _runtime_key: &str) {}

    fn invalidate_session_token(&self, token: &str) {
        self.invalidated_session_tokens
            .lock()
            .expect("invalidated session tokens lock should not be poisoned")
            .push(token.to_string());
    }

    fn invalidate_remember_me_token(&self, _token: &str) {}

    fn store_oauth2_authorization_state(
        &self,
        _runtime_key: &str,
        _session_token: &str,
        _registration_id: &str,
        _state: &str,
    ) {
    }

    fn take_oauth2_authorization_state(
        &self,
        _runtime_key: &str,
        _session_token: &str,
        _registration_id: &str,
    ) -> Option<String> {
        None
    }
}

#[async_trait::async_trait]
impl AuthActivityPort for AuthSessionPorts {
    async fn persisted_list_authentication_activity(
        &self,
        _user_id: Option<&str>,
    ) -> Result<Vec<PersistedAuthenticationActivity>, String> {
        Ok(Vec::new())
    }

    async fn persisted_cleanup_authentication_activity(&self) -> Result<u64, String> {
        Ok(0)
    }

    async fn persisted_record_failed_authentication_activity(
        &self,
        _email: Option<&str>,
        _source: &str,
        _error: &str,
        _ip: Option<&str>,
        _user_agent: Option<&str>,
    ) -> Option<()> {
        None
    }

    async fn persisted_record_successful_authentication_activity(
        &self,
        user: &AuthUser,
        source: &str,
        api_key: AuthenticationActivityApiKey<'_>,
        _ip: Option<&str>,
        _user_agent: Option<&str>,
    ) -> Option<()> {
        self.activity_calls
            .lock()
            .expect("activity calls lock should not be poisoned")
            .push(format!(
                "success:{}:{}:{}:{}",
                user.id,
                source,
                api_key.id.unwrap_or_default(),
                api_key.comment.unwrap_or_default()
            ));
        Some(())
    }
}

fn request_defaults() -> AuthSessionRequest {
    AuthSessionRequest {
        api_key: None,
        basic: None,
        session_token: None,
        remember_me_token: None,
        empty_auth_token_supplied: false,
        remember_me_requested: false,
        session_runtime_key: "session-runtime".to_string(),
        remember_me_runtime_key: "remember-runtime".to_string(),
        activity: AuthSessionActivityContext::default(),
    }
}

fn sample_activity() -> AuthSessionActivityContext {
    AuthSessionActivityContext {
        ip: Some("198.51.100.42".to_string()),
        user_agent: Some("reader".to_string()),
    }
}

fn sample_user(id: &str) -> AuthUser {
    AuthUser {
        id: id.to_string(),
        email: format!("{id}@example.org"),
        password: String::new(),
        roles: Vec::new(),
        shared_all_libraries: true,
        shared_library_ids: Vec::new(),
        labels_allow: Vec::new(),
        labels_exclude: Vec::new(),
        age_restriction: None,
    }
}
