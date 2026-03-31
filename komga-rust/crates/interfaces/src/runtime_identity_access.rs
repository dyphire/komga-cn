#![allow(clippy::type_complexity)]

#[cfg(test)]
use std::collections::HashMap;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
#[cfg(test)]
use std::sync::Mutex;
use std::sync::{Arc, OnceLock};

use axum::http::HeaderMap;
use komga_application::identity_access::{
    AuthOutcome, AuthUser, KoboStoreSyncMergeResult, KoboSyncPointState, KoboSyncSnapshot,
    PersistedApiKey, PersistedApiKeyMetadata, PersistedAuthenticationActivity,
};
use serde_json::Value;
use sqlx::SqlitePool;

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

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

#[derive(Clone)]
pub struct KoreaderBookTarget {
    pub id: String,
    pub page_count: u64,
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
    pub contributor_names: Vec<String>,
    pub isbn: Option<String>,
    pub publisher_name: Option<String>,
    pub cover_image_id: Option<String>,
    pub series_id: Option<String>,
    pub series_name: Option<String>,
    pub series_number: Option<String>,
    pub series_number_float: Option<f64>,
    pub oneshot: bool,
}

#[derive(Clone, Debug)]
pub enum KoreaderBookLookupError {
    Persistence,
    Conflict,
}

