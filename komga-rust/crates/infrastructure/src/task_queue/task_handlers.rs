use komga_application::task_processing::TaskQueueRecord;

use super::{TaskExecutionError, TaskExecutionOutcome};

pub(crate) async fn execute(
    runtime: &super::JobRuntime<'_>,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
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
