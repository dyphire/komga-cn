use crate::task_processing::{
    BookPayload, ImportBookPayload, TaskKind, TaskQueueRecord, TaskRequest,
};

use super::{BooksImportEntry, BooksImportPayload};

const IMPORT_BOOK_PRIORITY: i32 = 100;

pub(super) fn build_import_task_records(
    payload: BooksImportPayload,
    mut next_task_id: impl FnMut() -> String,
) -> anyhow::Result<Vec<TaskQueueRecord>> {
    payload
        .books
        .into_iter()
        .map(|book| {
            let group_id = book.series_id.clone();
            let task_id = next_task_id();
            let task_payload = ImportBookPayload::new(
                book.source_file.to_string_lossy().to_string(),
                book.series_id.clone(),
                payload.copy_mode,
                book.destination_name.clone(),
                book.upgrade_book_id.clone(),
            );
            let task_record = TaskRequest::with_payload(TaskKind::ImportBook, task_payload)
                .priority(IMPORT_BOOK_PRIORITY)
                .group(group_id)
                .into_queue_record_with_id(&task_id);

            Ok(task_record)
        })
        .collect()
}

pub(super) fn kotlin_import_book_task_id_suffix(book: &BooksImportEntry) -> String {
    format!("{}_{}", book.series_id, book.source_file.display())
}

pub(super) fn import_follow_up_analyze_task(
    book_id: &str,
    import_priority: i32,
    series_id: &str,
) -> TaskQueueRecord {
    TaskRequest::with_payload(TaskKind::AnalyzeBook, BookPayload::new(book_id))
        .priority(import_priority.saturating_add(1))
        .group(series_id)
        .into_queue_record()
}

pub(super) fn import_follow_up_metadata_task(book_id: &str, series_id: &str) -> TaskQueueRecord {
    TaskRequest::with_payload(TaskKind::RefreshBookMetadata, BookPayload::new(book_id))
        .priority(4)
        .group(series_id)
        .into_queue_record()
}

pub(super) fn import_follow_up_local_artwork_task(book_id: &str) -> TaskQueueRecord {
    TaskRequest::with_payload(TaskKind::RefreshBookLocalArtwork, BookPayload::new(book_id))
        .priority(4)
        .into_queue_record()
}
