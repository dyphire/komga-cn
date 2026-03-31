use super::{RuntimeConfig, TaskExecutionError, TaskQueueRecord, TaskQueueScheduler};
use crate::filesystem::FilesystemImportPort;
use komga_application::media_assets::MediaImportService;

pub(super) fn try_execute(
    scheduler: &mut TaskQueueScheduler,
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
) -> Option<Result<(), TaskExecutionError>> {
    let result = match task.simple_type.as_str() {
        "IMPORT_BOOKS_BATCH" => {
            let follow_up_tasks = match process_import_books_batch_task(runtime, task) {
                Ok(tasks) => tasks,
                Err(error) => return Some(Err(error)),
            };
            for follow_up in follow_up_tasks {
                scheduler.enqueue(follow_up);
            }
            Ok(())
        }
        "IMPORT_BOOK" => {
            let follow_up_tasks = match process_import_book_task(runtime, task) {
                Ok(tasks) => tasks,
                Err(error) => return Some(Err(error)),
            };
            for follow_up in follow_up_tasks {
                scheduler.enqueue(follow_up);
            }
            Ok(())
        }
        _ => return None,
    };

    Some(result)
}

fn process_import_books_batch_task(
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
) -> Result<Vec<TaskQueueRecord>, TaskExecutionError> {
    let payload = task.payload.clone().ok_or_else(|| {
        TaskExecutionError::invalid_task("IMPORT_BOOKS_BATCH task requires serialized payload")
    })?;
    let runtime = runtime.task_runtime_context();
    if !runtime.owns_main_database {
        return Ok(Vec::new());
    }

    let database_file = runtime.database_file.clone();

    std::thread::spawn(move || {
        let async_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "build import books batch runtime failed: {error}",
                ))
            })?;

        async_runtime.block_on(async move {
            MediaImportService::new(FilesystemImportPort::new(database_file.as_path()))
                .process_queued_books_payload(&payload)
                .await
                .map_err(TaskExecutionError::runtime)
        })
    })
    .join()
    .map_err(|_| TaskExecutionError::runtime("import books batch worker thread panicked"))?
}

fn process_import_book_task(
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
) -> Result<Vec<TaskQueueRecord>, TaskExecutionError> {
    let payload = task.payload.clone().ok_or_else(|| {
        TaskExecutionError::invalid_task("IMPORT_BOOK task requires serialized payload")
    })?;
    let runtime = runtime.task_runtime_context();
    if !runtime.owns_main_database {
        return Ok(Vec::new());
    }

    let database_file = runtime.database_file.clone();

    std::thread::spawn(move || {
        let async_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                TaskExecutionError::runtime(format!("build import book runtime failed: {error}"))
            })?;

        async_runtime.block_on(async move {
            MediaImportService::new(FilesystemImportPort::new(database_file.as_path()))
                .process_queued_book_payload(&payload)
                .await
                .map_err(TaskExecutionError::runtime)
        })
    })
    .join()
    .map_err(|_| TaskExecutionError::runtime("import book worker thread panicked"))?
}
