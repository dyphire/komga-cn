use crate::app;
use config::{Config as LayeredConfig, Environment, File as ConfigFile, FileFormat};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::env;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use super::error::ConfigError;
use super::profile::{
    CompatProfile, DEFAULT_CONFIG_DIR, DEFAULT_LOG_FILE_NAME, PlatformProfile, RuntimeMode,
};
use super::shadow::ShadowPolicy;

const DEFAULT_BIND_ADDRESS: &str = "127.0.0.1:25600";
const ADDR_ENV: &str = "KOMGA_RUST_ADDR";
const MODE_ENV: &str = "KOMGA_RUST_MODE";
const CONFIG_DIR_ENV: &str = "KOMGA_CONFIG_DIR";
const COMPAT_PROFILE_ENV: &str = "KOMGA_RUST_COMPAT_PROFILE";
const PLATFORM_PROFILE_ENV: &str = "KOMGA_RUST_PLATFORM_PROFILE";
const SERVER_PORT_ENV: &str = "SERVER_PORT";
const SERVER_CONTEXT_PATH_ENV: &str = "SERVER_SERVLET_CONTEXT_PATH";
const SHADOW_ISOLATION_ROOT_ENV: &str = "KOMGA_RUST_SHADOW_ISOLATION_ROOT";
const ALLOW_SHADOW_WRITES_ENV: &str = "KOMGA_RUST_ALLOW_SHADOW_WRITES";
const LOG_FILE_ENV: &str = "LOGGING_FILE_NAME";
const KEPUBIFY_PATH_ENV: &str = "KOMGA_KEPUBIFY_PATH";
const KOBO_KEPUBIFY_PATH_ENV: &str = "KOMGA_KOBO_KEPUBIFY_PATH";
const DATABASE_FILE_ENV: &str = "KOMGA_DATABASE_FILE";
const TASKS_DB_FILE_ENV: &str = "KOMGA_TASKS_DB_FILE";
const LUCENE_DATA_DIRECTORY_ENV: &str = "KOMGA_LUCENE_DATA_DIRECTORY";
const FONTS_DATA_DIRECTORY_ENV: &str = "KOMGA_FONTS_DATA_DIRECTORY";
const LEGACY_WEBUI_DIRECTORY_NAME: &str = "public";
const LEGACY_WEBUI_ENTRYPOINT: &str = "index.html";

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OAuth2ClientConfig {
    pub registration_id: String,
    pub client_name: String,
    pub client_id: String,
    pub client_secret: String,
    pub authorization_uri: String,
    pub token_uri: String,
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
    pub database_file: PathBuf,
    pub tasks_db_file: PathBuf,
    pub lucene_data_directory: PathBuf,
    pub fonts_data_directory: PathBuf,
    pub kepubify_path: Option<PathBuf>,
    pub oauth2_clients: Vec<OAuth2ClientConfig>,
    pub shadow_policy: ShadowPolicy,
}

