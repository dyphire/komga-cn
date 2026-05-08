use super::*;
use axum::extract::FromRef;

#[cfg(test)]
use komga_application::identity_access::AuthTokenSource;
use komga_application::identity_access::{
    CreateAuthUserInput, ResolvedAuthToken, UpdateAuthUserInput, UpdateAuthUserResult,
};

#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::sync::Mutex;

#[derive(Clone)]
pub struct PersistedBookMediaFile {
    pub file_name: String,
    pub media_type: String,
    pub file_path: PathBuf,
}

#[derive(Clone)]
pub struct PersistedReadProgressRecord {
    pub page: i64,
    pub completed: bool,
    pub created: String,
    pub last_modified: String,
    pub device_id: String,
    pub device_name: String,
    pub locator: Option<Vec<u8>>,
}

#[derive(Clone)]
pub struct KoreaderBookTarget {
    pub id: String,
    pub page_count: u64,
    pub media_type: String,
}

#[derive(Clone)]
pub struct KoboMetadataRecord {
    pub title: String,
    pub summary: String,
    pub release_date: Option<String>,
    pub created_date: Option<String>,
    pub language: String,
    pub file_size: u64,
    pub file_name: String,
    pub media_type: String,
    pub contributor_names: Vec<String>,
    pub isbn: Option<String>,
    pub publisher_name: Option<String>,
    pub cover_image_id: Option<String>,
    pub series_id: Option<String>,
    pub series_name: Option<String>,
    pub series_number: Option<String>,
    pub series_number_float: Option<f64>,
    pub oneshot: bool,
    pub is_kepub: bool,
    pub is_pre_paginated: bool,
}

#[derive(Clone, Debug)]
pub enum KoreaderBookLookupError {
    Persistence,
    Conflict,
}

