use std::collections::BTreeMap;
use std::path::PathBuf;

use super::error::ConfigError;

pub(super) const DEFAULT_CONFIG_DIR: &str = ".komga";
pub(super) const DEFAULT_LOG_FILE_NAME: &str = "komga.log";
const HOME_ENV: &str = "HOME";
const LOCALAPPDATA_ENV: &str = "LOCALAPPDATA";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeMode {
    Snapshot,
    Localdb,
    Shadow,
    Canary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatProfile {
    SnapshotAligned,
    JavaLiveLocaldb,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformProfile {
    Default,
    Docker,
    Mac,
    Windows,
}

impl RuntimeMode {
    pub(super) fn parse(value: &str) -> Result<Self, ConfigError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "snapshot" => Ok(Self::Snapshot),
            "localdb" => Ok(Self::Localdb),
            "shadow" => Ok(Self::Shadow),
            "canary" => Ok(Self::Canary),
            other => Err(ConfigError::InvalidMode(other.to_string())),
        }
    }

    pub(super) fn default_compat_profile(self) -> CompatProfile {
        match self {
            Self::Localdb => CompatProfile::JavaLiveLocaldb,
            Self::Snapshot | Self::Shadow | Self::Canary => CompatProfile::SnapshotAligned,
        }
    }
}

impl CompatProfile {
    pub(super) fn parse(value: &str) -> Result<Self, ConfigError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "snapshot-aligned" | "snapshot" => Ok(Self::SnapshotAligned),
            "java-live-localdb" | "localdb" => Ok(Self::JavaLiveLocaldb),
            other => Err(ConfigError::InvalidCompatProfile(other.to_string())),
        }
    }
}

impl PlatformProfile {
    pub(super) fn parse(value: &str) -> Result<Self, ConfigError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "default" => Ok(Self::Default),
            "docker" => Ok(Self::Docker),
            "mac" => Ok(Self::Mac),
            "windows" => Ok(Self::Windows),
            other => Err(ConfigError::InvalidPlatformProfile(other.to_string())),
        }
    }

    pub(super) fn default_config_dir(self, env: &BTreeMap<String, String>) -> Option<PathBuf> {
        match self {
            Self::Default | Self::Docker => env
                .get(HOME_ENV)
                .map(|home| PathBuf::from(home).join(DEFAULT_CONFIG_DIR))
                .or_else(|| Some(PathBuf::from(DEFAULT_CONFIG_DIR))),
            Self::Mac => env.get(HOME_ENV).map(|home| {
                PathBuf::from(home)
                    .join("Library")
                    .join("Application Support")
                    .join("Komga")
            }),
            Self::Windows => env
                .get(LOCALAPPDATA_ENV)
                .map(|root| PathBuf::from(root).join("Komga")),
        }
    }

    pub(super) fn default_log_file(self, env: &BTreeMap<String, String>) -> Option<PathBuf> {
        match self {
            Self::Mac => env.get(HOME_ENV).map(|home| {
                PathBuf::from(home)
                    .join("Library")
                    .join("Logs")
                    .join("Komga")
                    .join(DEFAULT_LOG_FILE_NAME)
            }),
            _ => None,
        }
    }

    pub(super) fn default_kepubify_path(self) -> Option<PathBuf> {
        match self {
            Self::Docker => Some(PathBuf::from("/usr/bin/kepubify")),
            Self::Mac => Some(PathBuf::from("kepubify")),
            Self::Windows => Some(PathBuf::from("kepubify.exe")),
            Self::Default => None,
        }
    }
}
