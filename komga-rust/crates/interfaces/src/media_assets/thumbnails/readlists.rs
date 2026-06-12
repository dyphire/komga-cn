use super::shared::{
    load_readlist_mosaic_bytes, parse_thumbnail_upload, response_from_thumbnail_bytes,
    response_from_thumbnail_jpeg_bytes, set_one_hour_private_cache_control, thumbnail_dimensions,
};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::Multipart;
use serde_json::json;

use crate::cache::asset_ok_response;
use crate::identity_access::auth::{Admin, Authenticated};
use crate::state::MediaAssetsState;

use super::super::access_control::{
    user_can_access_readlist_media, visible_readlist_book_ids_for_user,
};
use super::super::http_helpers::internal_error_response;

async fn readlist_exists(app: &MediaAssetsState, readlist_id: &str) -> Result<bool, Response> {
    app.thumbnail_reader
        .readlist_exists(readlist_id)
        .await
        .map_err(internal_error_response)
}

async fn ensure_readlist_exists(app: &MediaAssetsState, readlist_id: &str) -> Result<(), Response> {
    match readlist_exists(app, readlist_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(StatusCode::NOT_FOUND.into_response()),
        Err(response) => Err(response),
    }
}

pub(crate) async fn readlist_thumbnail(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    let visible_book_ids = match visible_readlist_book_ids_for_user(&app, &readlist_id, &user).await
    {
        Ok(Some(book_ids)) if !book_ids.is_empty() => book_ids,
        Ok(_) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };

    match app.thumbnail_reader.readlist_thumbnails(&readlist_id).await {
        Ok(rows) => {
            if let Some(thumbnail) = rows.first() {
                let mut response =
                    response_from_thumbnail_jpeg_bytes(&headers, thumbnail.thumbnail.clone());
                set_one_hour_private_cache_control(&mut response);
                return response;
            }

            match load_readlist_mosaic_bytes(&app, visible_book_ids).await {
                Ok(Some(bytes)) => {
                    let mut response = response_from_thumbnail_bytes(&headers, bytes, "image/jpeg");
                    set_one_hour_private_cache_control(&mut response);
                    return response;
                }
                Ok(None) => {}
                Err(error) => return internal_error_response(error),
            }

            if let Err(response) = readlist_exists(&app, &readlist_id).await {
                return response;
            }
        }
        Err(error) => return internal_error_response(error),
    }

    StatusCode::NOT_FOUND.into_response()
}

pub(crate) async fn readlist_thumbnails(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(readlist_id): Path<String>,
) -> Response {
    match user_can_access_readlist_media(&app, &readlist_id, &user).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }

    match app.thumbnail_reader.readlist_thumbnails(&readlist_id).await {
        Ok(rows) => {
            if !rows.is_empty() {
                return Json(
                    rows.into_iter()
                        .map(|row| {
                            json!({
                                "id": row.id,
                                "readListId": row.readlist_id,
                                "type": row.thumbnail_type.persisted_name(),
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

            match readlist_exists(&app, &readlist_id).await {
                Ok(true) => return Json(json!([])).into_response(),
                Ok(false) => {}
                Err(response) => return response,
            }
        }
        Err(error) => return internal_error_response(error),
    }

    StatusCode::NOT_FOUND.into_response()
}

pub(crate) async fn readlist_thumbnail_by_id(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    match user_can_access_readlist_media(&app, &readlist_id, &user).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }

    match app.thumbnail_reader.readlist_thumbnails(&readlist_id).await {
        Ok(rows) => {
            if let Some(thumbnail) = rows.into_iter().find(|row| row.id == thumbnail_id) {
                return asset_ok_response(
                    thumbnail.media_type.as_str(),
                    thumbnail.thumbnail,
                    None,
                    None,
                );
            }

            if let Err(response) = readlist_exists(&app, &readlist_id).await {
                return response;
            }
        }
        Err(error) => return internal_error_response(error),
    }

    StatusCode::NOT_FOUND.into_response()
}

pub(crate) async fn readlist_thumbnail_upload(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(readlist_id): Path<String>,
    multipart: Multipart,
) -> Response {
    if let Err(response) = ensure_readlist_exists(&app, &readlist_id).await {
        return response;
    }

    let upload = match parse_thumbnail_upload(multipart, "readlist").await {
        Ok(parsed) => parsed,
        Err(response) => return response,
    };
    let Some(dimensions) = thumbnail_dimensions(&upload.bytes) else {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    };

    match app
        .thumbnails
        .insert_readlist(
            &readlist_id,
            &upload.bytes,
            upload.media_type.as_str(),
            dimensions.width,
            dimensions.height,
            upload.selected,
        )
        .await
    {
        Ok(thumbnail) => Json(json!({
            "id": thumbnail.id,
            "readListId": thumbnail.readlist_id,
            "type": thumbnail.thumbnail_type.persisted_name(),
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

pub(crate) async fn readlist_thumbnail_select(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Err(response) = ensure_readlist_exists(&app, &readlist_id).await {
        return response;
    }

    match app
        .thumbnails
        .select_readlist(&readlist_id, &thumbnail_id)
        .await
    {
        Ok(_) => StatusCode::ACCEPTED.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(crate) async fn readlist_thumbnail_delete(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path((readlist_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    match app
        .thumbnails
        .delete_readlist(&readlist_id, &thumbnail_id)
        .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}