#[derive(Clone, Debug, Default)]
pub struct AuthenticationActivityWriteInput {
    pub source: String,
    pub api_key_id: Option<String>,
    pub api_key_comment: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait IdentityService: Send + Sync {
    fn auth_token_user(&self, headers: &HeaderMap) -> Option<AuthUser>;
    fn auth_token_resolution(&self, headers: &HeaderMap) -> Option<ResolvedAuthToken>;
    fn session_token_for_user_with_runtime_key(&self, user: &AuthUser, runtime_key: &str)
    -> String;
    fn remember_me_token_for_user_with_runtime_key(
        &self,
        user: &AuthUser,
        runtime_key: &str,
    ) -> Option<String>;
    fn sync_session_runtime_settings(&self, runtime_key: &str, max_inactive_seconds: u64);
    fn sync_remember_me_runtime_database_file(&self, runtime_key: &str);
    fn sync_remember_me_runtime_settings(&self, runtime_key: &str, key: &str, duration_days: u64);
    fn remember_me_max_age_seconds(&self, runtime_key: &str) -> u64;
    fn invalidate_user_sessions(&self, user_id: &str);
    fn invalidate_user_sessions_with_runtime_key(&self, user_id: &str, runtime_key: &str);
    fn invalidate_session_token(&self, token: &str);
    fn invalidate_remember_me_token(&self, token: &str);
    fn store_oauth2_authorization_state(
        &self,
        runtime_key: &str,
        session_token: &str,
        registration_id: &str,
        state: &str,
    );
    fn take_oauth2_authorization_state(
        &self,
        runtime_key: &str,
        session_token: &str,
        registration_id: &str,
    ) -> Option<String>;
    async fn persisted_basic_user(&self, headers: &HeaderMap) -> Option<AuthOutcome>;
    async fn persisted_api_key_user(&self, headers: &HeaderMap) -> Option<AuthOutcome>;
    async fn persisted_api_key_user_by_token(&self, api_key: &str) -> Option<AuthOutcome>;
    async fn persisted_api_key_metadata(
        &self,
        headers: &HeaderMap,
    ) -> Option<PersistedApiKeyMetadata>;
    async fn persisted_users(&self) -> Option<Vec<AuthUser>>;
    async fn persisted_update_password_by_user_id(
        &self,
        user_id: &str,
        password: &str,
    ) -> Option<bool>;
    async fn persisted_create_api_key(
        &self,
        user_id: &str,
        comment: &str,
    ) -> Option<PersistedApiKey>;
    async fn persisted_api_key_comment_exists(&self, user_id: &str, comment: &str) -> Option<bool>;
    async fn persisted_list_api_keys(&self, user_id: &str) -> Option<Vec<PersistedApiKey>>;
    async fn persisted_delete_api_key_by_id(&self, user_id: &str, api_key_id: &str)
    -> Option<bool>;
    async fn persisted_list_authentication_activity(
        &self,
        user_id: Option<&str>,
    ) -> Option<Vec<PersistedAuthenticationActivity>>;
    async fn persisted_cleanup_authentication_activity(&self) -> Option<u64>;
    async fn persisted_latest_authentication_activity_by_user_and_api_key(
        &self,
        user_id: &str,
        api_key_id: &str,
    ) -> Option<PersistedAuthenticationActivity>;
    async fn persisted_record_failed_authentication_activity(
        &self,
        email: Option<&str>,
        input: AuthenticationActivityWriteInput,
        error: &str,
    ) -> Option<()>;
    async fn persisted_record_successful_authentication_activity(
        &self,
        user: &AuthUser,
        input: AuthenticationActivityWriteInput,
    ) -> Option<()>;
    async fn ensure_oauth_user(
        &self,
        email: &str,
        allow_create: bool,
    ) -> Result<Option<AuthUser>, sqlx::Error>;
    fn configured_api_key(&self) -> Option<String>;
    async fn load_book_created_timestamp(
        &self,
        book_id: &str,
    ) -> Result<Option<String>, sqlx::Error>;
    async fn load_book_last_epub_position_locator(
        &self,
        book_id: &str,
    ) -> Result<Option<Value>, sqlx::Error>;
    async fn load_book_media_file(
        &self,
        book_id: &str,
    ) -> Result<Option<PersistedBookMediaFile>, sqlx::Error>;
    async fn load_kobo_metadata_record(
        &self,
        book_id: &str,
    ) -> Result<Option<KoboMetadataRecord>, sqlx::Error>;
    async fn load_kobo_sync_page(
        &self,
        user: &AuthUser,
        user_id: &str,
        current_api_key_id: Option<&str>,
        ongoing_sync_point_id: Option<&str>,
        last_successful_sync_point_id: Option<&str>,
        limit: usize,
    ) -> Result<KoboSyncPage, sqlx::Error>;
    async fn load_koreader_book_target(
        &self,
        book_hash: &str,
    ) -> Result<Option<KoreaderBookTarget>, KoreaderBookLookupError>;
    async fn load_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<PersistedReadProgressRecord>, sqlx::Error>;
    async fn load_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> Result<Option<(String, Vec<u8>)>, sqlx::Error>;
    async fn persist_read_progress_with_locator(
        &self,
        book_id: &str,
        user_id: &str,
        page: i64,
        completed: bool,
        device_id: &str,
        device_name: &str,
        timestamp: &str,
        locator: Option<Value>,
    ) -> Result<(), String>;
    async fn persisted_book_exists(&self, book_id: &str) -> Result<bool, sqlx::Error>;
    async fn proxy_kobo_store_library_sync(
        &self,
        forwarded_headers: &[(String, String)],
        query: Option<&str>,
        raw_sync_token: &str,
    ) -> Result<KoboStoreSyncMergeResult, ()>;
    async fn remove_sync_point(&self, sync_point_id: &str) -> Result<(), sqlx::Error>;
    async fn create_auth_user(
        &self,
        input: CreateAuthUserInput,
    ) -> Result<Option<AuthUser>, sqlx::Error>;
    async fn delete_auth_user(&self, target_user_id: &str) -> Result<bool, sqlx::Error>;
    async fn update_auth_user(
        &self,
        target_user_id: &str,
        patch: UpdateAuthUserInput,
    ) -> Result<UpdateAuthUserResult, sqlx::Error>;
    async fn open_auth_pool(&self) -> Result<SqlitePool, sqlx::Error>;
}

#[cfg(test)]
#[derive(Default)]
struct RuntimeIdentityAccessTestState {
    session_users: HashMap<String, AuthUser>,
    remember_me_users: HashMap<String, AuthUser>,
    oauth2_authorization_states: HashMap<(String, String, String), String>,
    koreader_book_targets:
        HashMap<(PathBuf, String), Result<Option<KoreaderBookTarget>, KoreaderBookLookupError>>,
}

#[cfg(test)]
fn test_state() -> &'static Mutex<RuntimeIdentityAccessTestState> {
    static TEST_STATE: std::sync::OnceLock<Mutex<RuntimeIdentityAccessTestState>> =
        std::sync::OnceLock::new();
    TEST_STATE.get_or_init(|| Mutex::new(RuntimeIdentityAccessTestState::default()))
}

