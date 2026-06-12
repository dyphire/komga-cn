use std::collections::BTreeMap;

use super::env_config::RuntimeConfig;
use super::error::ConfigError;

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
