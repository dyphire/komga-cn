use super::JobRuntime;
use crate::filesystem::import::FilesystemImportPort;
use komga_application::media_assets::MediaImportService;
use komga_application::task_processing::{TaskExecutionOutcome, TaskProcessingError};
use std::sync::Arc;

pub(in crate::task_queue) async fn execute_import_book(
    runtime: &JobRuntime<'_>,
    payload: String,
    priority: i32,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    if !runtime.database().owns_main_database() {
        return Ok(TaskExecutionOutcome::completed());
    }

    let service = MediaImportService::new(Arc::new(FilesystemImportPort::new(
        runtime.database().read_pool().clone(),
        runtime.database().write_pool().clone(),
    )));
    service
        .process_queued_book_payload(&payload, priority)
        .await
        .map(TaskExecutionOutcome::with_follow_up_tasks)
        .map_err(TaskProcessingError::runtime)
}
