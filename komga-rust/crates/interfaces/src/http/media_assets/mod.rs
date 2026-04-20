use std::collections::BTreeSet;
use std::path::{Path as FsPath, PathBuf};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, Query};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use image::ImageFormat;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::http::cache::{
    asset_etag, asset_not_modified_response, asset_ok_response, file_last_modified_header_value,
    if_modified_since_matches, if_none_match_matches,
};
use crate::http::discovery::detail::{
    resolve_book_id_for_persisted, resolve_series_id_for_persisted,
};
use crate::http::discovery_auth::principal::principal_from_user_payload;
use crate::http::identity_access::auth::{
    AuthUser, require_admin, require_request_admin, require_request_auth,
    require_request_file_download, resolved_auth_user, resolved_request_auth_user, resolved_token,
    user_has_role, user_id, user_is_admin, user_payload_json, user_shared_all_libraries,
    user_shared_library_ids,
};
use crate::http::request_urls::app_absolute_url;
use crate::http::state::HttpAppState;
use crate::media_assets_runtime_access::PersistedMediaFileRecord;
#[cfg(test)]
use crate::media_assets_runtime_access::facade::{
    load_archive_page_rows, load_generated_pdf_page_rows, read_pdf_page_as_single_page_pdf,
    resolve_book_page_bytes,
};
use komga_application::task_processing::TaskQueueRecord;

use super::helpers::{
    invalid_progression_payload, invalid_read_progress_payload, mark_runtime_owned,
    set_read_progress,
};

pub(crate) mod access_control;
mod archive_payload;
mod epub_positions;
mod files;
pub(crate) mod handlers;
pub(crate) mod http_helpers;
mod import;
mod import_internals;
pub(crate) mod manifest_persistence;
mod manifests;
mod media_helpers;
mod operations;
mod pages;
pub(crate) mod read_progress;
mod thumbnails;
pub(crate) mod types;

use self::access_control::{
    user_can_access_book_media, user_can_access_collection_media, user_can_access_library,
    user_can_access_readlist_media, user_can_access_series_media, visible_readlist_books_for_user,
};
use self::archive_payload::{build_stored_zip_archive, readlist_archive_entry_name};
use self::epub_positions::load_persisted_epub_positions;
use self::http_helpers::{
    attachment_disposition, format_size_bytes, inline_disposition, internal_error_response,
};
use self::import_internals::parse_books_import_payload;
use self::manifest_persistence::build_persisted_book_manifest;
use self::media_helpers::{
    book_media_is_epub, book_media_is_pdf, book_media_is_single_image,
    book_media_supports_page_api, content_type_from_filename,
};
#[cfg(test)]
use self::media_helpers::{
    normalize_epub_resource_href, parse_epub_fixed_layout, parse_epub_kobo_spans,
};
use self::types::{
    BooksImportEntry, BooksImportPayload, ImportCopyMode, ManifestBuildOutcome, ManifestProfile,
    ManifestVariant, PersistedBookMedia, PersistedBookPageRow,
};

async fn load_persisted_book_media_from_services(
    app: &HttpAppState,
    book_id: &str,
) -> Result<Option<PersistedBookMedia>, String> {
    app.services
        .media_assets
        .load_persisted_book_media(app.auth_db.database_file.clone(), book_id.to_string())
        .await
}

async fn load_persisted_book_pages_from_services(
    app: &HttpAppState,
    book_id: &str,
) -> Result<Vec<komga_application::media_assets::BookPageRecord>, String> {
    app.services
        .media_assets
        .load_persisted_book_pages(app.auth_db.database_file.clone(), book_id.to_string())
        .await
}

async fn load_persisted_book_media_files_from_services(
    app: &HttpAppState,
    book_id: &str,
) -> Result<Vec<String>, String> {
    app.services
        .media_assets
        .load_persisted_book_media_files(app.auth_db.database_file.clone(), book_id.to_string())
        .await
}

async fn load_persisted_media_file_records_from_services(
    app: &HttpAppState,
    book_id: &str,
) -> Result<Vec<PersistedMediaFileRecord>, String> {
    app.services
        .media_assets
        .load_persisted_media_file_records(app.auth_db.database_file.clone(), book_id.to_string())
        .await
}

async fn load_persisted_manifest_book_from_services(
    app: &HttpAppState,
    book_id: &str,
) -> Result<Option<(String, String, String)>, String> {
    app.services
        .media_assets
        .load_persisted_manifest_book(app.auth_db.database_file.clone(), book_id.to_string())
        .await
}

async fn load_book_progression_from_services(
    app: &HttpAppState,
    book_id: &str,
    user_id: &str,
) -> Result<Option<Value>, String> {
    app.services
        .media_assets
        .load_book_progression(
            app.auth_db.database_file.clone(),
            book_id.to_string(),
            user_id.to_string(),
        )
        .await
}

