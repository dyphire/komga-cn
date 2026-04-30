use super::shared::{
    load_book_thumbnail_source_bytes, parse_thumbnail_upload, response_from_thumbnail_bytes,
    response_from_thumbnail_jpeg_bytes, response_from_thumbnail_small_jpeg_bytes,
    thumbnail_dimensions, thumbnail_max_edge_from_setting,
};
use super::*;
use axum::extract::State;
use std::sync::Arc;

pub async fn book_thumbnail(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_auth(&headers, app.auth_db.db.database_file()).await {
        return response;
    }

    if let Ok(Some(media)) = load_persisted_book_media_from_services(&app, &book_id).await {
        let Some(user) = resolved_request_auth_user(&headers, app.auth_db.db.database_file()).await
        else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        if !user_can_access_book_media(&app, &book_id, &user, &media).await {
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

async fn book_thumbnail_opds_response(
    app: &HttpAppState,
    headers: &HeaderMap,
    book_id: &str,
) -> Response {
    if let Ok(Some(media)) = load_persisted_book_media_from_services(app, book_id).await {
        let Some(user) = resolved_request_auth_user(headers, app.auth_db.db.database_file()).await
        else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        if !user_can_access_book_media(app, book_id, &user, &media).await {
            return StatusCode::FORBIDDEN.into_response();
        }

        if let Some(bytes) = load_book_thumbnail_source_bytes(app, book_id, &media).await {
            return response_from_thumbnail_jpeg_bytes(headers, bytes);
        }

        return StatusCode::NOT_FOUND.into_response();
    }

    StatusCode::NOT_FOUND.into_response()
}

async fn book_thumbnail_opds_small_response(
    app: &HttpAppState,
    headers: &HeaderMap,
    book_id: &str,
    max_edge: u32,
) -> Response {
    if let Ok(Some(media)) = load_persisted_book_media_from_services(app, book_id).await {
        let Some(user) = resolved_request_auth_user(headers, app.auth_db.db.database_file()).await
        else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        if !user_can_access_book_media(app, book_id, &user, &media).await {
            return StatusCode::FORBIDDEN.into_response();
        }

        match load_selected_book_thumbnail_from_services(app, book_id).await {
            Ok(Some(thumbnail)) => {
                if thumbnail.thumbnail_type == "GENERATED" {
                    return response_from_thumbnail_bytes(
                        headers,
                        thumbnail.thumbnail,
                        thumbnail.media_type.as_str(),
                    );
                }

                return response_from_thumbnail_small_jpeg_bytes(
                    headers,
                    thumbnail.thumbnail,
                    thumbnail.media_type.as_str(),
                    max_edge,
                );
            }
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(error) => return internal_error_response(error),
        }
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn book_thumbnail_opds(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_auth(&headers, app.auth_db.db.database_file()).await {
        return response;
    }

    book_thumbnail_opds_response(&app, &headers, &book_id).await
}

pub async fn book_thumbnail_opds_small(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_auth(&headers, app.auth_db.db.database_file()).await {
        return response;
    }

    let settings = match app.services.server_settings.load_settings().await {
        Ok(settings) => settings,
        Err(error) => return internal_error_response(error),
    };

    book_thumbnail_opds_small_response(
        &app,
        &headers,
        &book_id,
        thumbnail_max_edge_from_setting(settings.thumbnail_size),
    )
    .await
}

pub async fn book_thumbnail_by_id(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path((book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_request_auth(&headers, app.auth_db.db.database_file()).await {
        return response;
    }

    if let Ok(Some(media)) = load_persisted_book_media_from_services(&app, &book_id).await {
        let Some(user) = resolved_request_auth_user(&headers, app.auth_db.db.database_file()).await
        else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        if !user_can_access_book_media(&app, &book_id, &user, &media).await {
            return StatusCode::FORBIDDEN.into_response();
        }
    }

    match load_book_thumbnail_by_id_from_services(&app, &thumbnail_id).await {
        Ok(Some(thumbnail)) => response_from_thumbnail_jpeg_bytes(&headers, thumbnail.thumbnail),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_thumbnails(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_request_auth(&headers, app.auth_db.db.database_file()).await {
        return response;
    }

    if let Ok(Some(media)) = load_persisted_book_media_from_services(&app, &book_id).await {
        let Some(user) = resolved_request_auth_user(&headers, app.auth_db.db.database_file()).await
        else {
            return StatusCode::UNAUTHORIZED.into_response();
        };
        if !user_can_access_book_media(&app, &book_id, &user, &media).await {
            return StatusCode::FORBIDDEN.into_response();
        }
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
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
    multipart: Multipart,
) -> Response {
    if let Some(response) = require_request_admin(&headers, app.auth_db.db.database_file()).await {
        return response;
    }

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
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path((_book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_request_admin(&headers, app.auth_db.db.database_file()).await {
        return response;
    }

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
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    Path((_book_id, thumbnail_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_request_admin(&headers, app.auth_db.db.database_file()).await {
        return response;
    }

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
