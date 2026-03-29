use axum::extract::Extension;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures_util::stream::{self, StreamExt};
use serde_json::json;
use std::collections::BTreeMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;

use crate::http::identity_access::auth::{
    resolved_auth_user, resolved_token, user_id, user_is_admin,
};

use super::super::super::{
    OperationalState, PERSISTED_OWNERSHIP_MARKER, ReadProgressState, SEARCH_OWNERSHIP_HEADER,
};
use super::diff::{append_snapshot_events, sse_event};
use super::snapshot::load_sse_snapshot;

pub(crate) async fn sse_events(
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
    let authenticated_user_id = user_id(&user).to_string();
    let token = resolved_token(&headers);
    let admin = user_is_admin(&user);
    let session_expired_user_id = headers
        .get("x-komga-session-expired-user-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let snapshot_state = Arc::new(AsyncMutex::new(
        load_sse_snapshot(
            state.runtime.database_file.as_path(),
            authenticated_user_id.as_str(),
        )
        .await,
    ));

    let stream = IntervalStream::new(interval(Duration::from_secs(5)))
        .then(move |_| {
            let state = state.clone();
            let read_progress = read_progress.clone();
            let token = token.clone();
            let authenticated_user_id = authenticated_user_id.clone();
            let session_expired_user_id = session_expired_user_id.clone();
            let snapshot_state = snapshot_state.clone();

            async move {
                let mut events = Vec::<Event>::new();

                if admin {
                    let count_by_type = (state.count_task_queue_by_type)();
                    let count_by_type = kotlin_visible_task_type_counts(count_by_type);
                    let total_count: usize = count_by_type.values().sum();
                    events.push(sse_event(
                        "TaskQueueStatus",
                        json!({
                            "count": total_count,
                            "countByType": count_by_type,
                        }),
                    ));

                    let import_events = {
                        let mut sse_state = state
                            .sse
                            .lock()
                            .expect("sse state lock should not be poisoned");
                        std::mem::take(&mut sse_state.book_import_events)
                    };
                    for event in import_events {
                        events.push(sse_event(
                            "BookImported",
                            json!({
                                "bookId": event.book_id,
                                "sourceFile": event.source_file,
                                "success": event.success,
                                "message": event.message,
                            }),
                        ));
                    }
                }

                if let Some(changed_book_id) = changed_read_progress_book_id(&read_progress, &token)
                {
                    events.push(sse_event(
                        "ReadProgressChanged",
                        json!({
                            "bookId": changed_book_id,
                            "userId": authenticated_user_id.as_str(),
                        }),
                    ));
                }

                {
                    let mut previous_snapshot = snapshot_state.lock().await;
                    let current_snapshot = load_sse_snapshot(
                        state.runtime.database_file.as_path(),
                        authenticated_user_id.as_str(),
                    )
                    .await;
                    append_snapshot_events(
                        &mut events,
                        &previous_snapshot,
                        &current_snapshot,
                        authenticated_user_id.as_str(),
                    );
                    *previous_snapshot = current_snapshot;
                }

                if session_expired_user_id
                    .as_deref()
                    .is_some_and(|expired_user_id| {
                        expired_user_id == authenticated_user_id.as_str()
                    })
                {
                    events.push(sse_event(
                        "SessionExpired",
                        json!({ "userId": authenticated_user_id.as_str() }),
                    ));
                }

                if events.is_empty() {
                    events.push(Event::default().comment("heartbeat"));
                }

                events
                    .into_iter()
                    .map(Ok::<Event, Infallible>)
                    .collect::<Vec<_>>()
            }
        })
        .flat_map(stream::iter);

    let mut response = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/event-stream")],
        Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))),
    )
        .into_response();
    response.headers_mut().insert(
        HeaderName::from_static(SEARCH_OWNERSHIP_HEADER),
        HeaderValue::from_static(PERSISTED_OWNERSHIP_MARKER),
    );
    response
}

fn changed_read_progress_book_id(read_progress: &ReadProgressState, token: &str) -> Option<String> {
    if token.is_empty() {
        return None;
    }

    read_progress
        .progress_by_token
        .lock()
        .expect("read-progress state lock should not be poisoned")
        .get(token)
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
