use super::*;
use komga_application::task_processing::ScanOneLibrary;

pub(in crate::task_queue) async fn execute_scan_library(
    runtime: &JobRuntime<'_>,
    request: ScanOneLibrary,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    let pipeline = SqliteFilesystemLibraryScanPipeline::for_runtime(runtime);
    let result = pipeline.execute_scan(request).await?;
    Ok(TaskExecutionOutcome::with_follow_up_tasks(
        result.follow_up_tasks,
    ))
}
