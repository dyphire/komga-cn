use axum::extract::Extension;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use futures_util::stream;
use komga_application::media_assets::{
    current_runtime_book_import_event_cursor, pending_runtime_book_import_events,
};
use serde_json::json;
use std::collections::{BTreeMap, VecDeque};
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};
use tokio::time::{Instant, MissedTickBehavior, interval_at};

use crate::http::helpers::api_file_path;
use crate::http::identity_access::auth::{resolved_auth_user, user_id, user_is_admin};

use super::super::super::{OperationalState, PERSISTED_OWNERSHIP_MARKER, SEARCH_OWNERSHIP_HEADER};
use super::diff::{append_snapshot_events, sse_event};
use super::snapshot::{SseSnapshot, load_sse_snapshot};

pub(crate) async fn sse_events(
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
    let admin = user_is_admin(&user);
    let database_file = state.runtime.database_file.clone();
    let previous_snapshot =
        load_sse_snapshot(database_file.as_path(), authenticated_user_id.as_str()).await;
    let last_files_fingerprint = database_files_fingerprint(database_file.as_path());
    let last_session_expired_event_id = current_session_expired_event_cursor(&state);
    let last_book_import_event_id = current_runtime_book_import_event_cursor();

    let mut change_probe_interval = interval_at(
        Instant::now() + Duration::from_millis(250),
        Duration::from_millis(250),
    );
    change_probe_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

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
            database_file,
            change_probe_interval,
            heartbeat_interval,
            task_interval,
            last_files_fingerprint,
            last_session_expired_event_id,
            last_book_import_event_id,
            pending_events: VecDeque::new(),
            previous_snapshot,
            state,
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
                        return Some((Ok::<Event, Infallible>(task_queue_status_event(&stream_state.state)), stream_state));
                    }
                    _ = stream_state.change_probe_interval.tick() => {
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

pub(crate) fn register_session_expired_event(state: &OperationalState, user_id: &str) {
    let mut sse_state = state
        .sse
        .lock()
        .expect("sse state lock should not be poisoned");
    sse_state.next_session_expired_event_id += 1;
    let event_id = sse_state.next_session_expired_event_id;
    sse_state
        .session_expired_events
        .push(crate::http::state::SessionExpiredSseEvent {
            id: event_id,
            user_id: user_id.to_string(),
        });
}

struct SseStreamState {
    admin: bool,
    authenticated_user_id: String,
    database_file: PathBuf,
    change_probe_interval: tokio::time::Interval,
    heartbeat_interval: tokio::time::Interval,
    task_interval: tokio::time::Interval,
    last_files_fingerprint: Vec<TrackedFileFingerprint>,
    last_session_expired_event_id: u64,
    last_book_import_event_id: u64,
    pending_events: VecDeque<Event>,
    previous_snapshot: SseSnapshot,
    state: OperationalState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TrackedFileFingerprint {
    path: String,
    len: u64,
    modified_nanos: u128,
}

async fn poll_runtime_events(stream_state: &mut SseStreamState) {
    let (last_session_expired_event_id, session_events) = pending_session_expired_events(
        &stream_state.state,
        stream_state.authenticated_user_id.as_str(),
        stream_state.last_session_expired_event_id,
    );
    stream_state.last_session_expired_event_id = last_session_expired_event_id;
    stream_state.pending_events.extend(session_events);

    if stream_state.admin {
        let (last_book_import_event_id, book_import_events) =
            pending_book_import_events(stream_state.last_book_import_event_id);
        stream_state.last_book_import_event_id = last_book_import_event_id;
        stream_state.pending_events.extend(book_import_events);
    }

    let current_fingerprint = database_files_fingerprint(stream_state.database_file.as_path());
    if current_fingerprint == stream_state.last_files_fingerprint {
        return;
    }

    let current_snapshot = load_sse_snapshot(
        stream_state.database_file.as_path(),
        stream_state.authenticated_user_id.as_str(),
    )
    .await;
    let mut events = Vec::new();
    append_snapshot_events(
        &mut events,
        &stream_state.previous_snapshot,
        &current_snapshot,
        stream_state.authenticated_user_id.as_str(),
    );

    stream_state.previous_snapshot = current_snapshot;
    stream_state.last_files_fingerprint = current_fingerprint;
    stream_state.pending_events.extend(events);
}

fn current_session_expired_event_cursor(state: &OperationalState) -> u64 {
    state
        .sse
        .lock()
        .expect("sse state lock should not be poisoned")
        .next_session_expired_event_id
}

fn pending_book_import_events(last_seen_event_id: u64) -> (u64, Vec<Event>) {
    let (current_cursor, events) = pending_runtime_book_import_events(last_seen_event_id);
    let mapped = events
        .into_iter()
        .map(|event| {
            sse_event(
                "BookImported",
                json!({
                    "bookId": event.book_id,
                    "sourceFile": api_file_path(&event.source_file),
                    "success": event.success,
                    "message": event.message,
                }),
            )
        })
        .collect::<Vec<_>>();

    (current_cursor, mapped)
}

fn pending_session_expired_events(
    state: &OperationalState,
    user_id: &str,
    last_seen_event_id: u64,
) -> (u64, Vec<Event>) {
    let sse_state = state
        .sse
        .lock()
        .expect("sse state lock should not be poisoned");
    let mut newest_event_id = last_seen_event_id;
    let mut events = Vec::new();

    for event in &sse_state.session_expired_events {
        if event.id <= last_seen_event_id {
            continue;
        }

        newest_event_id = newest_event_id.max(event.id);
        if event.user_id == user_id {
            events.push(sse_event(
                "SessionExpired",
                json!({ "userId": event.user_id }),
            ));
        }
    }

    (newest_event_id, events)
}

fn task_queue_status_event(state: &OperationalState) -> Event {
    let count_by_type = kotlin_visible_task_type_counts((state.count_task_queue_by_type)());
    let total_count: usize = count_by_type.values().sum();
    sse_event(
        "TaskQueueStatus",
        json!({
            "count": total_count,
            "countByType": count_by_type,
        }),
    )
}

fn database_files_fingerprint(database_file: &Path) -> Vec<TrackedFileFingerprint> {
    tracked_database_paths(database_file)
        .into_iter()
        .filter_map(|path| {
            let metadata = std::fs::metadata(&path).ok()?;
            let modified_nanos = metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_nanos())
                .unwrap_or_default();
            Some(TrackedFileFingerprint {
                path: path.to_string_lossy().to_string(),
                len: metadata.len(),
                modified_nanos,
            })
        })
        .collect()
}

fn tracked_database_paths(database_file: &Path) -> Vec<PathBuf> {
    let base = database_file.to_path_buf();
    let base_name = base.to_string_lossy().to_string();
    [
        base,
        PathBuf::from(format!("{base_name}-wal")),
        PathBuf::from(format!("{base_name}-shm")),
        PathBuf::from(format!("{base_name}-journal")),
    ]
    .into_iter()
    .collect()
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
