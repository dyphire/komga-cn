use super::JobRuntime;
use super::{TaskExecutionError, TaskExecutionOutcome, TaskQueueRecord};

pub(super) async fn execute_task(
    runtime: &JobRuntime<'_>,
    task: &TaskQueueRecord,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let task_target = super::task_identity::task_target(task);
    super::task_handlers::execute(runtime, task, task_target).await
}
