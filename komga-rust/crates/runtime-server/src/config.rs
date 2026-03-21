use crate::app;
use std::collections::BTreeMap;
use std::env;
use std::net::SocketAddr;
use std::path::PathBuf;

const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1:25600";
const ADDR_ENV: &str = "KOMGA_RUST_ADDR";
const MODE_ENV: &str = "KOMGA_RUST_MODE";
const CONFIG_DIR_ENV: &str = "KOMGA_CONFIG_DIR";
const COMPAT_PROFILE_ENV: &str = "KOMGA_RUST_COMPAT_PROFILE";
const PLATFORM_PROFILE_ENV: &str = "KOMGA_RUST_PLATFORM_PROFILE";
const SERVER_CONTEXT_PATH_ENV: &str = "SERVER_SERVLET_CONTEXT_PATH";
const SHADOW_ISOLATION_ROOT_ENV: &str = "KOMGA_RUST_SHADOW_ISOLATION_ROOT";
const ALLOW_SHADOW_WRITES_ENV: &str = "KOMGA_RUST_ALLOW_SHADOW_WRITES";
const LOG_FILE_ENV: &str = "LOGGING_FILE_NAME";
const KEPUBIFY_PATH_ENV: &str = "KOMGA_KEPUBIFY_PATH";
const SHADOW_BLOCK_REASON: &str = "shadow mode requires explicit isolation or opt-in";
const SEARCH_INDEX_OWNERSHIP_REASON: &str =
    "search index ownership remains with java writer in shadow mode";
