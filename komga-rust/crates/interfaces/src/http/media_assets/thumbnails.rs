use super::*;

pub async fn book_thumbnail(
    Extension(_profile): Extension<RuntimeProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), &book_id).await
    {
        if let Some(user) = resolved_auth_user(&headers)
            && !user_can_access_book_media(auth_db.database_file.as_path(), &book_id, &user, &media)
                .await
        {
            return StatusCode::FORBIDDEN.into_response();
        }

        match load_selected_book_thumbnail(auth_db.database_file.as_path(), &book_id).await {
            Ok(Some(thumbnail)) => {
                let etag = asset_etag(thumbnail.thumbnail.as_slice());
                if if_none_match_matches(&headers, etag.as_str()) {
                    return asset_not_modified_response(Some(etag.as_str()), None);
                }

                return asset_ok_response(
                    thumbnail.media_type.as_str(),
                    thumbnail.thumbnail,
                    Some(etag.as_str()),
                    None,
                );
            }
            Ok(None) => {}
            Err(error) => return internal_error_response(error),
        }

        if book_media_supports_page_image(&media)
            && let Some(bytes) = read_media_file_bytes(&media.file_path)
        {
            let content_type = content_type_from_filename(&media.file_name, &media.media_type);
            let etag = asset_etag(bytes.as_slice());
            let last_modified = file_last_modified_header_value(media.file_path.as_path());
            if if_none_match_matches(&headers, etag.as_str()) {
                return asset_not_modified_response(Some(etag.as_str()), last_modified.as_deref());
            }

            return asset_ok_response(
                content_type.as_str(),
                bytes,
                Some(etag.as_str()),
                last_modified.as_deref(),
            );
        }

        return StatusCode::NOT_FOUND.into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn book_thumbnail_by_id(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if let Ok(Some(media)) =
        load_persisted_book_media(auth_db.database_file.as_path(), &book_id).await
    {
        if let Some(user) = resolved_auth_user(&headers)
            && !user_can_access_book_media(auth_db.database_file.as_path(), &book_id, &user, &media)
                .await
        {
            return StatusCode::FORBIDDEN.into_response();
        }

        return match load_book_thumbnail_by_id(
            auth_db.database_file.as_path(),
            &book_id,
            &thumbnail_id,
        )
        .await
        {
            Ok(Some(thumbnail)) => {
                let etag = asset_etag(thumbnail.thumbnail.as_slice());
                if if_none_match_matches(&headers, etag.as_str()) {
                    asset_not_modified_response(Some(etag.as_str()), None)
                } else {
                    asset_ok_response(
                        thumbnail.media_type.as_str(),
                        thumbnail.thumbnail,
                        Some(etag.as_str()),
                        None,
                    )
                }
            }
            Ok(None) => StatusCode::NOT_FOUND.into_response(),
            Err(error) => internal_error_response(error),
        };
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn book_thumbnails(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if let Ok(rows) =
        load_persisted_book_thumbnails(auth_db.database_file.as_path(), &book_id).await
        && !rows.is_empty()
    {
        let mut response = Json(
            rows.into_iter()
                .map(|row| {
                    json!({
                        "id": row.id,
                        "type": row.thumbnail_type,
                        "selected": row.selected,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .into_response();
        mark_runtime_owned(&mut response);
        return response;
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn book_thumbnail_upload(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    if !persisted_book_exists(auth_db.database_file.as_path(), &book_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let media_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("image/jpeg");
    if !media_type.starts_with("image/") && !media_type.starts_with("multipart/form-data") {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }

    if body.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "book thumbnail upload body must not be empty",
            })),
        )
            .into_response();
    }

    let thumbnail_bytes = body.to_vec();

    match insert_book_thumbnail(
        auth_db.database_file.as_path(),
        &book_id,
        &thumbnail_bytes,
        media_type,
        true,
    )
    .await
    {
        Ok(thumbnail) => Json(json!({
            "id": thumbnail.id,
            "type": thumbnail.thumbnail_type,
            "selected": thumbnail.selected,
        }))
        .into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_thumbnail_select(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match select_book_thumbnail(auth_db.database_file.as_path(), &book_id, &thumbnail_id).await {
        Ok(true) => {
            let mut response = StatusCode::ACCEPTED.into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_thumbnail_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match delete_book_thumbnail(auth_db.database_file.as_path(), &book_id, &thumbnail_id).await {
        Ok(true) => {
            let mut response = StatusCode::ACCEPTED.into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn readlist_thumbnail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    match load_persisted_readlist_thumbnails(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(rows) => {
            if let Some(thumbnail) = rows.first() {
                let etag = asset_etag(thumbnail.thumbnail.as_slice());
                if if_none_match_matches(&headers, etag.as_str()) {
                    return asset_not_modified_response(Some(etag.as_str()), None);
                }

                return asset_ok_response(
                    thumbnail.media_type.as_str(),
                    thumbnail.thumbnail.clone(),
                    Some(etag.as_str()),
                    None,
                );
            }

            if persisted_readlist_exists(auth_db.database_file.as_path(), &readlist_id)
                .await
                .unwrap_or(false)
            {
                return StatusCode::NOT_FOUND.into_response();
            }
        }
        Err(error) => return internal_error_response(error),
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn readlist_thumbnails(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    match load_persisted_readlist_thumbnails(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(rows) => {
            if !rows.is_empty() {
                return Json(
                    rows.into_iter()
                        .map(|row| {
                            json!({
                                "id": row.id,
                                "type": row.thumbnail_type,
                                "selected": row.selected,
                            })
                        })
                        .collect::<Vec<_>>(),
                )
                .into_response();
            }

            if persisted_readlist_exists(auth_db.database_file.as_path(), &readlist_id)
                .await
                .unwrap_or(false)
            {
                return Json(json!([])).into_response();
            }
        }
        Err(error) => return internal_error_response(error),
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn readlist_thumbnail_by_id(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    match load_persisted_readlist_thumbnails(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(rows) => {
            if let Some(thumbnail) = rows.into_iter().find(|row| row.id == thumbnail_id) {
                let etag = asset_etag(thumbnail.thumbnail.as_slice());
                if if_none_match_matches(&headers, etag.as_str()) {
                    return asset_not_modified_response(Some(etag.as_str()), None);
                }

                return asset_ok_response(
                    thumbnail.media_type.as_str(),
                    thumbnail.thumbnail,
                    Some(etag.as_str()),
                    None,
                );
            }

            if persisted_readlist_exists(auth_db.database_file.as_path(), &readlist_id)
                .await
                .unwrap_or(false)
            {
                return StatusCode::NOT_FOUND.into_response();
            }
        }
        Err(error) => return internal_error_response(error),
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn readlist_thumbnail_upload(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    if !persisted_readlist_exists(auth_db.database_file.as_path(), &readlist_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let media_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("image/jpeg");
    if !media_type.starts_with("image/") && !media_type.starts_with("multipart/form-data") {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }

    if body.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "readlist thumbnail upload body must not be empty",
            })),
        )
            .into_response();
    }

    match insert_readlist_thumbnail(
        auth_db.database_file.as_path(),
        &readlist_id,
        body.as_ref(),
        media_type,
        true,
    )
    .await
    {
        Ok(thumbnail) => Json(json!({
            "id": thumbnail.id,
            "type": thumbnail.thumbnail_type,
            "selected": thumbnail.selected,
        }))
        .into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn readlist_thumbnail_select(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match select_readlist_thumbnail(auth_db.database_file.as_path(), &readlist_id, &thumbnail_id)
        .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn readlist_thumbnail_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match delete_readlist_thumbnail(auth_db.database_file.as_path(), &readlist_id, &thumbnail_id)
        .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_thumbnail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !persisted_collection_exists(auth_db.database_file.as_path(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_persisted_collection_thumbnails(auth_db.database_file.as_path(), &collection_id)
        .await
    {
        Ok(rows) => {
            if let Some(thumbnail) = rows.first() {
                let etag = asset_etag(thumbnail.thumbnail.as_slice());
                if if_none_match_matches(&headers, etag.as_str()) {
                    asset_not_modified_response(Some(etag.as_str()), None)
                } else {
                    asset_ok_response(
                        thumbnail.media_type.as_str(),
                        thumbnail.thumbnail.clone(),
                        Some(etag.as_str()),
                        None,
                    )
                }
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        }
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_thumbnails(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !persisted_collection_exists(auth_db.database_file.as_path(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_persisted_collection_thumbnails(auth_db.database_file.as_path(), &collection_id)
        .await
    {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|row| {
                    json!({
                        "id": row.id,
                        "type": row.thumbnail_type,
                        "selected": row.selected,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_thumbnail_by_id(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((collection_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !persisted_collection_exists(auth_db.database_file.as_path(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_persisted_collection_thumbnails(auth_db.database_file.as_path(), &collection_id)
        .await
    {
        Ok(rows) => {
            if let Some(thumbnail) = rows.into_iter().find(|row| row.id == thumbnail_id) {
                let etag = asset_etag(thumbnail.thumbnail.as_slice());
                if if_none_match_matches(&headers, etag.as_str()) {
                    asset_not_modified_response(Some(etag.as_str()), None)
                } else {
                    asset_ok_response(
                        thumbnail.media_type.as_str(),
                        thumbnail.thumbnail,
                        Some(etag.as_str()),
                        None,
                    )
                }
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        }
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_thumbnail_upload(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    if !persisted_collection_exists(auth_db.database_file.as_path(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let media_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("image/jpeg");
    if !media_type.starts_with("image/") && !media_type.starts_with("multipart/form-data") {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }

    if body.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "collection thumbnail upload body must not be empty",
            })),
        )
            .into_response();
    }

    let thumbnail_bytes = body.to_vec();

    match insert_collection_thumbnail(
        auth_db.database_file.as_path(),
        &collection_id,
        &thumbnail_bytes,
        media_type,
        true,
    )
    .await
    {
        Ok(thumbnail) => Json(json!({
            "id": thumbnail.id,
            "type": thumbnail.thumbnail_type,
            "selected": thumbnail.selected,
        }))
        .into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_thumbnail_select(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((collection_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match select_collection_thumbnail(
        auth_db.database_file.as_path(),
        &collection_id,
        &thumbnail_id,
    )
    .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_thumbnail_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((collection_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match delete_collection_thumbnail(
        auth_db.database_file.as_path(),
        &collection_id,
        &thumbnail_id,
    )
    .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_thumbnail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;

    if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_selected_series_thumbnail(auth_db.database_file.as_path(), &resolved_series_id).await
    {
        Ok(Some(thumbnail)) => {
            let etag = asset_etag(thumbnail.thumbnail.as_slice());
            if if_none_match_matches(&headers, etag.as_str()) {
                return asset_not_modified_response(Some(etag.as_str()), None);
            }

            return asset_ok_response(
                thumbnail.media_type.as_str(),
                thumbnail.thumbnail,
                Some(etag.as_str()),
                None,
            );
        }
        Ok(None) => {}
        Err(error) => return internal_error_response(error),
    }

    if let Ok(Some(media)) =
        load_persisted_series_thumbnail_media(auth_db.database_file.as_path(), &resolved_series_id)
            .await
        && let Some(bytes) = read_media_file_bytes(&media.file_path)
    {
        let etag = asset_etag(bytes.as_slice());
        let last_modified = file_last_modified_header_value(media.file_path.as_path());
        if if_none_match_matches(&headers, etag.as_str()) {
            return asset_not_modified_response(Some(etag.as_str()), last_modified.as_deref());
        }

        return asset_ok_response(
            "image/jpeg",
            bytes,
            Some(etag.as_str()),
            last_modified.as_deref(),
        );
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn series_thumbnails(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_persisted_series_thumbnails(auth_db.database_file.as_path(), &resolved_series_id)
        .await
    {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|row| {
                    json!({
                        "id": row.id,
                        "type": row.thumbnail_type,
                        "selected": row.selected,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_thumbnail_by_id(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((series_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_series_thumbnail_by_id(
        auth_db.database_file.as_path(),
        &resolved_series_id,
        &thumbnail_id,
    )
    .await
    {
        Ok(Some(thumbnail)) => {
            let etag = asset_etag(thumbnail.thumbnail.as_slice());
            if if_none_match_matches(&headers, etag.as_str()) {
                asset_not_modified_response(Some(etag.as_str()), None)
            } else {
                asset_ok_response(
                    thumbnail.media_type.as_str(),
                    thumbnail.thumbnail,
                    Some(etag.as_str()),
                    None,
                )
            }
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_thumbnail_upload(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let media_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("image/jpeg");
    if !media_type.starts_with("image/") && !media_type.starts_with("multipart/form-data") {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    }

    if body.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "error": "series thumbnail upload body must not be empty",
            })),
        )
            .into_response();
    }

    match insert_series_thumbnail(
        auth_db.database_file.as_path(),
        &resolved_series_id,
        body.as_ref(),
        media_type,
        true,
    )
    .await
    {
        Ok(thumbnail) => Json(json!({
            "id": thumbnail.id,
            "type": thumbnail.thumbnail_type,
            "selected": thumbnail.selected,
        }))
        .into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_thumbnail_select(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((series_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    match select_series_thumbnail(
        auth_db.database_file.as_path(),
        &resolved_series_id,
        &thumbnail_id,
    )
    .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_thumbnail_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path((series_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    match delete_series_thumbnail(
        auth_db.database_file.as_path(),
        &resolved_series_id,
        &thumbnail_id,
    )
    .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}
