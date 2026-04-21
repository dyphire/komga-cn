use super::*;

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

#[derive(Clone, Debug)]
pub struct SharedLibrariesInput {
    pub all: bool,
    pub library_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct AuthUserAgeRestrictionInput {
    pub age: i64,
    pub allow_only: bool,
}

#[derive(Clone, Debug)]
pub struct CreateAuthUserInput {
    pub user_id: String,
    pub email: String,
    pub password_hash: String,
    pub roles: Vec<String>,
    pub shared_libraries: SharedLibrariesInput,
    pub labels_allow: Vec<String>,
    pub labels_exclude: Vec<String>,
    pub age_restriction: Option<AuthUserAgeRestrictionInput>,
}

#[derive(Clone, Debug)]
pub struct UpdateAuthUserInput {
    pub roles: Option<Vec<String>>,
    pub shared_libraries: Option<SharedLibrariesInput>,
    pub labels_allow: Option<Vec<String>>,
    pub labels_exclude: Option<Vec<String>>,
    pub age_restriction: Option<Option<AuthUserAgeRestrictionInput>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateAuthUserResult {
    pub updated: bool,
    pub expire_sessions: bool,
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
    fn auth_token_user(&self, headers: HeaderMap) -> Option<AuthUser>;
    fn session_token_for_user_with_runtime_key(
        &self,
        user: AuthUser,
        runtime_key: String,
    ) -> String;
    fn remember_me_token_for_user_with_runtime_key(
        &self,
        user: AuthUser,
        runtime_key: String,
    ) -> Option<String>;
    fn sync_session_runtime_settings(&self, runtime_key: String, max_inactive_seconds: u64);
    fn sync_remember_me_runtime_database_file(&self, runtime_key: String, database_file: PathBuf);
    fn sync_remember_me_runtime_settings(
        &self,
        runtime_key: String,
        key: String,
        duration_days: u64,
    );
    fn remember_me_max_age_seconds(&self, runtime_key: String) -> u64;
    fn invalidate_user_sessions(&self, user_id: String);
    fn invalidate_user_sessions_with_runtime_key(&self, user_id: String, runtime_key: String);
    fn invalidate_session_token(&self, token: String);
    fn invalidate_remember_me_token(&self, token: String);
    fn store_oauth2_authorization_state(
        &self,
        runtime_key: String,
        session_token: String,
        registration_id: String,
        state: String,
    );
    fn take_oauth2_authorization_state(
        &self,
        runtime_key: String,
        session_token: String,
        registration_id: String,
    ) -> Option<String>;
    async fn persisted_basic_user(
        &self,
        headers: HeaderMap,
        database_file: PathBuf,
    ) -> Option<AuthOutcome>;
    async fn persisted_api_key_user(
        &self,
        headers: HeaderMap,
        database_file: PathBuf,
    ) -> Option<AuthOutcome>;
    async fn persisted_api_key_user_by_token(
        &self,
        api_key: String,
        database_file: PathBuf,
    ) -> Option<AuthOutcome>;
    async fn persisted_api_key_metadata(
        &self,
        headers: HeaderMap,
        database_file: PathBuf,
    ) -> Option<PersistedApiKeyMetadata>;
    async fn persisted_users(&self, database_file: PathBuf) -> Option<Vec<AuthUser>>;
    async fn persisted_update_password_by_user_id(
        &self,
        database_file: PathBuf,
        user_id: String,
        password: String,
    ) -> Option<bool>;
    async fn persisted_create_api_key(
        &self,
        database_file: PathBuf,
        user_id: String,
        comment: String,
    ) -> Option<PersistedApiKey>;
    async fn persisted_api_key_comment_exists(
        &self,
        database_file: PathBuf,
        user_id: String,
        comment: String,
    ) -> Option<bool>;
    async fn persisted_list_api_keys(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Option<Vec<PersistedApiKey>>;
    async fn persisted_delete_api_key_by_id(
        &self,
        database_file: PathBuf,
        user_id: String,
        api_key_id: String,
    ) -> Option<bool>;
    async fn persisted_list_authentication_activity(
        &self,
        database_file: PathBuf,
        user_id: Option<String>,
    ) -> Option<Vec<PersistedAuthenticationActivity>>;
    async fn persisted_cleanup_authentication_activity(
        &self,
        database_file: PathBuf,
    ) -> Option<u64>;
    async fn persisted_latest_authentication_activity_by_user_and_api_key(
        &self,
        database_file: PathBuf,
        user_id: String,
        api_key_id: String,
    ) -> Option<PersistedAuthenticationActivity>;
    async fn persisted_record_failed_authentication_activity(
        &self,
        database_file: PathBuf,
        email: Option<String>,
        input: AuthenticationActivityWriteInput,
        error: String,
    ) -> Option<()>;
    async fn persisted_record_successful_authentication_activity(
        &self,
        database_file: PathBuf,
        user: AuthUser,
        input: AuthenticationActivityWriteInput,
    ) -> Option<()>;
    async fn ensure_oauth_user(
        &self,
        database_file: PathBuf,
        email: String,
        allow_create: bool,
    ) -> Result<Option<AuthUser>, sqlx::Error>;
    fn configured_api_key(&self) -> Option<String>;
    async fn load_book_created_timestamp(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<String>, sqlx::Error>;
    async fn load_book_last_epub_position_locator(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<Value>, sqlx::Error>;
    async fn load_book_media_file(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<PersistedBookMediaFile>, sqlx::Error>;
    async fn load_kobo_metadata_record(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<KoboMetadataRecord>, sqlx::Error>;
    async fn load_kobo_sync_page(
        &self,
        database_file: PathBuf,
        user: AuthUser,
        user_id: String,
        current_api_key_id: Option<String>,
        ongoing_sync_point_id: Option<String>,
        last_successful_sync_point_id: Option<String>,
        limit: usize,
    ) -> Result<KoboSyncPage, sqlx::Error>;
    async fn load_koreader_book_target(
        &self,
        database_file: PathBuf,
        book_hash: String,
    ) -> Result<Option<KoreaderBookTarget>, KoreaderBookLookupError>;
    async fn load_read_progress(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: String,
    ) -> Result<Option<PersistedReadProgressRecord>, sqlx::Error>;
    async fn load_thumbnail_by_id(
        &self,
        database_file: PathBuf,
        thumbnail_id: String,
    ) -> Result<Option<(String, Vec<u8>)>, sqlx::Error>;
    async fn persist_read_progress_with_locator(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: String,
        page: i64,
        completed: bool,
        device_id: String,
        device_name: String,
        timestamp: String,
        locator: Option<Value>,
    ) -> Result<(), String>;
    async fn persisted_book_exists(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<bool, sqlx::Error>;
    async fn proxy_kobo_store_library_sync(
        &self,
        forwarded_headers: Vec<(String, String)>,
        query: Option<String>,
        raw_sync_token: String,
    ) -> Result<KoboStoreSyncMergeResult, ()>;
    async fn remove_sync_point(
        &self,
        database_file: PathBuf,
        sync_point_id: String,
    ) -> Result<(), sqlx::Error>;
    async fn create_auth_user(
        &self,
        database_file: PathBuf,
        input: CreateAuthUserInput,
    ) -> Result<Option<AuthUser>, sqlx::Error>;
    async fn delete_auth_user(
        &self,
        database_file: PathBuf,
        target_user_id: String,
    ) -> Result<bool, sqlx::Error>;
    async fn update_auth_user(
        &self,
        database_file: PathBuf,
        target_user_id: String,
        patch: UpdateAuthUserInput,
    ) -> Result<UpdateAuthUserResult, sqlx::Error>;
    async fn open_auth_pool(&self, database_file: PathBuf) -> Result<SqlitePool, sqlx::Error>;
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

#[cfg(test)]
pub(crate) fn default_test_identity_service() -> Box<dyn IdentityService> {
    Box::new(TestIdentityService)
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
    fn auth_token_user(&self, headers: HeaderMap) -> Option<AuthUser> {
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
    }

    fn session_token_for_user_with_runtime_key(
        &self,
        user: AuthUser,
        runtime_key: String,
    ) -> String {
        let token = format!("test-session-{runtime_key}-{}", user.id);
        test_state()
            .lock()
            .expect("runtime identity access test state lock should not be poisoned")
            .session_users
            .insert(token.clone(), user);
        token
    }

    fn remember_me_token_for_user_with_runtime_key(
        &self,
        user: AuthUser,
        runtime_key: String,
    ) -> Option<String> {
        let token = format!("test-remember-me-{runtime_key}-{}", user.id);
        test_state()
            .lock()
            .expect("runtime identity access test state lock should not be poisoned")
            .remember_me_users
            .insert(token.clone(), user);
        Some(token)
    }

    fn sync_session_runtime_settings(&self, _runtime_key: String, _max_inactive_seconds: u64) {}
    fn sync_remember_me_runtime_database_file(
        &self,
        _runtime_key: String,
        _database_file: PathBuf,
    ) {
    }
    fn sync_remember_me_runtime_settings(
        &self,
        _runtime_key: String,
        _key: String,
        _duration_days: u64,
    ) {
    }
    fn remember_me_max_age_seconds(&self, _runtime_key: String) -> u64 {
        365 * 24 * 60 * 60
    }
    fn invalidate_user_sessions(&self, _user_id: String) {}
    fn invalidate_user_sessions_with_runtime_key(&self, _user_id: String, _runtime_key: String) {}
    fn invalidate_session_token(&self, _token: String) {}
    fn invalidate_remember_me_token(&self, _token: String) {}

    fn store_oauth2_authorization_state(
        &self,
        runtime_key: String,
        session_token: String,
        registration_id: String,
        state: String,
    ) {
        test_state()
            .lock()
            .expect("runtime identity access test state lock should not be poisoned")
            .oauth2_authorization_states
            .insert((runtime_key, session_token, registration_id), state);
    }

    fn take_oauth2_authorization_state(
        &self,
        runtime_key: String,
        session_token: String,
        registration_id: String,
    ) -> Option<String> {
        test_state()
            .lock()
            .expect("runtime identity access test state lock should not be poisoned")
            .oauth2_authorization_states
            .remove(&(runtime_key, session_token, registration_id))
    }

    async fn persisted_basic_user(
        &self,
        _headers: HeaderMap,
        _database_file: PathBuf,
    ) -> Option<AuthOutcome> {
        Some(AuthOutcome::Missing)
    }
    async fn persisted_api_key_user(
        &self,
        _headers: HeaderMap,
        _database_file: PathBuf,
    ) -> Option<AuthOutcome> {
        Some(AuthOutcome::Missing)
    }
    async fn persisted_api_key_user_by_token(
        &self,
        _api_key: String,
        _database_file: PathBuf,
    ) -> Option<AuthOutcome> {
        None
    }
    async fn persisted_api_key_metadata(
        &self,
        _headers: HeaderMap,
        _database_file: PathBuf,
    ) -> Option<PersistedApiKeyMetadata> {
        None
    }
    async fn persisted_users(&self, _database_file: PathBuf) -> Option<Vec<AuthUser>> {
        Some(vec![])
    }
    async fn persisted_update_password_by_user_id(
        &self,
        _database_file: PathBuf,
        _user_id: String,
        _password: String,
    ) -> Option<bool> {
        Some(false)
    }
    async fn persisted_create_api_key(
        &self,
        _database_file: PathBuf,
        _user_id: String,
        _comment: String,
    ) -> Option<PersistedApiKey> {
        None
    }
    async fn persisted_api_key_comment_exists(
        &self,
        _database_file: PathBuf,
        _user_id: String,
        _comment: String,
    ) -> Option<bool> {
        Some(false)
    }
    async fn persisted_list_api_keys(
        &self,
        _database_file: PathBuf,
        _user_id: String,
    ) -> Option<Vec<PersistedApiKey>> {
        Some(vec![])
    }
    async fn persisted_delete_api_key_by_id(
        &self,
        _database_file: PathBuf,
        _user_id: String,
        _api_key_id: String,
    ) -> Option<bool> {
        Some(false)
    }
    async fn persisted_list_authentication_activity(
        &self,
        _database_file: PathBuf,
        _user_id: Option<String>,
    ) -> Option<Vec<PersistedAuthenticationActivity>> {
        Some(vec![])
    }
    async fn persisted_cleanup_authentication_activity(
        &self,
        _database_file: PathBuf,
    ) -> Option<u64> {
        Some(0)
    }
    async fn persisted_latest_authentication_activity_by_user_and_api_key(
        &self,
        _database_file: PathBuf,
        _user_id: String,
        _api_key_id: String,
    ) -> Option<PersistedAuthenticationActivity> {
        None
    }
    async fn persisted_record_failed_authentication_activity(
        &self,
        _database_file: PathBuf,
        _email: Option<String>,
        _input: AuthenticationActivityWriteInput,
        _error: String,
    ) -> Option<()> {
        Some(())
    }
    async fn persisted_record_successful_authentication_activity(
        &self,
        _database_file: PathBuf,
        _user: AuthUser,
        _input: AuthenticationActivityWriteInput,
    ) -> Option<()> {
        Some(())
    }
    async fn ensure_oauth_user(
        &self,
        _database_file: PathBuf,
        _email: String,
        _allow_create: bool,
    ) -> Result<Option<AuthUser>, sqlx::Error> {
        Ok(None)
    }
    fn configured_api_key(&self) -> Option<String> {
        None
    }
    async fn load_book_created_timestamp(
        &self,
        _database_file: PathBuf,
        _book_id: String,
    ) -> Result<Option<String>, sqlx::Error> {
        Ok(None)
    }
    async fn load_book_last_epub_position_locator(
        &self,
        _database_file: PathBuf,
        _book_id: String,
    ) -> Result<Option<Value>, sqlx::Error> {
        Ok(None)
    }
    async fn load_book_media_file(
        &self,
        _database_file: PathBuf,
        _book_id: String,
    ) -> Result<Option<PersistedBookMediaFile>, sqlx::Error> {
        Ok(None)
    }
    async fn load_kobo_metadata_record(
        &self,
        _database_file: PathBuf,
        _book_id: String,
    ) -> Result<Option<KoboMetadataRecord>, sqlx::Error> {
        Ok(None)
    }
    async fn load_kobo_sync_page(
        &self,
        _database_file: PathBuf,
        _user: AuthUser,
        _user_id: String,
        _current_api_key_id: Option<String>,
        _ongoing_sync_point_id: Option<String>,
        _last_successful_sync_point_id: Option<String>,
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
        database_file: PathBuf,
        book_hash: String,
    ) -> Result<Option<KoreaderBookTarget>, KoreaderBookLookupError> {
        test_state()
            .lock()
            .expect("runtime identity access test state lock should not be poisoned")
            .koreader_book_targets
            .get(&(database_file, book_hash))
            .cloned()
            .unwrap_or(Ok(None))
    }
    async fn load_read_progress(
        &self,
        _database_file: PathBuf,
        _book_id: String,
        _user_id: String,
    ) -> Result<Option<PersistedReadProgressRecord>, sqlx::Error> {
        Ok(None)
    }
    async fn load_thumbnail_by_id(
        &self,
        _database_file: PathBuf,
        _thumbnail_id: String,
    ) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
        Ok(None)
    }
    async fn persist_read_progress_with_locator(
        &self,
        _database_file: PathBuf,
        _book_id: String,
        _user_id: String,
        _page: i64,
        _completed: bool,
        _device_id: String,
        _device_name: String,
        _timestamp: String,
        _locator: Option<Value>,
    ) -> Result<(), String> {
        Ok(())
    }
    async fn persisted_book_exists(
        &self,
        _database_file: PathBuf,
        _book_id: String,
    ) -> Result<bool, sqlx::Error> {
        Ok(false)
    }
    async fn proxy_kobo_store_library_sync(
        &self,
        _forwarded_headers: Vec<(String, String)>,
        _query: Option<String>,
        _raw_sync_token: String,
    ) -> Result<KoboStoreSyncMergeResult, ()> {
        Ok(KoboStoreSyncMergeResult {
            events: vec![],
            raw_sync_token: None,
            should_continue: false,
        })
    }
    async fn remove_sync_point(
        &self,
        _database_file: PathBuf,
        _sync_point_id: String,
    ) -> Result<(), sqlx::Error> {
        Ok(())
    }
    async fn create_auth_user(
        &self,
        _database_file: PathBuf,
        _input: CreateAuthUserInput,
    ) -> Result<Option<AuthUser>, sqlx::Error> {
        Ok(None)
    }
    async fn delete_auth_user(
        &self,
        _database_file: PathBuf,
        _target_user_id: String,
    ) -> Result<bool, sqlx::Error> {
        Ok(false)
    }
    async fn update_auth_user(
        &self,
        _database_file: PathBuf,
        _target_user_id: String,
        _patch: UpdateAuthUserInput,
    ) -> Result<UpdateAuthUserResult, sqlx::Error> {
        Ok(UpdateAuthUserResult {
            updated: false,
            expire_sessions: false,
        })
    }
    async fn open_auth_pool(&self, _database_file: PathBuf) -> Result<SqlitePool, sqlx::Error> {
        Err(sqlx::Error::PoolClosed)
    }
}
