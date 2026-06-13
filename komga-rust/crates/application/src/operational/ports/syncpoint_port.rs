#[async_trait::async_trait]
pub trait SyncpointPort: Send + Sync {
    async fn delete_syncpoints_by_user(&self, user_id: &str) -> Result<(), String>;
    async fn delete_syncpoints_by_user_and_key_ids(
        &self,
        user_id: &str,
        key_ids: &[String],
    ) -> Result<(), String>;
}
