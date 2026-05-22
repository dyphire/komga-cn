use std::path::{Path as FsPath, PathBuf};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, Query};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use image::ImageFormat;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::cache::{
    asset_etag, asset_not_modified_response, asset_ok_response, file_last_modified_header_value,
    if_modified_since_matches, if_none_match_matches,
};
use crate::discovery_auth::principal::principal_from_user_payload;
use crate::identity_access::auth::{
    AuthUser, resolved_auth_user, resolved_request_auth_user, resolved_token, user_id,
    user_is_admin, user_payload_json, user_shared_all_libraries, user_shared_library_ids,
};
use crate::state::MediaAssetsState;
use komga_application::task_processing::{TaskKind, TaskQueueRecord, TaskRequest};
#[cfg(test)]
use komga_infrastructure::filesystem::media_access::page_content::{
    load_archive_page_rows, load_generated_pdf_page_rows, read_pdf_page_as_single_page_pdf,
    resolve_book_page_bytes,
};

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
mod manifests;
pub(crate) mod media_helpers;
mod operations;
pub(crate) mod page_resolution;
mod pages;
pub(crate) mod read_progress;
pub(crate) mod thumbnails;
pub(crate) mod types;

use self::access_control::{
    user_can_access_book_media, user_can_access_collection_media, user_can_access_readlist_media,
    user_can_access_series_media, visible_readlist_books_for_user,
};
use self::archive_payload::{build_stored_zip_archive, readlist_archive_entry_name};
use self::epub_positions::load_persisted_epub_positions;
use self::http_helpers::{attachment_disposition, inline_disposition, internal_error_response};
use self::media_helpers::{book_media_is_epub, content_type_from_filename};
#[cfg(test)]
use self::media_helpers::{
    book_media_supports_page_api, normalize_epub_resource_href, parse_epub_fixed_layout,
    parse_epub_kobo_spans,
};
use self::types::PersistedBookMedia;

async fn resolve_book_id_for_persisted(app: &MediaAssetsState, requested_book_id: &str) -> String {
    let Some(index) = requested_book_id
        .strip_prefix("book-")
        .and_then(|value| value.parse::<usize>().ok())
    else {
        return requested_book_id.to_string();
    };

    if index == 0 {
        return requested_book_id.to_string();
    }

    if matches!(
        app.book_detail
            .load_persisted_book_resource(requested_book_id)
            .await,
        Ok(Some(_))
    ) {
        return requested_book_id.to_string();
    }

    match app.book_detail.load_book_id_by_sorted_position(index).await {
        Ok(Some(book_id)) => book_id,
        _ => requested_book_id.to_string(),
    }
}

async fn resolve_series_id_for_persisted(
    app: &MediaAssetsState,
    requested_series_id: &str,
) -> String {
    let Some(index) = requested_series_id
        .strip_prefix("series-")
        .and_then(|value| value.parse::<usize>().ok())
    else {
        return requested_series_id.to_string();
    };

    if index == 0 {
        return requested_series_id.to_string();
    }

    if matches!(
        app.series_detail
            .load_persisted_series_resource(requested_series_id)
            .await,
        Ok(Some(_))
    ) {
        return requested_series_id.to_string();
    }

    match app
        .series_detail
        .load_series_id_by_sorted_position(index)
        .await
    {
        Ok(Some(series_id)) => series_id,
        _ => requested_series_id.to_string(),
    }
}

async fn load_persisted_book_media_from_services(
    app: &MediaAssetsState,
    book_id: &str,
) -> Result<Option<PersistedBookMedia>, String> {
    app.reader.book_media(book_id).await
}

async fn load_book_progression_from_services(
    app: &MediaAssetsState,
    book_id: &str,
    user_id: &str,
) -> Result<Option<Value>, String> {
    app.reader.book_progression(book_id, user_id).await
}

