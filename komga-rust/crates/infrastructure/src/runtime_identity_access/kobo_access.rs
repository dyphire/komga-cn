#![allow(clippy::too_many_arguments)]

use std::path::Path;

use komga_application::identity_access::{AuthUser, KoboStoreSyncMergeResult, KoboSyncPage};
use serde_json::Value;

use super::backend_contract::{
    KoboMetadataRecord, KoreaderBookLookupError, KoreaderBookTarget, PersistedBookMediaFile,
    PersistedReadProgressRecord,
};
use super::backend_state::backend;

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

pub async fn load_kobo_metadata_record(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<KoboMetadataRecord>, sqlx::Error> {
    (backend().load_kobo_metadata_record)(database_file.to_path_buf(), book_id.to_string()).await
}

pub async fn load_kobo_sync_page(
    database_file: &Path,
    user: &AuthUser,
    user_id: &str,
    current_api_key_id: Option<&str>,
    ongoing_sync_point_id: Option<&str>,
    last_successful_sync_point_id: Option<&str>,
    limit: usize,
) -> Result<KoboSyncPage, sqlx::Error> {
    (backend().load_kobo_sync_page)(
        database_file.to_path_buf(),
        user.clone(),
        user_id.to_string(),
        current_api_key_id.map(str::to_string),
        ongoing_sync_point_id.map(str::to_string),
        last_successful_sync_point_id.map(str::to_string),
        limit,
    )
    .await
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

pub async fn load_thumbnail_by_id(
    database_file: &Path,
    thumbnail_id: &str,
) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
    (backend().load_thumbnail_by_id)(database_file.to_path_buf(), thumbnail_id.to_string()).await
}

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
        query.map(str::to_string),
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