#[derive(Clone)]
pub struct RuntimeIdentityAccessBackend {
    pub auth_token_user: Arc<dyn Fn(HeaderMap) -> Option<AuthUser> + Send + Sync>,
    pub session_token_for_user_with_namespace:
        Arc<dyn Fn(AuthUser, String) -> String + Send + Sync>,
    pub remember_me_token_for_user_with_namespace:
        Arc<dyn Fn(AuthUser, String) -> Option<String> + Send + Sync>,
    pub configure_remember_me_store: Arc<dyn Fn(PathBuf) -> String + Send + Sync>,
    pub invalidate_user_sessions: Arc<dyn Fn(String) + Send + Sync>,
    pub invalidate_session_token: Arc<dyn Fn(String) + Send + Sync>,
    pub invalidate_remember_me_token: Arc<dyn Fn(String) + Send + Sync>,
    pub persisted_basic_user:
        Arc<dyn Fn(HeaderMap, PathBuf) -> BoxFuture<Option<AuthOutcome>> + Send + Sync>,
    pub persisted_api_key_user:
        Arc<dyn Fn(HeaderMap, PathBuf) -> BoxFuture<Option<AuthOutcome>> + Send + Sync>,
    pub persisted_api_key_user_by_token:
        Arc<dyn Fn(String, PathBuf) -> BoxFuture<Option<AuthOutcome>> + Send + Sync>,
    pub persisted_api_key_metadata:
        Arc<dyn Fn(HeaderMap, PathBuf) -> BoxFuture<Option<PersistedApiKeyMetadata>> + Send + Sync>,
    pub persisted_users: Arc<dyn Fn(PathBuf) -> BoxFuture<Option<Vec<AuthUser>>> + Send + Sync>,
    pub persisted_update_password_by_user_id:
        Arc<dyn Fn(PathBuf, String, String) -> BoxFuture<Option<bool>> + Send + Sync>,
    pub persisted_create_api_key:
        Arc<dyn Fn(PathBuf, String, String) -> BoxFuture<Option<PersistedApiKey>> + Send + Sync>,
    pub persisted_api_key_comment_exists:
        Arc<dyn Fn(PathBuf, String, String) -> BoxFuture<Option<bool>> + Send + Sync>,
    pub persisted_list_api_keys:
        Arc<dyn Fn(PathBuf, String) -> BoxFuture<Option<Vec<PersistedApiKey>>> + Send + Sync>,
    pub persisted_delete_api_key_by_id:
        Arc<dyn Fn(PathBuf, String, String) -> BoxFuture<Option<bool>> + Send + Sync>,
    pub persisted_list_authentication_activity: Arc<
        dyn Fn(PathBuf, Option<String>) -> BoxFuture<Option<Vec<PersistedAuthenticationActivity>>>
            + Send
            + Sync,
    >,
    pub persisted_cleanup_authentication_activity:
        Arc<dyn Fn(PathBuf) -> BoxFuture<Option<u64>> + Send + Sync>,
    pub persisted_latest_authentication_activity_by_user_and_api_key: Arc<
        dyn Fn(PathBuf, String, String) -> BoxFuture<Option<PersistedAuthenticationActivity>>
            + Send
            + Sync,
    >,
    pub persisted_record_successful_authentication_activity: Arc<
        dyn Fn(PathBuf, AuthUser, String, Option<String>, Option<String>) -> BoxFuture<Option<()>>
            + Send
            + Sync,
    >,
    pub ensure_oauth_user: Arc<
        dyn Fn(PathBuf, String, bool) -> BoxFuture<Result<Option<AuthUser>, sqlx::Error>>
            + Send
            + Sync,
    >,
    pub configured_api_key: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    pub configured_api_key_comment: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    pub configured_api_key_id: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    pub load_book_created_timestamp: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<String>, sqlx::Error>> + Send + Sync,
    >,
    pub load_book_last_epub_position_locator:
        Arc<dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<Value>, sqlx::Error>> + Send + Sync>,
    pub load_book_media_file: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<PersistedBookMediaFile>, sqlx::Error>>
            + Send
            + Sync,
    >,
    pub load_book_page_count:
        Arc<dyn Fn(PathBuf, String) -> BoxFuture<Result<u64, sqlx::Error>> + Send + Sync>,
    pub load_kobo_metadata_record: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<KoboMetadataRecord>, sqlx::Error>>
            + Send
            + Sync,
    >,
    pub load_kobo_sync_snapshot: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<KoboSyncSnapshot, sqlx::Error>> + Send + Sync,
    >,
    pub load_koreader_book_target: Arc<
        dyn Fn(
                PathBuf,
                String,
            )
                -> BoxFuture<Result<Option<KoreaderBookTarget>, KoreaderBookLookupError>>
            + Send
            + Sync,
    >,
    pub load_read_progress: Arc<
        dyn Fn(
                PathBuf,
                String,
                String,
            ) -> BoxFuture<Result<Option<PersistedReadProgressRecord>, sqlx::Error>>
            + Send
            + Sync,
    >,
    pub load_sync_point_marker:
        Arc<dyn Fn(PathBuf, String, String) -> BoxFuture<Option<String>> + Send + Sync>,
    pub load_sync_point_state:
        Arc<dyn Fn(PathBuf, String, String) -> BoxFuture<Option<KoboSyncPointState>> + Send + Sync>,
    pub load_thumbnail_by_id: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<(String, Vec<u8>)>, sqlx::Error>>
            + Send
            + Sync,
    >,
    pub persist_read_progress_with_locator: Arc<
        dyn Fn(
                PathBuf,
                String,
                String,
                i64,
                bool,
                String,
                String,
                String,
                Option<Value>,
            ) -> BoxFuture<Result<(), String>>
            + Send
            + Sync,
    >,
    pub persisted_book_exists:
        Arc<dyn Fn(PathBuf, String) -> BoxFuture<Result<bool, sqlx::Error>> + Send + Sync>,
    pub proxy_kobo_store_library_sync: Arc<
        dyn Fn(
                Vec<(String, String)>,
                Option<String>,
                String,
            ) -> BoxFuture<Result<KoboStoreSyncMergeResult, ()>>
            + Send
            + Sync,
    >,
    pub remove_sync_point:
        Arc<dyn Fn(PathBuf, String) -> BoxFuture<Result<(), sqlx::Error>> + Send + Sync>,
    pub save_sync_point: Arc<
        dyn Fn(PathBuf, String, KoboSyncPointState) -> BoxFuture<Result<(), sqlx::Error>>
            + Send
            + Sync,
    >,
    pub create_auth_user: Arc<
        dyn Fn(PathBuf, CreateAuthUserInput) -> BoxFuture<Result<Option<AuthUser>, sqlx::Error>>
            + Send
            + Sync,
    >,
    pub delete_auth_user:
        Arc<dyn Fn(PathBuf, String) -> BoxFuture<Result<bool, sqlx::Error>> + Send + Sync>,
    pub update_auth_user: Arc<
        dyn Fn(PathBuf, String, UpdateAuthUserInput) -> BoxFuture<Result<bool, sqlx::Error>>
            + Send
            + Sync,
    >,
    pub open_auth_pool:
        Arc<dyn Fn(PathBuf) -> BoxFuture<Result<SqlitePool, sqlx::Error>> + Send + Sync>,
}

