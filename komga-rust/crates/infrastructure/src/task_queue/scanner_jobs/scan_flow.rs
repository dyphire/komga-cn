use super::*;
use crate::task_queue::TaskRuntimeContext;
use komga_application::task_processing::{
    LibraryScanPipeline, ScanOneLibrary, ScanOneLibraryResult,
};

pub(super) async fn try_execute(
    runtime: &TaskRuntimeContext,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<TaskExecutionOutcome, TaskExecutionError>> {
    if task.simple_type != "ScanLibrary" {
        return None;
    }

    Some(handle_scan_library(runtime, task, task_target).await)
}

async fn handle_scan_library(
    runtime: &TaskRuntimeContext,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let library_id = task
        .payload
        .as_deref()
        .and_then(parse_scan_library_payload_library_id)
        .or_else(|| task_target.map(strip_scan_library_deep_suffix));
    let Some(library_id) = library_id else {
        return Err(TaskExecutionError::invalid_task(
            "ScanLibrary task must include a library id",
        ));
    };
    let library_id = library_id.to_string();

    let deep_scan = task
        .payload
        .as_deref()
        .and_then(parse_scan_library_payload_deep)
        .or_else(|| task_target.and_then(parse_scan_library_task_target_deep_scan))
        .unwrap_or(false);
    let result = if !runtime.owns_filesystem_scan_output {
        ScanOneLibraryResult::skipped_external_owned(library_id)
    } else {
        let pipeline = SqliteFilesystemLibraryScanPipeline::for_runtime(runtime);
        pipeline
            .run(ScanOneLibrary::new(library_id, deep_scan))
            .await
            .map_err(|error| TaskExecutionError::runtime(error.to_string()))?
    };
    Ok(TaskExecutionOutcome::with_follow_up_tasks(
        result.follow_up_tasks,
    ))
}

fn parse_scan_library_payload_library_id(payload: &str) -> Option<String> {
    let payload = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    payload.get("libraryId")?.as_str().map(str::to_string)
}

fn strip_scan_library_deep_suffix(task_target: &str) -> String {
    task_target
        .split_once("_DEEP_")
        .map(|(id, _)| id)
        .unwrap_or(task_target)
        .to_string()
}

fn parse_scan_library_task_target_deep_scan(task_target: &str) -> Option<bool> {
    task_target
        .rsplit_once("_DEEP_")
        .and_then(|(_, deep_scan)| deep_scan.parse::<bool>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use komga_application::task_processing::{BookPayload, LibraryPayload, TaskKind, TaskRequest};

    #[test]
    fn find_duplicate_pages_to_delete_task_uses_kotlin_compatible_unique_id() {
        let task = TaskRequest::new(TaskKind::FindDuplicatePagesToDelete)
            .priority(85)
            .into_queue_record_with_id("library-1");

        assert_eq!(task.id, "FindDuplicatePagesToDelete_library-1".to_string());
        assert_eq!(task.simple_type, "FindDuplicatePagesToDelete".to_string());
        assert_eq!(task.priority, 85);
        assert_eq!(task.group, None);
        assert!(task.payload.is_none());
    }

    #[test]
    fn hash_book_task_uses_kotlin_compatible_unique_id() {
        let task = TaskRequest::with_payload(TaskKind::HashBook, BookPayload::new("book-1"))
            .priority(0)
            .into_queue_record();

        assert_eq!(task.id, "HashBook_book-1".to_string());
        assert_eq!(task.simple_type, "HashBook".to_string());
        assert_eq!(task.priority, 0);
        assert_eq!(task.group, None);
        assert_eq!(
            task.payload.as_deref(),
            Some(r#"{"bookId":"book-1","groupId":null,"priority":0,"uniqueId":"HashBook_book-1"}"#),
        );
    }

    #[test]
    fn hash_book_koreader_task_uses_kotlin_compatible_unique_id() {
        let task =
            TaskRequest::with_payload(TaskKind::HashBookKoreader, BookPayload::new("book-1"))
                .priority(0)
                .into_queue_record();

        assert_eq!(task.id, "HashBookKoreader_book-1".to_string());
        assert_eq!(task.simple_type, "HashBookKoreader".to_string());
        assert_eq!(task.priority, 0);
        assert_eq!(task.group, None);
        assert_eq!(
            task.payload.as_deref(),
            Some(
                r#"{"bookId":"book-1","groupId":null,"priority":0,"uniqueId":"HashBookKoreader_book-1"}"#
            ),
        );
    }

    #[test]
    fn find_books_with_missing_page_hash_task_uses_kotlin_compatible_unique_id() {
        let task = TaskRequest::with_payload(
            TaskKind::FindBooksWithMissingPageHash,
            LibraryPayload::new("library-1"),
        )
        .priority(0)
        .into_queue_record();

        assert_eq!(
            task.id,
            "FindBooksWithMissingPageHash_library-1".to_string()
        );
        assert_eq!(task.simple_type, "FindBooksWithMissingPageHash".to_string());
        assert_eq!(task.group, None);
        assert_eq!(
            task.payload.as_deref(),
            Some(
                r#"{"groupId":null,"libraryId":"library-1","priority":0,"uniqueId":"FindBooksWithMissingPageHash_library-1"}"#
            ),
        );
    }

    #[test]
    fn repair_extension_task_uses_kotlin_compatible_unique_id() {
        let task = TaskRequest::with_payload(TaskKind::RepairExtension, BookPayload::new("book-1"))
            .priority(12)
            .group("series-1")
            .into_queue_record();

        assert_eq!(task.id, "RepairExtension_book-1".to_string());
        assert_eq!(task.simple_type, "RepairExtension".to_string());
        assert_eq!(task.priority, 12);
        assert_eq!(task.group.as_deref(), Some("series-1"));
        assert_eq!(
            task.payload.as_deref(),
            Some(
                r#"{"bookId":"book-1","groupId":"series-1","priority":12,"uniqueId":"RepairExtension_book-1"}"#
            ),
        );
    }

    #[test]
    fn strip_scan_library_deep_suffix_supports_underscore_legacy_ids() {
        assert_eq!(
            strip_scan_library_deep_suffix("library-1_DEEP_true"),
            "library-1".to_string()
        );
    }

    #[test]
    fn parse_scan_library_task_target_deep_scan_parses_underscore_suffix() {
        assert_eq!(
            parse_scan_library_task_target_deep_scan("library-1_DEEP_true"),
            Some(true)
        );
        assert_eq!(
            parse_scan_library_task_target_deep_scan("library-1_DEEP_false"),
            Some(false)
        );
    }

    #[test]
    fn payload_deep_flag_remains_authoritative_over_legacy_task_target_suffix() {
        let task = TaskQueueRecord::new("ScanLibrary_library-1_DEEP_true", 100, None)
            .with_simple_type("ScanLibrary")
            .with_payload(
                r#"{"libraryId":"library-1","scanDeep":false,"priority":100,"groupId":null,"uniqueId":"ScanLibrary_library-1_DEEP_true"}"#,
            );

        let deep_scan = task
            .payload
            .as_deref()
            .and_then(parse_scan_library_payload_deep)
            .or_else(|| {
                Some("library-1_DEEP_true").and_then(parse_scan_library_task_target_deep_scan)
            })
            .unwrap_or(false);

        assert!(!deep_scan);
    }
}