impl RuntimeConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        let cli = RuntimeCli::default();
        let env = env::vars().collect::<BTreeMap<_, _>>();
        let config = Self::resolve_with_env(&cli, &env)?;
        config.ensure_startup_runtime_layout()?;
        Ok(config)
    }

    pub fn resolve_with_env(
        cli: &RuntimeCli,
        env: &BTreeMap<String, String>,
    ) -> Result<Self, ConfigError> {
        let mode = preferred_string(cli.mode.as_deref(), env.get(MODE_ENV).map(String::as_str))
            .map(RuntimeMode::parse)
            .transpose()?
            .unwrap_or(RuntimeMode::Localdb);

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

        let bootstrap_config_dir = cli
            .config_dir
            .clone()
            .or_else(|| env.get(CONFIG_DIR_ENV).map(PathBuf::from))
            .or_else(|| platform_profile.default_config_dir(env))
            .or_else(|| default_home_config_dir(env))
            .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_DIR));

        let layered = build_layered_config(&bootstrap_config_dir, env)?;

        let resolved_config_dir_raw = cli
            .config_dir
            .as_ref()
            .map(path_to_string)
            .or_else(|| env.get(CONFIG_DIR_ENV).cloned())
            .or_else(|| read_string(&layered, &["komga.config-dir"]))
            .or_else(|| {
                platform_profile
                    .default_config_dir(env)
                    .as_ref()
                    .map(path_to_string)
            })
            .or_else(|| default_home_config_dir(env).as_ref().map(path_to_string))
            .unwrap_or_else(|| DEFAULT_CONFIG_DIR.to_string());
        let resolved_config_dir = PathBuf::from(expand_path_placeholders(
            &resolved_config_dir_raw,
            &bootstrap_config_dir,
            env,
        ));

        let bind_address = match preferred_string(
            cli.address.as_deref(),
            env.get(ADDR_ENV).map(String::as_str),
        ) {
            Some(raw) => raw.parse().map_err(ConfigError::InvalidAddress)?,
            None => SocketAddr::from(([127, 0, 0, 1], resolve_server_port(env, &layered)?)),
        };

        let server_context_path =
            preferred_string(None, env.get(SERVER_CONTEXT_PATH_ENV).map(String::as_str))
                .map(str::to_string)
                .or_else(|| {
                    read_string(
                        &layered,
                        &["server.servlet.context-path", "server.servlet.context.path"],
                    )
                })
                .unwrap_or_default();
        if !is_valid_startup_context_path(&server_context_path) {
            return Err(ConfigError::InvalidContextPath(server_context_path));
        }

        let log_file = cli
            .log_file
            .as_ref()
            .map(path_to_string)
            .or_else(|| env.get(LOG_FILE_ENV).cloned())
            .or_else(|| read_string(&layered, &["logging.file.name"]))
            .or_else(|| {
                platform_profile
                    .default_log_file(env)
                    .as_ref()
                    .map(path_to_string)
            })
            .map(|value| PathBuf::from(expand_path_placeholders(&value, &resolved_config_dir, env)))
            .unwrap_or_else(|| default_log_file_for_config_dir(&resolved_config_dir));

        let database_file = env
            .get(DATABASE_FILE_ENV)
            .cloned()
            .or_else(|| read_string(&layered, &["komga.database.file"]))
            .map(|value| PathBuf::from(expand_path_placeholders(&value, &resolved_config_dir, env)))
            .unwrap_or_else(|| resolved_config_dir.join("database.sqlite"));

        let tasks_db_file = env
            .get(TASKS_DB_FILE_ENV)
            .cloned()
            .or_else(|| read_string(&layered, &["komga.tasks-db.file", "komga.tasks.db.file"]))
            .map(|value| PathBuf::from(expand_path_placeholders(&value, &resolved_config_dir, env)))
            .unwrap_or_else(|| resolved_config_dir.join("tasks.sqlite"));

        let lucene_data_directory = env
            .get(LUCENE_DATA_DIRECTORY_ENV)
            .cloned()
            .or_else(|| {
                read_string(
                    &layered,
                    &["komga.lucene.data-directory", "komga.lucene.data.directory"],
                )
            })
            .map(|value| PathBuf::from(expand_path_placeholders(&value, &resolved_config_dir, env)))
            .unwrap_or_else(|| resolved_config_dir.join("lucene"));

        let fonts_data_directory = env
            .get(FONTS_DATA_DIRECTORY_ENV)
            .cloned()
            .or_else(|| {
                read_string(
                    &layered,
                    &["komga.fonts.data-directory", "komga.fonts.data.directory"],
                )
            })
            .map(|value| PathBuf::from(expand_path_placeholders(&value, &resolved_config_dir, env)))
            .unwrap_or_else(|| resolved_config_dir.join("fonts"));

        let kepubify_path = cli
            .kepubify_path
            .as_ref()
            .map(path_to_string)
            .or_else(|| env.get(KEPUBIFY_PATH_ENV).cloned())
            .or_else(|| env.get(KOBO_KEPUBIFY_PATH_ENV).cloned())
            .or_else(|| {
                read_string(
                    &layered,
                    &[
                        "komga.kobo.kepubify-path",
                        "komga.kobo.kepubify.path",
                        "komga.kobo.kepubifypath",
                    ],
                )
            })
            .map(|value| PathBuf::from(expand_path_placeholders(&value, &resolved_config_dir, env)))
            .or_else(|| platform_profile.default_kepubify_path());

        let isolation_root = cli
            .shadow_isolation_root
            .clone()
            .or_else(|| env.get(SHADOW_ISOLATION_ROOT_ENV).map(PathBuf::from));

        let oauth2_clients = resolve_oauth2_clients(&layered, env);

        let allow_shadow_writes = if cli.allow_shadow_writes {
            true
        } else {
            env.get(ALLOW_SHADOW_WRITES_ENV)
                .map(String::as_str)
                .map(parse_bool)
                .transpose()?
                .unwrap_or(false)
        };

        let config = Self {
            bind_address,
            mode,
            compat_profile,
            platform_profile,
            config_dir: Some(resolved_config_dir),
            server_context_path: Some(server_context_path),
            log_file,
            database_file,
            tasks_db_file,
            lucene_data_directory,
            fonts_data_directory,
            kepubify_path,
            oauth2_clients,
            shadow_policy: ShadowPolicy {
                isolation_root,
                allow_shadow_writes,
            },
        };

        config.validate_single_writer_storage_ownership(env)?;

        Ok(config)
    }

    pub fn for_compat_profile(compat_profile: CompatProfile) -> Self {
        let config_dir = PathBuf::from(DEFAULT_CONFIG_DIR);
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
            config_dir: Some(config_dir.clone()),
            server_context_path: Some(String::new()),
            log_file: default_log_file_for_config_dir(&config_dir),
            database_file: config_dir.join("database.sqlite"),
            tasks_db_file: config_dir.join("tasks.sqlite"),
            lucene_data_directory: config_dir.join("lucene"),
            fonts_data_directory: config_dir.join("fonts"),
            kepubify_path: None,
            oauth2_clients: vec![],
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

    pub fn discover_webui_assets_layout(&self) -> Option<PathBuf> {
        self.webui_layout_candidates()
            .into_iter()
            .find(|candidate| candidate.join(LEGACY_WEBUI_ENTRYPOINT).is_file())
    }

    pub fn resolve_webui_assets_layout(&self) -> Result<PathBuf, ConfigError> {
        self.discover_webui_assets_layout()
            .ok_or_else(|| ConfigError::MissingWebUiAssetsLayout {
                candidates: self.webui_layout_candidates(),
            })
    }

    fn ensure_startup_runtime_layout(&self) -> Result<(), ConfigError> {
        if let Some(config_dir) = self.config_dir.as_ref() {
            ensure_runtime_directories(
                config_dir,
                &self.log_file,
                &self.database_file,
                &self.tasks_db_file,
                &self.lucene_data_directory,
                &self.fonts_data_directory,
            )?;
        }
        let _ = self.resolve_webui_assets_layout()?;
        validate_temp_directory()
    }

    fn validate_single_writer_storage_ownership(
        &self,
        env: &BTreeMap<String, String>,
    ) -> Result<(), ConfigError> {
        if !matches!(self.mode, RuntimeMode::Shadow | RuntimeMode::Canary) {
            return Ok(());
        }

        let Some(config_dir) = self.config_dir.as_ref() else {
            return Ok(());
        };

        let legacy_main = config_dir.join("database.sqlite");
        let legacy_tasks = config_dir.join("tasks.sqlite");
        let legacy_search = config_dir.join("lucene");

        let mut mixed_targets = Vec::new();
        if self.database_file == legacy_main {
            mixed_targets.push("database.sqlite");
        }
        if self.tasks_db_file == legacy_tasks {
            mixed_targets.push("tasks.sqlite");
        }
        if self.lucene_data_directory == legacy_search {
            mixed_targets.push("legacy search directory");
        }

        if !mixed_targets.is_empty() {
            return Err(ConfigError::MixedWriterStorageOwnership {
                details: format!(
                    "startup mode '{}' would write legacy-owned targets [{}] under {}",
                    self.mode.as_str(),
                    mixed_targets.join(", "),
                    config_dir.display(),
                ),
            });
        }

        if self.shadow_policy.allow_shadow_writes
            && let Some(isolation_root) = self.shadow_policy.isolation_root.as_ref()
        {
            let mut outside_isolation = Vec::new();
            if !self.database_file.starts_with(isolation_root) {
                outside_isolation.push(self.database_file.display().to_string());
            }
            if !self.tasks_db_file.starts_with(isolation_root) {
                outside_isolation.push(self.tasks_db_file.display().to_string());
            }
            if !self.lucene_data_directory.starts_with(isolation_root) {
                outside_isolation.push(self.lucene_data_directory.display().to_string());
            }

            if !outside_isolation.is_empty() {
                return Err(ConfigError::MixedWriterStorageOwnership {
                    details: format!(
                        "shadow isolation root '{}' does not own [{}]",
                        isolation_root.display(),
                        outside_isolation.join(", "),
                    ),
                });
            }
        }

        if self.mode == RuntimeMode::Canary {
            return Err(ConfigError::MixedWriterStorageOwnership {
                details: "canary mode storage ownership is not wired yet and is blocked by design"
                    .to_string(),
            });
        }

        if is_legacy_home_config_dir(config_dir, env)
            && (self.database_file == legacy_main
                || self.tasks_db_file == legacy_tasks
                || self.lucene_data_directory == legacy_search)
        {
            return Err(ConfigError::MixedWriterStorageOwnership {
                details: format!(
                    "legacy config-dir '{}' stays Java-owned during shadow startup",
                    config_dir.display(),
                ),
            });
        }

        Ok(())
    }
}

