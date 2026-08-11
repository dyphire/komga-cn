use axum::extract::{Extension, Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::DeviceThumbnailBinary;
use std::net::SocketAddr;

use crate::access_log::RequestConnectionInfo;
use crate::identity_access::device_auth::auth_resolvers::required_kobo_user;
use crate::identity_access::device_auth::load_kobo_proxy_enabled;
use crate::state::{IdentityAccessState, IdentityState};

use super::ensure_kobo_book_access;

struct KoboThumbnailRequest<'a> {
    app: &'a IdentityAccessState,
    auth_token: &'a str,
    headers: &'a HeaderMap,
    remote_addr: Option<SocketAddr>,
    thumbnail_id: &'a str,
    width: &'a str,
    height: &'a str,
}

pub(crate) async fn kobo_book_thumbnail(
    State(app): State<IdentityAccessState>,
    Path((auth_token, thumbnail_id, width, height, _)): Path<(
        String,
        String,
        String,
        String,
        String,
    )>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
) -> Response {
    kobo_book_thumbnail_response(KoboThumbnailRequest {
        app: &app,
        auth_token: auth_token.as_str(),
        headers: &headers,
        remote_addr: connection_info.remote_addr(),
        thumbnail_id: thumbnail_id.as_str(),
        width: width.as_str(),
        height: height.as_str(),
    })
    .await
}

pub(crate) async fn kobo_book_thumbnail_with_quality(
    State(app): State<IdentityAccessState>,
    Path((auth_token, thumbnail_id, width, height, _, _)): Path<(
        String,
        String,
        String,
        String,
        String,
        String,
    )>,
    Extension(connection_info): Extension<RequestConnectionInfo>,
    headers: HeaderMap,
) -> Response {
    kobo_book_thumbnail_response(KoboThumbnailRequest {
        app: &app,
        auth_token: auth_token.as_str(),
        headers: &headers,
        remote_addr: connection_info.remote_addr(),
        thumbnail_id: thumbnail_id.as_str(),
        width: width.as_str(),
        height: height.as_str(),
    })
    .await
}

async fn kobo_book_thumbnail_response(req: KoboThumbnailRequest<'_>) -> Response {
    let current_user = match required_kobo_user(
        &req.app.identity,
        req.auth_token,
        req.headers,
        req.remote_addr,
    )
    .await
    {
        Ok(user) => user,
        Err(status) => return status.into_response(),
    };

    match load_thumbnail_by_id(&req.app.identity, req.thumbnail_id).await {
        Ok(Some(thumbnail)) => {
            if let Err(status) =
                ensure_kobo_book_access(req.app, &current_user, &thumbnail.book_id).await
            {
                return status.into_response();
            }
            let jpeg_bytes = if thumbnail.media_type.eq_ignore_ascii_case("image/jpeg") {
                thumbnail.bytes
            } else {
                match encode_kobo_thumbnail_as_jpeg(&thumbnail.bytes) {
                    Some(jpeg_bytes) => jpeg_bytes,
                    None => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                }
            };

            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg"))],
                jpeg_bytes,
            )
                .into_response()
        }
        Ok(None) => {
            let proxy_enabled =
                match load_kobo_proxy_enabled(req.app.server_settings.as_ref()).await {
                    Ok(enabled) => enabled,
                    Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
                };
            if proxy_enabled {
                let location = format!(
                    "https://cdn.kobo.com/book-images/{}/{}/{}/false/image.jpg",
                    req.thumbnail_id, req.width, req.height
                );
                return (
                    StatusCode::TEMPORARY_REDIRECT,
                    [(
                        header::LOCATION,
                        HeaderValue::from_str(location.as_str()).unwrap_or_else(|_| {
                            HeaderValue::from_static(
                                "https://cdn.kobo.com/book-images/invalid/0/0/false/image.jpg",
                            )
                        }),
                    )],
                )
                    .into_response();
            }
            StatusCode::NOT_FOUND.into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn encode_kobo_thumbnail_as_jpeg(bytes: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory(bytes).ok()?;
    let mut output = std::io::Cursor::new(Vec::new());
    image.write_to(&mut output, image::ImageFormat::Jpeg).ok()?;
    Some(output.into_inner())
}

async fn load_thumbnail_by_id(
    identity: &IdentityState,
    thumbnail_id: &str,
) -> Result<Option<DeviceThumbnailBinary>, String> {
    identity
        .device_sync()
        .load_thumbnail_by_id(thumbnail_id)
        .await
}