async fn load_book_page_count_from_services(
    app: &MediaAssetsState,
    book_id: &str,
) -> Result<Option<u64>, String> {
    app.reader.book_page_count(book_id).await
}

async fn persisted_readlist_exists_from_services(
    app: &MediaAssetsState,
    readlist_id: &str,
) -> Result<bool, String> {
    app.reader.readlist_exists(readlist_id).await
}

async fn load_persisted_readlist_thumbnails_from_services(
    app: &MediaAssetsState,
    readlist_id: &str,
) -> Result<Vec<komga_application::media_assets::ReadlistThumbnailRecord>, String> {
    app.reader.readlist_thumbnails(readlist_id).await
}

async fn insert_readlist_thumbnail_from_services(
    app: &MediaAssetsState,
    readlist_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<komga_application::media_assets::ReadlistThumbnailRecord, String> {
    app.thumbnails
        .insert_readlist(readlist_id, thumbnail, media_type, width, height, selected)
        .await
}

async fn select_readlist_thumbnail_from_services(
    app: &MediaAssetsState,
    readlist_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    app.thumbnails
        .select_readlist(readlist_id, thumbnail_id)
        .await
}

async fn delete_readlist_thumbnail_from_services(
    app: &MediaAssetsState,
    readlist_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    app.thumbnails
        .delete_readlist(readlist_id, thumbnail_id)
        .await
}

async fn load_series_book_ids_from_media_services(
    app: &MediaAssetsState,
    series_id: &str,
) -> Result<Vec<String>, String> {
    app.reader.series_book_ids(series_id).await
}

async fn load_selected_book_thumbnail_from_services(
    app: &MediaAssetsState,
    book_id: &str,
) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
    app.reader.selected_book_thumbnail(book_id).await
}

async fn persisted_book_exists_from_services(
    app: &MediaAssetsState,
    book_id: &str,
) -> Result<bool, String> {
    app.reader.book_exists(book_id).await
}

async fn persisted_series_exists_from_services(
    app: &MediaAssetsState,
    series_id: &str,
) -> Result<bool, String> {
    app.reader.series_exists(series_id).await
}

async fn persisted_collection_exists_from_services(
    app: &MediaAssetsState,
    collection_id: &str,
) -> Result<bool, String> {
    app.reader.collection_exists(collection_id).await
}

async fn load_book_thumbnail_by_id_from_services(
    app: &MediaAssetsState,
    thumbnail_id: &str,
) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
    app.reader.book_thumbnail_by_id(thumbnail_id).await
}

async fn load_persisted_book_thumbnails_from_services(
    app: &MediaAssetsState,
    book_id: &str,
) -> Result<Vec<komga_application::media_assets::EntityThumbnailRecord>, String> {
    app.reader.book_thumbnails(book_id).await
}