impl RuntimeConfig {
    fn webui_layout_candidates(&self) -> Vec<PathBuf> {
        let mut candidates = Vec::new();

        if let Some(config_dir) = self.config_dir.as_ref() {
            candidates.push(config_dir.join(LEGACY_WEBUI_DIRECTORY_NAME));
        }

        if let Ok(exe_path) = std::env::current_exe()
            && let Some(exe_dir) = exe_path.parent()
        {
            candidates.push(exe_dir.join(LEGACY_WEBUI_DIRECTORY_NAME));
            candidates.push(exe_dir.join("resources").join(LEGACY_WEBUI_DIRECTORY_NAME));
        }

        if let Ok(current_dir) = std::env::current_dir() {
            candidates.push(current_dir.join(LEGACY_WEBUI_DIRECTORY_NAME));
            candidates.push(
                current_dir
                    .join("komga")
                    .join("src")
                    .join("main")
                    .join("resources")
                    .join(LEGACY_WEBUI_DIRECTORY_NAME),
            );
            candidates.push(current_dir.join("komga-webui").join("dist"));
        }

        dedup_paths(candidates)
    }
}

fn build_layered_config(
    config_dir: &Path,
    env: &BTreeMap<String, String>,
) -> Result<LayeredConfig, ConfigError> {
    let env_source = env
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect::<HashMap<_, _>>();

    LayeredConfig::builder()
        .set_default("server.port", 25600)
        .map_err(|error| ConfigError::InvalidConfigSource(error.to_string()))?
        .set_default("server.servlet.context-path", "")
        .map_err(|error| ConfigError::InvalidConfigSource(error.to_string()))?
        .set_default("komga.config-dir", config_dir.to_string_lossy().to_string())
        .map_err(|error| ConfigError::InvalidConfigSource(error.to_string()))?
        .add_source(
            ConfigFile::from(config_dir.join("application.yml"))
                .format(FileFormat::Yaml)
                .required(false),
        )
        .add_source(
            ConfigFile::from(config_dir.join("application.yaml"))
                .format(FileFormat::Yaml)
                .required(false),
        )
        .add_source(
            ConfigFile::from(config_dir.join("application.properties"))
                .format(FileFormat::Ini)
                .required(false),
        )
        .add_source(
            Environment::default()
                .separator("_")
                .try_parsing(true)
                .source(Some(env_source)),
        )
        .build()
        .map_err(|error| ConfigError::InvalidConfigSource(error.to_string()))
}

