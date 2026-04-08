use axum::response::sse::Event;
use serde_json::json;
use std::collections::{HashMap, HashSet};

use super::snapshot::{
    BookSnapshot, CollectionSnapshot, LibrarySnapshot, ReadListSnapshot, SeriesSnapshot,
    SseSnapshot, ThumbnailBookSnapshot, ThumbnailCollectionSnapshot, ThumbnailReadListSnapshot,
    ThumbnailSnapshot,
};

pub(super) fn sse_event(name: &str, payload: serde_json::Value) -> Event {
    Event::default()
        .event(name)
        .data(serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()))
}

pub(super) fn append_snapshot_events(
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
    append_thumbnail_collection_events(
        events,
        &previous.thumbnails_collection,
        &current.thumbnails_collection,
    );
    append_thumbnail_readlist_events(
        events,
        &previous.thumbnails_readlist,
        &current.thumbnails_readlist,
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
    for (thumbnail_id, current_snapshot) in current {
        match previous.get(thumbnail_id) {
            None => events.push(sse_event(
                "ThumbnailBookAdded",
                json!({
                    "bookId": current_snapshot.book_id,
                    "seriesId": current_snapshot.series_id,
                    "selected": current_snapshot.selected,
                }),
            )),
            Some(previous_snapshot) if previous_snapshot != current_snapshot => {
                if previous_snapshot.selected && !current_snapshot.selected {
                    events.push(sse_event(
                        "ThumbnailBookDeleted",
                        json!({
                            "bookId": previous_snapshot.book_id,
                            "seriesId": previous_snapshot.series_id,
                            "selected": previous_snapshot.selected,
                        }),
                    ));
                }
                events.push(sse_event(
                    "ThumbnailBookAdded",
                    json!({
                        "bookId": current_snapshot.book_id,
                        "seriesId": current_snapshot.series_id,
                        "selected": current_snapshot.selected,
                    }),
                ));
            }
            _ => {}
        }
    }
    for (thumbnail_id, previous_snapshot) in previous {
        if !current.contains_key(thumbnail_id) {
            events.push(sse_event(
                "ThumbnailBookDeleted",
                json!({
                    "bookId": previous_snapshot.book_id,
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

fn append_thumbnail_readlist_events(
    events: &mut Vec<Event>,
    previous: &HashMap<String, ThumbnailReadListSnapshot>,
    current: &HashMap<String, ThumbnailReadListSnapshot>,
) {
    let deleted_readlist_ids = previous
        .iter()
        .filter(|(thumbnail_id, _)| !current.contains_key(*thumbnail_id))
        .map(|(_, snapshot)| snapshot.readlist_id.clone())
        .collect::<HashSet<_>>();

    for (thumbnail_id, current_snapshot) in current {
        match previous.get(thumbnail_id) {
            None => events.push(sse_event(
                "ThumbnailReadListAdded",
                json!({
                    "readListId": current_snapshot.readlist_id,
                    "selected": current_snapshot.selected,
                }),
            )),
            Some(previous_snapshot) if previous_snapshot != current_snapshot => {
                if previous_snapshot.selected && !current_snapshot.selected {
                    events.push(sse_event(
                        "ThumbnailReadListDeleted",
                        json!({
                            "readListId": previous_snapshot.readlist_id,
                            "selected": previous_snapshot.selected,
                        }),
                    ));
                }
                if current_snapshot.selected
                    && !deleted_readlist_ids.contains(&current_snapshot.readlist_id)
                {
                    events.push(sse_event(
                        "ThumbnailReadListAdded",
                        json!({
                            "readListId": current_snapshot.readlist_id,
                            "selected": current_snapshot.selected,
                        }),
                    ));
                }
            }
            _ => {}
        }
    }
    for (thumbnail_id, previous_snapshot) in previous {
        if !current.contains_key(thumbnail_id) {
            events.push(sse_event(
                "ThumbnailReadListDeleted",
                json!({
                    "readListId": previous_snapshot.readlist_id,
                    "selected": previous_snapshot.selected,
                }),
            ));
        }
    }
}

fn append_thumbnail_collection_events(
    events: &mut Vec<Event>,
    previous: &HashMap<String, ThumbnailCollectionSnapshot>,
    current: &HashMap<String, ThumbnailCollectionSnapshot>,
) {
    for (thumbnail_id, current_snapshot) in current {
        match previous.get(thumbnail_id) {
            None => events.push(sse_event(
                "ThumbnailSeriesCollectionAdded",
                json!({
                    "collectionId": current_snapshot.collection_id,
                    "selected": current_snapshot.selected,
                }),
            )),
            Some(previous_snapshot) if previous_snapshot != current_snapshot => {
                if previous_snapshot.selected && !current_snapshot.selected {
                    events.push(sse_event(
                        "ThumbnailSeriesCollectionDeleted",
                        json!({
                            "collectionId": previous_snapshot.collection_id,
                            "selected": previous_snapshot.selected,
                        }),
                    ));
                }
                events.push(sse_event(
                    "ThumbnailSeriesCollectionAdded",
                    json!({
                        "collectionId": current_snapshot.collection_id,
                        "selected": current_snapshot.selected,
                    }),
                ));
            }
            _ => {}
        }
    }
    for (thumbnail_id, previous_snapshot) in previous {
        if !current.contains_key(thumbnail_id) {
            events.push(sse_event(
                "ThumbnailSeriesCollectionDeleted",
                json!({
                    "collectionId": previous_snapshot.collection_id,
                    "selected": previous_snapshot.selected,
                }),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::response::IntoResponse;
    use axum::response::sse::Sse;
    use futures_util::stream;
    use std::convert::Infallible;

    async fn event_frame(event: Event) -> String {
        let response = Sse::new(stream::iter(vec![Ok::<_, Infallible>(event)])).into_response();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("sse body should be readable");
        String::from_utf8(body.to_vec()).expect("event frame should be utf-8")
    }

    async fn assert_single_event_contains(mut events: Vec<Event>, expected_parts: &[&str]) {
        assert_eq!(events.len(), 1);
        let frame = event_frame(events.pop().expect("single event should exist")).await;
        for expected_part in expected_parts {
            assert!(
                frame.contains(expected_part),
                "missing {expected_part} in frame: {frame}"
            );
        }
    }

    async fn event_frames(events: Vec<Event>) -> Vec<String> {
        let mut frames = Vec::with_capacity(events.len());
        for event in events {
            frames.push(event_frame(event).await);
        }
        frames
    }

    #[tokio::test]
    async fn append_books_events_emits_book_changed_when_last_modified_changes() {
        let previous = HashMap::from([(
            "book-1".to_string(),
            BookSnapshot {
                series_id: "series-1".to_string(),
                library_id: "library-1".to_string(),
                last_modified: "2024-01-01 00:00:00.000".to_string(),
            },
        )]);
        let current = HashMap::from([(
            "book-1".to_string(),
            BookSnapshot {
                series_id: "series-1".to_string(),
                library_id: "library-1".to_string(),
                last_modified: "2024-01-01 00:00:01.000".to_string(),
            },
        )]);
        let mut events = Vec::new();

        append_books_events(&mut events, &previous, &current);

        assert_single_event_contains(
            events,
            &[
                "BookChanged",
                "bookId",
                "book-1",
                "seriesId",
                "series-1",
                "libraryId",
                "library-1",
            ],
        )
        .await;
    }

    #[tokio::test]
    async fn append_thumbnail_readlist_events_suppresses_added_when_delete_housekeeping_reselects_sibling()
     {
        let previous = HashMap::from([
            (
                "deleted-thumb".to_string(),
                ThumbnailReadListSnapshot {
                    readlist_id: "readlist-1".to_string(),
                    selected: true,
                    last_modified: "2024-01-01 00:00:00.000".to_string(),
                },
            ),
            (
                "remaining-thumb".to_string(),
                ThumbnailReadListSnapshot {
                    readlist_id: "readlist-1".to_string(),
                    selected: false,
                    last_modified: "2024-01-01 00:00:00.000".to_string(),
                },
            ),
        ]);
        let current = HashMap::from([(
            "remaining-thumb".to_string(),
            ThumbnailReadListSnapshot {
                readlist_id: "readlist-1".to_string(),
                selected: true,
                last_modified: "2024-01-01 00:00:01.000".to_string(),
            },
        )]);
        let mut events = Vec::new();

        append_thumbnail_readlist_events(&mut events, &previous, &current);

        assert_single_event_contains(
            events,
            &[
                "ThumbnailReadListDeleted",
                "readListId",
                "readlist-1",
                "selected",
                "true",
            ],
        )
        .await;
    }

    #[tokio::test]
    async fn append_thumbnail_readlist_events_still_emits_added_for_plain_reselect_without_delete()
    {
        let previous = HashMap::from([(
            "thumb-1".to_string(),
            ThumbnailReadListSnapshot {
                readlist_id: "readlist-1".to_string(),
                selected: false,
                last_modified: "2024-01-01 00:00:00.000".to_string(),
            },
        )]);
        let current = HashMap::from([(
            "thumb-1".to_string(),
            ThumbnailReadListSnapshot {
                readlist_id: "readlist-1".to_string(),
                selected: true,
                last_modified: "2024-01-01 00:00:01.000".to_string(),
            },
        )]);
        let mut events = Vec::new();

        append_thumbnail_readlist_events(&mut events, &previous, &current);

        assert_single_event_contains(
            events,
            &[
                "ThumbnailReadListAdded",
                "readListId",
                "readlist-1",
                "selected",
                "true",
            ],
        )
        .await;
    }

    #[tokio::test]
    async fn append_thumbnail_readlist_events_emit_deleted_and_added_when_selection_moves_between_existing_thumbnails()
     {
        let previous = HashMap::from([
            (
                "thumb-old".to_string(),
                ThumbnailReadListSnapshot {
                    readlist_id: "readlist-1".to_string(),
                    selected: true,
                    last_modified: "2024-01-01 00:00:00.000".to_string(),
                },
            ),
            (
                "thumb-new".to_string(),
                ThumbnailReadListSnapshot {
                    readlist_id: "readlist-1".to_string(),
                    selected: false,
                    last_modified: "2024-01-01 00:00:00.000".to_string(),
                },
            ),
        ]);
        let current = HashMap::from([
            (
                "thumb-old".to_string(),
                ThumbnailReadListSnapshot {
                    readlist_id: "readlist-1".to_string(),
                    selected: false,
                    last_modified: "2024-01-01 00:00:01.000".to_string(),
                },
            ),
            (
                "thumb-new".to_string(),
                ThumbnailReadListSnapshot {
                    readlist_id: "readlist-1".to_string(),
                    selected: true,
                    last_modified: "2024-01-01 00:00:01.000".to_string(),
                },
            ),
        ]);
        let mut events = Vec::new();

        append_thumbnail_readlist_events(&mut events, &previous, &current);

        let frames = event_frames(events).await;
        assert_eq!(frames.len(), 2);
        assert!(frames.iter().any(|frame| {
            frame.contains("ThumbnailReadListDeleted")
                && frame.contains("readlist-1")
                && frame.contains("selected")
                && frame.contains("true")
        }));
        assert!(frames.iter().any(|frame| {
            frame.contains("ThumbnailReadListAdded")
                && frame.contains("readlist-1")
                && frame.contains("selected")
                && frame.contains("true")
        }));
    }

    #[tokio::test]
    async fn append_thumbnail_book_events_emits_added_for_selected_false_sibling_upload() {
        let previous = HashMap::from([(
            "thumb-selected".to_string(),
            ThumbnailBookSnapshot {
                book_id: "book-1".to_string(),
                series_id: "series-1".to_string(),
                selected: true,
                last_modified: "2024-01-01 00:00:00.000".to_string(),
            },
        )]);
        let current = HashMap::from([
            (
                "thumb-selected".to_string(),
                ThumbnailBookSnapshot {
                    book_id: "book-1".to_string(),
                    series_id: "series-1".to_string(),
                    selected: true,
                    last_modified: "2024-01-01 00:00:00.000".to_string(),
                },
            ),
            (
                "thumb-new".to_string(),
                ThumbnailBookSnapshot {
                    book_id: "book-1".to_string(),
                    series_id: "series-1".to_string(),
                    selected: false,
                    last_modified: "2024-01-01 00:00:01.000".to_string(),
                },
            ),
        ]);
        let mut events = Vec::new();

        append_thumbnail_book_events(&mut events, &previous, &current);

        assert_single_event_contains(
            events,
            &[
                "ThumbnailBookAdded",
                "bookId",
                "book-1",
                "seriesId",
                "series-1",
                "selected",
                "false",
            ],
        )
        .await;
    }

    #[tokio::test]
    async fn append_thumbnail_book_events_emits_added_when_existing_thumbnail_becomes_selected() {
        let previous = HashMap::from([(
            "thumb-1".to_string(),
            ThumbnailBookSnapshot {
                book_id: "book-1".to_string(),
                series_id: "series-1".to_string(),
                selected: false,
                last_modified: "2024-01-01 00:00:00.000".to_string(),
            },
        )]);
        let current = HashMap::from([(
            "thumb-1".to_string(),
            ThumbnailBookSnapshot {
                book_id: "book-1".to_string(),
                series_id: "series-1".to_string(),
                selected: true,
                last_modified: "2024-01-01 00:00:01.000".to_string(),
            },
        )]);
        let mut events = Vec::new();

        append_thumbnail_book_events(&mut events, &previous, &current);

        assert_single_event_contains(
            events,
            &[
                "ThumbnailBookAdded",
                "bookId",
                "book-1",
                "seriesId",
                "series-1",
                "selected",
                "true",
            ],
        )
        .await;
    }

    #[tokio::test]
    async fn append_thumbnail_collection_events_emits_added_when_existing_thumbnail_becomes_selected()
     {
        let previous = HashMap::from([(
            "thumb-1".to_string(),
            ThumbnailCollectionSnapshot {
                collection_id: "collection-1".to_string(),
                selected: false,
                last_modified: "2024-01-01 00:00:00.000".to_string(),
            },
        )]);
        let current = HashMap::from([(
            "thumb-1".to_string(),
            ThumbnailCollectionSnapshot {
                collection_id: "collection-1".to_string(),
                selected: true,
                last_modified: "2024-01-01 00:00:01.000".to_string(),
            },
        )]);
        let mut events = Vec::new();

        append_thumbnail_collection_events(&mut events, &previous, &current);

        assert_single_event_contains(
            events,
            &[
                "ThumbnailSeriesCollectionAdded",
                "collectionId",
                "collection-1",
                "selected",
                "true",
            ],
        )
        .await;
    }
}