static BACKEND: OnceLock<RuntimeIdentityAccessBackend> = OnceLock::new();
#[cfg(test)]
static TEST_BACKEND: OnceLock<RuntimeIdentityAccessBackend> = OnceLock::new();

pub fn install_runtime_identity_access(backend: RuntimeIdentityAccessBackend) {
    let _ = BACKEND.set(backend);
}

fn backend() -> &'static RuntimeIdentityAccessBackend {
    if let Some(backend) = BACKEND.get() {
        return backend;
    }

    #[cfg(test)]
    {
        TEST_BACKEND.get_or_init(default_test_backend)
    }

    #[cfg(not(test))]
    {
        panic!("runtime identity access backend should be installed before use");
    }
}

#[cfg(test)]
#[derive(Default)]
struct RuntimeIdentityAccessTestState {
    session_users: HashMap<String, AuthUser>,
    remember_me_users: HashMap<String, AuthUser>,
    koreader_book_targets:
        HashMap<(PathBuf, String), Result<Option<KoreaderBookTarget>, KoreaderBookLookupError>>,
}

#[cfg(test)]
fn test_state() -> &'static Mutex<RuntimeIdentityAccessTestState> {
    static TEST_STATE: OnceLock<Mutex<RuntimeIdentityAccessTestState>> = OnceLock::new();
    TEST_STATE.get_or_init(|| Mutex::new(RuntimeIdentityAccessTestState::default()))
}