#[cfg(test)]
#[derive(Clone, Default)]
struct TestIdentityService;

#[derive(Clone)]
pub struct IdentityState {
    pub service: Arc<dyn IdentityService>,
}

#[derive(Clone)]
pub struct IdentityAccessState {
    pub(crate) discovery_auth: DiscoveryAuthState,
    pub(crate) auth_db: AuthDatabaseState,
    pub(crate) operational: OperationalState,
    pub(crate) identity: IdentityState,
    pub(crate) server_settings: Arc<dyn ServerSettingsService>,
    pub(crate) media_assets: Arc<dyn MediaAssetsService>,
}

impl FromRef<Arc<HttpAppState>> for IdentityState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            service: app.services.runtime_identity.clone(),
        }
    }
}

impl FromRef<Arc<HttpAppState>> for IdentityAccessState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            discovery_auth: app.discovery_auth.clone(),
            auth_db: app.auth_db.clone(),
            operational: app.operational.clone(),
            identity: IdentityState::from_ref(app),
            server_settings: app.services.server_settings.clone(),
            media_assets: app.services.media_assets.clone(),
        }
    }
}

#[cfg(test)]
pub(crate) fn default_test_identity_service() -> Arc<dyn IdentityService> {
    Arc::new(TestIdentityService)
}

#[cfg(test)]
pub(crate) fn seed_koreader_book_target(
    database_file: &std::path::Path,
    book_hash: &str,
    result: Result<Option<KoreaderBookTarget>, KoreaderBookLookupError>,
) {
    test_state()
        .lock()
        .expect("runtime identity access test state lock should not be poisoned")
        .koreader_book_targets
        .insert((database_file.to_path_buf(), book_hash.to_string()), result);
}

#[cfg(test)]
#[async_trait::async_trait]
impl IdentityService for TestIdentityService {
    fn auth_token_user(&self, headers: &HeaderMap) -> Option<AuthUser> {
        self.auth_token_resolution(headers)
            .map(|resolved| resolved.user)
    }

    fn auth_token_resolution(&self, headers: &HeaderMap) -> Option<ResolvedAuthToken> {
        headers
            .get("X-Auth-Token")
            .and_then(|value| value.to_str().ok())
            .and_then(|token| {
                test_state()
                    .lock()
                    .expect("runtime identity access test state lock should not be poisoned")
                    .session_users
                    .get(token)
                    .cloned()
            })
            .map(|user| ResolvedAuthToken {
                user,
                source: AuthTokenSource::Session,
            })
    }

    fn session_token_for_user_with_runtime_key(
        &self,
        user: &AuthUser,
        runtime_key: &str,
    ) -> String {
        let token = format!("test-session-{runtime_key}-{}", user.id);
        test_state()
            .lock()
            .expect("runtime identity access test state lock should not be poisoned")
            .session_users
            .insert(token.clone(), user.clone());
        token
    }

