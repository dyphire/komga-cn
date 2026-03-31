use super::*;
use komga_application::task_processing::TaskQueueAdminPort;

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
        let persisted_store = SqliteTaskQueueStore::new(runtime.tasks_db_file.clone());
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
            return;
        }
        self.admin.enqueue(task);
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
            selected.push(task);
        }
        self.reload_admin_from_store();
        selected
    }

    pub fn complete(&mut self, task_id: &str) -> bool {
        if let Some(store) = &self.persisted_store {
            let removed = store.delete_task(task_id);
            self.reload_admin_from_store();
            return removed;
        }

        self.admin.complete(task_id)
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
            let disowned = self.admin.disown_all();
            if let Some(store) = &self.persisted_store {
                store.disown_all();
            }
            self.reload_admin_from_store();
            return disowned;
        }

        self.admin.disown_all()
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
        loop {
            let batch = self.take_available_batch();
            if batch.is_empty() {
                return Ok(processed);
            }

            let mut batch_iter = batch.into_iter();
            while let Some(task) = batch_iter.next() {
                match self.execute_claimed_task(runtime, &task) {
                    Ok(()) => {
                        let _ = self.complete(&task.id);
                        processed += 1;
                    }
                    Err(error) => {
                        if error.is_unsupported_task() {
                            let _ = self.complete(&task.id);
                            continue;
                        }
                        self.disown_task(&task.id);
                        for remaining in batch_iter {
                            self.disown_task(&remaining.id);
                        }
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
        self.disown_all();
        self.process_available(runtime)
    }

    fn disown_task(&mut self, task_id: &str) {
        if let Some(store) = &self.persisted_store {
            store.disown_task(task_id);
            self.reload_admin_from_store();
            return;
        }

        let _ = self.admin.disown(task_id);
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
}

fn load_admin_from_store(store: &SqliteTaskQueueStore) -> TaskQueueAdmin {
    let mut admin = TaskQueueOrchestrator::new("runtime-store", true);
    for record in store.load_records() {
        let owner = record.owner.clone();
        let task = TaskQueueRecord {
            id: record.id,
            simple_type: record.simple_type,
            priority: record.priority,
            group: record.group,
            payload: record.payload,
            owner: None,
            order: 0,
        };
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
