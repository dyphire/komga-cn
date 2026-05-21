use super::*;
use komga_application::task_processing::{
    LibraryTaskBatch, QueueStatus, SubmitUrgency, TaskKind, TaskQueue, TaskQueueAdmin, TaskRequest,
};
use tokio::sync::{Mutex, Notify};
use tracing::{error, info};

#[derive(Debug)]
pub(super) struct SchedulerInner {
    pub(crate) admin: TaskQueueOrchestrator,
    admin_loaded: bool,
    persisted_store: Option<SqliteTaskQueueStore>,
}

#[derive(Clone, Debug)]
pub struct TaskQueueScheduler {
    consumer_owner: String,
    consumes_queue: bool,
    inner: std::sync::Arc<Mutex<SchedulerInner>>,
    wakeup: std::sync::Arc<Notify>,
}

impl TaskQueueScheduler {
    pub async fn for_runtime(
        config: impl TaskRuntimeConfig,
        consumer_owner: impl Into<String>,
    ) -> Self {
        Self::for_runtime_with_wakeup(config, consumer_owner, std::sync::Arc::new(Notify::new()))
            .await
    }

    pub async fn for_runtime_with_wakeup(
        config: impl TaskRuntimeConfig,
        consumer_owner: impl Into<String>,
        wakeup: std::sync::Arc<Notify>,
    ) -> Self {
        let runtime = config.task_runtime_context();
        let worker = runtime.worker();
        let consumes_queue = worker.consumes_queue();
        let persisted_store = if consumes_queue {
            SqliteTaskQueueStore::new(worker.tasks_db_file().to_path_buf()).await
        } else {
            None
        };
        let consumer_owner = consumer_owner.into();
        let admin_loaded = persisted_store.is_none();
        let admin = TaskQueueOrchestrator::new(consumer_owner.clone(), true);

        Self {
            consumer_owner,
            consumes_queue,
            inner: std::sync::Arc::new(Mutex::new(SchedulerInner {
                admin,
                admin_loaded,
                persisted_store,
            })),
            wakeup,
        }
    }

    pub async fn enqueue(&self, task: TaskQueueRecord) {
        let mut inner = self.inner.lock().await;
        if let Some(store) = &inner.persisted_store {
            store.persist_task(&store_record(&task)).await;
            if inner.admin_loaded {
                inner.admin.enqueue(task.clone());
            }
            self.log_task_event_with_inner(&inner, "task_enqueue", &task, "queued", None);
            return;
        }
        inner.admin.enqueue(task.clone());
        self.log_task_event_with_inner(&inner, "task_enqueue", &task, "queued", None);
    }

    pub async fn enqueue_kind(&self, kind: TaskKind, target_id: &str) {
        let record = kind.request_for(target_id);
        self.enqueue(record).await;
    }

    pub async fn enqueue_request(&self, request: TaskRequest) {
        let record = request.into_queue_record();
        self.enqueue(record).await;
    }

    pub async fn enqueue_batch(&self, batch: LibraryTaskBatch) {
        for record in batch.into_queue_records() {
            self.enqueue(record).await;
        }
    }

    pub async fn take_next(&self) -> Option<TaskQueueRecord> {
        if !self.consumes_queue {
            return None;
        }

        let mut inner = self.inner.lock().await;
        self.ensure_admin_loaded(&mut inner).await;

        let task = inner.admin.take_available(&self.consumer_owner)?;
        if let Some(store) = &inner.persisted_store {
            store.claim_task(&task.id, &self.consumer_owner).await;
        }

        self.log_task_event_with_inner(&inner, "task_claim", &task, "claimed", None);

        Some(task)
    }

    pub async fn complete(&self, task_id: &str) -> bool {
        let mut inner = self.inner.lock().await;
        let task = self.current_task_from_inner(&inner, task_id);
        if let Some(store) = &inner.persisted_store {
            let removed = store.delete_task(task_id).await;
            if removed && let Some(task) = task.as_ref() {
                inner.admin.complete(task_id);
                self.log_task_event_with_inner(&inner, "task_complete", task, "completed", None);
            }
            return removed;
        }

        let removed = inner.admin.complete(task_id);
        if removed && let Some(task) = task.as_ref() {
            self.log_task_event_with_inner(&inner, "task_complete", task, "completed", None);
        }
        removed
    }

