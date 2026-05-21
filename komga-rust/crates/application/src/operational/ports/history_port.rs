use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait HistoryPort: Send + Sync {
    async fn load_history_page(
        &self,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, String>;
}
