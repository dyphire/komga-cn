use super::*;

mod cleanup_delete_jobs;
mod conversion_jobs;
mod metadata_jobs;

pub(super) fn try_execute(
    scheduler: &mut TaskQueueScheduler,
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<(), TaskExecutionError>> {
    if let Some(result) = metadata_jobs::try_execute(scheduler, runtime, task, task_target) {
        return Some(result);
    }
    if let Some(result) = cleanup_delete_jobs::try_execute(runtime, task, task_target) {
        return Some(result);
    }

    conversion_jobs::try_execute(scheduler, runtime, task, task_target)
}
