use async_trait::async_trait;

#[async_trait]
pub trait AnnouncementPort: Send + Sync {
    async fn load_announcement_read_ids(&self, user_id: &str) -> Result<Vec<String>, String>;
    async fn save_announcements_read(&self, user_id: &str, ids: &[String]) -> Result<(), String>;
}
