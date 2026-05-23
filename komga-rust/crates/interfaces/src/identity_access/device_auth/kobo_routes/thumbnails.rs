use super::*;

pub async fn kobo_book_thumbnail(
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
    kobo_book_thumbnail_response(
        &app,
        app.server_settings.as_ref(),
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
        thumbnail_id.as_str(),
        width.as_str(),
        height.as_str(),
    )
    .await
}

pub async fn kobo_book_thumbnail_with_quality(
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
    kobo_book_thumbnail_response(
        &app,
        app.server_settings.as_ref(),
        auth_token.as_str(),
        &headers,
        connection_info.remote_addr(),
        thumbnail_id.as_str(),
        width.as_str(),
        height.as_str(),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn kobo_book_thumbnail_response(
    app: &IdentityAccessState,
    server_settings: &dyn komga_application::operational::ServerSettingsPort,
    auth_token: &str,
    headers: &HeaderMap,
    remote_addr: Option<SocketAddr>,
    thumbnail_id: &str,
    width: &str,
    height: &str,
) -> Response {
    if let Err(status) = required_kobo_user(&app.identity, auth_token, headers, remote_addr).await {
        return status.into_response();
    }

    match load_thumbnail_by_id(app, thumbnail_id).await {
        Ok(Some((media_type, bytes))) => {
            let jpeg_bytes = if media_type.eq_ignore_ascii_case("image/jpeg") {
                bytes
            } else {
                match encode_kobo_thumbnail_as_jpeg(&bytes) {
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
            if load_kobo_proxy_enabled(server_settings).await {
                let location = format!(
                    "https://cdn.kobo.com/book-images/{thumbnail_id}/{width}/{height}/false/image.jpg"
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
    app: &IdentityAccessState,
    thumbnail_id: &str,
) -> Result<Option<(String, Vec<u8>)>, String> {
    app.identity
        .device_sync()
        .load_thumbnail_by_id(thumbnail_id)
        .await
}
