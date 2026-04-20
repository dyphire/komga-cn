use super::*;

#[async_trait]
pub trait TaskQueueService: Send + Sync {
    async fn enqueue_task_records(
        &self,
        task_records: Vec<TaskQueueRecord>,
        urgent: bool,
    ) -> Result<(), String>;

    async fn clear_unowned_tasks(&self) -> usize;

    async fn count_task_queue_by_type(&self) -> BTreeMap<String, usize>;

    async fn apply_task_pool_size(&self, value: usize) -> Result<(), String>;
}

#[async_trait]
pub trait ServerSettingsService: Send + Sync {
    async fn load_map(&self) -> Result<BTreeMap<String, Option<String>>, String>;

    async fn load_settings(&self) -> Result<PersistedServerSettings, String>;

    async fn apply_changes(&self, changes: &[(String, Option<String>)]) -> Result<(), String>;
}
