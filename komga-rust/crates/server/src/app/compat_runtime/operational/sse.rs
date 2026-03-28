use axum::extract::Extension;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use futures_util::stream::{self, StreamExt};
use komga_persistence::sqlite::connect_pool;
use serde_json::json;
use sqlx::Row;
use std::collections::{BTreeMap, HashMap};
use std::convert::Infallible;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::interval;
use tokio_stream::wrappers::IntervalStream;

use crate::app::runtime_auth::{resolved_auth_user, resolved_token, user_id, user_is_admin};

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
                    let count_by_type = state
                        .task_queue
                        .lock()
                        .expect("task queue state lock should not be poisoned")
                        .count_by_simple_type();
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
        HeaderValue::from_static(SHADOW_JAVA_WRITER_MARKER),
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

#[derive(Clone, Default)]
struct SseSnapshot {
    libraries: HashMap<String, LibrarySnapshot>,
    series: HashMap<String, SeriesSnapshot>,
    books: HashMap<String, BookSnapshot>,
    readlists: HashMap<String, ReadListSnapshot>,
    collections: HashMap<String, CollectionSnapshot>,
    thumbnails_book: HashMap<String, ThumbnailBookSnapshot>,
    thumbnails_series: HashMap<String, ThumbnailSnapshot>,
    thumbnails_collection: HashMap<String, ThumbnailSnapshot>,
    thumbnails_readlist: HashMap<String, ThumbnailSnapshot>,
    read_progress: HashMap<String, String>,
    read_progress_series: HashMap<String, String>,
}

