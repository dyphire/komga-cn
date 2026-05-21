use komga_application::task_processing::TaskKind;

use super::JobRuntime;
use super::{TaskExecutionError, TaskExecutionOutcome, TaskQueueRecord};

pub(super) async fn execute_task(
    runtime: &JobRuntime<'_>,
    task: &TaskQueueRecord,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let task_target = super::task_identity::task_target(task);
    let kind = TaskKind::parse(&task.simple_type)
        .map_err(|_| TaskExecutionError::unsupported_task(&task.simple_type))?;
    super::task_handlers::execute(runtime, task, task_target, kind).await
}
