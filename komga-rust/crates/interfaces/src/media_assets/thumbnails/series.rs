use super::shared::{
    load_series_thumbnail, parse_thumbnail_upload, response_from_thumbnail_bytes,
    thumbnail_dimensions,
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
use komga_application::discovery::resolve_persisted_series_id;
use komga_application::media_assets::ThumbnailType;

use super::super::access_control::{
    user_can_access_series_media, user_has_unrestricted_all_libraries,
};
use super::super::http_helpers::internal_error_response;

async fn ensure_series_exists(app: &MediaAssetsState, series_id: &str) -> Result<(), Response> {
    match app.thumbnail_reader.series_exists(series_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(StatusCode::NOT_FOUND.into_response()),
        Err(error) => Err(internal_error_response(error)),
    }
}

pub(crate) async fn series_thumbnail(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(series_id): Path<String>,
) -> Response {
    let resolved_series_id =
        resolve_persisted_series_id(app.series_id_resolver.as_ref(), &series_id).await;

    if let Err(response) = ensure_series_exists(&app, &resolved_series_id).await {
        return response;
    }

    match user_can_access_series_media(&app, &resolved_series_id, &user).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(error) => return internal_error_response(error),
    }

    match load_series_thumbnail(&app, &resolved_series_id).await {
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

pub(crate) async fn series_thumbnails(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(series_id): Path<String>,
) -> Response {
    let resolved_series_id =
        resolve_persisted_series_id(app.series_id_resolver.as_ref(), &series_id).await;
    if let Err(response) = ensure_series_exists(&app, &resolved_series_id).await {
        return response;
    }
    match user_can_access_series_media(&app, &resolved_series_id, &user).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(error) => return internal_error_response(error),
    }

    match app
        .thumbnail_reader
        .series_thumbnails(&resolved_series_id)
        .await
    {
        Ok(rows) => Json(
            rows.into_iter()
                .map(|row| {
                    json!({
                        "id": row.id,
                        "seriesId": row.series_id,
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
        .into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(crate) async fn series_thumbnail_by_id(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path((series_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    let resolved_series_id =
        resolve_persisted_series_id(app.series_id_resolver.as_ref(), &series_id).await;
    let unrestricted_all_libraries = user_has_unrestricted_all_libraries(&user);
    if !unrestricted_all_libraries {
        if let Err(response) = ensure_series_exists(&app, &resolved_series_id).await {
            return response;
        }
        match user_can_access_series_media(&app, &resolved_series_id, &user).await {
            Ok(true) => {}
            Ok(false) => return StatusCode::FORBIDDEN.into_response(),
            Err(error) => return internal_error_response(error),
        }
    }

    match app
        .thumbnail_reader
        .series_thumbnail_by_id(&thumbnail_id)
        .await
    {
        Ok(Some(thumbnail)) => {
            if !unrestricted_all_libraries {
                match user_can_access_series_media(&app, &thumbnail.owner_id, &user).await {
                    Ok(true) => {}
                    Ok(false) => return StatusCode::FORBIDDEN.into_response(),
                    Err(error) => return internal_error_response(error),
                }
            }
            asset_ok_response(
                thumbnail.media_type.as_str(),
                thumbnail.thumbnail,
                None,
                None,
            )
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(crate) async fn series_thumbnail_upload(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(series_id): Path<String>,
    multipart: Multipart,
) -> Response {
    let resolved_series_id =
        resolve_persisted_series_id(app.series_id_resolver.as_ref(), &series_id).await;
    if let Err(response) = ensure_series_exists(&app, &resolved_series_id).await {
        return response;
    }
    match app
        .thumbnail_reader
        .series_oneshot(&resolved_series_id)
        .await
    {
        Ok(Some(true)) => return StatusCode::BAD_REQUEST.into_response(),
        Ok(Some(false)) => {}
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }

    let upload = match parse_thumbnail_upload(multipart, "series").await {
        Ok(parsed) => parsed,
        Err(response) => return response,
    };
    let Some(dimensions) = thumbnail_dimensions(&upload.bytes) else {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    };

    match app
        .thumbnails
        .insert_series(
            &resolved_series_id,
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
            "seriesId": thumbnail.series_id,
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

pub(crate) async fn series_thumbnail_select(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path((series_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    let resolved_series_id =
        resolve_persisted_series_id(app.series_id_resolver.as_ref(), &series_id).await;
    if let Err(response) = ensure_series_exists(&app, &resolved_series_id).await {
        return response;
    }
    match app
        .thumbnail_reader
        .series_thumbnail_by_id(&thumbnail_id)
        .await
    {
        Ok(Some(thumbnail)) if thumbnail.owner_id != resolved_series_id => {
            return StatusCode::BAD_REQUEST.into_response();
        }
        Ok(Some(_)) => {}
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }
    match app
        .thumbnails
        .select_series(&resolved_series_id, &thumbnail_id)
        .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(crate) async fn series_thumbnail_delete(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path((series_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    let resolved_series_id =
        resolve_persisted_series_id(app.series_id_resolver.as_ref(), &series_id).await;
    if let Err(response) = ensure_series_exists(&app, &resolved_series_id).await {
        return response;
    }
    let thumbnail = match app
        .thumbnail_reader
        .series_thumbnail_by_id(&thumbnail_id)
        .await
    {
        Ok(Some(thumbnail)) if thumbnail.owner_id != resolved_series_id => {
            return StatusCode::BAD_REQUEST.into_response();
        }
        Ok(thumbnail) => thumbnail,
        Err(error) => return internal_error_response(error),
    };
    let Some(thumbnail) = thumbnail else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if thumbnail.thumbnail_type != ThumbnailType::UserUploaded {
        return StatusCode::BAD_REQUEST.into_response();
    }

    match app
        .thumbnails
        .delete_series(&resolved_series_id, &thumbnail_id)
        .await
    {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}
