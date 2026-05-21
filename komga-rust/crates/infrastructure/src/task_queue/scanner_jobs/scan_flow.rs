use super::*;

pub(in crate::task_queue) async fn execute_scan_library(
    runtime: &JobRuntime<'_>,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    let pipeline = SqliteFilesystemLibraryScanPipeline::for_runtime(runtime);
    let result = pipeline.execute_scan_task(task, task_target).await?;
    Ok(TaskExecutionOutcome::with_follow_up_tasks(
        result.follow_up_tasks,
    ))
}

#[cfg(test)]
mod tests {
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
}