fn resolve_server_port(
    env: &BTreeMap<String, String>,
    layered: &LayeredConfig,
) -> Result<u16, ConfigError> {
    if let Some(raw) = env.get(SERVER_PORT_ENV) {
        return parse_port(raw);
    }

    if let Ok(port) = layered.get_int("server.port") {
        return u16::try_from(port).map_err(|_| ConfigError::InvalidPort(port.to_string()));
    }

    Ok(25600)
}

fn parse_port(raw: &str) -> Result<u16, ConfigError> {
    raw.trim()
        .parse::<u16>()
        .map_err(|_| ConfigError::InvalidPort(raw.to_string()))
}

fn read_string(layered: &LayeredConfig, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| layered.get_string(key).ok())
}

fn path_to_string(path: &PathBuf) -> String {
    path.to_string_lossy().to_string()
}

fn expand_path_placeholders(
    value: &str,
    resolved_config_dir: &Path,
    env: &BTreeMap<String, String>,
) -> String {
    let mut expanded = value.replace(r#"\${"#, "${");
    expanded = expanded.replace(
        "${komga.config-dir}",
        &resolved_config_dir.to_string_lossy(),
    );
    if let Some(home) = env.get("HOME").or_else(|| env.get("USERPROFILE")) {
        expanded = expanded.replace("${user.home}", home);
    }
    expanded
}

fn ensure_runtime_directories(
    config_dir: &Path,
    log_file: &Path,
    database_file: &Path,
    tasks_db_file: &Path,
    lucene_data_directory: &Path,
    fonts_data_directory: &Path,
) -> Result<(), ConfigError> {
    create_dir(config_dir)?;

    if let Some(parent) = log_file.parent() {
        create_dir(parent)?;
    }
    if let Some(parent) = database_file.parent() {
        create_dir(parent)?;
    }
    if let Some(parent) = tasks_db_file.parent() {
        create_dir(parent)?;
    }
    create_dir(lucene_data_directory)?;
    create_dir(fonts_data_directory)?;

    Ok(())
}

fn create_dir(path: &Path) -> Result<(), ConfigError> {
    std::fs::create_dir_all(path).map_err(|source| ConfigError::DirectoryCreate {
        path: path.to_path_buf(),
        source,
    })
}

fn validate_temp_directory() -> Result<(), ConfigError> {
    let temp_dir = std::env::temp_dir();
    match std::fs::metadata(&temp_dir) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        _ => Err(ConfigError::InvalidTempDirectory(temp_dir)),
    }
}

