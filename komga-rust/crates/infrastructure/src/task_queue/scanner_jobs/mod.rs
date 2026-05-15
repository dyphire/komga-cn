use super::*;
mod hashing_jobs;
mod scan_flow;

pub(super) async fn try_execute(
    runtime: &JobRuntime<'_>,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<TaskExecutionOutcome, TaskExecutionError>> {
    if let Some(result) = scan_flow::try_execute(runtime, task, task_target).await {
        return Some(result);
    }

    hashing_jobs::try_execute(runtime, task, task_target).await
}
