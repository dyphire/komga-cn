use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

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
#[allow(clippy::type_complexity)]
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
    pub open_auth_pool:
        Arc<dyn Fn(PathBuf) -> BoxFuture<Result<SqlitePool, sqlx::Error>> + Send + Sync>,
}
