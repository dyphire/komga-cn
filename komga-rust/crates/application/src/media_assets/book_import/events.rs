use crate::runtime_sse::{RuntimeSseEvent, RuntimeSseEventLog, RuntimeSseEventSink};

use super::ImportBookOutcome;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeBookImportEvent {
    pub id: u64,
    pub book_id: Option<String>,
    pub source_file: String,
    pub success: bool,
    pub message: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeBookImportEventBatch {
    pub current_cursor: u64,
    pub events: Vec<RuntimeBookImportEvent>,
}

pub(super) fn register_runtime_book_added_event(
    runtime_events: &dyn RuntimeSseEventSink,
    outcome: &ImportBookOutcome,
    series_id: &str,
) {
    runtime_events.register(RuntimeSseEvent::BookAdded {
        book_id: outcome.imported_book_id.clone(),
        series_id: series_id.to_string(),
        library_id: outcome.library_id.clone(),
    });
}

pub fn register_runtime_book_import_event(
    runtime_events: &dyn RuntimeSseEventSink,
    book_id: Option<String>,
    source_file: impl Into<String>,
    success: bool,
    message: Option<String>,
) {
    runtime_events.register(RuntimeSseEvent::BookImported {
        book_id,
        source_file: source_file.into(),
        success,
        message,
    });
}

pub fn current_runtime_book_import_event_cursor(runtime_events: &dyn RuntimeSseEventLog) -> u64 {
    runtime_events.current_cursor()
}

pub fn pending_runtime_book_import_events(
    runtime_events: &dyn RuntimeSseEventLog,
    last_seen_event_id: u64,
) -> RuntimeBookImportEventBatch {
    let batch = runtime_events.pending_events(last_seen_event_id, "", true);
    let events = batch
        .events
        .iter()
        .filter_map(|event| {
            let RuntimeSseEvent::BookImported {
                book_id,
                source_file,
                success,
                message,
            } = &event.event
            else {
                return None;
            };

            Some(RuntimeBookImportEvent {
                id: event.id,
                book_id: book_id.clone(),
                source_file: source_file.clone(),
                success: *success,
                message: message.clone(),
            })
        })
        .collect::<Vec<_>>();

    RuntimeBookImportEventBatch {
        current_cursor: batch.current_cursor,
        events,
    }
}
