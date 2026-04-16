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
use crate::http::state::{AuthDatabaseState, OperationalState, ReadProgressState, RuntimeProfile};
use crate::media_assets_runtime_access::PersistedMediaFileRecord;
use crate::media_assets_runtime_access::facade::*;
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
use self::media_helpers::{normalize_epub_resource_href, parse_epub_fixed_layout, parse_epub_kobo_spans};
use self::types::{
    BooksImportEntry, BooksImportPayload, ImportCopyMode, ManifestBuildOutcome, ManifestProfile,
    ManifestVariant, PersistedBookMedia, PersistedBookPageRow,
};

fn process_task_side_effects(
    state: &OperationalState,
    task_records: Vec<TaskQueueRecord>,
) -> Result<(), String> {
    (state.enqueue_task_records)(task_records, true)
}

fn enqueue_task_records(
    state: &OperationalState,
    task_records: Vec<TaskQueueRecord>,
) -> Response {
    if let Err(error) = process_task_side_effects(state, task_records) {
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
