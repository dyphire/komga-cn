use std::collections::BTreeMap;

use async_trait::async_trait;

use super::PersistedServerSettings;

/// Port for reading and writing server settings.
#[async_trait]
pub trait ServerSettingsPort: Send + Sync {
    async fn load_map(&self) -> Result<BTreeMap<String, Option<String>>, String>;

    async fn load_settings(&self) -> Result<PersistedServerSettings, String>;

    async fn apply_changes(&self, changes: &[(String, Option<String>)]) -> Result<(), String>;
}
