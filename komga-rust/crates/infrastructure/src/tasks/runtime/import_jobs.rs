use super::{RuntimeConfig, TaskExecutionError, TaskQueueRecord, TaskQueueScheduler};
use crate::filesystem::FilesystemImportPort;
use komga_application::media_assets::MediaImportService;
use std::future::Future;
use std::path::PathBuf;

pub(super) fn try_execute(
    scheduler: &mut TaskQueueScheduler,
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
) -> Option<Result<(), TaskExecutionError>> {
    match task.simple_type.as_str() {
        "IMPORT_BOOKS_BATCH" => Some(
            process_import_books_batch_task(runtime, task)
                .map(|follow_up_tasks| enqueue_follow_up_tasks(scheduler, follow_up_tasks)),
        ),
        "IMPORT_BOOK" => Some(
            process_import_book_task(runtime, task)
                .map(|follow_up_tasks| enqueue_follow_up_tasks(scheduler, follow_up_tasks)),
        ),
        _ => None,
    }
}

fn enqueue_follow_up_tasks(
    scheduler: &mut TaskQueueScheduler,
    follow_up_tasks: Vec<TaskQueueRecord>,
) {
    for follow_up in follow_up_tasks {
        scheduler.enqueue(follow_up);
    }
}

fn process_import_books_batch_task(
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
) -> Result<Vec<TaskQueueRecord>, TaskExecutionError> {
    process_import_task(
        runtime,
        task,
        "IMPORT_BOOKS_BATCH task requires serialized payload",
        "build import books batch runtime failed",
        "import books batch worker thread panicked",
        |service, payload, priority| async move {
            service
                .process_queued_books_payload(&payload, priority)
                .await
        },
    )
}

fn process_import_book_task(
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
) -> Result<Vec<TaskQueueRecord>, TaskExecutionError> {
    process_import_task(
        runtime,
        task,
        "IMPORT_BOOK task requires serialized payload",
        "build import book runtime failed",
        "import book worker thread panicked",
        |service, payload, priority| async move {
            service
                .process_queued_book_payload(&payload, priority)
                .await
        },
    )
}

fn process_import_task<F, Fut>(
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
    missing_payload_message: &'static str,
    build_runtime_error: &'static str,
    panic_error: &'static str,
    process: F,
) -> Result<Vec<TaskQueueRecord>, TaskExecutionError>
where
    F: FnOnce(MediaImportService<FilesystemImportPort>, String, i32) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Vec<TaskQueueRecord>, String>>,
{
    let Some((database_file, payload, priority)) =
        prepare_import_task(runtime, task, missing_payload_message)?
    else {
        return Ok(Vec::new());
    };

    run_import_worker(
        database_file,
        payload,
        priority,
        build_runtime_error,
        panic_error,
        process,
    )
}

fn prepare_import_task(
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
    missing_payload_message: &str,
) -> Result<Option<(PathBuf, String, i32)>, TaskExecutionError> {
    let payload = task
        .payload
        .clone()
        .ok_or_else(|| TaskExecutionError::invalid_task(missing_payload_message))?;
    let runtime = runtime.task_runtime_context();
    if !runtime.owns_main_database {
        return Ok(None);
    }

    Ok(Some((
        runtime.database_file.clone(),
        payload,
        task.priority,
    )))
}

fn run_import_worker<F, Fut>(
    database_file: PathBuf,
    payload: String,
    priority: i32,
    build_runtime_error: &'static str,
    panic_error: &'static str,
    process: F,
) -> Result<Vec<TaskQueueRecord>, TaskExecutionError>
where
    F: FnOnce(MediaImportService<FilesystemImportPort>, String, i32) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Vec<TaskQueueRecord>, String>>,
{
    std::thread::spawn(move || {
        let async_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                TaskExecutionError::runtime(format!("{build_runtime_error}: {error}"))
            })?;

        async_runtime.block_on(async move {
            let service =
                MediaImportService::new(FilesystemImportPort::new(database_file.as_path()));
            process(service, payload, priority)
                .await
                .map_err(TaskExecutionError::runtime)
        })
    })
    .join()
    .map_err(|_| TaskExecutionError::runtime(panic_error))?
}
