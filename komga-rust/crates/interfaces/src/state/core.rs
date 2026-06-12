use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeProfile {
    SnapshotAligned,
    LiveLocaldb,
}

#[derive(Clone)]
pub struct AuthDatabaseState {
    pub database_file: PathBuf,
    pub demo_mode: bool,
    pub session_runtime_key: String,
    pub remember_me_runtime_key: String,
}

#[derive(Clone)]
pub struct RuntimeState {
    pub tasks_db_file: PathBuf,
    pub lucene_data_directory: PathBuf,
    pub fonts_data_directory: PathBuf,
    pub log_file: PathBuf,
    pub config_dir: Option<PathBuf>,
    pub bind_address: SocketAddr,
    pub configuration_bind_address: SocketAddr,
    pub server_context_path: Option<String>,
    pub configuration_server_context_path: Option<String>,
    pub actuator_enabled: bool,
    pub dev_cors_enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationalBuildMetadata {
    pub version: String,
    pub build_time: String,
    pub git_branch: Option<String>,
    pub git_commit_id: Option<String>,
    pub git_commit_time: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OAuth2ClientConfig {
    pub registration_id: String,
    pub client_name: String,
    pub client_id: String,
    pub client_secret: String,
    pub authorization_uri: Option<String>,
    pub token_uri: Option<String>,
    pub user_info_uri: Option<String>,
    pub issuer_uri: Option<String>,
    pub jwk_set_uri: Option<String>,
    pub redirect_uri: Option<String>,
    pub client_authentication_method: Option<String>,
    pub scopes: Vec<String>,
}