const DEFAULT_CONFIG_DIR: &str = ".komga";
const DEFAULT_LOG_FILE_NAME: &str = "komga.log";
const HOME_ENV: &str = "HOME";
const LOCALAPPDATA_ENV: &str = "LOCALAPPDATA";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RuntimeCli {
    pub address: Option<String>,
    pub mode: Option<String>,
    pub compat_profile: Option<String>,
    pub platform_profile: Option<String>,
    pub config_dir: Option<PathBuf>,
    pub log_file: Option<PathBuf>,
    pub kepubify_path: Option<PathBuf>,
    pub shadow_isolation_root: Option<PathBuf>,
    pub allow_shadow_writes: bool,
}

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShadowPolicy {
    pub isolation_root: Option<PathBuf>,
    pub allow_shadow_writes: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeConfig {
    pub bind_address: SocketAddr,
    pub mode: RuntimeMode,
    pub compat_profile: CompatProfile,
    pub platform_profile: PlatformProfile,
    pub config_dir: Option<PathBuf>,
    pub server_context_path: Option<String>,
    pub log_file: PathBuf,
    pub kepubify_path: Option<PathBuf>,
    pub shadow_policy: ShadowPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriterKind {
    MainDatabase,
    TasksDatabase,
    SearchIndex,
    FilesystemScanOutput,
    SidecarOutput,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriterDecision {
    Allowed,
    Isolated,
    Blocked { reason: &'static str },
}

impl WriterDecision {
    pub fn allows_write(self) -> bool {
        matches!(self, Self::Allowed | Self::Isolated)
    }
}

impl RuntimeConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let cli = RuntimeCli::default();
        let env = env::vars().collect::<BTreeMap<_, _>>();
        Self::resolve_with_env(&cli, &env)
    }

    pub fn resolve_with_env(
        cli: &RuntimeCli,
        env: &BTreeMap<String, String>,
    ) -> Result<Self, ConfigError> {
        let bind_address = preferred_string(
            cli.address.as_deref(),
            env.get(ADDR_ENV).map(String::as_str),
        )
        .unwrap_or(DEFAULT_BIND_ADDRESS)
        .parse()
        .map_err(ConfigError::InvalidAddress)?;

        let mode = preferred_string(cli.mode.as_deref(), env.get(MODE_ENV).map(String::as_str))
            .map(RuntimeMode::parse)
            .transpose()?
            .unwrap_or(RuntimeMode::Snapshot);

        let compat_profile = preferred_string(
            cli.compat_profile.as_deref(),
            env.get(COMPAT_PROFILE_ENV).map(String::as_str),
        )
        .map(CompatProfile::parse)
        .transpose()?
        .unwrap_or_else(|| mode.default_compat_profile());

        let platform_profile = preferred_string(
            cli.platform_profile.as_deref(),
            env.get(PLATFORM_PROFILE_ENV).map(String::as_str),
        )
        .map(PlatformProfile::parse)
        .transpose()?
        .unwrap_or(PlatformProfile::Default);

        let config_dir = cli
            .config_dir
            .clone()
            .or_else(|| env.get(CONFIG_DIR_ENV).map(PathBuf::from))
            .or_else(|| platform_profile.default_config_dir(env));

        let server_context_path =
            preferred_string(None, env.get(SERVER_CONTEXT_PATH_ENV).map(String::as_str))
                .map(str::to_string)
                .or_else(|| Some(String::new()));

        let log_file = cli
            .log_file
            .clone()
            .or_else(|| env.get(LOG_FILE_ENV).map(PathBuf::from))
            .or_else(|| platform_profile.default_log_file(env))
            .or_else(|| config_dir.as_ref().map(default_log_file_for_config_dir))
            .unwrap_or_else(|| default_log_file_for_config_dir(&PathBuf::from(DEFAULT_CONFIG_DIR)));

        let kepubify_path = cli
            .kepubify_path
            .clone()
            .or_else(|| env.get(KEPUBIFY_PATH_ENV).map(PathBuf::from))
            .or_else(|| platform_profile.default_kepubify_path());

        let isolation_root = cli
            .shadow_isolation_root
            .clone()
            .or_else(|| env.get(SHADOW_ISOLATION_ROOT_ENV).map(PathBuf::from));

        let allow_shadow_writes = if cli.allow_shadow_writes {
            true
        } else {
            env.get(ALLOW_SHADOW_WRITES_ENV)
                .map(String::as_str)
                .map(parse_bool)
                .transpose()?
                .unwrap_or(false)
        };

        Ok(Self {
            bind_address,
            mode,
            compat_profile,
            platform_profile,
            config_dir,
            server_context_path,
            log_file,
            kepubify_path,
            shadow_policy: ShadowPolicy {
                isolation_root,
                allow_shadow_writes,
            },
        })
    }

    pub fn for_compat_profile(compat_profile: CompatProfile) -> Self {
        Self {
            bind_address: DEFAULT_BIND_ADDRESS
                .parse()
                .expect("default bind address should parse"),
            mode: match compat_profile {
                CompatProfile::SnapshotAligned => RuntimeMode::Snapshot,
                CompatProfile::JavaLiveLocaldb => RuntimeMode::Localdb,
            },
            compat_profile,
            platform_profile: PlatformProfile::Default,
            config_dir: Some(PathBuf::from(DEFAULT_CONFIG_DIR)),
            server_context_path: Some(String::new()),
            log_file: default_log_file_for_config_dir(&PathBuf::from(DEFAULT_CONFIG_DIR)),
            kepubify_path: None,
            shadow_policy: ShadowPolicy {
                isolation_root: None,
                allow_shadow_writes: false,
            },
        }
    }

    pub fn app_compat_profile(&self) -> app::CompatProfile {
        match self.compat_profile {
            CompatProfile::SnapshotAligned => app::CompatProfile::SnapshotAligned,
            CompatProfile::JavaLiveLocaldb => app::CompatProfile::JavaLiveLocaldb,
        }
    }

    pub fn writer_decision(&self, writer: WriterKind) -> WriterDecision {
        match self.mode {
            RuntimeMode::Snapshot | RuntimeMode::Localdb => WriterDecision::Allowed,
            RuntimeMode::Canary => WriterDecision::Blocked {
                reason: "canary mode requires explicit cutover wiring",
            },
            RuntimeMode::Shadow => {
                if matches!(writer, WriterKind::SearchIndex) {
                    return WriterDecision::Blocked {
                        reason: SEARCH_INDEX_OWNERSHIP_REASON,
                    };
                }

                if self.shadow_policy.allow_shadow_writes {
                    if self.shadow_policy.isolation_root.is_some() {
                        WriterDecision::Isolated
                    } else {
                        WriterDecision::Blocked {
                            reason: SHADOW_BLOCK_REASON,
                        }
                    }
                } else {
                    match writer {
                        WriterKind::MainDatabase
                        | WriterKind::TasksDatabase
                        | WriterKind::SearchIndex
                        | WriterKind::FilesystemScanOutput
                        | WriterKind::SidecarOutput => WriterDecision::Blocked {
                            reason: SHADOW_BLOCK_REASON,
                        },
                    }
                }
            }
        }
    }
}

impl RuntimeMode {
    fn parse(value: &str) -> Result<Self, ConfigError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "snapshot" => Ok(Self::Snapshot),
            "localdb" => Ok(Self::Localdb),
            "shadow" => Ok(Self::Shadow),
            "canary" => Ok(Self::Canary),
            other => Err(ConfigError::InvalidMode(other.to_string())),
        }
    }

    fn default_compat_profile(self) -> CompatProfile {
        match self {
            Self::Localdb => CompatProfile::JavaLiveLocaldb,
            Self::Snapshot | Self::Shadow | Self::Canary => CompatProfile::SnapshotAligned,
        }
    }
}

