use std::collections::BTreeMap;

use super::cli_args::RuntimeCli;
use super::env_config::{AdminActionConfig, RuntimeConfig};
use super::error::ConfigError;

mod runtime_resolution;
mod startup;

pub(crate) use startup::{
    default_log_file_for_config_dir, ensure_runtime_directories, is_default_home_config_dir,
    is_valid_startup_context_path, preferred_string, validate_temp_directory,
};

pub(crate) fn resolve_runtime_config_with_env(
    cli: &RuntimeCli,
    env: &BTreeMap<String, String>,
) -> Result<RuntimeConfig, ConfigError> {
    runtime_resolution::resolve_with_env(cli, env)
}

pub(crate) fn resolve_admin_action_config_with_env(
    cli: &RuntimeCli,
    env: &BTreeMap<String, String>,
) -> Result<AdminActionConfig, ConfigError> {
    runtime_resolution::resolve_admin_action_with_env(cli, env)
}
