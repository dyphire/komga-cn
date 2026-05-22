use super::*;
use crate::identity_access::auth::FileDownload;
use crate::media_responses;
use crate::opds::content_opds::opds_catalog_unauthorized_response;
use crate::state::MediaAssetsState;
use axum::extract::State;

pub async fn readlist_file(
    State(app): State<MediaAssetsState>,
    FileDownload(user): FileDownload,
    Path(readlist_id): Path<String>,
) -> Response {
    match user_can_access_readlist_media(&app, &readlist_id, &user).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }

    let visible_books = match visible_readlist_books_for_user(&app, &readlist_id, &user).await {
        Ok(books) => books,
        Err(error) => return internal_error_response(error),
    };

    match app.reader.readlist_name(&readlist_id).await {
        Ok(Some(name)) => {
            let file_name = format!("{name}.zip");
            let content_disposition = attachment_disposition(&file_name);

            let mut archive_entries = Vec::new();
            for (index, book) in visible_books.into_iter().enumerate() {
                let Some(media) = (match app.reader.book_media(&book.id).await {
                    Ok(media) => media,
                    Err(error) => return internal_error_response(error),
                }) else {
                    continue;
                };
                let Some(bytes) = app.content.read_media_file_bytes(&media.file_path).await else {
                    continue;
                };
                archive_entries.push((readlist_archive_entry_name(index, &media.file_name), bytes));
            }

            let body = match build_stored_zip_archive(archive_entries) {
                Ok(body) => body,
                Err(error) => return internal_error_response(error),
            };

            return (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "application/zip"),
                    (header::CONTENT_DISPOSITION, content_disposition.as_str()),
                ],
                body,
            )
                .into_response();
        }
        Ok(None) => {}
        Err(error) => return internal_error_response(error),
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn series_file(
    State(app): State<MediaAssetsState>,
    FileDownload(user): FileDownload,
    Path(series_id): Path<String>,
) -> Response {
    match app.reader.series_archive_entries(&series_id).await {
        Ok(Some((series_title, _library_id, entries))) => {
            match user_can_access_series_media(&app, &series_id, &user).await {
                Ok(true) => {}
                Ok(false) => return StatusCode::FORBIDDEN.into_response(),
                Err(error) => return internal_error_response(error),
            }

            let mut archive_entries = Vec::new();
            for (file_name, file_path) in entries {
                if let Some(bytes) = app.content.read_media_file_bytes(&file_path).await {
                    archive_entries.push((file_name, bytes));
                }
            }

            let archive_payload = match build_stored_zip_archive(archive_entries) {
                Ok(payload) => payload,
                Err(error) => return internal_error_response(error),
            };

            let file_name = format!("{series_title}.zip");
            let content_disposition = attachment_disposition(&file_name);
            (
                StatusCode::OK,
                [
                    (header::CONTENT_TYPE, "application/zip"),
                    (header::CONTENT_DISPOSITION, content_disposition.as_str()),
                ],
                archive_payload,
            )
                .into_response()
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_resource(
    State(app): State<MediaAssetsState>,
    headers: HeaderMap,
    Path((book_id, resource_path)): Path<(String, String)>,
) -> Response {
    book_resource_response_for_route(&app, headers, book_id, resource_path, false).await
}

pub async fn book_resource_opds_v2(
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

    if app.content.is_font_resource(resource_name) {
        return book_font_resource_response(app, &headers, &book_id, resource_name).await;
    }

    book_protected_resource_response(app, &headers, &book_id, resource_name, opds_v2_unauthorized)
        .await
}

async fn book_font_resource_response(
    app: &MediaAssetsState,
    headers: &HeaderMap,
    book_id: &str,
    resource_name: &str,
) -> Response {
    let media = match load_epub_book_media(app, book_id).await {
        Ok(media) => media,
        Err(response) => return response,
    };

    book_resource_response(app, headers, &media, resource_name).await
}

async fn book_protected_resource_response(
    app: &MediaAssetsState,
    headers: &HeaderMap,
    book_id: &str,
    resource_name: &str,
    opds_v2_unauthorized: bool,
) -> Response {
    let Some(user) = resolved_request_auth_user(&app.identity, headers).await else {
        return if opds_v2_unauthorized {
            opds_catalog_unauthorized_response(headers)
        } else {
            StatusCode::UNAUTHORIZED.into_response()
        };
    };
    let media = match load_epub_book_media(app, book_id).await {
        Ok(media) => media,
        Err(response) => return response,
    };

    if !user_can_access_book_media(app.reader.as_ref(), book_id, &user, &media).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    book_resource_response(app, headers, &media, resource_name).await
}

async fn load_epub_book_media(
    app: &MediaAssetsState,
    book_id: &str,
) -> Result<PersistedBookMedia, Response> {
    let Some(media) = (match app.reader.book_media(book_id).await {
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
) -> Response {
    let Some(bytes) = app
        .content
        .read_epub_resource_bytes(media.file_path.as_path(), resource_name)
        .await
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let last_modified = file_last_modified_header_value(media.file_path.as_path());
    if let Some(last_modified) = last_modified.as_deref()
        && if_modified_since_matches(headers, last_modified)
    {
        let mut response = asset_not_modified_response(None, Some(last_modified));
        response.headers_mut().insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("script-src 'none'; object-src 'none';"),
        );
        return response;
    }

    let file_name = resource_name
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(resource_name);
    let content_disposition = inline_disposition(file_name);

    let mut response = asset_ok_response(
        content_type_from_filename(resource_name, "application/octet-stream").as_str(),
        bytes,
        None,
        last_modified.as_deref(),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&content_disposition)
            .expect("resource content disposition should be valid"),
    );
    response.headers_mut().insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("script-src 'none'; object-src 'none';"),
    );
    response
}

pub async fn book_file(
    State(app): State<MediaAssetsState>,
    FileDownload(user): FileDownload,
    Path(book_id): Path<String>,
) -> Response {
    book_file_response_for_user(&app, &user, &book_id).await
}

pub async fn book_file_with_suffix(
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
    media_responses::book_file_response(app.reader.as_ref(), app.content.as_ref(), user, book_id)
        .await
}