impl CompatProfile {
    fn parse(value: &str) -> Result<Self, ConfigError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "snapshot-aligned" | "snapshot" => Ok(Self::SnapshotAligned),
            "java-live-localdb" | "localdb" => Ok(Self::JavaLiveLocaldb),
            other => Err(ConfigError::InvalidCompatProfile(other.to_string())),
        }
    }
}

impl PlatformProfile {
    fn parse(value: &str) -> Result<Self, ConfigError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "default" => Ok(Self::Default),
            "docker" => Ok(Self::Docker),
            "mac" => Ok(Self::Mac),
            "windows" => Ok(Self::Windows),
            other => Err(ConfigError::InvalidPlatformProfile(other.to_string())),
        }
    }

    fn default_config_dir(self, env: &BTreeMap<String, String>) -> Option<PathBuf> {
        match self {
            Self::Default | Self::Docker => Some(PathBuf::from(DEFAULT_CONFIG_DIR)),
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

    fn default_log_file(self, env: &BTreeMap<String, String>) -> Option<PathBuf> {
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

    fn default_kepubify_path(self) -> Option<PathBuf> {
        match self {
            Self::Docker => Some(PathBuf::from("/usr/bin/kepubify")),
            Self::Mac => Some(PathBuf::from("kepubify")),
            Self::Windows => Some(PathBuf::from("kepubify.exe")),
            Self::Default => None,
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    InvalidAddress(std::net::AddrParseError),
    InvalidMode(String),
    InvalidCompatProfile(String),
    InvalidPlatformProfile(String),
    InvalidBoolean(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAddress(error) => write!(f, "invalid KOMGA_RUST_ADDR: {error}"),
            Self::InvalidMode(value) => write!(f, "invalid KOMGA_RUST_MODE: {value}"),
            Self::InvalidCompatProfile(value) => {
                write!(f, "invalid KOMGA_RUST_COMPAT_PROFILE: {value}")
            }
            Self::InvalidPlatformProfile(value) => {
                write!(f, "invalid KOMGA_RUST_PLATFORM_PROFILE: {value}")
            }
            Self::InvalidBoolean(value) => {
                write!(f, "invalid shadow write boolean value: {value}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

fn preferred_string<'a>(cli: Option<&'a str>, env: Option<&'a str>) -> Option<&'a str> {
    cli.filter(|value| !value.trim().is_empty())
        .or_else(|| env.filter(|value| !value.trim().is_empty()))
}

fn parse_bool(value: &str) -> Result<bool, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        other => Err(ConfigError::InvalidBoolean(other.to_string())),
    }
}

fn default_log_file_for_config_dir(config_dir: &PathBuf) -> PathBuf {
    config_dir.join("logs").join(DEFAULT_LOG_FILE_NAME)
}