#[cfg(test)]
fn default_test_backend() -> RuntimeIdentityAccessBackend {
    RuntimeIdentityAccessBackend {
        auth_token_user: Arc::new(|headers| {
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
        }),
        session_token_for_user_with_namespace: Arc::new(|user, namespace| {
            let token = format!("test-session-{namespace}-{}", user.id);
            test_state()
                .lock()
                .expect("runtime identity access test state lock should not be poisoned")
                .session_users
                .insert(token.clone(), user);
            token
        }),
        remember_me_token_for_user_with_namespace: Arc::new(|user, namespace| {
            let token = format!("test-remember-me-{namespace}-{}", user.id);
            test_state()
                .lock()
                .expect("runtime identity access test state lock should not be poisoned")
                .remember_me_users
                .insert(token.clone(), user);
            Some(token)
        }),
        configure_remember_me_store: Arc::new(|store_root| {
            format!("test-remember-me:{}", store_root.display())
        }),
        invalidate_user_sessions: Arc::new(|_| {}),
        invalidate_session_token: Arc::new(|_| {}),
        invalidate_remember_me_token: Arc::new(|_| {}),
        persisted_basic_user: Arc::new(|_, _| Box::pin(async { Some(AuthOutcome::Missing) })),
        persisted_api_key_user: Arc::new(|_, _| Box::pin(async { Some(AuthOutcome::Missing) })),
        persisted_api_key_user_by_token: Arc::new(|_, _| Box::pin(async { None })),
        persisted_api_key_metadata: Arc::new(|_, _| Box::pin(async { None })),
        persisted_users: Arc::new(|_| Box::pin(async { Some(vec![]) })),
        persisted_update_password_by_user_id: Arc::new(|_, _, _| Box::pin(async { Some(false) })),
        persisted_create_api_key: Arc::new(|_, _, _| Box::pin(async { None })),
        persisted_api_key_comment_exists: Arc::new(|_, _, _| Box::pin(async { Some(false) })),
        persisted_list_api_keys: Arc::new(|_, _| Box::pin(async { Some(vec![]) })),
        persisted_delete_api_key_by_id: Arc::new(|_, _, _| Box::pin(async { Some(false) })),
        persisted_list_authentication_activity: Arc::new(|_, _| Box::pin(async { Some(vec![]) })),
        persisted_cleanup_authentication_activity: Arc::new(|_| Box::pin(async { Some(0) })),
        persisted_latest_authentication_activity_by_user_and_api_key: Arc::new(|_, _, _| {
            Box::pin(async { None })
        }),
        persisted_record_successful_authentication_activity: Arc::new(|_, _, _, _, _| {
            Box::pin(async { Some(()) })
        }),
        ensure_oauth_user: Arc::new(|_, _, _| Box::pin(async { Ok(None) })),
        configured_api_key: Arc::new(|| None),
        configured_api_key_comment: Arc::new(|| None),
        configured_api_key_id: Arc::new(|| None),
        load_book_created_timestamp: Arc::new(|_, _| Box::pin(async { Ok(None) })),
        load_book_last_epub_position_locator: Arc::new(|_, _| Box::pin(async { Ok(None) })),
        load_book_media_file: Arc::new(|_, _| Box::pin(async { Ok(None) })),
        load_book_page_count: Arc::new(|_, _| Box::pin(async { Ok(0) })),
        load_kobo_metadata_record: Arc::new(|_, _| Box::pin(async { Ok(None) })),
        load_kobo_sync_snapshot: Arc::new(|_, _| {
            Box::pin(async {
                Ok(KoboSyncSnapshot {
                    books: HashMap::new(),
                    progress: HashMap::new(),
                    readlists: HashMap::new(),
                })
            })
        }),
        load_koreader_book_target: Arc::new(|database_file, book_hash| {
            Box::pin(async move {
                test_state()
                    .lock()
                    .expect("runtime identity access test state lock should not be poisoned")
                    .koreader_book_targets
                    .get(&(database_file, book_hash))
                    .cloned()
                    .unwrap_or(Ok(None))
            })
        }),
        load_read_progress: Arc::new(|_, _, _| Box::pin(async { Ok(None) })),
        load_sync_point_marker: Arc::new(|_, _, _| Box::pin(async { None })),
        load_sync_point_state: Arc::new(|_, _, _| Box::pin(async { None })),
        load_thumbnail_by_id: Arc::new(|_, _| Box::pin(async { Ok(None) })),
        persist_read_progress_with_locator: Arc::new(|_, _, _, _, _, _, _, _, _| {
            Box::pin(async { Ok(()) })
        }),
        persisted_book_exists: Arc::new(|_, _| Box::pin(async { Ok(false) })),
        proxy_kobo_store_library_sync: Arc::new(|_, _, _| {
            Box::pin(async {
                Ok(KoboStoreSyncMergeResult {
                    events: vec![],
                    raw_sync_token: None,
                    should_continue: false,
                })
            })
        }),
        remove_sync_point: Arc::new(|_, _| Box::pin(async { Ok(()) })),
        save_sync_point: Arc::new(|_, _, _| Box::pin(async { Ok(()) })),
        create_auth_user: Arc::new(|_, _| Box::pin(async { Ok(None) })),
        delete_auth_user: Arc::new(|_, _| Box::pin(async { Ok(false) })),
        update_auth_user: Arc::new(|_, _, _| Box::pin(async { Ok(false) })),
        open_auth_pool: Arc::new(|_| Box::pin(async { Err(sqlx::Error::PoolClosed) })),
    }
}

