use super::*;
use axum::extract::State;
use std::sync::Arc;

pub async fn readlist_file(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) =
        require_request_file_download(&headers, app.auth_db.db.database_file()).await
    {
        return response;
    }

    let Some(user) = resolved_request_auth_user(&headers, app.auth_db.db.database_file()).await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    match user_can_access_readlist_media(&app, &readlist_id, &user).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }

    let readlist_books = match app
        .services
        .opds_persisted
        .load_readlist_books(readlist_id.clone())
        .await
    {
        Ok(books) => books,
        Err(error) => return internal_error_response(error),
    };
    let visible_books = readlist_books
        .into_iter()
        .filter(|book| user_can_access_library(&user, &book.library_id))
        .collect::<Vec<_>>();

    match load_persisted_readlist_name_from_services(&app, &readlist_id).await {
        Ok(Some(name)) => {
            let file_name = format!("{name}.zip");
            let content_disposition = attachment_disposition(&file_name);

            let mut archive_entries = Vec::new();
            for (index, book) in visible_books.into_iter().enumerate() {
                let Some(media) =
                    (match load_persisted_book_media_from_services(&app, &book.id).await {
                        Ok(media) => media,
                        Err(error) => return internal_error_response(error),
                    })
                else {
                    continue;
                };
                let Some(bytes) = read_media_file_bytes_from_services(&app, &media.file_path).await
                else {
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
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) =
        require_request_file_download(&headers, app.auth_db.db.database_file()).await
    {
        return response;
    }

    let Some(user) = resolved_request_auth_user(&headers, app.auth_db.db.database_file()).await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    match load_series_archive_entries_from_services(&app, &series_id).await {
        Ok(Some((series_title, _library_id, entries))) => {
            match user_can_access_series_media(&app, &series_id, &user).await {
                Ok(true) => {}
                Ok(false) => return StatusCode::FORBIDDEN.into_response(),
                Err(error) => return internal_error_response(error),
            }

            let mut archive_entries = Vec::new();
            for (file_name, file_path) in entries {
                if let Some(bytes) = read_media_file_bytes_from_services(&app, &file_path).await {
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
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path((book_id, resource_path)): Path<(String, String)>,
) -> Response {
    let resource_name = resource_path.trim_start_matches('/');
    if resource_name.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let is_font = is_font_resource_from_services(&app, resource_name);
    if !is_font
        && let Some(response) = require_request_auth(&headers, app.auth_db.db.database_file()).await
    {
        return response;
    }

    let Some(media) = (match load_persisted_book_media_from_services(&app, &book_id).await {
        Ok(media) => media,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    if !book_media_is_epub(&media) {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": format!("Book media type '{}' not compatible with requested profile", media.media_type),
            })),
        )
            .into_response();
    }

    if !is_font {
        let Some(user) = resolved_request_auth_user(&headers, app.auth_db.db.database_file()).await
        else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        if !user_can_access_book_media(&app, &book_id, &user, &media).await {
            return StatusCode::FORBIDDEN.into_response();
        }
    }

    let Some(bytes) =
        read_epub_resource_bytes_from_services(&app, media.file_path.as_path(), resource_name)
            .await
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let last_modified = file_last_modified_header_value(media.file_path.as_path());
    if let Some(last_modified) = last_modified.as_deref()
        && if_modified_since_matches(&headers, last_modified)
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
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    book_file_response(&app, &headers, &book_id).await
}

pub async fn book_file_with_suffix(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path((book_id, _file_name)): Path<(String, String)>,
) -> Response {
    book_file_response(&app, &headers, &book_id).await
}

async fn book_file_response(app: &HttpAppState, headers: &HeaderMap, book_id: &str) -> Response {
    if let Some(response) =
        require_request_file_download(headers, app.auth_db.db.database_file()).await
    {
        return response;
    }

    if let Ok(Some(media)) = load_persisted_book_media_from_services(app, book_id).await {
        let Some(user) = resolved_request_auth_user(headers, app.auth_db.db.database_file()).await
        else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        if !user_can_access_book_media(app, book_id, &user, &media).await {
            return StatusCode::FORBIDDEN.into_response();
        }

        let Some(body) = read_media_file_bytes_from_services(app, &media.file_path).await else {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "File not found, it may have moved" })),
            )
                .into_response();
        };

        let content_type = media.media_type.clone();
        let content_disposition = attachment_disposition(&media.file_name);

        return (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, content_type.as_str()),
                (header::CONTENT_DISPOSITION, content_disposition.as_str()),
            ],
            body,
        )
            .into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}
