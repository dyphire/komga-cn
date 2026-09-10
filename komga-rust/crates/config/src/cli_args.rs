use std::path::PathBuf;

pub(crate) const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1:25600";
pub(crate) const ADDR_ENV: &str = "KOMGA_RUST_ADDR";
pub(crate) const MODE_ENV: &str = "KOMGA_RUST_MODE";
pub(crate) const CONFIG_DIR_ENV: &str = "KOMGA_CONFIG_DIR";
pub(crate) const RUNTIME_PROFILE_ENV: &str = "KOMGA_RUST_RUNTIME_PROFILE";
pub(crate) const PLATFORM_PROFILE_ENV: &str = "KOMGA_RUST_PLATFORM_PROFILE";
pub(crate) const SPRING_PROFILES_ACTIVE_ENV: &str = "SPRING_PROFILES_ACTIVE";
pub(crate) const SERVER_PORT_ENV: &str = "SERVER_PORT";
pub(crate) const SERVER_CONTEXT_PATH_ENV: &str = "SERVER_SERVLET_CONTEXT_PATH";
pub(crate) const WRITER_ISOLATION_ROOT_ENV: &str = "KOMGA_RUST_SHADOW_ISOLATION_ROOT";
pub(crate) const ALLOW_ISOLATED_WRITES_ENV: &str = "KOMGA_RUST_ALLOW_SHADOW_WRITES";
pub(crate) const LOG_FILE_ENV: &str = "LOGGING_FILE_NAME";
pub(crate) const DATABASE_FILE_ENV: &str = "KOMGA_DATABASE_FILE";
pub(crate) const TASKS_DB_FILE_ENV: &str = "KOMGA_TASKS_DB_FILE";
pub(crate) const LUCENE_DATA_DIRECTORY_ENV: &str = "KOMGA_LUCENE_DATA_DIRECTORY";
pub(crate) const FONTS_DATA_DIRECTORY_ENV: &str = "KOMGA_FONTS_DATA_DIRECTORY";
pub(crate) const SORT_LOCALE_ENV: &str = "KOMGA_SORT_LOCALE";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCli {
    pub address: Option<String>,
    pub mode: Option<String>,
    pub runtime_profile: Option<String>,
    pub platform_profile: Option<String>,
    pub config_dir: Option<PathBuf>,
    pub log_file: Option<PathBuf>,
    pub writer_isolation_root: Option<PathBuf>,
    pub allow_isolated_writes: bool,
}
