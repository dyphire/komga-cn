#[async_trait::async_trait]
pub trait AnnouncementPort: Send + Sync {
    async fn load_announcement_read_ids(&self, user_id: &str) -> anyhow::Result<Vec<String>>;
    async fn save_announcements_read(&self, user_id: &str, ids: &[String]) -> anyhow::Result<()>;
}
