use crate::task_processing::TaskQueueRecord;
use serde_json::json;

use super::LibraryRecord;

const MANUAL_SCAN_PRIORITY: i32 = 100;
const BACKGROUND_SCAN_PRIORITY: i32 = 4;

fn scan_library_task_id(library_id: &str, deep_scan: bool) -> String {
    format!("SCAN_LIBRARY:{library_id}:DEEP:{deep_scan}")
}

fn scan_library_task_payload(
    library_id: &str,
    deep_scan: bool,
    priority: i32,
    task_id: &str,
) -> String {
    json!({
        "libraryId": library_id,
        "scanDeep": deep_scan,
        "priority": priority,
        "groupId": serde_json::Value::Null,
        "uniqueId": task_id,
    })
    .to_string()
}

fn scan_library_task_record(library_id: &str, deep_scan: bool, priority: i32) -> TaskQueueRecord {
    let task_id = scan_library_task_id(library_id, deep_scan);
    let payload = scan_library_task_payload(library_id, deep_scan, priority, &task_id);
    TaskQueueRecord::new(task_id, priority, None).with_payload(payload)
}

pub(super) fn manual_scan_library_task_record(
    library_id: &str,
    deep_scan: bool,
) -> TaskQueueRecord {
    scan_library_task_record(library_id, deep_scan, MANUAL_SCAN_PRIORITY)
}

pub(super) fn background_scan_library_task_record(
    library_id: &str,
    deep_scan: bool,
) -> TaskQueueRecord {
    scan_library_task_record(library_id, deep_scan, BACKGROUND_SCAN_PRIORITY)
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

pub(super) fn analyze_library_task_records(books: Vec<(String, String)>) -> Vec<TaskQueueRecord> {
    books
        .into_iter()
        .map(|(book_id, series_id)| {
            TaskQueueRecord::new(format!("ANALYZE_BOOK:{book_id}"), 90, Some(series_id))
        })
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
            None,
        ));
    }
    task_records
}

pub(super) fn empty_trash_task_records(library_id: &str) -> Vec<TaskQueueRecord> {
    vec![TaskQueueRecord::new(
        format!("EMPTY_TRASH:{library_id}"),
        70,
        None,
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
    fn manual_scan_library_task_record_is_high_priority_and_ungrouped() {
        let task = manual_scan_library_task_record("library-1", true);

        assert_eq!(task.id, "SCAN_LIBRARY:library-1:DEEP:true");
        assert_eq!(task.priority, MANUAL_SCAN_PRIORITY);
        assert_eq!(task.group, None);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                task.payload
                    .as_deref()
                    .expect("manual scan task should carry payload metadata"),
            )
            .expect("manual scan payload should be valid json"),
            json!({
                "libraryId": "library-1",
                "scanDeep": true,
                "priority": MANUAL_SCAN_PRIORITY,
                "groupId": serde_json::Value::Null,
                "uniqueId": "SCAN_LIBRARY:library-1:DEEP:true"
            })
        );
    }

    #[test]
    fn background_scan_library_task_record_uses_default_priority_and_no_group() {
        let task = background_scan_library_task_record("library-1", false);

        assert_eq!(task.id, "SCAN_LIBRARY:library-1:DEEP:false");
        assert_eq!(task.priority, BACKGROUND_SCAN_PRIORITY);
        assert_eq!(task.group, None);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(
                task.payload
                    .as_deref()
                    .expect("background scan task should carry payload metadata"),
            )
            .expect("background scan payload should be valid json"),
            json!({
                "libraryId": "library-1",
                "scanDeep": false,
                "priority": BACKGROUND_SCAN_PRIORITY,
                "groupId": serde_json::Value::Null,
                "uniqueId": "SCAN_LIBRARY:library-1:DEEP:false"
            })
        );
    }

    #[test]
    fn analyze_library_task_records_group_books_by_series_id() {
        let task_records =
            analyze_library_task_records(vec![("book-1".to_string(), "series-1".to_string())]);

        assert_eq!(task_records.len(), 1);
        assert_eq!(task_records[0].id, "ANALYZE_BOOK:book-1");
        assert_eq!(task_records[0].group.as_deref(), Some("series-1"));
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

    #[test]
    fn metadata_refresh_task_records_leave_series_local_artwork_ungrouped() {
        let task_records = metadata_refresh_task_records(
            vec!["series-1".to_string()],
            vec![("book-1".to_string(), "series-1".to_string())],
        );

        let series_artwork = task_records
            .iter()
            .find(|task| task.id == "REFRESH_SERIES_LOCAL_ARTWORK:series-1")
            .expect("series local artwork task should be emitted");

        assert_eq!(series_artwork.group, None);
    }

    #[test]
    fn empty_trash_task_records_are_ungrouped() {
        let task_records = empty_trash_task_records("library-1");

        assert_eq!(task_records.len(), 1);
        assert_eq!(task_records[0].id, "EMPTY_TRASH:library-1");
        assert_eq!(task_records[0].group, None);
    }
}
