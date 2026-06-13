use std::collections::BTreeMap;

pub type ClientGlobalSettings = BTreeMap<String, ClientGlobalSetting>;
pub type ClientUserSettings = BTreeMap<String, ClientUserSetting>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientGlobalSetting {
    pub value: String,
    pub allow_unauthorized: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClientUserSetting {
    pub value: String,
}

#[async_trait::async_trait]
pub trait ClientSettingsPort: Send + Sync {
    async fn load_client_settings_global(
        &self,
        allow_unauthorized_only: bool,
    ) -> Result<ClientGlobalSettings, String>;
    async fn load_client_settings_user(&self, user_id: &str) -> Result<ClientUserSettings, String>;
    async fn upsert_client_settings_global(
        &self,
        settings: &ClientGlobalSettings,
    ) -> Result<(), String>;
    async fn upsert_client_settings_user(
        &self,
        user_id: &str,
        settings: &ClientUserSettings,
    ) -> Result<(), String>;
    async fn delete_client_settings_global(&self, keys: &[String]) -> Result<(), String>;
    async fn delete_client_settings_user(
        &self,
        user_id: &str,
        keys: &[String],
    ) -> Result<(), String>;
}
