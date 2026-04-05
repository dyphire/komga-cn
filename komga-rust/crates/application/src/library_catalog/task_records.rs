use crate::task_processing::TaskQueueRecord;

use super::LibraryRecord;

pub(super) fn scan_library_task_record(library_id: &str, deep_scan: bool) -> TaskQueueRecord {
    let mut task = TaskQueueRecord::new(
        format!("SCAN_LIBRARY:{library_id}"),
        100,
        Some(library_id.to_string()),
    );
    if deep_scan {
        task.payload = Some(r#"{"deep":true}"#.to_string());
    }
    task
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

pub(super) fn analyze_library_task_records(book_ids: Vec<String>) -> Vec<TaskQueueRecord> {
    book_ids
        .into_iter()
        .map(|book_id| TaskQueueRecord::new(format!("ANALYZE_BOOK:{book_id}"), 90, Some(book_id)))
        .collect()
}

pub(super) fn metadata_refresh_task_records(
    series_ids: Vec<String>,
    book_ids: Vec<String>,
) -> Vec<TaskQueueRecord> {
    let mut task_records = Vec::with_capacity((book_ids.len() * 2) + series_ids.len());
    for book_id in book_ids {
        task_records.push(TaskQueueRecord::new(
            format!("REFRESH_BOOK_METADATA:{book_id}"),
            80,
            Some(book_id.clone()),
        ));
        task_records.push(TaskQueueRecord::new(
            format!("REFRESH_BOOK_LOCAL_ARTWORK:{book_id}"),
            80,
            Some(book_id),
        ));
    }
    for series_id in series_ids {
        task_records.push(TaskQueueRecord::new(
            format!("REFRESH_SERIES_LOCAL_ARTWORK:{series_id}"),
            80,
            Some(series_id),
        ));
    }
    task_records
}

pub(super) fn empty_trash_task_records(library_id: &str) -> Vec<TaskQueueRecord> {
    vec![TaskQueueRecord::new(
        format!("EMPTY_TRASH:{library_id}"),
        70,
        Some(library_id.to_string()),
    )]
}