async fn load_book_restrictions_from_services(
    app: &HttpAppState,
    book_id: &str,
) -> Result<Option<(Option<u16>, Vec<String>)>, String> {
    app.services
        .media_assets
        .load_book_restrictions(app.auth_db.database_file.clone(), book_id.to_string())
        .await
}

async fn load_book_page_count_from_services(
    app: &HttpAppState,
    book_id: &str,
) -> Result<Option<u64>, String> {
    app.services
        .media_assets
        .load_book_page_count(app.auth_db.database_file.clone(), book_id.to_string())
        .await
}

async fn persisted_readlist_exists_from_services(
    app: &HttpAppState,
    readlist_id: &str,
) -> Result<bool, String> {
    app.services
        .media_assets
        .persisted_readlist_exists(app.auth_db.database_file.clone(), readlist_id.to_string())
        .await
}

async fn load_persisted_book_page_row_from_services(
    app: &HttpAppState,
    book_id: &str,
    page_number: u64,
) -> Result<Option<komga_application::media_assets::BookPageRecord>, String> {
    app.services
        .media_assets
        .load_persisted_book_page_row(
            app.auth_db.database_file.clone(),
            book_id.to_string(),
            page_number,
        )
        .await
}

async fn load_persisted_readlist_thumbnails_from_services(
    app: &HttpAppState,
    readlist_id: &str,
) -> Result<Vec<komga_application::media_assets::ReadlistThumbnailRecord>, String> {
    app.services
        .media_assets
        .load_persisted_readlist_thumbnails(
            app.auth_db.database_file.clone(),
            readlist_id.to_string(),
        )
        .await
}

async fn insert_readlist_thumbnail_from_services(
    app: &HttpAppState,
    readlist_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<komga_application::media_assets::ReadlistThumbnailRecord, String> {
    app.services
        .media_assets
        .insert_readlist_thumbnail(
            app.auth_db.database_file.clone(),
            readlist_id.to_string(),
            thumbnail.to_vec(),
            media_type.to_string(),
            width,
            height,
            selected,
        )
        .await
}

async fn select_readlist_thumbnail_from_services(
    app: &HttpAppState,
    readlist_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    app.services
        .media_assets
        .select_readlist_thumbnail(
            app.auth_db.database_file.clone(),
            readlist_id.to_string(),
            thumbnail_id.to_string(),
        )
        .await
}

async fn delete_readlist_thumbnail_from_services(
    app: &HttpAppState,
    readlist_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    app.services
        .media_assets
        .delete_readlist_thumbnail(
            app.auth_db.database_file.clone(),
            readlist_id.to_string(),
            thumbnail_id.to_string(),
        )
        .await
}

async fn book_media_is_ready_status_from_services(
    app: &HttpAppState,
    book_id: &str,
) -> Result<bool, String> {
    app.services
        .media_assets
        .book_media_is_ready_status(app.auth_db.database_file.clone(), book_id.to_string())
        .await
}

async fn load_series_book_ids_from_media_services(
    app: &HttpAppState,
    series_id: &str,
) -> Result<Vec<String>, String> {
    app.services
        .media_assets
        .load_series_book_ids(app.auth_db.database_file.clone(), series_id.to_string())
        .await
}

async fn load_selected_book_thumbnail_from_services(
    app: &HttpAppState,
    book_id: &str,
) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
    app.services
        .media_assets
        .load_selected_book_thumbnail(app.auth_db.database_file.clone(), book_id.to_string())
        .await
}

async fn persisted_book_exists_from_services(
    app: &HttpAppState,
    book_id: &str,
) -> Result<bool, String> {
    app.services
        .media_assets
        .persisted_book_exists(app.auth_db.database_file.clone(), book_id.to_string())
        .await
}

async fn persisted_series_exists_from_services(
    app: &HttpAppState,
    series_id: &str,
) -> Result<bool, String> {
    app.services
        .media_assets
        .persisted_series_exists(app.auth_db.database_file.clone(), series_id.to_string())
        .await
}

async fn persisted_collection_exists_from_services(
    app: &HttpAppState,
    collection_id: &str,
) -> Result<bool, String> {
    app.services
        .media_assets
        .persisted_collection_exists(app.auth_db.database_file.clone(), collection_id.to_string())
        .await
}

async fn load_book_thumbnail_by_id_from_services(
    app: &HttpAppState,
    thumbnail_id: &str,
) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
    app.services
        .media_assets
        .load_book_thumbnail_by_id(app.auth_db.database_file.clone(), thumbnail_id.to_string())
        .await
}

async fn load_persisted_book_thumbnails_from_services(
    app: &HttpAppState,
    book_id: &str,
) -> Result<Vec<komga_application::media_assets::EntityThumbnailRecord>, String> {
    app.services
        .media_assets
        .load_persisted_book_thumbnails(app.auth_db.database_file.clone(), book_id.to_string())
        .await
}

