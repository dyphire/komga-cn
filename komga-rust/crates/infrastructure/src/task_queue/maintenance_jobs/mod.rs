use super::*;
use crate::task_queue::TaskRuntimeContext;

mod cleanup_delete_jobs;
mod conversion_jobs;
mod metadata_jobs;

pub(super) async fn try_execute(
    runtime: &TaskRuntimeContext,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<TaskExecutionOutcome, TaskExecutionError>> {
    if let Some(result) = metadata_jobs::try_execute(runtime, task, task_target).await {
        return Some(result);
    }

    if let Some(result) = cleanup_delete_jobs::try_execute(runtime, task, task_target).await {
        return Some(result);
    }

    conversion_jobs::try_execute(runtime, task, task_target).await
}
