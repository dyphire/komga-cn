use axum::extract::Extension;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::collections::BTreeMap;

use crate::app::placeholder_auth::{resolved_auth_user, resolved_token, user_id, user_is_admin};

use super::super::{
    OperationalState, ReadProgressState, SEARCH_OWNERSHIP_HEADER, SHADOW_JAVA_WRITER_MARKER,
};

pub(in crate::app::compat_runtime) async fn sse_events(
    Extension(read_progress): Extension<ReadProgressState>,
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    if !state
        .sse
        .lock()
        .expect("sse state lock should not be poisoned")
        .accepting_connections
    {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let authenticated_user_id = user_id(&user);

    let mut body = String::from(": connected\n\n: heartbeat\n\n");

    if user_is_admin(&user) {
        let count_by_type = state
            .task_queue
            .lock()
            .expect("task queue state lock should not be poisoned")
            .count_by_simple_type();
        let count_by_type = kotlin_visible_task_type_counts(count_by_type);
        let total_count: usize = count_by_type.values().sum();
        body.push_str("event: TaskQueueStatus\n");
        body.push_str(
            format!(
                "data: {}\n\n",
                json!({
                    "count": total_count,
                    "countByType": count_by_type,
                })
            )
            .as_str(),
        );
    }

    if let Some(changed_book_id) = changed_read_progress_book_id(&read_progress, &headers) {
        body.push_str("event: ReadProgressChanged\n");
        body.push_str(
            format!(
                "data: {}\n\n",
                json!({
                    "bookId": changed_book_id,
                    "userId": authenticated_user_id,
                })
            )
            .as_str(),
        );
    }

    if let Some(user_id_header) = headers.get("x-komga-session-expired-user-id") {
        if let Ok(expired_user_id) = user_id_header.to_str()
            && expired_user_id == authenticated_user_id
        {
            body.push_str("event: SessionExpired\n");
            body.push_str(
                format!("data: {}\n\n", json!({ "userId": authenticated_user_id })).as_str(),
            );
        }
    }

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

fn changed_read_progress_book_id(
    read_progress: &ReadProgressState,
    headers: &HeaderMap,
) -> Option<String> {
    let token = resolved_token(headers);
    if token.is_empty() {
        return None;
    }

    read_progress
        .progress_by_token
        .lock()
        .expect("read-progress state lock should not be poisoned")
        .get(&token)
        .and_then(|progress| progress.keys().next().cloned())
}

fn kotlin_visible_task_type_counts(
    count_by_type: BTreeMap<String, usize>,
) -> BTreeMap<String, usize> {
    count_by_type
        .into_iter()
        .map(|(task_type, count)| (kotlin_visible_task_type_key(&task_type), count))
        .collect()
}

fn kotlin_visible_task_type_key(task_type: &str) -> String {
    match task_type {
        "SCAN_LIBRARY" => "scanLibrary".to_string(),
        "ANALYZE_BOOK" => "analyzeBook".to_string(),
        _ => task_type.to_string(),
    }
}
