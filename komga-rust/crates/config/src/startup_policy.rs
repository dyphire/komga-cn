use std::collections::BTreeMap;

use super::env_config::RuntimeConfig;
use super::error::ConfigError;
use super::path_resolution::{
    ensure_runtime_directories, is_default_home_config_dir, validate_temp_directory,
};
use super::profile::RuntimeMode;

#[path = "cli/runtime_checks.rs"]
mod runtime_checks;

pub(crate) fn ensure_startup_runtime_layout(config: &RuntimeConfig) -> Result<(), ConfigError> {
    runtime_checks::ensure_startup_runtime_layout(config)
}

pub(crate) fn validate_single_writer_storage_ownership(
    config: &RuntimeConfig,
    env: &BTreeMap<String, String>,
) -> Result<(), ConfigError> {
    runtime_checks::validate_single_writer_storage_ownership(config, env)
}
