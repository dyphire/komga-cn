use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};

use crate::app::placeholder_auth::{resolved_auth_user, user_is_admin};

use super::super::{SEARCH_OWNERSHIP_HEADER, SHADOW_JAVA_WRITER_MARKER};

pub(in crate::app::compat_runtime) async fn sse_events(headers: HeaderMap) -> Response {
    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let body = if user_is_admin(user) {
        concat!(
            ": connected\n\n",
            "event: TaskQueueStatus\n",
            "data: {\"count\":0,\"countByType\":{}}\n\n",
        )
    } else {
        ": connected\n\n"
    };

    let mut response = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/event-stream")],
        body,
    )
        .into_response();
    response.headers_mut().insert(
        HeaderName::from_static(SEARCH_OWNERSHIP_HEADER),
        HeaderValue::from_static(SHADOW_JAVA_WRITER_MARKER),
    );
    response
}
