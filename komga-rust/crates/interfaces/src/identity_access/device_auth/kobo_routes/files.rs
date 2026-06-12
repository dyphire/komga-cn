use axum::Json;
use axum::extract::{Extension, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::{AuthUserRole, user_has_role, user_is_admin};
use serde::Deserialize;
use serde_json::json;
use std::path::Path as FsPath;

use crate::access_log::RequestConnectionInfo;
use crate::identity_access::device_auth::auth_resolvers::required_kobo_user;
use crate::media_assets::access_control::user_can_access_book_media;
use crate::media_assets::http_helpers::{attachment_disposition, internal_error_response};
use crate::state::IdentityAccessState;

#[derive(Deserialize, Default)]
pub(crate) struct KoboBookFileQuery {
    convert_kepub: Option<bool>,
}

fn convert_epub_to_kepub_bytes(input_file: &FsPath) -> Result<Vec<u8>, String> {
    komga_kepubify::convert_epub_file_to_bytes(input_file)
}

fn missing_kobo_file_response() -> Response {
    (
        StatusCode::NOT_FOUND,
        Json(json!({ "error": "File not found, it may have moved" })),
    )
        .into_response()
}

fn kobo_kepub_file_name(file_name: &str) -> String {
    if let Some((base, ext)) = file_name.rsplit_once('.')
        && ext.eq_ignore_ascii_case("epub")
    {
        return format!("{base}.kepub.epub");
    }
    format!("{file_name}.kepub.epub")
}

pub(crate) async fn kobo_book_file_epub(
    State(app): State<IdentityAccessState>,
    Path((auth_token, book_id)): Path<(String, String)>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
    Query(query): Query<KoboBookFileQuery>,
) -> Response {
    let current_user = match required_kobo_user(
        &app.identity,
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
    )
    .await
    {
        Ok(current_user) => current_user,
        Err(status) => return status.into_response(),
    };

    if !user_is_admin(&current_user) && !user_has_role(&current_user, AuthUserRole::FileDownload) {
        return StatusCode::FORBIDDEN.into_response();
    }

    let media = match app.book_media_reader.book_media(&book_id).await {
        Ok(Some(media)) => media,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    match user_can_access_book_media(
        app.book_media_reader.as_ref(),
        &book_id,
        &current_user,
        &media,
    )
    .await
    {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(error) => return internal_error_response(error),
    }

    let mut file_name = media.file_name.clone();
    let mut media_type = media.media_type.clone();

    let body = if query.convert_kepub.unwrap_or(false) && media.media_type == "application/epub+zip"
    {
        match app.content_resolver.media_file_exists(&media.file_path) {
            Ok(true) => {}
            Ok(false) => return missing_kobo_file_response(),
            Err(error) => return internal_error_response(error),
        }

        match convert_epub_to_kepub_bytes(&media.file_path) {
            Ok(converted_body) => {
                file_name = kobo_kepub_file_name(media.file_name.as_str());
                media_type = "application/epub+zip".to_string();
                converted_body
            }
            Err(_) => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(json!({ "error": "Kepub conversion failed" })),
                )
                    .into_response();
            }
        }
    } else {
        match app
            .content_resolver
            .read_media_file_bytes(&media.file_path)
            .await
        {
            Ok(Some(body)) => body,
            Ok(None) => return missing_kobo_file_response(),
            Err(error) => return internal_error_response(error),
        }
    };

    let content_disposition = attachment_disposition(&file_name);

    (
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_str(&media_type)
                    .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
            ),
            (
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(content_disposition.as_str())
                    .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
            ),
        ],
        body,
    )
        .into_response()
}