async fn insert_book_thumbnail_from_services(
    app: &HttpAppState,
    book_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<komga_application::media_assets::EntityThumbnailRecord, String> {
    app.services
        .media_assets
        .insert_book_thumbnail(
            app.auth_db.database_file.clone(),
            book_id.to_string(),
            thumbnail.to_vec(),
            media_type.to_string(),
            width,
            height,
            selected,
        )
        .await
}

async fn select_book_thumbnail_from_services(
    app: &HttpAppState,
    thumbnail_id: &str,
) -> Result<bool, String> {
    app.services
        .media_assets
        .select_book_thumbnail(app.auth_db.database_file.clone(), thumbnail_id.to_string())
        .await
}

async fn delete_book_thumbnail_from_services(
    app: &HttpAppState,
    book_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    app.services
        .media_assets
        .delete_book_thumbnail(
            app.auth_db.database_file.clone(),
            book_id.to_string(),
            thumbnail_id.to_string(),
        )
        .await
}

fn read_media_file_bytes_from_services(app: &HttpAppState, path: &FsPath) -> Option<Vec<u8>> {
    app.services
        .media_assets
        .read_media_file_bytes(path.to_path_buf())
}

fn read_media_file_size_from_services(app: &HttpAppState, path: &FsPath) -> Option<i64> {
    app.services
        .media_assets
        .read_media_file_size(path.to_path_buf())
}

fn is_font_resource_from_services(app: &HttpAppState, resource_name: &str) -> bool {
    app.services
        .media_assets
        .is_font_resource(resource_name.to_string())
}

fn read_epub_resource_bytes_from_services(
    app: &HttpAppState,
    path: &FsPath,
    resource_name: &str,
) -> Option<Vec<u8>> {
    app.services
        .media_assets
        .read_epub_resource_bytes(path.to_path_buf(), resource_name.to_string())
}

async fn load_persisted_readlist_name_from_services(
    app: &HttpAppState,
    readlist_id: &str,
) -> Result<Option<String>, String> {
    app.services
        .media_assets
        .load_persisted_readlist_name(app.auth_db.database_file.clone(), readlist_id.to_string())
        .await
}

async fn load_series_archive_entries_from_services(
    app: &HttpAppState,
    series_id: &str,
) -> Result<Option<(String, String, Vec<(String, PathBuf)>)>, String> {
    app.services
        .media_assets
        .load_series_archive_entries(app.auth_db.database_file.clone(), series_id.to_string())
        .await
}

async fn load_persisted_collection_thumbnails_from_services(
    app: &HttpAppState,
    collection_id: &str,
) -> Result<Vec<komga_application::media_assets::CollectionThumbnailRecord>, String> {
    app.services
        .media_assets
        .load_persisted_collection_thumbnails(
            app.auth_db.database_file.clone(),
            collection_id.to_string(),
        )
        .await
}

async fn insert_collection_thumbnail_from_services(
    app: &HttpAppState,
    collection_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<komga_application::media_assets::CollectionThumbnailRecord, String> {
    app.services
        .media_assets
        .insert_collection_thumbnail(
            app.auth_db.database_file.clone(),
            collection_id.to_string(),
            thumbnail.to_vec(),
            media_type.to_string(),
            width,
            height,
            selected,
        )
        .await
}

async fn select_collection_thumbnail_from_services(
    app: &HttpAppState,
    thumbnail_id: &str,
) -> Result<bool, String> {
    app.services
        .media_assets
        .select_collection_thumbnail(app.auth_db.database_file.clone(), thumbnail_id.to_string())
        .await
}

async fn delete_collection_thumbnail_from_services(
    app: &HttpAppState,
    collection_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    app.services
        .media_assets
        .delete_collection_thumbnail(
            app.auth_db.database_file.clone(),
            collection_id.to_string(),
            thumbnail_id.to_string(),
        )
        .await
}

async fn load_persisted_series_thumbnails_from_services(
    app: &HttpAppState,
    series_id: &str,
) -> Result<Vec<komga_application::media_assets::SeriesThumbnailRecord>, String> {
    app.services
        .media_assets
        .load_persisted_series_thumbnails(app.auth_db.database_file.clone(), series_id.to_string())
        .await
}

async fn load_series_thumbnail_by_id_from_services(
    app: &HttpAppState,
    thumbnail_id: &str,
) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
    app.services
        .media_assets
        .load_series_thumbnail_by_id(app.auth_db.database_file.clone(), thumbnail_id.to_string())
        .await
}

async fn load_persisted_series_oneshot_from_services(
    app: &HttpAppState,
    series_id: &str,
) -> Result<Option<bool>, String> {
    app.services
        .media_assets
        .load_persisted_series_oneshot(app.auth_db.database_file.clone(), series_id.to_string())
        .await
}

async fn insert_series_thumbnail_from_services(
    app: &HttpAppState,
    series_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<komga_application::media_assets::SeriesThumbnailRecord, String> {
    app.services
        .media_assets
        .insert_series_thumbnail(
            app.auth_db.database_file.clone(),
            series_id.to_string(),
            thumbnail.to_vec(),
            media_type.to_string(),
            width,
            height,
            selected,
        )
        .await
}

async fn select_series_thumbnail_from_services(
    app: &HttpAppState,
    series_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    app.services
        .media_assets
        .select_series_thumbnail(
            app.auth_db.database_file.clone(),
            series_id.to_string(),
            thumbnail_id.to_string(),
        )
        .await
}

async fn delete_series_thumbnail_from_services(
    app: &HttpAppState,
    series_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    app.services
        .media_assets
        .delete_series_thumbnail(
            app.auth_db.database_file.clone(),
            series_id.to_string(),
            thumbnail_id.to_string(),
        )
        .await
}

fn load_epub_cover_bytes_from_services(
    app: &HttpAppState,
    media: &PersistedBookMedia,
) -> Option<(Vec<u8>, String)> {
    app.services
        .media_assets
        .load_epub_cover_bytes(media.clone())
}

fn load_archive_page_row_from_services(
    app: &HttpAppState,
    media: &PersistedBookMedia,
    page_number: u64,
) -> Option<komga_application::media_assets::BookPageRecord> {
    app.services
        .media_assets
        .load_archive_page_row(media.clone(), page_number)
}

fn load_archive_page_rows_from_services(
    app: &HttpAppState,
    media: &PersistedBookMedia,
) -> Option<Vec<komga_application::media_assets::BookPageRecord>> {
    app.services
        .media_assets
        .load_archive_page_rows(media.clone())
}

fn load_pdf_page_row_from_services(
    app: &HttpAppState,
    media: &PersistedBookMedia,
    page_number: u64,
) -> Option<komga_application::media_assets::BookPageRecord> {
    app.services
        .media_assets
        .load_pdf_page_row(media.clone(), page_number)
}

fn load_generated_pdf_page_rows_from_services(
    app: &HttpAppState,
    media: &PersistedBookMedia,
) -> Vec<komga_application::media_assets::BookPageRecord> {
    app.services
        .media_assets
        .load_generated_pdf_page_rows(media.clone())
}

fn read_pdf_page_as_single_page_pdf_from_services(
    app: &HttpAppState,
    media: &PersistedBookMedia,
    page_number: u64,
) -> Option<Vec<u8>> {
    app.services
        .media_assets
        .read_pdf_page_as_single_page_pdf(media.clone(), page_number)
}

fn detect_pdf_page_count_from_services(
    app: &HttpAppState,
    media: &PersistedBookMedia,
) -> Option<u64> {
    app.services
        .media_assets
        .detect_pdf_page_count(media.clone())
}

fn resolve_book_page_bytes_from_services(
    app: &HttpAppState,
    media: &PersistedBookMedia,
    page: &PersistedBookPageRow,
    page_number: u64,
) -> Option<Vec<u8>> {
    app.services
        .media_assets
        .resolve_book_page_bytes(media.clone(), page.clone(), page_number)
}

async fn load_selected_series_thumbnail_from_services(
    app: &HttpAppState,
    series_id: &str,
) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
    app.services
        .media_assets
        .load_selected_series_thumbnail(app.auth_db.database_file.clone(), series_id.to_string())
        .await
}

async fn load_series_book_number_sorts_from_services(
    app: &HttpAppState,
    series_id: &str,
) -> Result<Vec<(String, f64)>, String> {
    app.services
        .media_assets
        .load_series_book_number_sorts(app.auth_db.database_file.clone(), series_id.to_string())
        .await
}

fn render_book_page_thumbnail_from_services(
    app: &HttpAppState,
    media: &PersistedBookMedia,
    page: &PersistedBookPageRow,
    page_number: u64,
    max_edge: u32,
) -> Option<Vec<u8>> {
    app.services.media_assets.render_book_page_thumbnail(
        media.clone(),
        page.clone(),
        page_number,
        max_edge,
    )
}

async fn process_task_side_effects(
    app: &HttpAppState,
    task_records: Vec<TaskQueueRecord>,
) -> Result<(), String> {
    app.services
        .task_queue
        .enqueue_task_records(task_records, true)
        .await
}

async fn enqueue_task_records(app: &HttpAppState, task_records: Vec<TaskQueueRecord>) -> Response {
    if let Err(error) = process_task_side_effects(app, task_records).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error })),
        )
            .into_response();
    }

    let mut response = StatusCode::ACCEPTED.into_response();
    mark_runtime_owned(&mut response);
    response
}

#[cfg(test)]
mod tests;