#[derive(Clone, Eq, PartialEq)]
struct LibrarySnapshot {
    last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
struct SeriesSnapshot {
    library_id: String,
    last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
struct BookSnapshot {
    series_id: String,
    library_id: String,
    last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
struct ReadListSnapshot {
    book_ids: Vec<String>,
    last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
struct CollectionSnapshot {
    series_ids: Vec<String>,
    last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
struct ThumbnailBookSnapshot {
    series_id: String,
    selected: bool,
    last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
struct ThumbnailSnapshot {
    selected: bool,
    last_modified: String,
}

fn sse_event(name: &str, payload: serde_json::Value) -> Event {
    Event::default()
        .event(name)
        .data(serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()))
}

fn append_snapshot_events(
    events: &mut Vec<Event>,
    previous: &SseSnapshot,
    current: &SseSnapshot,
    user_id: &str,
) {
    append_libraries_events(events, &previous.libraries, &current.libraries);
    append_series_events(events, &previous.series, &current.series);
    append_books_events(events, &previous.books, &current.books);
    append_readlists_events(events, &previous.readlists, &current.readlists);
    append_collections_events(events, &previous.collections, &current.collections);
    append_read_progress_events(
        events,
        &previous.read_progress,
        &current.read_progress,
        user_id,
    );
    append_read_progress_series_events(
        events,
        &previous.read_progress_series,
        &current.read_progress_series,
        user_id,
    );
    append_thumbnail_book_events(events, &previous.thumbnails_book, &current.thumbnails_book);
    append_thumbnail_events(
        events,
        &previous.thumbnails_series,
        &current.thumbnails_series,
        "ThumbnailSeriesAdded",
        "ThumbnailSeriesDeleted",
        "seriesId",
    );
    append_thumbnail_events(
        events,
        &previous.thumbnails_collection,
        &current.thumbnails_collection,
        "ThumbnailSeriesCollectionAdded",
        "ThumbnailSeriesCollectionDeleted",
        "collectionId",
    );
    append_thumbnail_events(
        events,
        &previous.thumbnails_readlist,
        &current.thumbnails_readlist,
        "ThumbnailReadListAdded",
        "ThumbnailReadListDeleted",
        "readListId",
    );
}

fn append_libraries_events(
    events: &mut Vec<Event>,
    previous: &HashMap<String, LibrarySnapshot>,
    current: &HashMap<String, LibrarySnapshot>,
) {
    for (library_id, current_snapshot) in current {
        match previous.get(library_id) {
            None => events.push(sse_event(
                "LibraryAdded",
                json!({ "libraryId": library_id }),
            )),
            Some(previous_snapshot) if previous_snapshot != current_snapshot => events.push(
                sse_event("LibraryChanged", json!({ "libraryId": library_id })),
            ),
            _ => {}
        }
    }
    for library_id in previous.keys() {
        if !current.contains_key(library_id) {
            events.push(sse_event(
                "LibraryDeleted",
                json!({ "libraryId": library_id }),
            ));
        }
    }
}

fn append_series_events(
    events: &mut Vec<Event>,
    previous: &HashMap<String, SeriesSnapshot>,
    current: &HashMap<String, SeriesSnapshot>,
) {
    for (series_id, current_snapshot) in current {
        match previous.get(series_id) {
            None => events.push(sse_event(
                "SeriesAdded",
                json!({
                    "seriesId": series_id,
                    "libraryId": current_snapshot.library_id,
                }),
            )),
            Some(previous_snapshot) if previous_snapshot != current_snapshot => {
                events.push(sse_event(
                    "SeriesChanged",
                    json!({
                        "seriesId": series_id,
                        "libraryId": current_snapshot.library_id,
                    }),
                ))
            }
            _ => {}
        }
    }
    for (series_id, previous_snapshot) in previous {
        if !current.contains_key(series_id) {
            events.push(sse_event(
                "SeriesDeleted",
                json!({
                    "seriesId": series_id,
                    "libraryId": previous_snapshot.library_id,
                }),
            ));
        }
    }
}

fn append_books_events(
    events: &mut Vec<Event>,
    previous: &HashMap<String, BookSnapshot>,
    current: &HashMap<String, BookSnapshot>,
) {
    for (book_id, current_snapshot) in current {
        match previous.get(book_id) {
            None => events.push(sse_event(
                "BookAdded",
                json!({
                    "bookId": book_id,
                    "seriesId": current_snapshot.series_id,
                    "libraryId": current_snapshot.library_id,
                }),
            )),
            Some(previous_snapshot) if previous_snapshot != current_snapshot => {
                events.push(sse_event(
                    "BookChanged",
                    json!({
                        "bookId": book_id,
                        "seriesId": current_snapshot.series_id,
                        "libraryId": current_snapshot.library_id,
                    }),
                ))
            }
            _ => {}
        }
    }
    for (book_id, previous_snapshot) in previous {
        if !current.contains_key(book_id) {
            events.push(sse_event(
                "BookDeleted",
                json!({
                    "bookId": book_id,
                    "seriesId": previous_snapshot.series_id,
                    "libraryId": previous_snapshot.library_id,
                }),
            ));
        }
    }
}

fn append_readlists_events(
    events: &mut Vec<Event>,
    previous: &HashMap<String, ReadListSnapshot>,
    current: &HashMap<String, ReadListSnapshot>,
) {
    for (readlist_id, current_snapshot) in current {
        match previous.get(readlist_id) {
            None => events.push(sse_event(
                "ReadListAdded",
                json!({
                    "readListId": readlist_id,
                    "bookIds": current_snapshot.book_ids,
                }),
            )),
            Some(previous_snapshot) if previous_snapshot != current_snapshot => {
                events.push(sse_event(
                    "ReadListChanged",
                    json!({
                        "readListId": readlist_id,
                        "bookIds": current_snapshot.book_ids,
                    }),
                ))
            }
            _ => {}
        }
    }
    for (readlist_id, previous_snapshot) in previous {
        if !current.contains_key(readlist_id) {
            events.push(sse_event(
                "ReadListDeleted",
                json!({
                    "readListId": readlist_id,
                    "bookIds": previous_snapshot.book_ids,
                }),
            ));
        }
    }
}

fn append_collections_events(
    events: &mut Vec<Event>,
    previous: &HashMap<String, CollectionSnapshot>,
    current: &HashMap<String, CollectionSnapshot>,
) {
    for (collection_id, current_snapshot) in current {
        match previous.get(collection_id) {
            None => events.push(sse_event(
                "CollectionAdded",
                json!({
                    "collectionId": collection_id,
                    "seriesIds": current_snapshot.series_ids,
                }),
            )),
            Some(previous_snapshot) if previous_snapshot != current_snapshot => {
                events.push(sse_event(
                    "CollectionChanged",
                    json!({
                        "collectionId": collection_id,
                        "seriesIds": current_snapshot.series_ids,
                    }),
                ))
            }
            _ => {}
        }
    }
    for (collection_id, previous_snapshot) in previous {
        if !current.contains_key(collection_id) {
            events.push(sse_event(
                "CollectionDeleted",
                json!({
                    "collectionId": collection_id,
                    "seriesIds": previous_snapshot.series_ids,
                }),
            ));
        }
    }
}

fn append_read_progress_events(
    events: &mut Vec<Event>,
    previous: &HashMap<String, String>,
    current: &HashMap<String, String>,
    user_id: &str,
) {
    for (book_id, current_last_modified) in current {
        if previous.get(book_id) != Some(current_last_modified) {
            events.push(sse_event(
                "ReadProgressChanged",
                json!({
                    "bookId": book_id,
                    "userId": user_id,
                }),
            ));
        }
    }
    for book_id in previous.keys() {
        if !current.contains_key(book_id) {
            events.push(sse_event(
                "ReadProgressDeleted",
                json!({
                    "bookId": book_id,
                    "userId": user_id,
                }),
            ));
        }
    }
}

fn append_read_progress_series_events(
    events: &mut Vec<Event>,
    previous: &HashMap<String, String>,
    current: &HashMap<String, String>,
    user_id: &str,
) {
    for (series_id, current_last_modified) in current {
        if previous.get(series_id) != Some(current_last_modified) {
            events.push(sse_event(
                "ReadProgressSeriesChanged",
                json!({
                    "seriesId": series_id,
                    "userId": user_id,
                }),
            ));
        }
    }
    for series_id in previous.keys() {
        if !current.contains_key(series_id) {
            events.push(sse_event(
                "ReadProgressSeriesDeleted",
                json!({
                    "seriesId": series_id,
                    "userId": user_id,
                }),
            ));
        }
    }
}

fn append_thumbnail_book_events(
    events: &mut Vec<Event>,
    previous: &HashMap<String, ThumbnailBookSnapshot>,
    current: &HashMap<String, ThumbnailBookSnapshot>,
) {
    for (book_id, current_snapshot) in current {
        if previous.get(book_id) != Some(current_snapshot) {
            events.push(sse_event(
                "ThumbnailBookAdded",
                json!({
                    "bookId": book_id,
                    "seriesId": current_snapshot.series_id,
                    "selected": current_snapshot.selected,
                }),
            ));
        }
    }
    for (book_id, previous_snapshot) in previous {
        if !current.contains_key(book_id) {
            events.push(sse_event(
                "ThumbnailBookDeleted",
                json!({
                    "bookId": book_id,
                    "seriesId": previous_snapshot.series_id,
                    "selected": previous_snapshot.selected,
                }),
            ));
        }
    }
}

fn append_thumbnail_events(
    events: &mut Vec<Event>,
    previous: &HashMap<String, ThumbnailSnapshot>,
    current: &HashMap<String, ThumbnailSnapshot>,
    added_event_name: &str,
    deleted_event_name: &str,
    key_name: &str,
) {
    for (id, current_snapshot) in current {
        if previous.get(id) != Some(current_snapshot) {
            events.push(sse_event(
                added_event_name,
                json!({
                    key_name: id,
                    "selected": current_snapshot.selected,
                }),
            ));
        }
    }
    for (id, previous_snapshot) in previous {
        if !current.contains_key(id) {
            events.push(sse_event(
                deleted_event_name,
                json!({
                    key_name: id,
                    "selected": previous_snapshot.selected,
                }),
            ));
        }
    }
}

async fn load_sse_snapshot(database_file: &Path, user_id: &str) -> SseSnapshot {
    let pool = match connect_pool(database_file, 1).await {
        Ok(pool) => pool,
        Err(_) => return SseSnapshot::default(),
    };

    let libraries_rows = sqlx::query(
        "SELECT ID, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM LIBRARY",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let libraries = libraries_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("ID"),
                LibrarySnapshot {
                    last_modified: row.get::<String, _>("LAST_MODIFIED"),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    let series_rows = sqlx::query(
        "SELECT ID, LIBRARY_ID, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM SERIES \
         WHERE DELETED_DATE IS NULL",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let series = series_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("ID"),
                SeriesSnapshot {
                    library_id: row.get::<String, _>("LIBRARY_ID"),
                    last_modified: row.get::<String, _>("LAST_MODIFIED"),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    let books_rows = sqlx::query(
        "SELECT ID, SERIES_ID, LIBRARY_ID, \
                COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM BOOK \
         WHERE DELETED_DATE IS NULL",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let books = books_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("ID"),
                BookSnapshot {
                    series_id: row.get::<String, _>("SERIES_ID"),
                    library_id: row.get::<String, _>("LIBRARY_ID"),
                    last_modified: row.get::<String, _>("LAST_MODIFIED"),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    let readlist_rows = sqlx::query(
        "SELECT rl.ID AS ID, \
                COALESCE(rl.LAST_MODIFIED_DATE, rl.CREATED_DATE, '') AS LAST_MODIFIED, \
                rb.BOOK_ID AS BOOK_ID \
         FROM READLIST rl \
         LEFT \
         JOIN READLIST_BOOK rb ON rb.READLIST_ID = rl.ID \
         ORDER BY rl.ID ASC, rb.NUMBER ASC, rb.BOOK_ID ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let mut readlists = HashMap::<String, ReadListSnapshot>::new();
    for row in readlist_rows {
        let readlist_id = row.get::<String, _>("ID");
        let snapshot = readlists
            .entry(readlist_id)
            .or_insert_with(|| ReadListSnapshot {
                book_ids: Vec::new(),
                last_modified: row.get::<String, _>("LAST_MODIFIED"),
            });
        if let Ok(book_id) = row.try_get::<String, _>("BOOK_ID") {
            snapshot.book_ids.push(book_id);
        }
    }

    let collection_rows = sqlx::query(
        "SELECT c.ID AS ID, COALESCE(c.LAST_MODIFIED_DATE, c.CREATED_DATE, '') AS LAST_MODIFIED, \
                cs.SERIES_ID AS SERIES_ID \
         FROM COLLECTION c \
         LEFT \
         JOIN COLLECTION_SERIES cs ON cs.COLLECTION_ID = c.ID \
         ORDER BY c.ID ASC, cs.NUMBER ASC, cs.SERIES_ID ASC",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let mut collections = HashMap::<String, CollectionSnapshot>::new();
    for row in collection_rows {
        let collection_id = row.get::<String, _>("ID");
        let snapshot = collections
            .entry(collection_id)
            .or_insert_with(|| CollectionSnapshot {
                series_ids: Vec::new(),
                last_modified: row.get::<String, _>("LAST_MODIFIED"),
            });
        if let Ok(series_id) = row.try_get::<String, _>("SERIES_ID") {
            snapshot.series_ids.push(series_id);
        }
    }

    let read_progress_rows = sqlx::query(
        "SELECT BOOK_ID, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM READ_PROGRESS \
         WHERE USER_ID = ?",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let read_progress = read_progress_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("BOOK_ID"),
                row.get::<String, _>("LAST_MODIFIED"),
            )
        })
        .collect::<HashMap<_, _>>();

    let read_progress_series_rows = sqlx::query(
        "SELECT SERIES_ID, \
                COALESCE(LAST_MODIFIED_DATE, MOST_RECENT_READ_DATE, '') AS LAST_MODIFIED \
         FROM READ_PROGRESS_SERIES \
         WHERE USER_ID = ?",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let read_progress_series = read_progress_series_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("SERIES_ID"),
                row.get::<String, _>("LAST_MODIFIED"),
            )
        })
        .collect::<HashMap<_, _>>();

    let thumbnail_book_rows = sqlx::query(
        "SELECT tb.BOOK_ID AS ID, COALESCE(b.SERIES_ID, '') AS SERIES_ID, tb.SELECTED AS \
           SELECTED, \
                COALESCE(tb.LAST_MODIFIED_DATE, tb.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM THUMBNAIL_BOOK tb \
         LEFT \
         JOIN BOOK b ON b.ID = tb.BOOK_ID",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let thumbnails_book = thumbnail_book_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("ID"),
                ThumbnailBookSnapshot {
                    series_id: row.get::<String, _>("SERIES_ID"),
                    selected: row.get::<bool, _>("SELECTED"),
                    last_modified: row.get::<String, _>("LAST_MODIFIED"),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    let thumbnail_series_rows = sqlx::query(
        "SELECT SERIES_ID AS ID, SELECTED, \
                COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM THUMBNAIL_SERIES",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let thumbnails_series = thumbnail_series_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("ID"),
                ThumbnailSnapshot {
                    selected: row.get::<bool, _>("SELECTED"),
                    last_modified: row.get::<String, _>("LAST_MODIFIED"),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    let thumbnail_collection_rows = sqlx::query(
        "SELECT COLLECTION_ID AS ID, SELECTED, \
                COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM THUMBNAIL_COLLECTION",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let thumbnails_collection = thumbnail_collection_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("ID"),
                ThumbnailSnapshot {
                    selected: row.get::<bool, _>("SELECTED"),
                    last_modified: row.get::<String, _>("LAST_MODIFIED"),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    let thumbnail_readlist_rows = sqlx::query(
        "SELECT READLIST_ID AS ID, SELECTED, \
                COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM THUMBNAIL_READLIST",
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();
    let thumbnails_readlist = thumbnail_readlist_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("ID"),
                ThumbnailSnapshot {
                    selected: row.get::<bool, _>("SELECTED"),
                    last_modified: row.get::<String, _>("LAST_MODIFIED"),
                },
            )
        })
        .collect::<HashMap<_, _>>();

    SseSnapshot {
        libraries,
        series,
        books,
        readlists,
        collections,
        thumbnails_book,
        thumbnails_series,
        thumbnails_collection,
        thumbnails_readlist,
        read_progress,
        read_progress_series,
    }
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