    fn remember_me_token_for_user_with_runtime_key(
        &self,
        user: &AuthUser,
        runtime_key: &str,
    ) -> Option<String> {
        let token = format!("test-remember-me-{runtime_key}-{}", user.id);
        test_state()
            .lock()
            .expect("runtime identity access test state lock should not be poisoned")
            .remember_me_users
            .insert(token.clone(), user.clone());
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
        365 * 24 * 60 * 60
    }
    fn invalidate_user_sessions(&self, _user_id: &str) {}
    fn invalidate_user_sessions_with_runtime_key(&self, _user_id: &str, _runtime_key: &str) {}
    fn invalidate_session_token(&self, _token: &str) {}
    fn invalidate_remember_me_token(&self, _token: &str) {}

    fn store_oauth2_authorization_state(
        &self,
        runtime_key: &str,
        session_token: &str,
        registration_id: &str,
        state: &str,
    ) {
        test_state()
            .lock()
            .expect("runtime identity access test state lock should not be poisoned")
            .oauth2_authorization_states
            .insert(
                (
                    runtime_key.to_string(),
                    session_token.to_string(),
                    registration_id.to_string(),
                ),
                state.to_string(),
            );
    }

    fn take_oauth2_authorization_state(
        &self,
        runtime_key: &str,
        session_token: &str,
        registration_id: &str,
    ) -> Option<String> {
        test_state()
            .lock()
            .expect("runtime identity access test state lock should not be poisoned")
            .oauth2_authorization_states
            .remove(&(
                runtime_key.to_string(),
                session_token.to_string(),
                registration_id.to_string(),
            ))
    }

