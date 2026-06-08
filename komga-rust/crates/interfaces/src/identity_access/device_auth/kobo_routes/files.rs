use super::*;
use serde::Deserialize;
use std::path::Path as FsPath;

#[derive(Deserialize, Default)]
pub struct KoboBookFileQuery {
    convert_kepub: Option<bool>,
}

fn convert_epub_to_kepub_bytes(input_file: &FsPath) -> Option<Vec<u8>> {
    komga_kepubify::convert_epub_file_to_bytes(input_file).ok()
}

fn kobo_kepub_file_name(file_name: &str) -> String {
    if let Some((base, ext)) = file_name.rsplit_once('.')
        && ext.eq_ignore_ascii_case("epub")
    {
        return format!("{base}.kepub.epub");
    }
    format!("{file_name}.kepub.epub")
}

pub async fn kobo_book_file_epub(
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

    if !user_is_admin(&current_user) && !user_has_role(&current_user, "FILE_DOWNLOAD") {
        return StatusCode::FORBIDDEN.into_response();
    }

    let media = match app.reader.book_media(&book_id).await {
        Ok(Some(media)) => media,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    if !user_can_access_book_media(app.reader.as_ref(), &book_id, &current_user, &media).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    let mut file_name = media.file_name.clone();
    let mut media_type = media.media_type.clone();

    let body = if query.convert_kepub.unwrap_or(false) && media.media_type == "application/epub+zip"
    {
        if let Some(converted_body) = convert_epub_to_kepub_bytes(&media.file_path) {
            file_name = kobo_kepub_file_name(media.file_name.as_str());
            media_type = "application/epub+zip".to_string();
            converted_body
        } else {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({ "error": "Kepub conversion failed" })),
            )
                .into_response();
        }
    } else {
        match app.content.read_media_file_bytes(&media.file_path).await {
            Some(body) => body,
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(json!({ "error": "File not found, it may have moved" })),
                )
                    .into_response();
            }
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
