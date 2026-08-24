use komga_application::task_processing::{
    ScanOneLibrary, TaskExecutionOutcome, TaskProcessingError,
};

use crate::media::library_scan::SqliteFilesystemLibraryScanPipeline;
use crate::tasks::JobRuntime;

pub(in crate::tasks) async fn execute_scan_library(
    runtime: &JobRuntime<'_>,
    request: ScanOneLibrary,
) -> Result<TaskExecutionOutcome, TaskProcessingError> {
    let pipeline = SqliteFilesystemLibraryScanPipeline::for_runtime(runtime)
        .await
        .map_err(TaskProcessingError::runtime)?;
    let result = pipeline.execute_scan(request).await?;
    Ok(TaskExecutionOutcome::with_follow_up_tasks(
        result.follow_up_tasks,
    ))
}
