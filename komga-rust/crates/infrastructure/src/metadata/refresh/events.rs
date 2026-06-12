use komga_application::runtime_sse::{RuntimeSseEvent, RuntimeSseEventSink};

pub(super) fn emit_book_changed(
    runtime_events: &dyn RuntimeSseEventSink,
    book_id: &str,
    series_id: &str,
    library_id: &str,
) {
    runtime_events.register(RuntimeSseEvent::BookChanged {
        book_id: book_id.to_string(),
        series_id: series_id.to_string(),
        library_id: library_id.to_string(),
    });
}

pub(super) fn emit_readlist(
    runtime_events: &dyn RuntimeSseEventSink,
    readlist_id: &str,
    book_ids: &[String],
    created: bool,
) {
    let event = if created {
        RuntimeSseEvent::ReadListAdded {
            readlist_id: readlist_id.to_string(),
            book_ids: book_ids.to_vec(),
        }
    } else {
        RuntimeSseEvent::ReadListChanged {
            readlist_id: readlist_id.to_string(),
            book_ids: book_ids.to_vec(),
        }
    };
    runtime_events.register(event);
}

pub(super) fn emit_series_changed(
    runtime_events: &dyn RuntimeSseEventSink,
    series_id: &str,
    library_id: &str,
) {
    runtime_events.register(RuntimeSseEvent::SeriesChanged {
        series_id: series_id.to_string(),
        library_id: library_id.to_string(),
    });
}

pub(super) fn emit_collection(
    runtime_events: &dyn RuntimeSseEventSink,
    collection_id: &str,
    series_ids: &[String],
    created: bool,
) {
    let event = if created {
        RuntimeSseEvent::CollectionAdded {
            collection_id: collection_id.to_string(),
            series_ids: series_ids.to_vec(),
        }
    } else {
        RuntimeSseEvent::CollectionChanged {
            collection_id: collection_id.to_string(),
            series_ids: series_ids.to_vec(),
        }
    };
    runtime_events.register(event);
}
