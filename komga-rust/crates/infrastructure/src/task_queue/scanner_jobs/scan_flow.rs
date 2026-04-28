use super::*;
use komga_application::task_processing::{
    DefaultLibraryTaskEmitter, LibraryScanPipeline, ScanOneLibrary, ScanOneLibraryResult,
    TaskRuntimeContext,
};

pub(super) async fn try_execute(
    runtime: &TaskRuntimeContext,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<TaskExecutionOutcome, TaskExecutionError>> {
    if task.simple_type != "SCAN_LIBRARY" {
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
            "SCAN_LIBRARY task must include a library id",
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
        let pipeline = SqliteFilesystemLibraryScanPipeline::for_runtime(
            runtime,
            DefaultLibraryTaskEmitter::default(),
        );
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
        .split_once(":DEEP:")
        .or_else(|| task_target.split_once("_DEEP_"))
        .map(|(id, _)| id)
        .unwrap_or(task_target)
        .to_string()
}

fn parse_scan_library_task_target_deep_scan(task_target: &str) -> Option<bool> {
    task_target
        .rsplit_once(":DEEP:")
        .or_else(|| task_target.rsplit_once("_DEEP_"))
        .and_then(|(_, deep_scan)| deep_scan.parse::<bool>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_duplicate_pages_to_delete_task_uses_kotlin_compatible_unique_id() {
        let task = runtime_follow_up_task(RuntimeFollowUpTask::FindDuplicatePagesToDelete {
            library_id: "library-1".to_string(),
            priority: 85,
        });

        assert_eq!(
            task.id,
            "FIND_DUPLICATE_PAGES_TO_DELETE_library-1".to_string()
        );
        assert_eq!(
            task.simple_type,
            "FIND_DUPLICATE_PAGES_TO_DELETE".to_string()
        );
        assert_eq!(task.priority, 85);
        assert_eq!(task.group, None);
        assert!(task.payload.is_none());
    }

    #[test]
    fn hash_book_task_uses_kotlin_compatible_unique_id() {
        let task = runtime_follow_up_task(RuntimeFollowUpTask::HashBook {
            book_id: "book-1".to_string(),
            priority: 0,
        });

        assert_eq!(task.id, "HASH_BOOK_book-1".to_string());
        assert_eq!(task.simple_type, "HASH_BOOK".to_string());
        assert_eq!(task.priority, 0);
        assert_eq!(task.group, None);
        assert_eq!(
            task.payload.as_deref(),
            Some(
                r#"{"bookId":"book-1","groupId":null,"priority":0,"uniqueId":"HASH_BOOK_book-1"}"#
            ),
        );
    }

    #[test]
    fn hash_book_koreader_task_uses_kotlin_compatible_unique_id() {
        let task = runtime_follow_up_task(RuntimeFollowUpTask::HashBookKoreader {
            book_id: "book-1".to_string(),
            priority: 0,
        });

        assert_eq!(task.id, "HASH_BOOK_KOREADER_book-1".to_string());
        assert_eq!(task.simple_type, "HASH_BOOK_KOREADER".to_string());
        assert_eq!(task.priority, 0);
        assert_eq!(task.group, None);
        assert_eq!(
            task.payload.as_deref(),
            Some(
                r#"{"bookId":"book-1","groupId":null,"priority":0,"uniqueId":"HASH_BOOK_KOREADER_book-1"}"#
            ),
        );
    }

    #[test]
    fn find_books_with_missing_page_hash_task_uses_kotlin_compatible_unique_id() {
        let task = runtime_follow_up_task(RuntimeFollowUpTask::FindBooksWithMissingPageHash {
            library_id: "library-1".to_string(),
        });

        assert_eq!(
            task.id,
            "FIND_BOOKS_WITH_MISSING_PAGE_HASH_library-1".to_string()
        );
        assert_eq!(
            task.simple_type,
            "FIND_BOOKS_WITH_MISSING_PAGE_HASH".to_string()
        );
        assert_eq!(task.group, None);
        assert_eq!(
            task.payload.as_deref(),
            Some(
                r#"{"groupId":null,"libraryId":"library-1","priority":0,"uniqueId":"FIND_BOOKS_WITH_MISSING_PAGE_HASH_library-1"}"#
            ),
        );
    }

    #[test]
    fn repair_extension_task_uses_kotlin_compatible_unique_id() {
        let task = runtime_follow_up_task(RuntimeFollowUpTask::RepairExtension {
            book_id: "book-1".to_string(),
            series_id: "series-1".to_string(),
            priority: 12,
        });

        assert_eq!(task.id, "REPAIR_EXTENSION_book-1".to_string());
        assert_eq!(task.simple_type, "REPAIR_EXTENSION".to_string());
        assert_eq!(task.priority, 12);
        assert_eq!(task.group.as_deref(), Some("series-1"));
        assert_eq!(
            task.payload.as_deref(),
            Some(
                r#"{"bookId":"book-1","groupId":"series-1","priority":12,"uniqueId":"REPAIR_EXTENSION_book-1"}"#
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
    fn parse_scan_library_task_target_deep_scan_supports_both_legacy_suffix_shapes() {
        assert_eq!(
            parse_scan_library_task_target_deep_scan("library-1:DEEP:true"),
            Some(true)
        );
        assert_eq!(
            parse_scan_library_task_target_deep_scan("library-1_DEEP_false"),
            Some(false)
        );
    }

    #[test]
    fn payload_deep_flag_remains_authoritative_over_legacy_task_target_suffix() {
        let task = TaskQueueRecord::new("SCAN_LIBRARY:library-1:DEEP:true", 100, None)
            .with_simple_type("SCAN_LIBRARY")
            .with_payload(
                r#"{"libraryId":"library-1","scanDeep":false,"priority":100,"groupId":null,"uniqueId":"SCAN_LIBRARY:library-1:DEEP:true"}"#,
            );

        let deep_scan = task
            .payload
            .as_deref()
            .and_then(parse_scan_library_payload_deep)
            .or_else(|| {
                Some("library-1:DEEP:true").and_then(parse_scan_library_task_target_deep_scan)
            })
            .unwrap_or(false);

        assert!(!deep_scan);
    }
}
