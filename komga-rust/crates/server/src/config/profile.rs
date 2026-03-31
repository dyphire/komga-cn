use std::collections::BTreeMap;
use std::path::PathBuf;

use super::error::ConfigError;

pub(super) const DEFAULT_CONFIG_DIR: &str = ".komga";
pub(super) const DEFAULT_LOG_FILE_NAME: &str = "komga.log";
const HOME_ENV: &str = "HOME";
const USERPROFILE_ENV: &str = "USERPROFILE";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeMode {
    Snapshot,
    Localdb,
    Isolated,
    Canary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeProfile {
    SnapshotAligned,
    LiveLocaldb,
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
            "isolated" => Ok(Self::Isolated),
            "canary" => Ok(Self::Canary),
            other => Err(ConfigError::InvalidMode(other.to_string())),
        }
    }

    pub(super) fn default_runtime_profile(self) -> RuntimeProfile {
        match self {
            Self::Localdb => RuntimeProfile::LiveLocaldb,
            Self::Snapshot | Self::Isolated | Self::Canary => RuntimeProfile::SnapshotAligned,
        }
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Snapshot => "snapshot",
            Self::Localdb => "localdb",
            Self::Isolated => "isolated",
            Self::Canary => "canary",
        }
    }
}

impl RuntimeProfile {
    pub(super) fn parse(value: &str) -> Result<Self, ConfigError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "snapshot-aligned" | "snapshot" => Ok(Self::SnapshotAligned),
            "java-live-localdb" | "localdb" => Ok(Self::LiveLocaldb),
            other => Err(ConfigError::InvalidRuntimeProfile(other.to_string())),
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
            Self::Default | Self::Docker | Self::Mac | Self::Windows => env
                .get(HOME_ENV)
                .or_else(|| env.get(USERPROFILE_ENV))
                .map(|home| PathBuf::from(home).join(DEFAULT_CONFIG_DIR))
                .or_else(|| Some(PathBuf::from(DEFAULT_CONFIG_DIR))),
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
}
