use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use futures_util::stream;
use komga_application::runtime_sse::{
    RuntimeSseEvent, RuntimeSseEventSink, RuntimeSseEventSubscription,
};
use serde_json::json;
use std::collections::{BTreeMap, VecDeque};
use std::convert::Infallible;
use std::time::Duration;
use tokio::time::{Instant, MissedTickBehavior, interval_at};

use crate::identity_access::auth::resolved_auth_user;
use crate::state::OperationalApiState;
use komga_application::identity_access::{user_id, user_is_admin};

fn sse_event(name: &str, payload: serde_json::Value) -> Event {
    Event::default()
        .event(name)
        .data(serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) async fn sse_events(
    State(app): State<OperationalApiState>,
    headers: HeaderMap,
) -> Response {
    let state = &app.operational;
    if !state.sse.is_accepting() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let user = match resolved_auth_user(&app.identity, &headers) {
        Ok(Some(user)) => user,
        Ok(None) => return StatusCode::UNAUTHORIZED.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let authenticated_user_id = user_id(&user).to_string();
    let admin = user_is_admin(&user);
    let last_runtime_event_id = app.runtime_events.current_cursor();
    let runtime_event_updates = app.runtime_events.subscribe();

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

    let mut pending_events = VecDeque::new();
    pending_events.push_back(Event::default().comment("heartbeat"));

    let stream = stream::unfold(
        SseStreamState {
            admin,
            authenticated_user_id,
            heartbeat_interval,
            task_interval,
            last_runtime_event_id,
            pending_events,
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
                        if !changed {
                            return None;
                        }
                        poll_runtime_events(&mut stream_state).await;
                    }
                }
            }
        },
    );

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/event-stream")],
        Sse::new(stream),
    )
        .into_response()
}

pub(crate) fn register_session_expired_event(
    runtime_events: &dyn RuntimeSseEventSink,
    user_id: &str,
) {
    runtime_events.register(RuntimeSseEvent::SessionExpired {
        user_id: user_id.to_string(),
    });
}

struct SseStreamState {
    admin: bool,
    authenticated_user_id: String,
    heartbeat_interval: tokio::time::Interval,
    task_interval: tokio::time::Interval,
    last_runtime_event_id: u64,
    pending_events: VecDeque<Event>,
    runtime_event_updates: Box<dyn RuntimeSseEventSubscription>,
    app: OperationalApiState,
}

async fn poll_runtime_events(stream_state: &mut SseStreamState) {
    let runtime_events = stream_state.app.runtime_events.pending_events(
        stream_state.last_runtime_event_id,
        stream_state.authenticated_user_id.as_str(),
        stream_state.admin,
    );
    stream_state.last_runtime_event_id = runtime_events.current_cursor;
    stream_state.pending_events.extend(
        runtime_events
            .events
            .into_iter()
            .map(|event| runtime_sse_event(&event.event)),
    );
}