    pub async fn disown_all(&self) -> usize {
        let mut inner = self.inner.lock().await;
        self.ensure_admin_loaded(&mut inner).await;
        let owned_tasks = self.current_owned_tasks_from_inner(&inner);
        if inner.persisted_store.is_some() {
            let disowned = inner.admin.disown_all();
            if let Some(store) = &inner.persisted_store {
                store.disown_all().await;
            }
            for task in &owned_tasks {
                self.log_task_event_with_inner(&inner, "task_disown", task, "disowned", None);
            }
            return disowned;
        }

        let disowned = inner.admin.disown_all();
        for task in &owned_tasks {
            self.log_task_event_with_inner(&inner, "task_disown", task, "disowned", None);
        }
        disowned
    }

    pub async fn clear_unowned(&self) -> usize {
        let mut inner = self.inner.lock().await;
        if inner.persisted_store.is_some() {
            self.ensure_admin_loaded(&mut inner).await;
            let store = inner
                .persisted_store
                .as_ref()
                .expect("persisted store should exist after presence check")
                .clone();
            let deleted = store.clear_unowned().await;
            inner.admin.clear_unowned();
            return deleted;
        }

        inner.admin.clear_unowned()
    }

    pub async fn count_by_simple_type(&self) -> BTreeMap<String, usize> {
        let mut inner = self.inner.lock().await;
        self.ensure_admin_loaded(&mut inner).await;
        inner.admin.count_by_simple_type()
    }

    pub async fn process_available(
        &self,
        runtime: &JobRuntime<'_>,
    ) -> Result<usize, TaskExecutionError> {
        if !self.consumes_queue {
            return Ok(0);
        }

        let mut processed = 0usize;
        let mut logged_start = false;
        loop {
            let Some(task) = self.take_next().await else {
                if logged_start {
                    self.log_process_available("completed", processed, None);
                }
                return Ok(processed);
            };
            if !logged_start {
                self.log_process_available("started", processed, None);
                logged_start = true;
            }

            self.log_task_start(&task);
            let outcome = task_executor::execute_task(runtime, &task).await;
            if let Err(error) = self
                .finalize_task_result(TaskExecutionResult { task, outcome }, &mut processed)
                .await
            {
                let error_message = error.to_string();
                self.log_process_available("failed", processed, Some(error_message.as_str()));
                return Err(error);
            }
        }
    }

    pub async fn recover_and_process(
        &self,
        runtime: &JobRuntime<'_>,
    ) -> Result<usize, TaskExecutionError> {
        let mut inner = self.inner.lock().await;
        self.ensure_admin_loaded(&mut inner).await;
        let recovered_tasks = self.current_owned_tasks_from_inner(&inner);
        drop(inner);
        self.disown_all().await;
        for task in &recovered_tasks {
            self.log_task_event("task_recover", task, "recovered", None);
        }
        self.process_available(runtime).await
    }

    pub(super) async fn fail_claimed_task(&self, task: &TaskQueueRecord, error_message: &str) {
        let mut inner = self.inner.lock().await;
        if let Some(store) = &inner.persisted_store {
            let removed = store.delete_task(&task.id).await;
            if removed {
                inner.admin.complete(&task.id);
                self.log_task_event_with_inner(
                    &inner,
                    "task_fail",
                    task,
                    "failed",
                    Some(error_message),
                );
            }
            return;
        }

        if inner.admin.complete(&task.id) {
            self.log_task_event_with_inner(
                &inner,
                "task_fail",
                task,
                "failed",
                Some(error_message),
            );
        }
    }

    pub(super) async fn finalize_task_result(
        &self,
        task_result: TaskExecutionResult,
        processed: &mut usize,
    ) -> Result<(), TaskExecutionError> {
        match task_result.outcome {
            Ok(outcome) => {
                outcome.enqueue_into(self).await;
                self.complete(&task_result.task.id).await;
                *processed += 1;
                Ok(())
            }
            Err(error) => {
                let error_message = error.to_string();
                self.fail_claimed_task(&task_result.task, error_message.as_str())
                    .await;
                Err(error)
            }
        }
    }

    async fn ensure_admin_loaded(&self, inner: &mut SchedulerInner) {
        if !inner.admin_loaded {
            if let Some(store) = &inner.persisted_store {
                inner.admin = load_admin_from_store(store).await;
            }
            inner.admin_loaded = true;
        }
    }

