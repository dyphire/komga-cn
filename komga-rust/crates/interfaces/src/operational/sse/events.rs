use axum::extract::Extension;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use futures_util::stream;
use komga_application::runtime_sse::{
    current_runtime_sse_event_cursor, pending_runtime_sse_events, register_runtime_sse_event,
    subscribe_runtime_sse_event_updates,
};
use serde_json::json;
use std::collections::{BTreeMap, VecDeque};
use std::convert::Infallible;
use std::time::Duration;
use tokio::time::{Instant, MissedTickBehavior, interval_at};

use crate::identity_access::auth::{resolved_auth_user, user_id, user_is_admin};
use crate::state::HttpAppState;

use super::super::super::{PERSISTED_OWNERSHIP_MARKER, SEARCH_OWNERSHIP_HEADER};

fn sse_event(name: &str, payload: serde_json::Value) -> Event {
    Event::default()
        .event(name)
        .data(serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn sse_events(
    Extension(app): Extension<HttpAppState>,
    headers: HeaderMap,
) -> Response {
    let state = &app.operational;
    if !state
        .sse
        .lock()
        .expect("sse state lock should not be poisoned")
        .accepting_connections
    {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let authenticated_user_id = user_id(&user).to_string();
    let admin = user_is_admin(&user);
    let last_runtime_event_id = current_runtime_sse_event_cursor();
    let runtime_event_updates = subscribe_runtime_sse_event_updates();

    let mut heartbeat_interval = interval_at(
        Instant::now() + Duration::from_secs(15),
        Duration::from_secs(15),
    );
    heartbeat_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let mut task_interval = interval_at(
        Instant::now() + Duration::from_secs(10),
        Duration::from_secs(10),
    );
    task_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    let stream = stream::unfold(
        SseStreamState {
            admin,
            authenticated_user_id,
            heartbeat_interval,
            task_interval,
            last_runtime_event_id,
            pending_events: VecDeque::new(),
            runtime_event_updates,
            app,
        },
        |mut stream_state| async move {
            loop {
                if let Some(event) = stream_state.pending_events.pop_front() {
                    return Some((Ok::<Event, Infallible>(event), stream_state));
                }

                tokio::select! {
                    _ = stream_state.heartbeat_interval.tick() => {
                        return Some((Ok::<Event, Infallible>(Event::default().comment("heartbeat")), stream_state));
                    }
                    _ = stream_state.task_interval.tick(), if stream_state.admin => {
                        return Some((Ok::<Event, Infallible>(task_queue_status_event(&stream_state.app).await), stream_state));
                    }
                    changed = stream_state.runtime_event_updates.changed() => {
                        if changed.is_err() {
                            return None;
                        }
                        poll_runtime_events(&mut stream_state).await;
                    }
                }
            }
        },
    );

    let mut response = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/event-stream")],
        Sse::new(stream),
    )
        .into_response();
    response.headers_mut().insert(
        HeaderName::from_static(SEARCH_OWNERSHIP_HEADER),
        HeaderValue::from_static(PERSISTED_OWNERSHIP_MARKER),
    );
    response
}

pub(crate) fn register_session_expired_event(user_id: &str) {
    register_runtime_sse_event(
        "SessionExpired",
        json!({ "userId": user_id }),
        false,
        Some(user_id.to_string()),
    );
}

struct SseStreamState {
    admin: bool,
    authenticated_user_id: String,
    heartbeat_interval: tokio::time::Interval,
    task_interval: tokio::time::Interval,
    last_runtime_event_id: u64,
    pending_events: VecDeque<Event>,
    runtime_event_updates: tokio::sync::watch::Receiver<u64>,
    app: HttpAppState,
}

async fn poll_runtime_events(stream_state: &mut SseStreamState) {
    let (last_runtime_event_id, runtime_events) = pending_runtime_sse_events(
        stream_state.last_runtime_event_id,
        stream_state.authenticated_user_id.as_str(),
        stream_state.admin,
    );
    stream_state.last_runtime_event_id = last_runtime_event_id;
    stream_state.pending_events.extend(
        runtime_events
            .into_iter()
            .map(|event| sse_event(event.name.as_str(), event.payload)),
    );
}

async fn task_queue_status_event(app: &HttpAppState) -> Event {
    let count_by_type =
        kotlin_visible_task_type_counts(app.services.task_queue.count_task_queue_by_type().await);
    let total_count: usize = count_by_type.values().sum();
    sse_event(
        "TaskQueueStatus",
        json!({
            "count": total_count,
            "countByType": count_by_type,
        }),
    )
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
