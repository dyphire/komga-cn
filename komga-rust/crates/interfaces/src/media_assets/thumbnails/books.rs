use super::shared::{
    parse_thumbnail_upload, response_from_thumbnail_bytes, response_from_thumbnail_jpeg_bytes,
    thumbnail_dimensions,
};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum_extra::extract::Multipart;
use komga_application::identity_access::AuthUser;

use crate::contracts::media_assets::BookThumbnailDto;
use crate::identity_access::auth::{Admin, Authenticated};
use crate::media_assets::types::PersistedBookMedia;
use crate::state::MediaAssetsState;

use super::super::access_control::user_can_access_book_media;
use super::super::http_helpers::internal_error_response;

async fn load_thumbnail_book_media(
    app: &MediaAssetsState,
    book_id: &str,
) -> Result<Option<PersistedBookMedia>, Response> {
    app.thumbnail_reader
        .book_media(book_id)
        .await
        .map_err(internal_error_response)
}

async fn ensure_thumbnail_book_access(
    app: &MediaAssetsState,
    book_id: &str,
    user: &AuthUser,
    media: &PersistedBookMedia,
) -> Result<(), Response> {
    match user_can_access_book_media(app.book_media_reader.as_ref(), book_id, user, media).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(StatusCode::FORBIDDEN.into_response()),
        Err(error) => Err(internal_error_response(error)),
    }
}

async fn ensure_book_thumbnail_belongs(
    app: &MediaAssetsState,
    book_id: &str,
    thumbnail_id: &str,
) -> Result<(), Response> {
    match app.thumbnail_reader.book_exists(book_id).await {
        Ok(true) => {}
        Ok(false) => return Err(StatusCode::NOT_FOUND.into_response()),
        Err(error) => return Err(internal_error_response(error)),
    }
    let belongs = app
        .thumbnail_reader
        .book_thumbnails(book_id)
        .await
        .map_err(internal_error_response)?
        .into_iter()
        .any(|thumbnail| thumbnail.id == thumbnail_id);
    if belongs {
        return Ok(());
    }

    match app
        .thumbnail_reader
        .book_thumbnail_by_id(thumbnail_id)
        .await
    {
        Ok(Some(_)) => Err(StatusCode::BAD_REQUEST.into_response()),
        Ok(None) => Err(StatusCode::NOT_FOUND.into_response()),
        Err(error) => Err(internal_error_response(error)),
    }
}

pub(crate) async fn book_thumbnail(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let media = match load_thumbnail_book_media(&app, &book_id).await {
        Ok(Some(media)) => media,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(response) => return response,
    };
    if let Err(response) = ensure_thumbnail_book_access(&app, &book_id, &user, &media).await {
        return response;
    }

    match app.thumbnail_reader.selected_book_thumbnail(&book_id).await {
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

pub(crate) async fn book_thumbnail_by_id(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path((book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    let media = match load_thumbnail_book_media(&app, &book_id).await {
        Ok(Some(media)) => media,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(response) => return response,
    };
    if let Err(response) = ensure_thumbnail_book_access(&app, &book_id, &user, &media).await {
        return response;
    }

    match app
        .thumbnail_reader
        .book_thumbnail_by_id(&thumbnail_id)
        .await
    {
        Ok(Some(thumbnail)) => {
            let owner_media = match load_thumbnail_book_media(&app, &thumbnail.owner_id).await {
                Ok(Some(media)) => media,
                Ok(None) => return StatusCode::NOT_FOUND.into_response(),
                Err(response) => return response,
            };
            if let Err(response) =
                ensure_thumbnail_book_access(&app, &thumbnail.owner_id, &user, &owner_media).await
            {
                return response;
            }
            response_from_thumbnail_jpeg_bytes(&headers, thumbnail.thumbnail)
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(crate) async fn book_thumbnails(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(book_id): Path<String>,
) -> Response {
    match load_thumbnail_book_media(&app, &book_id).await {
        Ok(Some(media)) => {
            if let Err(response) = ensure_thumbnail_book_access(&app, &book_id, &user, &media).await
            {
                return response;
            }
        }
        Ok(None) => {}
        Err(response) => return response,
    }

    match app.thumbnail_reader.book_thumbnails(&book_id).await {
        Ok(rows) => {
            if rows.is_empty() {
                match app.thumbnail_reader.book_exists(&book_id).await {
                    Ok(true) => {
                        return Json(Vec::<BookThumbnailDto>::new()).into_response();
                    }
                    Ok(false) => {}
                    Err(error) => return internal_error_response(error),
                }

                return StatusCode::NOT_FOUND.into_response();
            }

            Json(
                rows.iter()
                    .map(BookThumbnailDto::from_record)
                    .collect::<Vec<_>>(),
            )
            .into_response()
        }
        Err(error) => internal_error_response(error),
    }
}

pub(crate) async fn book_thumbnail_upload(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path(book_id): Path<String>,
    multipart: Multipart,
) -> Response {
    match app.thumbnail_reader.book_exists(&book_id).await {
        Ok(true) => {}
        Ok(false) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    }

    let upload = match parse_thumbnail_upload(multipart, "book").await {
        Ok(parsed) => parsed,
        Err(response) => return response,
    };
    let Some(dimensions) = thumbnail_dimensions(&upload.bytes) else {
        return StatusCode::UNSUPPORTED_MEDIA_TYPE.into_response();
    };

    match app
        .thumbnails
        .insert_book(
            &book_id,
            &upload.bytes,
            upload.media_type.as_str(),
            dimensions.width,
            dimensions.height,
            upload.selected,
        )
        .await
    {
        Ok(thumbnail) => Json(BookThumbnailDto::from_record(&thumbnail)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(crate) async fn book_thumbnail_select(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path((book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Err(response) = ensure_book_thumbnail_belongs(&app, &book_id, &thumbnail_id).await {
        return response;
    }

    match app.thumbnails.select_book(&thumbnail_id).await {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(crate) async fn book_thumbnail_delete(
    State(app): State<MediaAssetsState>,
    _: Admin,
    Path((book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Err(response) = ensure_book_thumbnail_belongs(&app, &book_id, &thumbnail_id).await {
        return response;
    }

    match app.thumbnails.delete_book(&thumbnail_id).await {
        Ok(true) => StatusCode::ACCEPTED.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) if error.to_string() == "only uploaded thumbnails can be deleted" => {
            StatusCode::BAD_REQUEST.into_response()
        }
        Err(error) => internal_error_response(error),
    }
}
