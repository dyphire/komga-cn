use super::shared::{
    load_readlist_mosaic_bytes, parse_thumbnail_upload, response_from_thumbnail_bytes,
    response_from_thumbnail_jpeg_bytes, set_one_hour_private_cache_control, thumbnail_dimensions,
};
use super::*;
use axum::extract::State;
use std::sync::Arc;

pub async fn readlist_thumbnail(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_auth(&headers, app.auth_db.db.database_file()).await {
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

    match load_persisted_readlist_thumbnails_from_services(&app, &readlist_id).await {
        Ok(rows) => {
            if let Some(thumbnail) = rows.first() {
                let mut response =
                    response_from_thumbnail_jpeg_bytes(&headers, thumbnail.thumbnail.clone());
                set_one_hour_private_cache_control(&mut response);
                return response;
            }

            match load_readlist_mosaic_bytes(&app, &readlist_id).await {
                Ok(Some(bytes)) => {
                    let mut response = response_from_thumbnail_bytes(&headers, bytes, "image/jpeg");
                    set_one_hour_private_cache_control(&mut response);
                    return response;
                }
                Ok(None) => {}
                Err(error) => return internal_error_response(error),
            }

            if persisted_readlist_exists_from_services(&app, &readlist_id)
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
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_auth(&headers, app.auth_db.db.database_file()).await {
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

    match load_persisted_readlist_thumbnails_from_services(&app, &readlist_id).await {
        Ok(rows) => {
            if !rows.is_empty() {
                return Json(
                    rows.into_iter()
                        .map(|row| {
                            json!({
                                "id": row.id,
                                "readListId": row.readlist_id,
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
                .into_response();
            }

            if persisted_readlist_exists_from_services(&app, &readlist_id)
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
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_request_auth(&headers, app.auth_db.db.database_file()).await {
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

    match load_persisted_readlist_thumbnails_from_services(&app, &readlist_id).await {
        Ok(rows) => {
            if let Some(thumbnail) = rows.into_iter().find(|row| row.id == thumbnail_id) {
                return asset_ok_response(
                    thumbnail.media_type.as_str(),
                    thumbnail.thumbnail,
                    None,
                    None,
                );
            }

            if persisted_readlist_exists_from_services(&app, &readlist_id)
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
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
    multipart: Multipart,
) -> Response {
    if let Some(response) = require_request_admin(&headers, app.auth_db.db.database_file()).await {
        return response;
    }

    if !persisted_readlist_exists_from_services(&app, &readlist_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let (thumbnail_bytes, media_type, selected) =
        match parse_thumbnail_upload(multipart, "readlist").await {
            Ok(parsed) => parsed,
            Err(response) => return response,
        };
    let Some((width, height)) = thumbnail_dimensions(&thumbnail_bytes) else {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    };

    match insert_readlist_thumbnail_from_services(
        &app,
        &readlist_id,
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
            "readListId": thumbnail.readlist_id,
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

pub async fn readlist_thumbnail_select(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_request_admin(&headers, app.auth_db.db.database_file()).await {
        return response;
    }

    if !persisted_readlist_exists_from_services(&app, &readlist_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match select_readlist_thumbnail_from_services(&app, &readlist_id, &thumbnail_id).await {
        Ok(_) => StatusCode::ACCEPTED.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn readlist_thumbnail_delete(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_request_admin(&headers, app.auth_db.db.database_file()).await {
        return response;
    }

    match delete_readlist_thumbnail_from_services(&app, &readlist_id, &thumbnail_id).await {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}