pub fn auth_token_user(headers: &HeaderMap) -> Option<AuthUser> {
    (backend().auth_token_user)(headers.clone())
}

pub fn session_token_for_user_with_namespace(user: &AuthUser, namespace: &str) -> String {
    (backend().session_token_for_user_with_namespace)(user.clone(), namespace.to_string())
}

pub fn remember_me_token_for_user_with_namespace(
    user: &AuthUser,
    namespace: &str,
) -> Option<String> {
    (backend().remember_me_token_for_user_with_namespace)(user.clone(), namespace.to_string())
}

pub fn configure_remember_me_store(store_root: &Path) -> String {
    (backend().configure_remember_me_store)(store_root.to_path_buf())
}

pub fn invalidate_user_sessions(user_id: &str) {
    (backend().invalidate_user_sessions)(user_id.to_string())
}

pub fn invalidate_session_token(token: &str) {
    (backend().invalidate_session_token)(token.to_string())
}

pub fn invalidate_remember_me_token(token: &str) {
    (backend().invalidate_remember_me_token)(token.to_string())
}

pub async fn persisted_basic_user(
    headers: &HeaderMap,
    database_file: &Path,
) -> Option<AuthOutcome> {
    (backend().persisted_basic_user)(headers.clone(), database_file.to_path_buf()).await
}

pub async fn persisted_api_key_user(
    headers: &HeaderMap,
    database_file: &Path,
) -> Option<AuthOutcome> {
    (backend().persisted_api_key_user)(headers.clone(), database_file.to_path_buf()).await
}

pub async fn persisted_api_key_user_by_token(
    api_key: &str,
    database_file: &Path,
) -> Option<AuthOutcome> {
    (backend().persisted_api_key_user_by_token)(api_key.to_string(), database_file.to_path_buf())
        .await
}

pub async fn persisted_api_key_metadata(
    headers: &HeaderMap,
    database_file: &Path,
) -> Option<PersistedApiKeyMetadata> {
    (backend().persisted_api_key_metadata)(headers.clone(), database_file.to_path_buf()).await
}

pub async fn persisted_users(database_file: &Path) -> Option<Vec<AuthUser>> {
    (backend().persisted_users)(database_file.to_path_buf()).await
}

pub async fn persisted_update_password_by_user_id(
    database_file: &Path,
    user_id: &str,
    password: &str,
) -> Option<bool> {
    (backend().persisted_update_password_by_user_id)(
        database_file.to_path_buf(),
        user_id.to_string(),
        password.to_string(),
    )
    .await
}

pub async fn persisted_create_api_key(
    database_file: &Path,
    user_id: &str,
    comment: &str,
) -> Option<PersistedApiKey> {
    (backend().persisted_create_api_key)(
        database_file.to_path_buf(),
        user_id.to_string(),
        comment.to_string(),
    )
    .await
}

pub async fn persisted_api_key_comment_exists(
    database_file: &Path,
    user_id: &str,
    comment: &str,
) -> Option<bool> {
    (backend().persisted_api_key_comment_exists)(
        database_file.to_path_buf(),
        user_id.to_string(),
        comment.to_string(),
    )
    .await
}

pub async fn persisted_list_api_keys(
    database_file: &Path,
    user_id: &str,
) -> Option<Vec<PersistedApiKey>> {
    (backend().persisted_list_api_keys)(database_file.to_path_buf(), user_id.to_string()).await
}

pub async fn persisted_delete_api_key_by_id(
    database_file: &Path,
    user_id: &str,
    api_key_id: &str,
) -> Option<bool> {
    (backend().persisted_delete_api_key_by_id)(
        database_file.to_path_buf(),
        user_id.to_string(),
        api_key_id.to_string(),
    )
    .await
}

