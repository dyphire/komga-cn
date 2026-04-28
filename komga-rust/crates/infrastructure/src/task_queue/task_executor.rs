use super::{TaskBatchExecutionResult, TaskExecutionError, TaskExecutionOutcome, TaskQueueRecord};
use futures_util::stream::{FuturesUnordered, StreamExt};
use komga_application::task_processing::TaskRuntimeContext;

pub(super) async fn execute_task(
    runtime: &TaskRuntimeContext,
    task: &TaskQueueRecord,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let task_target = super::queue_core::task_target(task);

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

pub(super) async fn execute_task_batch(
    runtime: &TaskRuntimeContext,
    batch: Vec<TaskQueueRecord>,
) -> Vec<TaskBatchExecutionResult> {
    execute_task_batch_with(runtime, batch, |runtime, task| async move {
        execute_task(&runtime, &task).await
    })
    .await
}

async fn execute_task_batch_with<F, Fut>(
    runtime: &TaskRuntimeContext,
    batch: Vec<TaskQueueRecord>,
    execute_task: F,
) -> Vec<TaskBatchExecutionResult>
where
    F: Fn(TaskRuntimeContext, TaskQueueRecord) -> Fut + Clone,
    Fut: std::future::Future<Output = Result<TaskExecutionOutcome, TaskExecutionError>>,
{
    let mut executions = FuturesUnordered::new();
    for task in batch {
        let runtime = runtime.clone();
        let execute_task = execute_task.clone();
        executions.push(async move {
            let outcome = execute_task(runtime, task.clone()).await;
            TaskBatchExecutionResult { task, outcome }
        });
    }

    let mut results = Vec::new();
    while let Some(result) = executions.next().await {
        results.push(result);
    }
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;
    use tokio::sync::Barrier;

    fn runtime_context() -> TaskRuntimeContext {
        TaskRuntimeContext {
            database_file: std::env::temp_dir().join("komga-task-batch-main.sqlite"),
            tasks_db_file: std::env::temp_dir().join("komga-task-batch-tasks.sqlite"),
            lucene_data_directory: std::env::temp_dir().join("komga-task-batch-lucene"),
            consumes_queue: true,
            owns_main_database: true,
            owns_filesystem_scan_output: true,
            owns_sidecar_output: true,
            owns_search_index: true,
            task_pool_size: 2,
        }
    }

    #[tokio::test]
    async fn execute_task_batch_starts_multiple_tasks_before_the_first_one_finishes() {
        let runtime = runtime_context();
        let started = Arc::new(AtomicUsize::new(0));
        let barrier = Arc::new(Barrier::new(2));
        let batch = vec![
            TaskQueueRecord::new("TEST_TASK:1", 0, None),
            TaskQueueRecord::new("TEST_TASK:2", 0, None),
        ];

        let results = execute_task_batch_with(&runtime, batch, {
            let started = started.clone();
            let barrier = barrier.clone();
            move |_runtime, _task| {
                let started = started.clone();
                let barrier = barrier.clone();
                async move {
                    let started_now = started.fetch_add(1, Ordering::SeqCst) + 1;
                    if started_now == 1 {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                        assert_eq!(
                            started.load(Ordering::SeqCst),
                            2,
                            "batch execution should start the second task before the first task completes",
                        );
                    }
                    barrier.wait().await;
                    Ok(TaskExecutionOutcome::completed())
                }
            }
        })
        .await;

        assert_eq!(started.load(Ordering::SeqCst), 2);
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|result| result.outcome.is_ok()));
    }
}
