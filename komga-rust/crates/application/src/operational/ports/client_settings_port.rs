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
    ) -> anyhow::Result<ClientGlobalSettings>;
    async fn load_client_settings_user(&self, user_id: &str) -> anyhow::Result<ClientUserSettings>;
    async fn upsert_client_settings_global(
        &self,
        settings: &ClientGlobalSettings,
    ) -> anyhow::Result<()>;
    async fn upsert_client_settings_user(
        &self,
        user_id: &str,
        settings: &ClientUserSettings,
    ) -> anyhow::Result<()>;
    async fn delete_client_settings_global(&self, keys: &[String]) -> anyhow::Result<()>;
    async fn delete_client_settings_user(
        &self,
        user_id: &str,
        keys: &[String],
    ) -> anyhow::Result<()>;
}
