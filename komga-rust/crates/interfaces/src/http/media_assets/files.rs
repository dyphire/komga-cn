use super::*;

pub async fn readlist_file(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_file_download(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let visible_books = match visible_readlist_books_for_user(
        auth_db.database_file.as_path(),
        &readlist_id,
        &user,
    )
    .await
    {
        Ok(books) if !books.is_empty() => books,
        Ok(_) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };

    match load_persisted_readlist_name(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(Some(name)) => {
            let file_name = format!("{name}.zip");
            let content_disposition = attachment_disposition(&file_name);

            let mut archive_entries = Vec::new();
            for (index, book) in visible_books.into_iter().enumerate() {
                let Some(media) = (match load_persisted_book_media(
                    auth_db.database_file.as_path(),
                    &book.id,
                )
                .await
                {
                    Ok(media) => media,
                    Err(error) => return internal_error_response(error),
                }) else {
                    continue;
                };
                let Some(bytes) = read_media_file_bytes(&media.file_path) else {
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
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_file_download(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    match load_series_archive_entries(auth_db.database_file.as_path(), &series_id).await {
        Ok(Some((series_title, _library_id, entries))) => {
            match user_can_access_series_media(auth_db.database_file.as_path(), &series_id, &user)
                .await
            {
                Ok(true) => {}
                Ok(false) => return StatusCode::FORBIDDEN.into_response(),
                Err(error) => return internal_error_response(error),
            }

            let archive_entries = entries
                .into_iter()
                .filter_map(|(file_name, file_path)| {
                    read_media_file_bytes(&file_path).map(|bytes| (file_name, bytes))
                })
                .collect::<Vec<_>>();
            if archive_entries.is_empty() {
                return StatusCode::NOT_FOUND.into_response();
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
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, resource_path)): Path<(String, String)>,
) -> Response {
    let resource_name = resource_path.trim_start_matches('/');
    if resource_name.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let is_font = is_font_resource(resource_name);
    if !is_font && let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(media) =
        (match load_persisted_book_media(auth_db.database_file.as_path(), &book_id).await {
            Ok(media) => media,
            Err(error) => return internal_error_response(error),
        })
    else {
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
        let Some(user) = resolved_auth_user(&headers) else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        if !user_can_access_library(&user, &media.library_id) {
            return StatusCode::FORBIDDEN.into_response();
        }
    }

    let Some(bytes) = read_epub_resource_bytes(media.file_path.as_path(), resource_name) else {
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
    Extension(_profile): Extension<RuntimeProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    book_file_response(&auth_db, &headers, &book_id).await
}

pub async fn book_file_with_suffix(
    Extension(_profile): Extension<RuntimeProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, _file_name)): Path<(String, String)>,
) -> Response {
    book_file_response(&auth_db, &headers, &book_id).await
}

async fn book_file_response(
    auth_db: &AuthDatabaseState,
    headers: &HeaderMap,
    book_id: &str,
) -> Response {
    if let Some(response) = require_file_download(headers) {
        return response;
    }

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), book_id).await
        && let Some(body) = read_media_file_bytes(&media.file_path)
    {
        if let Some(user) = resolved_auth_user(headers)
            && !user_can_access_book_media(auth_db.database_file.as_path(), book_id, &user, &media)
                .await
        {
            return StatusCode::FORBIDDEN.into_response();
        }

        let content_type = content_type_from_filename(&media.file_name, &media.media_type);
        let content_disposition = attachment_disposition(&media.file_name);

        if let Some((start, end)) = requested_byte_range(headers, body.len()) {
            let mut response =
                (StatusCode::PARTIAL_CONTENT, body[start..=end].to_vec()).into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_str(&content_type)
                    .expect("book file content type should be valid"),
            );
            response.headers_mut().insert(
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(&content_disposition)
                    .expect("book file content disposition should be valid"),
            );
            response
                .headers_mut()
                .insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
            response.headers_mut().insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes {start}-{end}/{}", body.len()))
                    .expect("book file content-range should be valid"),
            );

            return response;
        }

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
