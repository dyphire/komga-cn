use super::*;
use komga_application::task_processing::TaskQueueAdminPort;
use tracing::{error, info};

#[derive(Clone, Debug)]
pub struct TaskQueueScheduler {
    admin: TaskQueueAdmin,
    consumer_owner: String,
    consumes_queue: bool,
    persisted_store: Option<SqliteTaskQueueStore>,
    task_pool_size: usize,
}

impl TaskQueueScheduler {
    pub fn for_runtime(config: impl TaskRuntimeConfig, consumer_owner: impl Into<String>) -> Self {
        let runtime = config.task_runtime_context();
        let consumes_queue = runtime.consumes_queue;
        let persisted_store = if consumes_queue {
            SqliteTaskQueueStore::new(runtime.tasks_db_file.clone())
        } else {
            None
        };
        let consumer_owner = consumer_owner.into();
        let admin = persisted_store
            .as_ref()
            .map(load_admin_from_store)
            .unwrap_or_else(|| TaskQueueOrchestrator::new(consumer_owner.clone(), true));

        Self {
            admin,
            consumer_owner,
            consumes_queue,
            persisted_store,
            task_pool_size: 1,
        }
    }

    pub fn enqueue(&mut self, task: TaskQueueRecord) {
        if let Some(store) = &self.persisted_store {
            store.persist_task(&store_record(&task));
            self.reload_admin_from_store();
            self.log_task_event("task_enqueue", &task, "queued", None);
            return;
        }
        self.admin.enqueue(task.clone());
        self.log_task_event("task_enqueue", &task, "queued", None);
    }

    pub fn take_next(&mut self) -> Option<TaskQueueRecord> {
        if !self.consumes_queue {
            return None;
        }

        self.reload_admin_from_store();

        let task = self.admin.take_available(&self.consumer_owner)?;
        if let Some(store) = &self.persisted_store {
            store.claim_task(&task.id, &self.consumer_owner);
            self.reload_admin_from_store();
        }

        self.log_task_event("task_claim", &task, "claimed", None);

        Some(task)
    }

    pub fn take_available_batch(&mut self) -> Vec<TaskQueueRecord> {
        if !self.consumes_queue {
            return Vec::new();
        }

        if self.task_pool_size <= 1 {
            return self.take_next().into_iter().collect();
        }

        self.reload_admin_from_store();

        let mut selected = Vec::new();
        while selected.len() < self.task_pool_size {
            let Some(task) = self.admin.take_available(&self.consumer_owner) else {
                break;
            };
            if let Some(store) = &self.persisted_store {
                store.claim_task(&task.id, &self.consumer_owner);
            }
            self.log_task_event("task_claim", &task, "claimed", None);
            selected.push(task);
        }
        self.reload_admin_from_store();
        selected
    }

    pub fn complete(&mut self, task_id: &str) -> bool {
        let task = self.current_task(task_id);
        if let Some(store) = &self.persisted_store {
            let removed = store.delete_task(task_id);
            self.reload_admin_from_store();
            if removed && let Some(task) = task.as_ref() {
                self.log_task_event("task_complete", task, "completed", None);
            }
            return removed;
        }

        let removed = self.admin.complete(task_id);
        if removed && let Some(task) = task.as_ref() {
            self.log_task_event("task_complete", task, "completed", None);
        }
        removed
    }

    pub fn admin(&self) -> &TaskQueueAdmin {
        &self.admin
    }

    pub fn admin_mut(&mut self) -> &mut TaskQueueAdmin {
        &mut self.admin
    }

    pub fn task_pool_size(&self) -> usize {
        self.task_pool_size
    }

    pub fn set_task_pool_size(&mut self, task_pool_size: usize) {
        self.task_pool_size = task_pool_size.max(1);
    }

    pub fn disown_all(&mut self) -> usize {
        if self.persisted_store.is_some() {
            self.reload_admin_from_store();
        }
        let owned_tasks = self.current_owned_tasks();
        if self.persisted_store.is_some() {
            let disowned = self.admin.disown_all();
            if let Some(store) = &self.persisted_store {
                store.disown_all();
            }
            self.reload_admin_from_store();
            for task in &owned_tasks {
                self.log_task_event("task_disown", task, "disowned", None);
            }
            return disowned;
        }

        let disowned = self.admin.disown_all();
        for task in &owned_tasks {
            self.log_task_event("task_disown", task, "disowned", None);
        }
        disowned
    }

    pub fn clear_unowned(&mut self) -> usize {
        if let Some(store) = &self.persisted_store {
            let deleted = store.clear_unowned();
            self.reload_admin_from_store();
            return deleted;
        }

        self.admin.clear_unowned()
    }

    pub fn count_by_simple_type(&self) -> BTreeMap<String, usize> {
        self.admin.count_by_simple_type()
    }

