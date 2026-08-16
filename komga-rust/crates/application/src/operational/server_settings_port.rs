use std::collections::BTreeMap;

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
#[async_trait::async_trait]
pub trait ServerSettingsPort: Send + Sync {
    async fn load_map(&self) -> anyhow::Result<BTreeMap<String, Option<String>>>;

    async fn load_settings(&self) -> anyhow::Result<PersistedServerSettings>;

    async fn apply_changes(&self, changes: &[ServerSettingChange]) -> anyhow::Result<()>;
}
