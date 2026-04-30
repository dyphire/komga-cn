use super::TaskRuntimeContext;
use super::{TaskExecutionError, TaskExecutionOutcome, TaskQueueRecord};

pub(super) async fn execute_task(
    runtime: &TaskRuntimeContext,
    task: &TaskQueueRecord,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let task_target = super::queue_core::task_target(task);
    super::task_handlers::execute(runtime, task, task_target).await
}
