use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait ClientSettingsPort: Send + Sync {
    async fn load_client_settings_global(
        &self,
        allow_unauthorized_only: bool,
    ) -> Result<Value, String>;
    async fn load_client_settings_user(&self, user_id: &str) -> Result<Value, String>;
    async fn upsert_client_settings_global(
        &self,
        settings: &[(String, String, bool)],
    ) -> Result<(), String>;
    async fn upsert_client_settings_user(
        &self,
        user_id: &str,
        settings: &[(String, String)],
    ) -> Result<(), String>;
    async fn delete_client_settings_global(&self, keys: &[String]) -> Result<(), String>;
    async fn delete_client_settings_user(
        &self,
        user_id: &str,
        keys: &[String],
    ) -> Result<(), String>;
}
