use super::shared::{
    parse_thumbnail_upload, response_from_thumbnail_jpeg_bytes, thumbnail_dimensions,
};
use super::*;
use crate::identity_access::auth::{Admin, Authenticated};
use crate::state::MediaAssetsState;
use axum::extract::State;

pub async fn book_thumbnail(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Ok(Some(media)) = load_persisted_book_media_from_services(&app, &book_id).await {
        if !user_can_access_book_media(app.media_assets.as_ref(), &book_id, &user, &media).await {
            return StatusCode::FORBIDDEN.into_response();
        }

        match load_selected_book_thumbnail_from_services(&app, &book_id).await {
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

        return StatusCode::NOT_FOUND.into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn book_thumbnail_by_id(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path((book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Ok(Some(media)) = load_persisted_book_media_from_services(&app, &book_id).await
        && !user_can_access_book_media(app.media_assets.as_ref(), &book_id, &user, &media).await
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    match load_book_thumbnail_by_id_from_services(&app, &thumbnail_id).await {
        Ok(Some(thumbnail)) => response_from_thumbnail_jpeg_bytes(&headers, thumbnail.thumbnail),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_thumbnails(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(book_id): Path<String>,
) -> Response {
    if let Ok(Some(media)) = load_persisted_book_media_from_services(&app, &book_id).await
        && !user_can_access_book_media(app.media_assets.as_ref(), &book_id, &user, &media).await
    {
        return StatusCode::FORBIDDEN.into_response();
    }

    match load_persisted_book_thumbnails_from_services(&app, &book_id).await {
        Ok(rows) => {
            if rows.is_empty() {
                if persisted_book_exists_from_services(&app, &book_id)
                    .await
                    .unwrap_or(false)
                {
                    return Json(json!([])).into_response();
                }

                return StatusCode::NOT_FOUND.into_response();
            }

            let mut response = Json(
                rows.into_iter()
                    .map(|row| {
                        json!({
                            "id": row.id,
                            "bookId": row.book_id,
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
            mark_runtime_owned(&mut response);
            response
        }
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_thumbnail_upload(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(book_id): Path<String>,
    multipart: Multipart,
) -> Response {
    if !persisted_book_exists_from_services(&app, &book_id)
        .await
        .unwrap_or(false)
    {
        return StatusCode::NOT_FOUND.into_response();
    }

    let (thumbnail_bytes, media_type, selected) =
        match parse_thumbnail_upload(multipart, "book").await {
            Ok(parsed) => parsed,
            Err(response) => return response,
        };
    let Some((width, height)) = thumbnail_dimensions(&thumbnail_bytes) else {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    };

    match insert_book_thumbnail_from_services(
        &app,
        &book_id,
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
            "bookId": thumbnail.book_id,
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

pub async fn book_thumbnail_select(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path((_book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    match select_book_thumbnail_from_services(&app, &thumbnail_id).await {
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
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path((_book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    match delete_book_thumbnail_from_services(&app, &thumbnail_id).await {
        Ok(true) => {
            let mut response = StatusCode::ACCEPTED.into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) if error == "only uploaded thumbnails can be deleted" => {
            StatusCode::BAD_REQUEST.into_response()
        }
        Err(error) => internal_error_response(error),
    }
}
