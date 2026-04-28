use config::{Config as LayeredConfig, Environment, File as ConfigFile, FileFormat};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use super::cli_args::*;
use super::env_config::{
    AdminActionConfig, DEFAULT_SESSION_MAX_INACTIVE_SECONDS, OAuth2ClientConfig, RuntimeConfig,
};
use super::error::ConfigError;
use super::profile::{
    DEFAULT_CONFIG_DIR, DEFAULT_LOG_FILE_NAME, PlatformProfile, RuntimeMode, RuntimeProfile,
};
use super::writer_ownership::WriterOwnershipPolicy;

mod runtime_resolution;
mod startup;

pub(crate) use startup::{
    build_layered_config, default_home_config_dir, default_log_file_for_config_dir,
    ensure_runtime_directories, expand_path_placeholders, is_default_home_config_dir,
    is_valid_startup_context_path, path_to_string, preferred_string, read_string,
    resolve_bind_address_and_context_path, resolve_derived_runtime_paths,
    resolve_oauth2_clients_for_startup_slice, resolve_writer_ownership_policy_for_startup_slice,
    validate_temp_directory,
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
