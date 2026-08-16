use std::sync::Arc;

use komga_application::task_processing::{
    TaskExecutionResult, TaskProcessingError, TaskQueueRecord,
};
use tokio::sync::{Mutex as AsyncMutex, mpsc};

use super::execution_pool::TaskExecutionPoolHandle;
use super::{TaskQueueScheduler, TaskRuntimeContext};

pub type SharedTaskQueue = Arc<AsyncMutex<TaskQueueScheduler>>;

pub(super) struct BackgroundTaskExecutionLoop<'a> {
    task_queue: &'a SharedTaskQueue,
    task_execution_pool: &'a TaskExecutionPoolHandle,
    runtime: &'a TaskRuntimeContext,
    result_rx: &'a mut mpsc::UnboundedReceiver<TaskExecutionResult>,
}

impl<'a> BackgroundTaskExecutionLoop<'a> {
    pub(super) fn new(
        task_queue: &'a SharedTaskQueue,
        task_execution_pool: &'a TaskExecutionPoolHandle,
        runtime: &'a TaskRuntimeContext,
        result_rx: &'a mut mpsc::UnboundedReceiver<TaskExecutionResult>,
    ) -> Self {
        Self {
            task_queue,
            task_execution_pool,
            runtime,
            result_rx,
        }
    }

    pub(super) async fn drain(&mut self) -> Result<usize, TaskProcessingError> {
        let mut state = BackgroundTaskExecutionState::default();

        loop {
            if state.first_error.is_none() {
                let claimed = match self.claim_tasks_up_to_capacity(&mut state).await {
                    Ok(claimed) => claimed,
                    Err(error) => {
                        state.first_error = Some(error);
                        0
                    }
                };

                if claimed > 0 && !state.logged_start {
                    let task_queue = self.task_queue.lock().await;
                    task_queue.log_process_available("started", state.processed, None);
                    state.logged_start = true;
                }
            }

            if state.in_flight == 0 {
                return self.finish(state).await;
            }

            self.finish_one_in_flight_task(&mut state).await?;
        }
    }

    async fn claim_tasks_up_to_capacity(
        &self,
        state: &mut BackgroundTaskExecutionState,
    ) -> Result<usize, TaskProcessingError> {
        let mut claimed = 0usize;
        while state.in_flight < self.task_execution_pool.desired_size() {
            let task = {
                let task_queue = self.task_queue.lock().await;
                task_queue.take_next().await?
            };
            let Some(task) = task else {
                break;
            };

            self.submit_task(task).await?;
            state.in_flight += 1;
            claimed += 1;
        }
        Ok(claimed)
    }

    async fn submit_task(&self, task: TaskQueueRecord) -> Result<(), TaskProcessingError> {
        if let Err(error_message) = self
            .task_execution_pool
            .submit(task.clone(), self.runtime.clone())
        {
            let task_queue = self.task_queue.lock().await;
            let error_message = error_message.to_string();
            task_queue.fail_claimed_task(&task, &error_message).await?;
            return Err(TaskProcessingError::runtime(error_message));
        }

        {
            let task_queue = self.task_queue.lock().await;
            task_queue.log_task_start(&task);
        }
        Ok(())
    }

    async fn finish_one_in_flight_task(
        &mut self,
        state: &mut BackgroundTaskExecutionState,
    ) -> Result<(), TaskProcessingError> {
        let Some(task_result) = self.result_rx.recv().await else {
            let error = TaskProcessingError::runtime("task execution pool result channel closed");
            let error_message = error.to_string();
            let task_queue = self.task_queue.lock().await;
            task_queue.log_process_available(
                "failed",
                state.processed,
                Some(error_message.as_str()),
            );
            return Err(error);
        };
        state.in_flight = state.in_flight.saturating_sub(1);

        let finalize_result = {
            let task_queue = self.task_queue.lock().await;
            super::queue_orchestration::finalize_task_result(
                &task_queue,
                task_result,
                &mut state.processed,
            )
            .await
        };
        if let Err(error) = finalize_result
            && state.first_error.is_none()
        {
            state.first_error = Some(error);
        }
        Ok(())
    }

    async fn finish(
        &self,
        state: BackgroundTaskExecutionState,
    ) -> Result<usize, TaskProcessingError> {
        let task_queue = self.task_queue.lock().await;
        if let Some(error) = state.first_error {
            let error_message = error.to_string();
            task_queue.log_process_available(
                "failed",
                state.processed,
                Some(error_message.as_str()),
            );
            return Err(error);
        }
        if state.logged_start {
            task_queue.log_process_available("completed", state.processed, None);
        }
        Ok(state.processed)
    }
}

#[derive(Default)]
struct BackgroundTaskExecutionState {
    processed: usize,
    logged_start: bool,
    in_flight: usize,
    first_error: Option<TaskProcessingError>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_queue::test_support::RuntimeTestFixture;
    use komga_application::task_processing::TaskExecutionOutcome;

    #[tokio::test]
    async fn drain_finishes_in_flight_success_before_returning_first_error() {
        let fixture = RuntimeTestFixture::new("execution-loop-concurrent-failure");
        let runtime = fixture
            .runtime_context(true, true)
            .await
            .with_task_pool_size(2);
        let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
        scheduler
            .enqueue(
                TaskQueueRecord::new(
                    "UNSUPPORTED_TASK:execution-loop-failure",
                    2_000,
                    Some("failure-group".to_string()),
                )
                .with_simple_type("UNSUPPORTED_TASK"),
            )
            .await
            .expect("unsupported task should enqueue");
        scheduler
            .enqueue(TaskQueueRecord::new(
                "UpgradeIndex:execution-loop-success",
                1_000,
                Some("success-group".to_string()),
            ))
            .await
            .expect("successful task should enqueue");

        let task_queue = Arc::new(AsyncMutex::new(scheduler));
        let executed_tasks = Arc::new(AsyncMutex::new(Vec::new()));
        let execution_pool = TaskExecutionPoolHandle::new_for_test(2, {
            let executed_tasks = executed_tasks.clone();
            move |_runtime, task| {
                let executed_tasks = executed_tasks.clone();
                async move {
                    executed_tasks.lock().await.push(task.id.clone());
                    if task.simple_type == "UNSUPPORTED_TASK" {
                        return Err(TaskProcessingError::unsupported_task(&task.simple_type));
                    }
                    Ok(TaskExecutionOutcome::completed())
                }
            }
        });
        let mut result_rx = execution_pool
            .take_result_receiver()
            .expect("execution loop test should own the result receiver");

        let error = BackgroundTaskExecutionLoop::new(
            &task_queue,
            &execution_pool,
            &runtime,
            &mut result_rx,
        )
        .drain()
        .await
        .expect_err("failed task should fail the drain boundary");

        assert!(
            error
                .to_string()
                .contains("unsupported runtime task type: UNSUPPORTED_TASK")
        );
        let executed_tasks = executed_tasks.lock().await.clone();
        assert_eq!(executed_tasks.len(), 2);
        assert!(executed_tasks.contains(&"UNSUPPORTED_TASK:execution-loop-failure".to_string()));
        assert!(executed_tasks.contains(&"UpgradeIndex:execution-loop-success".to_string()));

        let remaining_by_type = task_queue
            .lock()
            .await
            .count_by_simple_type()
            .await
            .expect("execution-loop fixture queue counts should load");
        assert!(
            remaining_by_type.is_empty(),
            "success and failed tasks should both be finalized: {remaining_by_type:?}",
        );

        fixture.cleanup().await;
    }
}
