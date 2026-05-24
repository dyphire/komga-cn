use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::task_processing::TaskQueueAdmin;

use super::ServerSettingsPort;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedServerSettings {
    pub delete_empty_collections: bool,
    pub delete_empty_read_lists: bool,
    pub remember_me_key: String,
    pub remember_me_duration_days: u64,
    pub thumbnail_size: &'static str,
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
    pub thumbnail_size: Option<String>,
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
        let mut persistence_changes: Vec<(String, Option<String>)> = Vec::new();
        let mut task_pool_size_change: Option<u64> = None;

        if let Some(value) = command.delete_empty_collections {
            settings.delete_empty_collections = value;
            persistence_changes.push((
                "DELETE_EMPTY_COLLECTIONS".to_string(),
                Some(value.to_string()),
            ));
        }

        if let Some(value) = command.delete_empty_read_lists {
            settings.delete_empty_read_lists = value;
            persistence_changes.push((
                "DELETE_EMPTY_READLISTS".to_string(),
                Some(value.to_string()),
            ));
        }

        if let Some(value) = command.remember_me_duration_days {
            if value == 0 {
                return invalid_payload("rememberMeDurationDays must be greater than 0");
            }
            settings.remember_me_duration_days = value;
            persistence_changes.push(("REMEMBER_ME_DURATION".to_string(), Some(value.to_string())));
        }

        if command.renew_remember_me_key == Some(true) {
            settings.remember_me_key = generate_remember_me_key();
            persistence_changes.push((
                "REMEMBER_ME_KEY".to_string(),
                Some(settings.remember_me_key.clone()),
            ));
        }

        if let Some(value) = command.thumbnail_size {
            if !matches!(value.as_str(), "DEFAULT" | "MEDIUM" | "LARGE" | "XLARGE") {
                return invalid_payload("thumbnailSize is invalid");
            }
            settings.thumbnail_size = match value.as_str() {
                "DEFAULT" => "DEFAULT",
                "MEDIUM" => "MEDIUM",
                "LARGE" => "LARGE",
                "XLARGE" => "XLARGE",
                _ => unreachable!(),
            };
            persistence_changes.push((
                "THUMBNAIL_SIZE".to_string(),
                Some(settings.thumbnail_size.to_string()),
            ));
        }

        if let Some(value) = command.task_pool_size {
            if value == 0 {
                return invalid_payload("taskPoolSize must be greater than 0");
            }
            settings.task_pool_size = value;
            task_pool_size_change = Some(value);
            persistence_changes.push(("TASK_POOL_SIZE".to_string(), Some(value.to_string())));
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
                persistence_changes.push((
                    "SERVER_PORT".to_string(),
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
                        if !is_valid_context_path(&value) {
                            return invalid_payload("serverContextPath is invalid");
                        }
                        settings.server_context_path = Some(value);
                    }
                }
                persistence_changes.push((
                    "SERVER_CONTEXT_PATH".to_string(),
                    settings.server_context_path.clone(),
                ));
            }
        }

        if let Some(value) = command.kobo_proxy {
            settings.kobo_proxy = value;
            persistence_changes.push(("KOBO_PROXY".to_string(), Some(value.to_string())));
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
                persistence_changes.push((
                    "KOBO_PORT".to_string(),
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

fn invalid_payload<T>(message: &str) -> Result<T, ServerSettingsUpdateError> {
    Err(ServerSettingsUpdateError::InvalidPayload(
        message.to_string(),
    ))
}

fn generate_remember_me_key() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
    let raw = format!("{nanos:032x}{sequence:016x}");
    raw.chars().take(32).collect()
}

fn is_valid_context_path(value: &str) -> bool {
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
