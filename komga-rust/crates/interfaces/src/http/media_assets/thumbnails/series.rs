use super::shared::{
    load_series_thumbnail, parse_thumbnail_upload, response_from_thumbnail_bytes,
    thumbnail_dimensions,
};
use super::*;

pub async fn series_thumbnail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;

    if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match user_can_access_series_media(auth_db.database_file.as_path(), &resolved_series_id, &user)
        .await
    {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(error) => return internal_error_response(error),
    }

    match load_series_thumbnail(auth_db.database_file.as_path(), &resolved_series_id).await {
        Ok(Some(thumbnail)) => {
            return response_from_thumbnail_bytes(
                &headers,
                thumbnail.thumbnail,
                thumbnail.media_type.as_str(),
            );
        }
        Ok(None) => {}
        Err(error) => return internal_error_response(error),
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

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    match user_can_access_series_media(auth_db.database_file.as_path(), &resolved_series_id, &user)
        .await
    {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(error) => return internal_error_response(error),
    }

    match load_persisted_series_thumbnails(auth_db.database_file.as_path(), &resolved_series_id)
        .await
    {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|row| {
                    json!({
                        "id": row.id,
                        "seriesId": row.series_id,
                        "type": row.thumbnail_type,
                        "selected": row.selected,
                        "mediaType": row.media_type,
                        "fileSize": row.file_size,
                        "width": row.width,
                        "height": row.height,
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

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let resolved_series_id =
        resolve_series_id_for_persisted(auth_db.database_file.as_path(), &series_id).await;
    let unrestricted_all_libraries = user_shared_all_libraries(&user)
        && principal_from_user_payload(&user_payload_json(&user))
            .is_none_or(|principal| !principal.restrictions.is_restricted());
    if !unrestricted_all_libraries {
        if !persisted_series_exists(auth_db.database_file.as_path(), &resolved_series_id)
            .await
            .unwrap_or(false)
        {
            return StatusCode::NOT_FOUND.into_response();
        }

        match user_can_access_series_media(
            auth_db.database_file.as_path(),
            &resolved_series_id,
            &user,
        )
        .await
        {
            Ok(true) => {}
            Ok(false) => return StatusCode::FORBIDDEN.into_response(),
            Err(error) => return internal_error_response(error),
        }
    }

    match load_series_thumbnail_by_id(auth_db.database_file.as_path(), &thumbnail_id).await {
        Ok(Some(thumbnail)) => asset_ok_response(
            thumbnail.media_type.as_str(),
            thumbnail.thumbnail,
            None,
            None,
        ),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_thumbnail_upload(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
    multipart: Multipart,
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
    match load_persisted_series_oneshot(auth_db.database_file.as_path(), &resolved_series_id).await
    {
        Ok(Some(true)) => return StatusCode::BAD_REQUEST.into_response(),
        Ok(Some(false)) => {}
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }

    let (thumbnail_bytes, media_type, selected) =
        match parse_thumbnail_upload(multipart, "series").await {
            Ok(parsed) => parsed,
            Err(response) => return response,
        };
    let Some((width, height)) = thumbnail_dimensions(&thumbnail_bytes) else {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    };

    match insert_series_thumbnail(
        auth_db.database_file.as_path(),
        &resolved_series_id,
        &thumbnail_bytes,
        media_type.as_str(),
        width,
        height,
        selected,
    )
    .await
    {
        Ok(thumbnail) => Json(json!({
            "id": thumbnail.id,
            "seriesId": thumbnail.series_id,
            "type": thumbnail.thumbnail_type,
            "selected": thumbnail.selected,
            "mediaType": thumbnail.media_type,
            "fileSize": thumbnail.file_size,
            "width": thumbnail.width,
            "height": thumbnail.height,
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
    let thumbnail = match load_persisted_series_thumbnails(
        auth_db.database_file.as_path(),
        &resolved_series_id,
    )
    .await
    {
        Ok(rows) => rows.into_iter().find(|row| row.id == thumbnail_id),
        Err(error) => return internal_error_response(error),
    };
    let Some(thumbnail) = thumbnail else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if thumbnail.thumbnail_type != "USER_UPLOADED" {
        return StatusCode::BAD_REQUEST.into_response();
    }

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