fn runtime_sse_event(event: &RuntimeSseEvent) -> Event {
    match event {
        RuntimeSseEvent::LibraryAdded { library_id } => {
            sse_event("LibraryAdded", json!({ "libraryId": library_id }))
        }
        RuntimeSseEvent::LibraryChanged { library_id } => {
            sse_event("LibraryChanged", json!({ "libraryId": library_id }))
        }
        RuntimeSseEvent::LibraryDeleted { library_id } => {
            sse_event("LibraryDeleted", json!({ "libraryId": library_id }))
        }
        RuntimeSseEvent::SeriesAdded {
            series_id,
            library_id,
        } => sse_event(
            "SeriesAdded",
            json!({ "seriesId": series_id, "libraryId": library_id }),
        ),
        RuntimeSseEvent::SeriesChanged {
            series_id,
            library_id,
        } => sse_event(
            "SeriesChanged",
            json!({ "seriesId": series_id, "libraryId": library_id }),
        ),
        RuntimeSseEvent::BookAdded {
            book_id,
            series_id,
            library_id,
        } => sse_event(
            "BookAdded",
            json!({ "bookId": book_id, "seriesId": series_id, "libraryId": library_id }),
        ),
        RuntimeSseEvent::BookChanged {
            book_id,
            series_id,
            library_id,
        } => sse_event(
            "BookChanged",
            json!({ "bookId": book_id, "seriesId": series_id, "libraryId": library_id }),
        ),
        RuntimeSseEvent::BookImported {
            book_id,
            source_file,
            success,
            message,
        } => sse_event(
            "BookImported",
            json!({
                "bookId": book_id,
                "sourceFile": source_file,
                "success": success,
                "message": message,
            }),
        ),
        RuntimeSseEvent::CollectionAdded {
            collection_id,
            series_ids,
        } => sse_event(
            "CollectionAdded",
            json!({ "collectionId": collection_id, "seriesIds": series_ids }),
        ),
        RuntimeSseEvent::CollectionChanged {
            collection_id,
            series_ids,
        } => sse_event(
            "CollectionChanged",
            json!({ "collectionId": collection_id, "seriesIds": series_ids }),
        ),
        RuntimeSseEvent::CollectionDeleted {
            collection_id,
            series_ids,
        } => sse_event(
            "CollectionDeleted",
            json!({ "collectionId": collection_id, "seriesIds": series_ids }),
        ),
        RuntimeSseEvent::ReadListAdded {
            readlist_id,
            book_ids,
        } => sse_event(
            "ReadListAdded",
            json!({ "readListId": readlist_id, "bookIds": book_ids }),
        ),
        RuntimeSseEvent::ReadListChanged {
            readlist_id,
            book_ids,
        } => sse_event(
            "ReadListChanged",
            json!({ "readListId": readlist_id, "bookIds": book_ids }),
        ),
        RuntimeSseEvent::ReadListDeleted {
            readlist_id,
            book_ids,
        } => sse_event(
            "ReadListDeleted",
            json!({ "readListId": readlist_id, "bookIds": book_ids }),
        ),
        RuntimeSseEvent::ReadProgressChanged { book_id, user_id } => sse_event(
            "ReadProgressChanged",
            json!({ "bookId": book_id, "userId": user_id }),
        ),
        RuntimeSseEvent::ReadProgressDeleted { book_id, user_id } => sse_event(
            "ReadProgressDeleted",
            json!({ "bookId": book_id, "userId": user_id }),
        ),
        RuntimeSseEvent::ReadProgressSeriesChanged { series_id, user_id } => sse_event(
            "ReadProgressSeriesChanged",
            json!({ "seriesId": series_id, "userId": user_id }),
        ),
        RuntimeSseEvent::ReadProgressSeriesDeleted { series_id, user_id } => sse_event(
            "ReadProgressSeriesDeleted",
            json!({ "seriesId": series_id, "userId": user_id }),
        ),
        RuntimeSseEvent::ThumbnailBookAdded {
            book_id,
            series_id,
            selected,
        } => sse_event(
            "ThumbnailBookAdded",
            json!({ "bookId": book_id, "seriesId": series_id, "selected": selected }),
        ),
        RuntimeSseEvent::ThumbnailBookDeleted {
            book_id,
            series_id,
            selected,
        } => sse_event(
            "ThumbnailBookDeleted",
            json!({ "bookId": book_id, "seriesId": series_id, "selected": selected }),
        ),
        RuntimeSseEvent::ThumbnailSeriesAdded {
            series_id,
            selected,
        } => sse_event(
            "ThumbnailSeriesAdded",
            json!({ "seriesId": series_id, "selected": selected }),
        ),
        RuntimeSseEvent::ThumbnailSeriesDeleted {
            series_id,
            selected,
        } => sse_event(
            "ThumbnailSeriesDeleted",
            json!({ "seriesId": series_id, "selected": selected }),
        ),
        RuntimeSseEvent::ThumbnailReadListAdded {
            readlist_id,
            selected,
        } => sse_event(
            "ThumbnailReadListAdded",
            json!({ "readListId": readlist_id, "selected": selected }),
        ),
        RuntimeSseEvent::ThumbnailReadListDeleted {
            readlist_id,
            selected,
        } => sse_event(
            "ThumbnailReadListDeleted",
            json!({ "readListId": readlist_id, "selected": selected }),
        ),
        RuntimeSseEvent::ThumbnailCollectionAdded {
            collection_id,
            selected,
        } => sse_event(
            "ThumbnailSeriesCollectionAdded",
            json!({ "collectionId": collection_id, "selected": selected }),
        ),
        RuntimeSseEvent::ThumbnailCollectionDeleted {
            collection_id,
            selected,
        } => sse_event(
            "ThumbnailSeriesCollectionDeleted",
            json!({ "collectionId": collection_id, "selected": selected }),
        ),
        RuntimeSseEvent::SessionExpired { user_id } => {
            sse_event("SessionExpired", json!({ "userId": user_id }))
        }
    }
}

async fn task_queue_status_event(app: &OperationalApiState) -> Event {
    let status = match app.task_queue.queue.status().await {
        Ok(status) => status,
        Err(error) => {
            return sse_event(
                "TaskQueueStatus",
                json!({
                    "count": 0,
                    "countByType": {},
                    "error": error,
                }),
            );
        }
    };
    let count_by_type = kotlin_visible_task_type_counts(status.counts);
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
        "ScanLibrary" => "scanLibrary".to_string(),
        "AnalyzeBook" => "analyzeBook".to_string(),
        _ => task_type.to_string(),
    }
}
