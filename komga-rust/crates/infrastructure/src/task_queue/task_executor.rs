use super::{TaskExecutionError, TaskExecutionOutcome, TaskQueueRecord};
use komga_application::task_processing::TaskRuntimeContext;

pub(super) async fn execute_task(
    runtime: &TaskRuntimeContext,
    task: &TaskQueueRecord,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let task_target = super::queue_core::task_target(task);

    if let Some(result) = super::scanner_jobs::try_execute(runtime, task, task_target).await {
        return result;
    }
    if let Some(result) = super::maintenance_jobs::try_execute(runtime, task, task_target).await {
        return result;
    }
    if let Some(result) = super::index_jobs::try_execute(runtime, task, task_target).await {
        return result;
    }
    if let Some(result) = super::import_jobs::try_execute(runtime, task).await {
        return result;
    }

    Err(TaskExecutionError::unsupported_task(&task.simple_type))
}
