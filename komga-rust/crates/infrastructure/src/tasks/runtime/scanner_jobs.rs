use super::*;

#[path = "scanner_jobs/hashing_jobs.rs"]
mod hashing_jobs;
#[path = "scanner_jobs/scan_flow.rs"]
mod scan_flow;

pub(super) fn try_execute(
    scheduler: &mut TaskQueueScheduler,
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<(), TaskExecutionError>> {
    if let Some(result) = scan_flow::try_execute(scheduler, runtime, task, task_target) {
        return Some(result);
    }

    hashing_jobs::try_execute(scheduler, runtime, task, task_target)
}