fn default_home_config_dir(env: &BTreeMap<String, String>) -> Option<PathBuf> {
    env.get("HOME")
        .or_else(|| env.get("USERPROFILE"))
        .map(|home| PathBuf::from(home).join(DEFAULT_CONFIG_DIR))
}

fn is_legacy_home_config_dir(path: &Path, env: &BTreeMap<String, String>) -> bool {
    default_home_config_dir(env)
        .as_ref()
        .is_some_and(|legacy| path == legacy)
}

fn is_valid_startup_context_path(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }

    if !value.starts_with('/') || value.ends_with('/') {
        return false;
    }

    value
        .chars()
        .all(|ch| ch == '/' || ch == '-' || ch == '_' || ch.is_ascii_alphanumeric())
}

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

fn dedup_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut ordered = Vec::new();
    for path in paths {
        if !ordered.iter().any(|existing| existing == &path) {
            ordered.push(path);
        }
    }
    ordered
}

fn resolve_oauth2_clients(
    layered: &LayeredConfig,
    env: &BTreeMap<String, String>,
) -> Vec<OAuth2ClientConfig> {
    let mut clients_by_registration_id = resolve_oauth2_clients_from_layered(layered)
        .into_iter()
        .map(|client| (client.registration_id.clone(), client))
        .collect::<BTreeMap<_, _>>();

    for client in resolve_oauth2_clients_from_env(env) {
        clients_by_registration_id.insert(client.registration_id.clone(), client);
    }

    clients_by_registration_id.into_values().collect()
}

