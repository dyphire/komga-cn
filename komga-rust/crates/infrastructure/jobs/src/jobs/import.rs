use crate::JobRuntime;
use komga_application::media_assets::BookImportService;
use komga_application::task_processing::{
    ImportBookPayload, TaskExecutionOutcome, TaskProcessingError,
};
use komga_infrastructure_media_access::FilesystemBookImport;
use std::sync::Arc;

pub(crate) async fn execute_import_book(
    runtime: &JobRuntime<'_>,
    payload: ImportBookPayload,
    priority: i32,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    if !runtime.database().owns_main_database() {
        return Ok(TaskExecutionOutcome::completed());
    }

    let service = BookImportService::new(
        Arc::new(FilesystemBookImport::new(
            runtime.database().task_read_pool().clone(),
            runtime.database().task_write_pool().clone(),
        )),
        runtime.runtime_events_arc(),
    );
    service
        .process_queued_book_payload(payload, priority)
        .await
        .map(TaskExecutionOutcome::with_follow_up_tasks)
        .map_err(TaskProcessingError::runtime)
}
