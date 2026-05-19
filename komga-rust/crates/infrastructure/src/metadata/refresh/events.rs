use komga_application::runtime_sse::register_runtime_sse_event;
use serde_json::json;

pub(super) fn emit_book_changed(book_id: &str, series_id: &str, library_id: &str) {
    register_runtime_sse_event(
        "BookChanged",
        json!({
            "bookId": book_id,
            "seriesId": series_id,
            "libraryId": library_id,
        }),
        false,
        None,
    );
}

pub(super) fn emit_readlist(readlist_id: &str, book_ids: &[String], created: bool) {
    register_runtime_sse_event(
        if created {
            "ReadListAdded"
        } else {
            "ReadListChanged"
        },
        json!({
            "readListId": readlist_id,
            "bookIds": book_ids,
        }),
        false,
        None,
    );
}

pub(super) fn emit_series_changed(series_id: &str, library_id: &str) {
    register_runtime_sse_event(
        "SeriesChanged",
        json!({
            "seriesId": series_id,
            "libraryId": library_id,
        }),
        false,
        None,
    );
}

pub(super) fn emit_collection(collection_id: &str, series_ids: &[String], created: bool) {
    register_runtime_sse_event(
        if created {
            "CollectionAdded"
        } else {
            "CollectionChanged"
        },
        json!({
            "collectionId": collection_id,
            "seriesIds": series_ids,
        }),
        false,
        None,
    );
}