    pub fn process_available(
        &mut self,
        runtime: &RuntimeConfig,
    ) -> Result<usize, TaskExecutionError> {
        if !self.consumes_queue {
            return Ok(0);
        }

        let mut processed = 0usize;
        let mut logged_start = false;
        loop {
            let batch = self.take_available_batch();
            if batch.is_empty() {
                if logged_start {
                    self.log_process_available("completed", processed, None);
                }
                return Ok(processed);
            }
            if !logged_start {
                self.log_process_available("started", processed, None);
                logged_start = true;
            }

            let mut batch_iter = batch.into_iter();
            while let Some(task) = batch_iter.next() {
                self.log_task_event("task_start", &task, "started", None);
                match self.execute_claimed_task(runtime, &task) {
                    Ok(()) => {
                        self.complete(&task.id);
                        processed += 1;
                    }
                    Err(error) => {
                        let error_message = error.to_string();
                        self.fail_claimed_task(&task, error_message.as_str());
                        self.disown_claimed_tasks_after_failure(batch_iter.collect());
                        self.log_process_available(
                            "failed",
                            processed,
                            Some(error_message.as_str()),
                        );
                        return Err(error);
                    }
                }
            }
        }
    }

    pub fn recover_and_process(
        &mut self,
        runtime: &RuntimeConfig,
    ) -> Result<usize, TaskExecutionError> {
        if self.persisted_store.is_some() {
            self.reload_admin_from_store();
        }
        let recovered_tasks = self.current_owned_tasks();
        self.disown_all();
        for task in &recovered_tasks {
            self.log_task_event("task_recover", task, "recovered", None);
        }
        self.process_available(runtime)
    }

    fn disown_claimed_task(&mut self, task: &TaskQueueRecord) {
        if let Some(store) = &self.persisted_store {
            store.disown_task(&task.id);
            self.reload_admin_from_store();
            self.log_task_event("task_disown", task, "disowned", None);
            return;
        }

        if self.admin.disown(&task.id) {
            self.log_task_event("task_disown", task, "disowned", None);
        }
    }

    fn disown_claimed_tasks_after_failure(&mut self, remaining_batch: Vec<TaskQueueRecord>) {
        if self.persisted_store.is_some() {
            for task in self.current_owned_tasks() {
                self.disown_claimed_task(&task);
            }
            return;
        }

        for task in remaining_batch {
            self.disown_claimed_task(&task);
        }
    }

    fn reload_admin_from_store(&mut self) {
        if let Some(store) = &self.persisted_store {
            self.admin = load_admin_from_store(store);
        }
    }

    fn execute_claimed_task(
        &mut self,
        runtime: &RuntimeConfig,
        task: &TaskQueueRecord,
    ) -> Result<(), TaskExecutionError> {
        let task_target = queue_core::task_target(task);

        if let Some(result) = scanner_jobs::try_execute(self, runtime, task, task_target) {
            return result;
        }
        if let Some(result) = maintenance_jobs::try_execute(self, runtime, task, task_target) {
            return result;
        }
        if let Some(result) = index_jobs::try_execute(self, runtime, task, task_target) {
            return result;
        }
        if let Some(result) = import_jobs::try_execute(self, runtime, task) {
            return result;
        }

        Err(TaskExecutionError::unsupported_task(&task.simple_type))
    }

    fn fail_claimed_task(&mut self, task: &TaskQueueRecord, error_message: &str) {
        if let Some(store) = &self.persisted_store {
            let removed = store.delete_task(&task.id);
            self.reload_admin_from_store();
            if removed {
                self.log_task_event("task_fail", task, "failed", Some(error_message));
            }
            return;
        }

        if self.admin.complete(&task.id) {
            self.log_task_event("task_fail", task, "failed", Some(error_message));
        }
    }

    fn current_task(&self, task_id: &str) -> Option<TaskQueueRecord> {
        self.persisted_store
            .as_ref()
            .and_then(|store| store_record_by_id(store, task_id))
    }

    fn current_owned_tasks(&self) -> Vec<TaskQueueRecord> {
        self.persisted_store
            .as_ref()
            .map(|store| {
                store
                    .load_records()
                    .into_iter()
                    .filter(|task| task.owner.as_deref() == Some(self.consumer_owner.as_str()))
                    .map(record_to_runtime_task)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn log_process_available(&self, outcome: &str, processed: usize, error_message: Option<&str>) {
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

    fn log_task_event(
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
}

fn load_admin_from_store(store: &SqliteTaskQueueStore) -> TaskQueueAdmin {
    let mut admin = TaskQueueOrchestrator::new("runtime-store", true);
    for record in store.load_records() {
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

fn store_record_by_id(store: &SqliteTaskQueueStore, task_id: &str) -> Option<TaskQueueRecord> {
    store
        .load_records()
        .into_iter()
        .find(|record| record.id == task_id)
        .map(record_to_runtime_task)
}
