use komga_application::task_processing::{
    TaskExecutionResult, TaskProcessingError, finalize_task_execution,
};

use super::JobRuntime;
use super::queue_scheduler::TaskQueueScheduler;
use super::task_job_dispatch::TaskJobDispatcher;

pub(super) async fn process_available_serial(
    scheduler: &TaskQueueScheduler,
    runtime: &JobRuntime<'_>,
) -> Result<usize, TaskProcessingError> {
    if !scheduler.consumes_queue() {
        return Ok(0);
    }

    let mut processed = 0usize;
    let mut logged_start = false;
    loop {
        let Some(task) = scheduler.take_next().await? else {
            if logged_start {
                scheduler.log_process_available("completed", processed, None);
            }
            return Ok(processed);
        };
        if !logged_start {
            scheduler.log_process_available("started", processed, None);
            logged_start = true;
        }

        scheduler.log_task_start(&task);
        let outcome = TaskJobDispatcher::new(*runtime).execute_record(&task).await;
        if let Err(error) = finalize_task_result(
            scheduler,
            TaskExecutionResult { task, outcome },
            &mut processed,
        )
        .await
        {
            let error_message = error.to_string();
            scheduler.log_process_available("failed", processed, Some(error_message.as_str()));
            return Err(error);
        }
    }
}

pub(super) async fn recover_and_process(
    scheduler: &TaskQueueScheduler,
    runtime: &JobRuntime<'_>,
) -> Result<usize, TaskProcessingError> {
    let recovered_tasks = scheduler.disown_all_and_collect_owned().await?;
    for task in &recovered_tasks {
        scheduler.log_task_event("task_recover", task, "recovered", None);
    }
    process_available_serial(scheduler, runtime).await
}

pub(super) async fn finalize_task_result(
    scheduler: &TaskQueueScheduler,
    task_result: TaskExecutionResult,
    processed: &mut usize,
) -> Result<(), TaskProcessingError> {
    finalize_task_execution(scheduler, task_result).await?;
    *processed += 1;
    Ok(())
}
