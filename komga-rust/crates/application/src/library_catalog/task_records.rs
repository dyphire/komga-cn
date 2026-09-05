use crate::task_processing::{
    BookPayload, BookSeriesRef, LibraryTaskCommand, TaskKind, TaskQueueRecord, TaskRequest,
    TaskSchedule, emit_library_task_batch,
};

use super::{LibraryBookSeriesRecord, LibraryRecord};

pub(super) fn manual_scan_library_task_record(
    library_id: &str,
    deep_scan: bool,
) -> TaskQueueRecord {
    emit_library_task_batch(LibraryTaskCommand::ScanLibrary {
        library_id: library_id.to_string(),
        deep_scan,
        schedule: TaskSchedule::Manual,
    })
    .into_queue_records()
    .into_iter()
    .next()
    .expect("scan-library emission should always produce exactly one task")
}

pub(super) fn background_scan_library_task_record(
    library_id: &str,
    deep_scan: bool,
) -> TaskQueueRecord {
    emit_library_task_batch(LibraryTaskCommand::ScanLibrary {
        library_id: library_id.to_string(),
        deep_scan,
        schedule: TaskSchedule::Background,
    })
    .into_queue_records()
    .into_iter()
    .next()
    .expect("scan-library emission should always produce exactly one task")
}

pub(super) fn library_should_rescan(previous: &LibraryRecord, next: &LibraryRecord) -> bool {
    previous.root != next.root
        || previous.scan_force_modified_time != next.scan_force_modified_time
        || previous.scan_cbx != next.scan_cbx
        || previous.scan_pdf != next.scan_pdf
        || previous.scan_epub != next.scan_epub
        || previous.oneshots_directory != next.oneshots_directory
        || previous.scan_directory_exclusions != next.scan_directory_exclusions
}

pub(super) fn analyze_library_task_records(
    books: Vec<LibraryBookSeriesRecord>,
) -> Vec<TaskQueueRecord> {
    emit_library_task_batch(LibraryTaskCommand::AnalyzeBooks {
        books: task_book_series_refs(books),
    })
    .into_queue_records()
}

pub(super) fn metadata_refresh_task_records(
    series_ids: Vec<String>,
    books: Vec<LibraryBookSeriesRecord>,
) -> Vec<TaskQueueRecord> {
    emit_library_task_batch(LibraryTaskCommand::RefreshMetadata {
        series_ids,
        books: task_book_series_refs(books),
    })
    .into_queue_records()
}

pub(super) fn book_metadata_refresh_task_records(
    books: Vec<LibraryBookSeriesRecord>,
) -> Vec<TaskQueueRecord> {
    books
        .into_iter()
        .map(|book| {
            TaskRequest::with_payload(
                TaskKind::RefreshBookMetadata,
                BookPayload::new(&book.book_id),
            )
            .group(book.series_id)
            .into_queue_record()
        })
        .collect()
}

pub(super) fn empty_trash_task_records(library_id: &str) -> Vec<TaskQueueRecord> {
    emit_library_task_batch(LibraryTaskCommand::EmptyTrash {
        library_id: library_id.to_string(),
    })
    .into_queue_records()
}

fn task_book_series_refs(books: Vec<LibraryBookSeriesRecord>) -> Vec<BookSeriesRef> {
    books
        .into_iter()
        .map(|book| BookSeriesRef::new(book.book_id, book.series_id))
        .collect()
}
