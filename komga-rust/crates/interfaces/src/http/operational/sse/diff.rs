use axum::response::sse::Event;
use serde_json::json;
use std::collections::HashMap;

use super::snapshot::{
    BookSnapshot, CollectionSnapshot, LibrarySnapshot, ReadListSnapshot, SeriesSnapshot,
    SseSnapshot, ThumbnailBookSnapshot, ThumbnailSnapshot,
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
