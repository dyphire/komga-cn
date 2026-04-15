use std::collections::BTreeMap;
use std::env;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use super::cli_args::DEFAULT_BIND_ADDRESS;
use super::error::ConfigError;
use super::path_resolution::{
    default_log_file_for_config_dir, resolve_admin_action_config_with_env,
    resolve_runtime_config_with_env,
};
use super::profile::{PlatformProfile, RuntimeMode, RuntimeProfile, DEFAULT_CONFIG_DIR};
use super::startup_policy::{
    ensure_startup_runtime_layout, validate_single_writer_storage_ownership,
};
use super::writer_ownership::WriterOwnershipPolicy;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OAuth2ClientConfig {
    pub registration_id: String,
    pub client_name: String,
    pub client_id: String,
    pub client_secret: String,
    pub authorization_uri: String,
    pub token_uri: String,
    pub scopes: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeConfig {
    pub bind_address: SocketAddr,
    pub mode: RuntimeMode,
    pub demo_mode: bool,
    pub oauth2_account_creation: bool,
    pub oidc_email_verification: bool,
    pub runtime_profile: RuntimeProfile,
    pub platform_profile: PlatformProfile,
    pub config_dir: Option<PathBuf>,
    pub server_context_path: Option<String>,
    pub log_file: PathBuf,
    pub database_file: PathBuf,
    pub tasks_db_file: PathBuf,
    pub lucene_data_directory: PathBuf,
    pub fonts_data_directory: PathBuf,
    pub oauth2_clients: Vec<OAuth2ClientConfig>,
    pub writer_ownership_policy: WriterOwnershipPolicy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminActionConfig {
    pub(crate) database_file: PathBuf,
}

impl RuntimeConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let cli = super::cli_args::RuntimeCli::default();
        let env = env::vars().collect::<BTreeMap<_, _>>();
        let config = Self::resolve_with_env(&cli, &env)?;
        ensure_startup_runtime_layout(&config)?;
        Ok(config)
    }

    pub fn resolve_with_env(
        cli: &super::cli_args::RuntimeCli,
        env: &BTreeMap<String, String>,
    ) -> Result<Self, ConfigError> {
        resolve_runtime_config_with_env(cli, env)
    }

    pub fn for_runtime_profile(runtime_profile: RuntimeProfile) -> Self {
        let config_dir = PathBuf::from(DEFAULT_CONFIG_DIR);
        Self {
            bind_address: DEFAULT_BIND_ADDRESS
                .parse()
                .expect("default bind address should parse"),
            mode: match runtime_profile {
                RuntimeProfile::SnapshotAligned => RuntimeMode::Snapshot,
                RuntimeProfile::LiveLocaldb => RuntimeMode::Localdb,
            },
            demo_mode: false,
            oauth2_account_creation: false,
            oidc_email_verification: true,
            runtime_profile,
            platform_profile: PlatformProfile::Default,
            config_dir: Some(config_dir.clone()),
            server_context_path: Some(String::new()),
            log_file: default_log_file_for_config_dir(&config_dir),
            database_file: config_dir.join("database.sqlite"),
            tasks_db_file: config_dir.join("tasks.sqlite"),
            lucene_data_directory: config_dir.join("lucene"),
            fonts_data_directory: config_dir.join("fonts"),
            oauth2_clients: vec![],
            writer_ownership_policy: WriterOwnershipPolicy {
                isolation_root: None,
                allow_isolated_writes: false,
            },
        }
    }

    pub(crate) fn validate_single_writer_storage_ownership(
        &self,
        env: &BTreeMap<String, String>,
    ) -> Result<(), ConfigError> {
        validate_single_writer_storage_ownership(self, env)
    }
}

impl AdminActionConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let cli = super::cli_args::RuntimeCli::default();
        let env = env::vars().collect::<BTreeMap<_, _>>();
        Self::resolve_with_env(&cli, &env)
    }

    pub fn resolve_with_env(
        cli: &super::cli_args::RuntimeCli,
        env: &BTreeMap<String, String>,
    ) -> Result<Self, ConfigError> {
        resolve_admin_action_config_with_env(cli, env)
    }

    pub fn database_file(&self) -> &Path {
        self.database_file.as_path()
    }
}