pub async fn persisted_list_authentication_activity(
    database_file: &Path,
    user_id: Option<&str>,
) -> Option<Vec<PersistedAuthenticationActivity>> {
    (backend().persisted_list_authentication_activity)(
        database_file.to_path_buf(),
        user_id.map(ToString::to_string),
    )
    .await
}

pub async fn persisted_cleanup_authentication_activity(database_file: &Path) -> Option<u64> {
    (backend().persisted_cleanup_authentication_activity)(database_file.to_path_buf()).await
}

pub async fn persisted_latest_authentication_activity_by_user_and_api_key(
    database_file: &Path,
    user_id: &str,
    api_key_id: &str,
) -> Option<PersistedAuthenticationActivity> {
    (backend().persisted_latest_authentication_activity_by_user_and_api_key)(
        database_file.to_path_buf(),
        user_id.to_string(),
        api_key_id.to_string(),
    )
    .await
}

pub async fn persisted_record_successful_authentication_activity(
    database_file: &Path,
    user: &AuthUser,
    source: &str,
    api_key_id: Option<&str>,
    api_key_comment: Option<&str>,
) -> Option<()> {
    (backend().persisted_record_successful_authentication_activity)(
        database_file.to_path_buf(),
        user.clone(),
        source.to_string(),
        api_key_id.map(ToString::to_string),
        api_key_comment.map(ToString::to_string),
    )
    .await
}

pub async fn ensure_oauth_user(
    database_file: &Path,
    email: &str,
    allow_create: bool,
) -> Result<Option<AuthUser>, sqlx::Error> {
    (backend().ensure_oauth_user)(database_file.to_path_buf(), email.to_string(), allow_create)
        .await
}

pub fn configured_api_key() -> Option<String> {
    (backend().configured_api_key)()
}

pub fn configured_api_key_comment() -> Option<String> {
    (backend().configured_api_key_comment)()
}

pub fn configured_api_key_id() -> Option<String> {
    (backend().configured_api_key_id)()
}

