use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::{AuthUser, AuthUserRole, user_has_role};
use serde_json::json;

use super::access_control::{
    user_can_access_book_media, user_can_access_series_media, visible_readlist_book_ids_for_user,
};
use super::http_helpers::{attachment_disposition, inline_disposition, internal_error_response};
use super::media_helpers::book_media_is_epub;
use super::types::PersistedBookMedia;
use crate::cache::file_last_modified_header_value;
use crate::identity_access::auth::{FileDownload, resolved_request_auth_user};
use crate::media_response_policy::MediaAssetResponse;
use crate::media_responses::BookMediaResponses;
use crate::opds_auth::opds_catalog_unauthorized_response;
use crate::state::MediaAssetsState;
use komga_application::media_assets::{
    ArchiveDelivery, ArchiveDeliveryAsset, ArchiveDeliveryService, content_type_from_filename,
};

pub(crate) async fn readlist_file(
    State(app): State<MediaAssetsState>,
    FileDownload(user): FileDownload,
    Path(readlist_id): Path<String>,
) -> Response {
    let Some(visible_book_ids) =
        (match visible_readlist_book_ids_for_user(&app, &readlist_id, &user).await {
            Ok(books) => books,
            Err(error) => return internal_error_response(error),
        })
    else {
        return StatusCode::NOT_FOUND.into_response();
    };
    archive_delivery_response(
        ArchiveDeliveryService::new(
            app.archive_reader.as_ref(),
            app.content.as_ref(),
            app.archive_builder.as_ref(),
        )
        .readlist_archive(&readlist_id, visible_book_ids)
        .await,
    )
}

pub(crate) async fn series_file(
    State(app): State<MediaAssetsState>,
    FileDownload(user): FileDownload,
    Path(series_id): Path<String>,
) -> Response {
    match app.archive_reader.series_archive_entries(&series_id).await {
        Ok(Some(archive)) => {
            match user_can_access_series_media(&app, &series_id, &user).await {
                Ok(true) => {}
                Ok(false) => return StatusCode::FORBIDDEN.into_response(),
                Err(error) => return internal_error_response(error),
            }

            archive_delivery_response(
                ArchiveDeliveryService::new(
                    app.archive_reader.as_ref(),
                    app.content.as_ref(),
                    app.archive_builder.as_ref(),
                )
                .series_archive_from_entries(archive.series_title, archive.entries)
                .await,
            )
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

fn archive_delivery_response(delivery: ArchiveDelivery) -> Response {
    match delivery {
        ArchiveDelivery::Asset(asset) => archive_asset_response(asset),
        ArchiveDelivery::NotFound => StatusCode::NOT_FOUND.into_response(),
        ArchiveDelivery::Internal(error) => internal_error_response(error),
    }
}

fn archive_asset_response(asset: ArchiveDeliveryAsset) -> Response {
    let content_disposition = attachment_disposition(&asset.file_name);
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/zip"),
            (header::CONTENT_DISPOSITION, content_disposition.as_str()),
        ],
        asset.bytes,
    )
        .into_response()
}

pub(crate) async fn book_resource(
    State(app): State<MediaAssetsState>,
    headers: HeaderMap,
    Path((book_id, resource_path)): Path<(String, String)>,
) -> Response {
    book_resource_response_for_route(&app, headers, book_id, resource_path, false).await
}

pub(crate) async fn book_resource_opds_v2(
    State(app): State<MediaAssetsState>,
    headers: HeaderMap,
    Path((book_id, resource_path)): Path<(String, String)>,
) -> Response {
    book_resource_response_for_route(&app, headers, book_id, resource_path, true).await
}

async fn book_resource_response_for_route(
    app: &MediaAssetsState,
    headers: HeaderMap,
    book_id: String,
    resource_path: String,
    opds_v2_unauthorized: bool,
) -> Response {
    let resource_name = resource_path.trim_start_matches('/');
    if resource_name.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }

    book_protected_resource_response(app, &headers, &book_id, resource_name, opds_v2_unauthorized)
        .await
}

async fn book_protected_resource_response(
    app: &MediaAssetsState,
    headers: &HeaderMap,
    book_id: &str,
    resource_name: &str,
    opds_v2_unauthorized: bool,
) -> Response {
    let user = match resolved_request_auth_user(&app.identity, headers).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return if opds_v2_unauthorized {
                opds_catalog_unauthorized_response(headers)
            } else {
                StatusCode::UNAUTHORIZED.into_response()
            };
        }
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let media = match load_epub_book_media(app, book_id).await {
        Ok(media) => media,
        Err(response) => return response,
    };
    if !user_has_role(&user, AuthUserRole::PageStreaming) {
        return StatusCode::FORBIDDEN.into_response();
    }

    match user_can_access_book_media(app.book_media_reader.as_ref(), book_id, &user, &media).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(error) => return internal_error_response(error),
    }

    let content_type = match app.manifest_reader.media_file_records(book_id).await {
        Ok(records) => records
            .into_iter()
            .find(|record| record.file_name == resource_name)
            .map(|record| record.media_type)
            .filter(|media_type| !media_type.is_empty())
            .unwrap_or_else(|| {
                content_type_from_filename(resource_name, "application/octet-stream")
            }),
        Err(error) => return internal_error_response(error),
    };

    book_resource_response(app, headers, &media, resource_name, &content_type).await
}

async fn load_epub_book_media(
    app: &MediaAssetsState,
    book_id: &str,
) -> Result<PersistedBookMedia, Response> {
    let Some(media) = (match app.book_media_reader.book_media(book_id).await {
        Ok(media) => media,
        Err(error) => return Err(internal_error_response(error)),
    }) else {
        return Err(StatusCode::NOT_FOUND.into_response());
    };

    if !book_media_is_epub(&media) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": format!("Book media type '{}' not compatible with requested profile", media.media_type),
            })),
        )
            .into_response());
    }

    Ok(media)
}

async fn book_resource_response(
    app: &MediaAssetsState,
    headers: &HeaderMap,
    media: &PersistedBookMedia,
    resource_name: &str,
    content_type: &str,
) -> Response {
    let bytes = match app
        .content
        .read_epub_resource_bytes(media.file_path.as_path(), resource_name)
        .await
    {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };

    let last_modified = file_last_modified_header_value(media.file_path.as_path());
    let file_name = resource_name
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(resource_name);
    let content_disposition = inline_disposition(file_name);

    MediaAssetResponse::new(content_type, bytes)
        .with_last_modified(last_modified)
        .with_content_disposition(Some(content_disposition))
        .with_header(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("script-src 'none'; object-src 'none';"),
        )
        .into_response(Some(headers))
}

pub(crate) async fn book_file(
    State(app): State<MediaAssetsState>,
    FileDownload(user): FileDownload,
    Path(book_id): Path<String>,
) -> Response {
    book_file_response_for_user(&app, &user, &book_id).await
}

pub(crate) async fn book_file_with_suffix(
    State(app): State<MediaAssetsState>,
    FileDownload(user): FileDownload,
    Path((book_id, _file_name)): Path<(String, String)>,
) -> Response {
    book_file_response_for_user(&app, &user, &book_id).await
}

async fn book_file_response_for_user(
    app: &MediaAssetsState,
    user: &AuthUser,
    book_id: &str,
) -> Response {
    BookMediaResponses::new(
        app.book_media_reader.as_ref(),
        app.book_media_content.as_ref(),
        app.book_id_resolver.as_ref(),
    )
    .book_file(user, book_id)
    .await
}