fn resolve_oauth2_clients_from_layered(layered: &LayeredConfig) -> Vec<OAuth2ClientConfig> {
    let Ok(root) = layered.clone().try_deserialize::<serde_json::Value>() else {
        return vec![];
    };

    let Some(registrations) = root
        .pointer("/spring/security/oauth2/client/registration")
        .and_then(serde_json::Value::as_object)
    else {
        return vec![];
    };

    let providers = root
        .pointer("/spring/security/oauth2/client/provider")
        .and_then(serde_json::Value::as_object);

    let mut clients = Vec::with_capacity(registrations.len());
    for (registration_id, registration_value) in registrations {
        let Some(registration) = registration_value.as_object() else {
            continue;
        };

        let Some(client_id) =
            read_object_string(registration, &["client-id", "clientId", "client_id"])
        else {
            continue;
        };
        let Some(client_secret) = read_object_string(
            registration,
            &["client-secret", "clientSecret", "client_secret"],
        ) else {
            continue;
        };
        let client_name =
            read_object_string(registration, &["client-name", "clientName", "client_name"])
                .unwrap_or_else(|| registration_id.to_string());

        let provider_id = read_object_string(registration, &["provider"])
            .unwrap_or_else(|| registration_id.to_string());
        let Some(provider) = providers
            .and_then(|all| all.get(&provider_id))
            .or_else(|| providers.and_then(|all| all.get(registration_id)))
            .and_then(serde_json::Value::as_object)
        else {
            continue;
        };

        let Some(authorization_uri) = read_object_string(
            provider,
            &["authorization-uri", "authorizationUri", "authorization_uri"],
        ) else {
            continue;
        };

        let Some(token_uri) = read_object_string(provider, &["token-uri", "tokenUri", "token_uri"])
        else {
            continue;
        };

        clients.push(OAuth2ClientConfig {
            registration_id: registration_id.to_string(),
            client_name,
            client_id,
            client_secret,
            authorization_uri,
            token_uri,
        });
    }

    clients
}

fn resolve_oauth2_clients_from_env(env: &BTreeMap<String, String>) -> Vec<OAuth2ClientConfig> {
    let registration_ids = env
        .keys()
        .filter_map(|key| {
            key.strip_prefix("SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_")
                .and_then(|value| value.strip_suffix("_CLIENT_ID"))
                .map(|value| value.to_ascii_lowercase())
        })
        .collect::<BTreeSet<_>>();

    let mut clients = Vec::with_capacity(registration_ids.len());
    for registration_id in registration_ids {
        let registration_key = registration_id.to_ascii_uppercase();
        let Some(client_id) = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_{registration_key}_CLIENT_ID"
            ))
            .cloned()
        else {
            continue;
        };
        let Some(client_secret) = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_{registration_key}_CLIENT_SECRET"
            ))
            .cloned()
        else {
            continue;
        };

        let client_name = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_{registration_key}_CLIENT_NAME"
            ))
            .cloned()
            .unwrap_or_else(|| registration_id.clone());

        let provider_id = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_REGISTRATION_{registration_key}_PROVIDER"
            ))
            .map(|value| value.to_ascii_lowercase())
            .unwrap_or_else(|| registration_id.clone());
        let provider_key = provider_id.to_ascii_uppercase();

        let Some(authorization_uri) = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_{provider_key}_AUTHORIZATION_URI"
            ))
            .cloned()
        else {
            continue;
        };

        let Some(token_uri) = env
            .get(&format!(
                "SPRING_SECURITY_OAUTH2_CLIENT_PROVIDER_{provider_key}_TOKEN_URI"
            ))
            .cloned()
        else {
            continue;
        };

        clients.push(OAuth2ClientConfig {
            registration_id,
            client_name,
            client_id,
            client_secret,
            authorization_uri,
            token_uri,
        });
    }

    clients
}

fn read_object_string(
    object: &serde_json::Map<String, serde_json::Value>,
    keys: &[&str],
) -> Option<String> {
    keys.iter().find_map(|key| {
        object
            .get(*key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
    })
}

fn default_log_file_for_config_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("logs").join(DEFAULT_LOG_FILE_NAME)
}
