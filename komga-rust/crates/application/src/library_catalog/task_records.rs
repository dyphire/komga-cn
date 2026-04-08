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
    books: Vec<(String, String)>,
) -> Vec<TaskQueueRecord> {
    let mut task_records = Vec::with_capacity((books.len() * 2) + series_ids.len());
    for (book_id, series_id) in books {
        task_records.push(
            TaskQueueRecord::new(
                format!("REFRESH_BOOK_METADATA_{book_id}"),
                80,
                Some(series_id),
            )
            .with_simple_type("REFRESH_BOOK_METADATA"),
        );
        task_records.push(
            TaskQueueRecord::new(format!("REFRESH_BOOK_LOCAL_ARTWORK_{book_id}"), 80, None)
                .with_simple_type("REFRESH_BOOK_LOCAL_ARTWORK"),
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_refresh_task_records_emit_kotlin_style_refresh_book_metadata_task() {
        let task_records = metadata_refresh_task_records(
            vec!["series-1".to_string()],
            vec![("book-1".to_string(), "series-1".to_string())],
        );

        let metadata = task_records
            .iter()
            .find(|task| task.id.starts_with("REFRESH_BOOK_METADATA"))
            .expect("book metadata task should be emitted");

        assert_eq!(metadata.id, "REFRESH_BOOK_METADATA_book-1");
        assert_eq!(metadata.simple_type, "REFRESH_BOOK_METADATA");
        assert_eq!(metadata.group.as_deref(), Some("series-1"));
    }

    #[test]
    fn metadata_refresh_task_records_emit_kotlin_style_refresh_book_local_artwork_task() {
        let task_records = metadata_refresh_task_records(
            vec!["series-1".to_string()],
            vec![("book-1".to_string(), "series-1".to_string())],
        );

        let local_artwork = task_records
            .iter()
            .find(|task| task.simple_type == "REFRESH_BOOK_LOCAL_ARTWORK")
            .expect("book local artwork task should be emitted");

        assert_eq!(local_artwork.id, "REFRESH_BOOK_LOCAL_ARTWORK_book-1");
        assert_eq!(local_artwork.priority, 80);
        assert_eq!(local_artwork.group, None);
    }
}
