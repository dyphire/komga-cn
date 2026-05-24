use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use komga_application::operational::SaveAnnouncementsReadError;

use crate::identity_access::auth::{Admin, user_id};
use crate::state::OperationalApiState;

pub(crate) async fn get_announcements(
    State(app): State<OperationalApiState>,
    admin: Admin,
) -> Response {
    match app
        .remote_feeds
        .announcements_for_user(user_id(&admin))
        .await
    {
        Ok(Some(feed)) => Json(feed).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

pub(crate) async fn put_announcements(
    State(app): State<OperationalApiState>,
    admin: Admin,
    body: Bytes,
) -> Response {
    match app
        .remote_feeds
        .save_announcements_read_from_body(user_id(&admin), &body)
        .await
    {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(SaveAnnouncementsReadError::InvalidPayload(_)) => {
            StatusCode::BAD_REQUEST.into_response()
        }
        Err(SaveAnnouncementsReadError::Persist(_)) => {
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

pub(crate) async fn get_releases(
    State(app): State<OperationalApiState>,
    _admin: Admin,
) -> Response {
    match app.remote_feeds.releases().await {
        Ok(releases) => Json(releases).into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
