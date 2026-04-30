use super::*;

#[async_trait]
pub trait ServerSettingsService: Send + Sync {
    async fn load_map(&self) -> Result<BTreeMap<String, Option<String>>, String>;

    async fn load_settings(&self) -> Result<PersistedServerSettings, String>;

    async fn apply_changes(&self, changes: &[(String, Option<String>)]) -> Result<(), String>;
}