    async fn persisted_basic_user(&self, _headers: &HeaderMap) -> Option<AuthOutcome> {
        Some(AuthOutcome::Missing)
    }
    async fn persisted_api_key_user(&self, _headers: &HeaderMap) -> Option<AuthOutcome> {
        Some(AuthOutcome::Missing)
    }
    async fn persisted_api_key_user_by_token(&self, _api_key: &str) -> Option<AuthOutcome> {
        None
    }
    async fn persisted_api_key_metadata(
        &self,
        _headers: &HeaderMap,
    ) -> Option<PersistedApiKeyMetadata> {
        None
    }
    async fn persisted_users(&self) -> Option<Vec<AuthUser>> {
        Some(vec![])
    }
    async fn persisted_update_password_by_user_id(
        &self,
        _user_id: &str,
        _password: &str,
    ) -> Option<bool> {
        Some(false)
    }
    async fn persisted_create_api_key(
        &self,
        _user_id: &str,
        _comment: &str,
    ) -> Option<PersistedApiKey> {
        None
    }
    async fn persisted_api_key_comment_exists(
        &self,
        _user_id: &str,
        _comment: &str,
    ) -> Option<bool> {
        Some(false)
    }
    async fn persisted_list_api_keys(&self, _user_id: &str) -> Option<Vec<PersistedApiKey>> {
        Some(vec![])
    }
    async fn persisted_delete_api_key_by_id(
        &self,
        _user_id: &str,
        _api_key_id: &str,
    ) -> Option<bool> {
        Some(false)
    }
    async fn persisted_list_authentication_activity(
        &self,
        _user_id: Option<&str>,
    ) -> Option<Vec<PersistedAuthenticationActivity>> {
        Some(vec![])
    }
    async fn persisted_cleanup_authentication_activity(&self) -> Option<u64> {
        Some(0)
    }
    async fn persisted_latest_authentication_activity_by_user_and_api_key(
        &self,
        _user_id: &str,
        _api_key_id: &str,
    ) -> Option<PersistedAuthenticationActivity> {
        None
    }
    async fn persisted_record_failed_authentication_activity(
        &self,
        _email: Option<&str>,
        _input: AuthenticationActivityWriteInput,
        _error: &str,
    ) -> Option<()> {
        Some(())
    }
    async fn persisted_record_successful_authentication_activity(
        &self,
        _user: &AuthUser,
        _input: AuthenticationActivityWriteInput,
    ) -> Option<()> {
        Some(())
    }
    async fn ensure_oauth_user(
        &self,
        _email: &str,
        _allow_create: bool,
    ) -> Result<Option<AuthUser>, sqlx::Error> {
        Ok(None)
    }
    fn configured_api_key(&self) -> Option<String> {
        None
    }
    async fn load_book_created_timestamp(
        &self,
        _book_id: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        Ok(None)
    }
    async fn load_book_last_epub_position_locator(
        &self,
        _book_id: &str,
    ) -> Result<Option<Value>, sqlx::Error> {
        Ok(None)
    }
    async fn load_book_media_file(
        &self,
        _book_id: &str,
    ) -> Result<Option<PersistedBookMediaFile>, sqlx::Error> {
        Ok(None)
    }
    async fn load_kobo_metadata_record(
        &self,
        _book_id: &str,
    ) -> Result<Option<KoboMetadataRecord>, sqlx::Error> {
        Ok(None)
    }
    async fn load_kobo_sync_page(
        &self,
        _user: &AuthUser,
        _user_id: &str,
        _current_api_key_id: Option<&str>,
        _ongoing_sync_point_id: Option<&str>,
        _last_successful_sync_point_id: Option<&str>,
        _limit: usize,
    ) -> Result<KoboSyncPage, sqlx::Error> {
        Ok(KoboSyncPage {
            to_sync_point_id: String::new(),
            from_sync_point_id: None,
            books_added: Vec::new(),
            books_changed: Vec::new(),
            books_removed: Vec::new(),
            books_read_progress_changed: Vec::new(),
            readlists_added: Vec::new(),
            readlists_changed: Vec::new(),
            readlists_removed: Vec::new(),
            should_continue: false,
        })
    }
    async fn load_koreader_book_target(
        &self,
        book_hash: &str,
    ) -> Result<Option<KoreaderBookTarget>, KoreaderBookLookupError> {
        test_state()
            .lock()
            .expect("runtime identity access test state lock should not be poisoned")
            .koreader_book_targets
            .iter()
            .find(|((_, hash), _)| hash == book_hash)
            .map(|(_, result)| result.clone())
            .unwrap_or(Ok(None))
    }
    async fn load_read_progress(
        &self,
        _book_id: &str,
        _user_id: &str,
    ) -> Result<Option<PersistedReadProgressRecord>, sqlx::Error> {
        Ok(None)
    }
    async fn load_thumbnail_by_id(
        &self,
        _thumbnail_id: &str,
    ) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
        Ok(None)
    }
    async fn persist_read_progress_with_locator(
        &self,
        _book_id: &str,
        _user_id: &str,
        _page: i64,
        _completed: bool,
        _device_id: &str,
        _device_name: &str,
        _timestamp: &str,
        _locator: Option<Value>,
    ) -> Result<(), String> {
        Ok(())
    }
    async fn persisted_book_exists(&self, _book_id: &str) -> Result<bool, sqlx::Error> {
        Ok(false)
    }
    async fn proxy_kobo_store_library_sync(
        &self,
        _forwarded_headers: &[(String, String)],
        _query: Option<&str>,
        _raw_sync_token: &str,
    ) -> Result<KoboStoreSyncMergeResult, ()> {
        Ok(KoboStoreSyncMergeResult {
            events: vec![],
            raw_sync_token: None,
            should_continue: false,
        })
    }
    async fn remove_sync_point(&self, _sync_point_id: &str) -> Result<(), sqlx::Error> {
        Ok(())
    }
    async fn create_auth_user(
        &self,
        _input: CreateAuthUserInput,
    ) -> Result<Option<AuthUser>, sqlx::Error> {
        Ok(None)
    }
    async fn delete_auth_user(&self, _target_user_id: &str) -> Result<bool, sqlx::Error> {
        Ok(false)
    }
    async fn update_auth_user(
        &self,
        _target_user_id: &str,
        _patch: UpdateAuthUserInput,
    ) -> Result<UpdateAuthUserResult, sqlx::Error> {
        Ok(UpdateAuthUserResult {
            updated: false,
            expire_sessions: false,
        })
    }
    async fn open_auth_pool(&self) -> Result<SqlitePool, sqlx::Error> {
        Err(sqlx::Error::PoolClosed)
    }
}
