use std::collections::BTreeMap;

use async_trait::async_trait;

use super::PersistedServerSettings;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerSettingChange {
    pub key: String,
    pub value: Option<String>,
}

impl ServerSettingChange {
    pub fn set(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: Some(value.into()),
        }
    }

    pub fn delete(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: None,
        }
    }
}

/// Port for reading and writing server settings.
#[async_trait]
pub trait ServerSettingsPort: Send + Sync {
    async fn load_map(&self) -> Result<BTreeMap<String, Option<String>>, String>;

    async fn load_settings(&self) -> Result<PersistedServerSettings, String>;

    async fn apply_changes(&self, changes: &[ServerSettingChange]) -> Result<(), String>;
}
