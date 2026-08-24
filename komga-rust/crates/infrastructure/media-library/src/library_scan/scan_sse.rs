use std::collections::HashMap;

use komga_application::runtime_sse::{RuntimeSseEvent, RuntimeSseEventSink};

use super::scan_models::PersistScannedLibraryOutcome;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RuntimeSseMutationKind {
    Added,
    Changed,
}

#[derive(Clone, Debug)]
pub(super) struct RuntimeSeriesSseRecord {
    pub(super) series_id: String,
    pub(super) library_id: String,
    pub(super) kind: RuntimeSseMutationKind,
}

#[derive(Clone, Debug)]
pub(super) struct RuntimeBookSseRecord {
    pub(super) book_id: String,
    pub(super) series_id: String,
    pub(super) library_id: String,
    pub(super) kind: RuntimeSseMutationKind,
}

#[derive(Clone, Debug)]
pub(super) enum RuntimeSseRecord {
    Series(RuntimeSeriesSseRecord),
    Book(RuntimeBookSseRecord),
}

#[derive(Default)]
pub(super) struct RuntimeSseEventBuffer {
    pub(super) events: Vec<RuntimeSseRecord>,
    series_indices: HashMap<String, usize>,
    book_indices: HashMap<String, usize>,
}

pub(super) fn merge_runtime_sse_mutation_kind(
    existing: RuntimeSseMutationKind,
    next: RuntimeSseMutationKind,
) -> RuntimeSseMutationKind {
    if matches!(existing, RuntimeSseMutationKind::Added)
        || matches!(next, RuntimeSseMutationKind::Added)
    {
        RuntimeSseMutationKind::Added
    } else {
        RuntimeSseMutationKind::Changed
    }
}

pub(super) fn record_series_runtime_sse_event(
    events: &mut RuntimeSseEventBuffer,
    series_id: &str,
    library_id: &str,
    kind: RuntimeSseMutationKind,
) {
    if let Some(index) = events.series_indices.get(series_id).copied() {
        let RuntimeSseRecord::Series(existing) = &mut events.events[index] else {
            unreachable!("series indices should only point at series events")
        };
        existing.kind = merge_runtime_sse_mutation_kind(existing.kind, kind);
        return;
    }

    let index = events.events.len();
    events
        .events
        .push(RuntimeSseRecord::Series(RuntimeSeriesSseRecord {
            series_id: series_id.to_string(),
            library_id: library_id.to_string(),
            kind,
        }));
    events.series_indices.insert(series_id.to_string(), index);
}

pub(super) fn record_book_runtime_sse_event(
    events: &mut RuntimeSseEventBuffer,
    book_id: &str,
    series_id: &str,
    library_id: &str,
    kind: RuntimeSseMutationKind,
) {
    if let Some(index) = events.book_indices.get(book_id).copied() {
        let RuntimeSseRecord::Book(existing) = &mut events.events[index] else {
            unreachable!("book indices should only point at book events")
        };
        existing.kind = merge_runtime_sse_mutation_kind(existing.kind, kind);
        return;
    }

    let index = events.events.len();
    events
        .events
        .push(RuntimeSseRecord::Book(RuntimeBookSseRecord {
            book_id: book_id.to_string(),
            series_id: series_id.to_string(),
            library_id: library_id.to_string(),
            kind,
        }));
    events.book_indices.insert(book_id.to_string(), index);
}

pub(super) fn emit_scanned_library_runtime_sse_events(
    runtime_events: &dyn RuntimeSseEventSink,
    library_id: &str,
    outcome: &PersistScannedLibraryOutcome,
) {
    if outcome.library_changed {
        runtime_events.register(RuntimeSseEvent::LibraryChanged {
            library_id: library_id.to_string(),
        });
    }

    for event in &outcome.runtime_events {
        match event {
            RuntimeSseRecord::Series(event) => {
                let event = match event.kind {
                    RuntimeSseMutationKind::Added => RuntimeSseEvent::SeriesAdded {
                        series_id: event.series_id.clone(),
                        library_id: event.library_id.clone(),
                    },
                    RuntimeSseMutationKind::Changed => RuntimeSseEvent::SeriesChanged {
                        series_id: event.series_id.clone(),
                        library_id: event.library_id.clone(),
                    },
                };
                runtime_events.register(event);
            }
            RuntimeSseRecord::Book(event) => {
                let event = match event.kind {
                    RuntimeSseMutationKind::Added => RuntimeSseEvent::BookAdded {
                        book_id: event.book_id.clone(),
                        series_id: event.series_id.clone(),
                        library_id: event.library_id.clone(),
                    },
                    RuntimeSseMutationKind::Changed => RuntimeSseEvent::BookChanged {
                        book_id: event.book_id.clone(),
                        series_id: event.series_id.clone(),
                        library_id: event.library_id.clone(),
                    },
                };
                runtime_events.register(event);
            }
        }
    }
}
