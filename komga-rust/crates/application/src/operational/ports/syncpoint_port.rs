#[async_trait::async_trait]
pub trait SyncpointPort: Send + Sync {
    async fn delete_syncpoints_by_user(&self, user_id: &str) -> anyhow::Result<()>;
    async fn delete_syncpoints_by_user_and_key_ids(
        &self,
        user_id: &str,
        key_ids: &[String],
    ) -> anyhow::Result<()>;
}