pub async fn load_book_created_timestamp(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<String>, sqlx::Error> {
    (backend().load_book_created_timestamp)(database_file.to_path_buf(), book_id.to_string()).await
}

pub async fn load_book_last_epub_position_locator(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<Value>, sqlx::Error> {
    (backend().load_book_last_epub_position_locator)(
        database_file.to_path_buf(),
        book_id.to_string(),
    )
    .await
}

pub async fn load_book_media_file(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<PersistedBookMediaFile>, sqlx::Error> {
    (backend().load_book_media_file)(database_file.to_path_buf(), book_id.to_string()).await
}

pub async fn load_book_page_count(database_file: &Path, book_id: &str) -> Result<u64, sqlx::Error> {
    (backend().load_book_page_count)(database_file.to_path_buf(), book_id.to_string()).await
}

pub async fn load_kobo_metadata_record(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<KoboMetadataRecord>, sqlx::Error> {
    (backend().load_kobo_metadata_record)(database_file.to_path_buf(), book_id.to_string()).await
}

pub async fn load_kobo_sync_snapshot(
    database_file: &Path,
    user_id: &str,
) -> Result<KoboSyncSnapshot, sqlx::Error> {
    (backend().load_kobo_sync_snapshot)(database_file.to_path_buf(), user_id.to_string()).await
}

pub async fn load_koreader_book_target(
    database_file: &Path,
    book_hash: &str,
) -> Result<Option<KoreaderBookTarget>, KoreaderBookLookupError> {
    (backend().load_koreader_book_target)(database_file.to_path_buf(), book_hash.to_string()).await
}

pub async fn load_read_progress(
    database_file: &Path,
    book_id: &str,
    user_id: &str,
) -> Result<Option<PersistedReadProgressRecord>, sqlx::Error> {
    (backend().load_read_progress)(
        database_file.to_path_buf(),
        book_id.to_string(),
        user_id.to_string(),
    )
    .await
}

pub async fn load_sync_point_marker(
    database_file: &Path,
    sync_point_id: &str,
    user_id: &str,
) -> Option<String> {
    (backend().load_sync_point_marker)(
        database_file.to_path_buf(),
        sync_point_id.to_string(),
        user_id.to_string(),
    )
    .await
}

pub async fn load_sync_point_state(
    database_file: &Path,
    sync_point_id: &str,
    user_id: &str,
) -> Option<KoboSyncPointState> {
    (backend().load_sync_point_state)(
        database_file.to_path_buf(),
        sync_point_id.to_string(),
        user_id.to_string(),
    )
    .await
}

pub async fn load_thumbnail_by_id(
    database_file: &Path,
    thumbnail_id: &str,
) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
    (backend().load_thumbnail_by_id)(database_file.to_path_buf(), thumbnail_id.to_string()).await
}

#[allow(clippy::too_many_arguments)]
pub async fn persist_read_progress_with_locator(
    database_file: &Path,
    book_id: &str,
    user_id: &str,
    page: i64,
    completed: bool,
    device_id: &str,
    device_name: &str,
    timestamp: &str,
    locator: Option<Value>,
) -> Result<(), String> {
    (backend().persist_read_progress_with_locator)(
        database_file.to_path_buf(),
        book_id.to_string(),
        user_id.to_string(),
        page,
        completed,
        device_id.to_string(),
        device_name.to_string(),
        timestamp.to_string(),
        locator,
    )
    .await
}

pub async fn persisted_book_exists(
    database_file: &Path,
    book_id: &str,
) -> Result<bool, sqlx::Error> {
    (backend().persisted_book_exists)(database_file.to_path_buf(), book_id.to_string()).await
}

pub async fn proxy_kobo_store_library_sync(
    forwarded_headers: &[(String, String)],
    query: Option<&str>,
    raw_sync_token: &str,
) -> Result<KoboStoreSyncMergeResult, ()> {
    (backend().proxy_kobo_store_library_sync)(
        forwarded_headers.to_vec(),
        query.map(ToString::to_string),
        raw_sync_token.to_string(),
    )
    .await
}

pub async fn remove_sync_point(
    database_file: &Path,
    sync_point_id: &str,
) -> Result<(), sqlx::Error> {
    (backend().remove_sync_point)(database_file.to_path_buf(), sync_point_id.to_string()).await
}

pub async fn save_sync_point(
    database_file: &Path,
    sync_point_id: &str,
    sync_point_state: &KoboSyncPointState,
) -> Result<(), sqlx::Error> {
    (backend().save_sync_point)(
        database_file.to_path_buf(),
        sync_point_id.to_string(),
        sync_point_state.clone(),
    )
    .await
}

pub async fn create_auth_user(
    database_file: &Path,
    input: CreateAuthUserInput,
) -> Result<Option<AuthUser>, sqlx::Error> {
    (backend().create_auth_user)(database_file.to_path_buf(), input).await
}

pub async fn delete_auth_user(
    database_file: &Path,
    target_user_id: &str,
) -> Result<bool, sqlx::Error> {
    (backend().delete_auth_user)(database_file.to_path_buf(), target_user_id.to_string()).await
}

pub async fn update_auth_user(
    database_file: &Path,
    target_user_id: &str,
    patch: UpdateAuthUserInput,
) -> Result<bool, sqlx::Error> {
    (backend().update_auth_user)(
        database_file.to_path_buf(),
        target_user_id.to_string(),
        patch,
    )
    .await
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;

    pub(crate) fn seed_koreader_book_target(
        database_file: &Path,
        book_hash: &str,
        result: Result<Option<KoreaderBookTarget>, KoreaderBookLookupError>,
    ) {
        test_state()
            .lock()
            .expect("runtime identity access test state lock should not be poisoned")
            .koreader_book_targets
            .insert((database_file.to_path_buf(), book_hash.to_string()), result);
    }
}
