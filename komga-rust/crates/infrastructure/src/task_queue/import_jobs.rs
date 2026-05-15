use super::JobRuntime;
use super::{TaskExecutionError, TaskExecutionOutcome, TaskQueueRecord};
use crate::filesystem::import::FilesystemImportPort;
use komga_application::media_assets::MediaImportService;
use std::future::Future;
use std::path::PathBuf;

pub(super) async fn try_execute(
    runtime: &JobRuntime<'_>,
    task: &TaskQueueRecord,
) -> Option<Result<TaskExecutionOutcome, TaskExecutionError>> {
    match task.simple_type.as_str() {
        "ImportBook" => Some(
            process_import_book_task(runtime, task)
                .await
                .map(TaskExecutionOutcome::with_follow_up_tasks),
        ),
        _ => None,
    }
}

async fn process_import_book_task(
    runtime: &JobRuntime<'_>,
    task: &TaskQueueRecord,
) -> Result<Vec<TaskQueueRecord>, TaskExecutionError> {
    process_import_task(
        runtime,
        task,
        "ImportBook task requires serialized payload",
        |service, payload, priority| async move {
            service
                .process_queued_book_payload(&payload, priority)
                .await
        },
    )
    .await
}

async fn process_import_task<F, Fut>(
    runtime: &JobRuntime<'_>,
    task: &TaskQueueRecord,
    missing_payload_message: &'static str,
    process: F,
) -> Result<Vec<TaskQueueRecord>, TaskExecutionError>
where
    F: FnOnce(MediaImportService<FilesystemImportPort>, String, i32) -> Fut,
    Fut: Future<Output = Result<Vec<TaskQueueRecord>, String>>,
{
    let Some((database_file, payload, priority)) =
        prepare_import_task(runtime, task, missing_payload_message)?
    else {
        return Ok(Vec::new());
    };

    let service = MediaImportService::new(FilesystemImportPort::new(database_file.as_path()));
    process(service, payload, priority)
        .await
        .map_err(TaskExecutionError::runtime)
}

fn prepare_import_task(
    runtime: &JobRuntime<'_>,
    task: &TaskQueueRecord,
    missing_payload_message: &str,
) -> Result<Option<(PathBuf, String, i32)>, TaskExecutionError> {
    let payload = task
        .payload
        .clone()
        .ok_or_else(|| TaskExecutionError::invalid_task(missing_payload_message))?;
    if !runtime.database().owns_main_database() {
        return Ok(None);
    }

    Ok(Some((
        runtime.database().main_db().database_file().to_path_buf(),
        payload,
        task.priority,
    )))
}