    #[cfg(test)]
    pub(super) async fn admin_for_test(&self) -> tokio::sync::MutexGuard<'_, SchedulerInner> {
        self.inner.lock().await
    }

    fn current_task_from_inner(
        &self,
        inner: &SchedulerInner,
        task_id: &str,
    ) -> Option<TaskQueueRecord> {
        inner
            .admin
            .tasks()
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
    }

    fn current_owned_tasks_from_inner(&self, inner: &SchedulerInner) -> Vec<TaskQueueRecord> {
        inner
            .admin
            .tasks()
            .iter()
            .filter(|task| task.owner.as_deref() == Some(self.consumer_owner.as_str()))
            .cloned()
            .collect()
    }

    pub(super) fn log_task_start(&self, task: &TaskQueueRecord) {
        self.log_task_event("task_start", task, "started", None);
    }

    pub(super) fn log_process_available(
        &self,
        outcome: &str,
        processed: usize,
        error_message: Option<&str>,
    ) {
        match error_message {
            Some(error_message) => error!(
                event = "task_process_available",
                consumer_owner = %self.consumer_owner,
                outcome,
                processed,
                error = error_message,
                "task scheduler lifecycle"
            ),
            None => info!(
                event = "task_process_available",
                consumer_owner = %self.consumer_owner,
                outcome,
                processed,
                "task scheduler lifecycle"
            ),
        }
    }

    pub(super) fn log_task_event(
        &self,
        event_name: &str,
        task: &TaskQueueRecord,
        outcome: &str,
        error_message: Option<&str>,
    ) {
        let group = task.group.as_deref().unwrap_or("");
        match error_message {
            Some(error_message) => error!(
                event = event_name,
                task_id = %task.id,
                task_type = %task.simple_type,
                priority = task.priority,
                group,
                consumer_owner = %self.consumer_owner,
                outcome,
                error = error_message,
                "task scheduler lifecycle"
            ),
            None => info!(
                event = event_name,
                task_id = %task.id,
                task_type = %task.simple_type,
                priority = task.priority,
                group,
                consumer_owner = %self.consumer_owner,
                outcome,
                "task scheduler lifecycle"
            ),
        }
    }

    fn log_task_event_with_inner(
        &self,
        _inner: &SchedulerInner,
        event_name: &str,
        task: &TaskQueueRecord,
        outcome: &str,
        error_message: Option<&str>,
    ) {
        self.log_task_event(event_name, task, outcome, error_message);
    }
}

#[async_trait::async_trait]
impl TaskQueue for TaskQueueScheduler {
    async fn enqueue(&self, kind: TaskKind, target_id: &str) {
        let record = kind.request_for(target_id);
        TaskQueueScheduler::enqueue(self, record).await;
    }

    async fn enqueue_request(&self, request: TaskRequest) {
        let record = request.into_queue_record();
        TaskQueueScheduler::enqueue(self, record).await;
    }

    async fn enqueue_batch(&self, batch: LibraryTaskBatch) {
        for record in batch.into_queue_records() {
            TaskQueueScheduler::enqueue(self, record).await;
        }
    }

    async fn enqueue_records(
        &self,
        records: Vec<TaskQueueRecord>,
        urgency: SubmitUrgency,
    ) -> Result<(), String> {
        for record in records {
            TaskQueueScheduler::enqueue(self, record).await;
        }
        if urgency == SubmitUrgency::Immediate {
            self.wakeup.notify_one();
        }
        Ok(())
    }

    async fn status(&self) -> QueueStatus {
        let counts = TaskQueueScheduler::count_by_simple_type(self).await;
        QueueStatus { counts }
    }
}

#[async_trait::async_trait]
impl TaskQueueAdmin for TaskQueueScheduler {
    async fn clear_unowned_tasks(&self) -> usize {
        TaskQueueScheduler::clear_unowned(self).await
    }

    async fn apply_pool_size(&self, _value: usize) -> Result<(), String> {
        Ok(())
    }

    fn wakeup(&self) {
        self.wakeup.notify_one();
    }
}

async fn load_admin_from_store(store: &SqliteTaskQueueStore) -> TaskQueueOrchestrator {
    let mut admin = TaskQueueOrchestrator::new("runtime-store", true);
    for record in store.load_records().await {
        let owner = record.owner.clone();
        let task = record_to_runtime_task(record);
        let id = task.id.clone();
        admin.enqueue(task);
        if let Some(owner) = owner {
            let _ = admin.claim(&id, &owner);
        }
    }
    admin
}

fn store_record(task: &TaskQueueRecord) -> PersistedTaskStoreRecord {
    PersistedTaskStoreRecord {
        id: task.id.clone(),
        simple_type: task.simple_type.clone(),
        priority: task.priority,
        group: task.group.clone(),
        payload: task.payload.clone(),
        owner: task.owner.clone(),
    }
}

fn record_to_runtime_task(record: PersistedTaskStoreRecord) -> TaskQueueRecord {
    TaskQueueRecord {
        id: record.id,
        simple_type: record.simple_type,
        priority: record.priority,
        group: record.group,
        payload: record.payload,
        owner: record.owner,
        order: 0,
    }
}
