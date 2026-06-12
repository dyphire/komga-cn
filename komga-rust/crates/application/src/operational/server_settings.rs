use std::sync::Arc;

use crate::random_tokens::random_hex_token;
use crate::task_processing::TaskQueueAdmin;

use super::{ServerSettingChange, ServerSettingsPort};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedServerSettings {
    pub delete_empty_collections: bool,
    pub delete_empty_read_lists: bool,
    pub remember_me_key: String,
    pub remember_me_duration_days: u64,
    pub thumbnail_size: ThumbnailSize,
    pub task_pool_size: u64,
    pub server_port: Option<u16>,
    pub server_context_path: Option<String>,
    pub kobo_proxy: bool,
    pub kobo_port: Option<u16>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ServerSettingsUpdateCommand {
    pub delete_empty_collections: Option<bool>,
    pub delete_empty_read_lists: Option<bool>,
    pub remember_me_duration_days: Option<u64>,
    pub renew_remember_me_key: Option<bool>,
    pub thumbnail_size: Option<ThumbnailSize>,
    pub task_pool_size: Option<u64>,
    pub server_port: ServerSettingPatch<u64>,
    pub server_context_path: ServerSettingPatch<String>,
    pub kobo_proxy: Option<bool>,
    pub kobo_port: ServerSettingPatch<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum ServerSettingPatch<T> {
    #[default]
    Unchanged,
    Clear,
    Set(T),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ThumbnailSize {
    #[default]
    Default,
    Medium,
    Large,
    XLarge,
}

impl ThumbnailSize {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "DEFAULT" => Some(Self::Default),
            "MEDIUM" => Some(Self::Medium),
            "LARGE" => Some(Self::Large),
            "XLARGE" => Some(Self::XLarge),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "DEFAULT",
            Self::Medium => "MEDIUM",
            Self::Large => "LARGE",
            Self::XLarge => "XLARGE",
        }
    }

    pub fn max_edge(self) -> u32 {
        match self {
            Self::Default => 300,
            Self::Medium => 600,
            Self::Large => 900,
            Self::XLarge => 1200,
        }
    }
}

#[derive(Clone)]
pub struct ServerSettingsService {
    settings: Arc<dyn ServerSettingsPort>,
    task_queue: Arc<dyn TaskQueueAdmin>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServerSettingsLoadError {
    Load(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServerSettingsUpdateError {
    InvalidPayload(String),
    Load(String),
    Persist(String),
    ApplyTaskPool(String),
}

impl ServerSettingsService {
    pub fn new(settings: Arc<dyn ServerSettingsPort>, task_queue: Arc<dyn TaskQueueAdmin>) -> Self {
        Self {
            settings,
            task_queue,
        }
    }

    pub async fn load(&self) -> Result<PersistedServerSettings, ServerSettingsLoadError> {
        self.settings
            .load_settings()
            .await
            .map_err(ServerSettingsLoadError::Load)
    }

    pub async fn update(
        &self,
        command: ServerSettingsUpdateCommand,
    ) -> Result<(), ServerSettingsUpdateError> {
        let mut settings = self
            .settings
            .load_settings()
            .await
            .map_err(ServerSettingsUpdateError::Load)?;
        let mut persistence_changes = Vec::<ServerSettingChange>::new();
        let mut task_pool_size_change: Option<u64> = None;

        if let Some(value) = command.delete_empty_collections {
            settings.delete_empty_collections = value;
            persistence_changes.push(ServerSettingChange::set(
                "DELETE_EMPTY_COLLECTIONS",
                value.to_string(),
            ));
        }

        if let Some(value) = command.delete_empty_read_lists {
            settings.delete_empty_read_lists = value;
            persistence_changes.push(ServerSettingChange::set(
                "DELETE_EMPTY_READLISTS",
                value.to_string(),
            ));
        }

        if let Some(value) = command.remember_me_duration_days {
            if value == 0 {
                return invalid_payload("rememberMeDurationDays must be greater than 0");
            }
            settings.remember_me_duration_days = value;
            persistence_changes.push(ServerSettingChange::set(
                "REMEMBER_ME_DURATION",
                value.to_string(),
            ));
        }

        if command.renew_remember_me_key == Some(true) {
            settings.remember_me_key = generate_remember_me_key();
            persistence_changes.push(ServerSettingChange::set(
                "REMEMBER_ME_KEY",
                settings.remember_me_key.clone(),
            ));
        }

        if let Some(value) = command.thumbnail_size {
            settings.thumbnail_size = value;
            persistence_changes.push(ServerSettingChange::set(
                "THUMBNAIL_SIZE",
                settings.thumbnail_size.as_str().to_string(),
            ));
        }

        if let Some(value) = command.task_pool_size {
            if value == 0 {
                return invalid_payload("taskPoolSize must be greater than 0");
            }
            settings.task_pool_size = value;
            task_pool_size_change = Some(value);
            persistence_changes.push(ServerSettingChange::set(
                "TASK_POOL_SIZE",
                value.to_string(),
            ));
        }

        match command.server_port {
            ServerSettingPatch::Unchanged => {}
            patch => {
                match patch {
                    ServerSettingPatch::Unchanged => {}
                    ServerSettingPatch::Clear => settings.server_port = None,
                    ServerSettingPatch::Set(value) => {
                        if !(1..=65535).contains(&value) {
                            return invalid_payload(
                                "serverPort must be an integer between 1 and 65535",
                            );
                        }
                        settings.server_port = Some(value as u16);
                    }
                }
                persistence_changes.push(server_setting_change(
                    "SERVER_PORT",
                    settings.server_port.map(|value| value.to_string()),
                ));
            }
        }

        match command.server_context_path {
            ServerSettingPatch::Unchanged => {}
            patch => {
                match patch {
                    ServerSettingPatch::Unchanged => {}
                    ServerSettingPatch::Clear => settings.server_context_path = None,
                    ServerSettingPatch::Set(value) => {
                        if !is_valid_server_context_path(&value) {
                            return invalid_payload("serverContextPath is invalid");
                        }
                        settings.server_context_path = Some(value);
                    }
                }
                persistence_changes.push(server_setting_change(
                    "SERVER_CONTEXT_PATH",
                    settings.server_context_path.clone(),
                ));
            }
        }

        if let Some(value) = command.kobo_proxy {
            settings.kobo_proxy = value;
            persistence_changes.push(ServerSettingChange::set("KOBO_PROXY", value.to_string()));
        }

        match command.kobo_port {
            ServerSettingPatch::Unchanged => {}
            patch => {
                match patch {
                    ServerSettingPatch::Unchanged => {}
                    ServerSettingPatch::Clear => settings.kobo_port = None,
                    ServerSettingPatch::Set(value) => {
                        if !(1..=65535).contains(&value) {
                            return invalid_payload(
                                "koboPort must be an integer between 1 and 65535",
                            );
                        }
                        settings.kobo_port = Some(value as u16);
                    }
                }
                persistence_changes.push(server_setting_change(
                    "KOBO_PORT",
                    settings.kobo_port.map(|value| value.to_string()),
                ));
            }
        }

        self.settings
            .apply_changes(&persistence_changes)
            .await
            .map_err(ServerSettingsUpdateError::Persist)?;

        if let Some(value) = task_pool_size_change {
            self.task_queue
                .apply_pool_size(value as usize)
                .await
                .map_err(ServerSettingsUpdateError::ApplyTaskPool)?;
        }

        Ok(())
    }
}

fn server_setting_change(key: &str, value: Option<String>) -> ServerSettingChange {
    match value {
        Some(value) => ServerSettingChange::set(key, value),
        None => ServerSettingChange::delete(key),
    }
}

fn invalid_payload<T>(message: &str) -> Result<T, ServerSettingsUpdateError> {
    Err(ServerSettingsUpdateError::InvalidPayload(
        message.to_string(),
    ))
}

fn generate_remember_me_key() -> String {
    random_hex_token(32)
}

pub fn is_valid_server_context_path(value: &str) -> bool {
    if value.is_empty() || !value.starts_with('/') || value.ends_with('/') {
        return false;
    }

    let Some(last) = value.chars().last() else {
        return false;
    };
    if !last.is_ascii_alphanumeric() {
        return false;
    }

    value
        .chars()
        .all(|ch| ch == '/' || ch == '-' || ch == '_' || ch.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnail_size_is_a_typed_setting_value() {
        assert_eq!(
            ThumbnailSize::parse("DEFAULT"),
            Some(ThumbnailSize::Default)
        );
        assert_eq!(ThumbnailSize::parse("MEDIUM"), Some(ThumbnailSize::Medium));
        assert_eq!(ThumbnailSize::parse("LARGE"), Some(ThumbnailSize::Large));
        assert_eq!(ThumbnailSize::parse("XLARGE"), Some(ThumbnailSize::XLarge));
        assert_eq!(ThumbnailSize::parse("small"), None);
        assert_eq!(ThumbnailSize::XLarge.as_str(), "XLARGE");
        assert_eq!(ThumbnailSize::XLarge.max_edge(), 1200);
    }

    #[test]
    fn generated_remember_me_key_uses_32_bytes_of_hex_entropy() {
        let key = generate_remember_me_key();

        assert_eq!(key.len(), 64);
        assert!(key.chars().all(|ch| ch.is_ascii_hexdigit()));
    }
}