async fn insert_book_thumbnail_from_services(
    app: &MediaAssetsState,
    book_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<komga_application::media_assets::EntityThumbnailRecord, String> {
    app.thumbnails
        .insert_book(book_id, thumbnail, media_type, width, height, selected)
        .await
}

async fn select_book_thumbnail_from_services(
    app: &MediaAssetsState,
    thumbnail_id: &str,
) -> Result<bool, String> {
    app.thumbnails.select_book(thumbnail_id).await
}

async fn delete_book_thumbnail_from_services(
    app: &MediaAssetsState,
    thumbnail_id: &str,
) -> Result<bool, String> {
    app.thumbnails.delete_book(thumbnail_id).await
}

async fn read_media_file_bytes_from_services(
    app: &MediaAssetsState,
    path: &FsPath,
) -> Option<Vec<u8>> {
    app.content.read_media_file_bytes(path).await
}

fn is_font_resource_from_services(app: &MediaAssetsState, resource_name: &str) -> bool {
    app.content.is_font_resource(resource_name)
}

async fn read_epub_resource_bytes_from_services(
    app: &MediaAssetsState,
    path: &FsPath,
    resource_name: &str,
) -> Option<Vec<u8>> {
    app.content
        .read_epub_resource_bytes(path, resource_name)
        .await
}

async fn load_persisted_readlist_name_from_services(
    app: &MediaAssetsState,
    readlist_id: &str,
) -> Result<Option<String>, String> {
    app.reader.readlist_name(readlist_id).await
}

async fn load_series_archive_entries_from_services(
    app: &MediaAssetsState,
    series_id: &str,
) -> Result<Option<(String, String, Vec<(String, PathBuf)>)>, String> {
    app.reader.series_archive_entries(series_id).await
}

async fn load_persisted_collection_thumbnails_from_services(
    app: &MediaAssetsState,
    collection_id: &str,
) -> Result<Vec<komga_application::media_assets::CollectionThumbnailRecord>, String> {
    app.reader.collection_thumbnails(collection_id).await
}

async fn insert_collection_thumbnail_from_services(
    app: &MediaAssetsState,
    collection_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<komga_application::media_assets::CollectionThumbnailRecord, String> {
    app.thumbnails
        .insert_collection(
            collection_id,
            thumbnail,
            media_type,
            width,
            height,
            selected,
        )
        .await
}

async fn select_collection_thumbnail_from_services(
    app: &MediaAssetsState,
    thumbnail_id: &str,
) -> Result<bool, String> {
    app.thumbnails.select_collection(thumbnail_id).await
}

async fn delete_collection_thumbnail_from_services(
    app: &MediaAssetsState,
    collection_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    app.thumbnails
        .delete_collection(collection_id, thumbnail_id)
        .await
}

async fn load_persisted_series_thumbnails_from_services(
    app: &MediaAssetsState,
    series_id: &str,
) -> Result<Vec<komga_application::media_assets::SeriesThumbnailRecord>, String> {
    app.reader.series_thumbnails(series_id).await
}

async fn load_series_thumbnail_by_id_from_services(
    app: &MediaAssetsState,
    thumbnail_id: &str,
) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
    app.reader.series_thumbnail_by_id(thumbnail_id).await
}

async fn load_persisted_series_oneshot_from_services(
    app: &MediaAssetsState,
    series_id: &str,
) -> Result<Option<bool>, String> {
    app.reader.series_oneshot(series_id).await
}

async fn insert_series_thumbnail_from_services(
    app: &MediaAssetsState,
    series_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<komga_application::media_assets::SeriesThumbnailRecord, String> {
    app.thumbnails
        .insert_series(series_id, thumbnail, media_type, width, height, selected)
        .await
}

async fn select_series_thumbnail_from_services(
    app: &MediaAssetsState,
    series_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    app.thumbnails.select_series(series_id, thumbnail_id).await
}

async fn delete_series_thumbnail_from_services(
    app: &MediaAssetsState,
    series_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    app.thumbnails.delete_series(series_id, thumbnail_id).await
}

async fn load_epub_cover_bytes_from_services(
    app: &MediaAssetsState,
    media: &PersistedBookMedia,
) -> Option<(Vec<u8>, String)> {
    app.content.epub_cover_bytes(media).await
}

async fn load_selected_series_thumbnail_from_services(
    app: &MediaAssetsState,
    series_id: &str,
) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
    app.reader.selected_series_thumbnail(series_id).await
}

async fn load_series_book_number_sorts_from_services(
    app: &MediaAssetsState,
    series_id: &str,
) -> Result<Vec<(String, f64)>, String> {
    app.reader.series_book_number_sorts(series_id).await
}

async fn process_task_side_effects(
    app: &MediaAssetsState,
    task_records: Vec<TaskQueueRecord>,
) -> Result<(), String> {
    app.task_queue
        .queue
        .enqueue_records(
            task_records,
            komga_application::task_processing::SubmitUrgency::Immediate,
        )
        .await
}

async fn enqueue_task_records(
    app: &MediaAssetsState,
    task_records: Vec<TaskQueueRecord>,
) -> Response {
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
