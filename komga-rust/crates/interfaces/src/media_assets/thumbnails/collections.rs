use super::shared::{
    load_collection_mosaic_bytes, parse_thumbnail_upload, response_from_thumbnail_bytes,
    response_from_thumbnail_jpeg_bytes, set_one_hour_private_cache_control, thumbnail_dimensions,
};
use super::*;
use crate::identity_access::auth::{Admin, Authenticated};
use crate::state::MediaAssetsState;
use axum::extract::State;

pub async fn collection_thumbnail(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
) -> Response {
    match user_can_access_collection_media(app.root.as_ref(), &collection_id, &user).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }

    if !persisted_collection_exists_from_services(app.root.as_ref(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_persisted_collection_thumbnails_from_services(app.root.as_ref(), &collection_id)
        .await
    {
        Ok(rows) => {
            if let Some(thumbnail) = rows.first() {
                let mut response =
                    response_from_thumbnail_jpeg_bytes(&headers, thumbnail.thumbnail.clone());
                set_one_hour_private_cache_control(&mut response);
                return response;
            }

            match load_collection_mosaic_bytes(app.root.as_ref(), &collection_id).await {
                Ok(Some(bytes)) => {
                    let mut response = response_from_thumbnail_bytes(&headers, bytes, "image/jpeg");
                    set_one_hour_private_cache_control(&mut response);
                    response
                }
                Ok(None) => StatusCode::NOT_FOUND.into_response(),
                Err(error) => internal_error_response(error),
            }
        }
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_thumbnails(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(collection_id): Path<String>,
) -> Response {
    match user_can_access_collection_media(app.root.as_ref(), &collection_id, &user).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }

    if !persisted_collection_exists_from_services(app.root.as_ref(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_persisted_collection_thumbnails_from_services(app.root.as_ref(), &collection_id)
        .await
    {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|row| {
                    json!({
                        "id": row.id,
                        "collectionId": row.collection_id,
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

pub async fn collection_thumbnail_by_id(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path((collection_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    match user_can_access_collection_media(app.root.as_ref(), &collection_id, &user).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }

    if !persisted_collection_exists_from_services(app.root.as_ref(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_persisted_collection_thumbnails_from_services(app.root.as_ref(), &collection_id)
        .await
    {
        Ok(rows) => {
            if let Some(thumbnail) = rows.into_iter().find(|row| row.id == thumbnail_id) {
                asset_ok_response(
                    thumbnail.media_type.as_str(),
                    thumbnail.thumbnail,
                    None,
                    None,
                )
            } else {
                StatusCode::NOT_FOUND.into_response()
            }
        }
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_thumbnail_upload(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(collection_id): Path<String>,
    multipart: Multipart,
) -> Response {
    if !persisted_collection_exists_from_services(app.root.as_ref(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let (thumbnail_bytes, media_type, selected) =
        match parse_thumbnail_upload(multipart, "collection").await {
            Ok(parsed) => parsed,
            Err(response) => return response,
        };
    let Some((width, height)) = thumbnail_dimensions(&thumbnail_bytes) else {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    };

    match insert_collection_thumbnail_from_services(
        app.root.as_ref(),
        &collection_id,
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
            "collectionId": thumbnail.collection_id,
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

pub async fn collection_thumbnail_select(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path((collection_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if !persisted_collection_exists_from_services(app.root.as_ref(), &collection_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    match select_collection_thumbnail_from_services(app.root.as_ref(), &thumbnail_id).await {
        Ok(_) => StatusCode::ACCEPTED.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_thumbnail_delete(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path((collection_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    match delete_collection_thumbnail_from_services(
        app.root.as_ref(),
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
