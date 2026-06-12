use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use config::Config as LayeredConfig;

use super::super::cli_args::{
    CONFIG_DIR_ENV, MODE_ENV, PLATFORM_PROFILE_ENV, RUNTIME_PROFILE_ENV, RuntimeCli,
    SPRING_PROFILES_ACTIVE_ENV,
};
use super::super::env_config::{
    AdminActionConfig, DEFAULT_SESSION_MAX_INACTIVE_SECONDS, RuntimeConfig,
};
use super::super::error::ConfigError;
use super::super::profile::{DEFAULT_CONFIG_DIR, PlatformProfile, RuntimeMode, RuntimeProfile};
use super::startup::{
    StartupNetworkConfig, build_layered_config, default_home_config_dir, expand_path_placeholders,
    path_to_string, preferred_string, read_string, resolve_bind_address_and_context_path,
    resolve_derived_runtime_paths, resolve_oauth2_clients_for_startup_slice,
    resolve_writer_ownership_policy_for_startup_slice,
};

fn active_profiles_contain_demo(layered: &LayeredConfig, env: &BTreeMap<String, String>) -> bool {
    env.get(SPRING_PROFILES_ACTIVE_ENV)
        .cloned()
        .or_else(|| read_string(layered, &["spring.profiles.active"]))
        .is_some_and(|profiles| {
            profiles
                .split(',')
                .map(str::trim)
                .any(|profile| profile.eq_ignore_ascii_case("demo"))
        })
}

fn parse_bool(value: &str) -> Result<bool, ConfigError> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        other => Err(ConfigError::InvalidBoolean(other.to_string())),
    }
}

fn read_bool(layered: &LayeredConfig, keys: &[&str]) -> Result<Option<bool>, ConfigError> {
    for key in keys {
        match layered.get_bool(key) {
            Ok(value) => return Ok(Some(value)),
            Err(config::ConfigError::NotFound(_)) => {}
            Err(_) => match layered.get_string(key) {
                Ok(value) => return parse_bool(&value).map(Some),
                Err(config::ConfigError::NotFound(_)) => {}
                Err(_) => return Err(ConfigError::InvalidBoolean((*key).to_string())),
            },
        }
    }
    Ok(None)
}

fn read_positive_u64(layered: &LayeredConfig, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        layered
            .get_int(key)
            .ok()
            .and_then(|value| u64::try_from(value).ok())
            .filter(|value| *value > 0)
            .or_else(|| {
                layered
                    .get_string(key)
                    .ok()
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .filter(|value| *value > 0)
            })
    })
}

fn resolve_config_bool(
    layered: &LayeredConfig,
    env: &BTreeMap<String, String>,
    env_key: &str,
    keys: &[&str],
    default: bool,
) -> Result<bool, ConfigError> {
    if let Some(value) = env.get(env_key) {
        return parse_bool(value);
    }

    Ok(read_bool(layered, keys)?.unwrap_or(default))
}

fn resolve_oidc_email_verification(
    layered: &LayeredConfig,
    env: &BTreeMap<String, String>,
) -> Result<bool, ConfigError> {
    resolve_config_bool(
        layered,
        env,
        "KOMGA_OIDC_EMAIL_VERIFICATION",
        &[
            "komga.oidcEmailVerification",
            "komga.oidc-email-verification",
            "komga.oidc_email_verification",
        ],
        true,
    )
}

fn resolve_oauth2_account_creation(
    layered: &LayeredConfig,
    env: &BTreeMap<String, String>,
) -> Result<bool, ConfigError> {
    resolve_config_bool(
        layered,
        env,
        "KOMGA_OAUTH2_ACCOUNT_CREATION",
        &[
            "komga.oauth2AccountCreation",
            "komga.oauth2-account-creation",
            "komga.oauth2_account_creation",
        ],
        false,
    )
}

fn resolve_session_max_inactive_seconds(
    layered: &LayeredConfig,
    env: &BTreeMap<String, String>,
) -> u64 {
    env.get("KOMGA_SESSION_MAX_INACTIVE_SECONDS")
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .or_else(|| {
            read_positive_u64(
                layered,
                &[
                    "komga.sessionMaxInactiveSeconds",
                    "komga.session-max-inactive-seconds",
                    "komga.session_max_inactive_seconds",
                ],
            )
        })
        .unwrap_or(DEFAULT_SESSION_MAX_INACTIVE_SECONDS)
}

struct ResolvedConfigInputs {
    layered: LayeredConfig,
    resolved_config_dir: PathBuf,
    platform_profile: PlatformProfile,
}

fn resolve_config_inputs(
    cli: &RuntimeCli,
    env: &BTreeMap<String, String>,
) -> Result<ResolvedConfigInputs, ConfigError> {
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
        .or_else(|| {
            preferred_string(None, env.get(CONFIG_DIR_ENV).map(String::as_str)).map(PathBuf::from)
        })
        .or_else(|| platform_profile.default_config_dir(env))
        .or_else(|| default_home_config_dir(env))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_DIR));

    let layered = build_layered_config(&bootstrap_config_dir, env)?;
    let resolved_config_dir = resolve_config_dir(
        cli,
        env,
        &layered,
        platform_profile,
        bootstrap_config_dir.as_path(),
    );

    Ok(ResolvedConfigInputs {
        layered,
        resolved_config_dir,
        platform_profile,
    })
}

fn resolve_config_dir(
    cli: &RuntimeCli,
    env: &BTreeMap<String, String>,
    layered: &LayeredConfig,
    platform_profile: PlatformProfile,
    bootstrap_config_dir: &Path,
) -> PathBuf {
    let resolved_config_dir_raw = cli
        .config_dir
        .as_ref()
        .map(path_to_string)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            preferred_string(None, env.get(CONFIG_DIR_ENV).map(String::as_str)).map(str::to_string)
        })
        .or_else(|| read_string(layered, &["komga.config-dir"]))
        .or_else(|| {
            platform_profile
                .default_config_dir(env)
                .as_ref()
                .map(path_to_string)
        })
        .or_else(|| default_home_config_dir(env).as_ref().map(path_to_string))
        .unwrap_or_else(|| DEFAULT_CONFIG_DIR.to_string());

    PathBuf::from(expand_path_placeholders(
        &resolved_config_dir_raw,
        bootstrap_config_dir,
        env,
    ))
}

pub(crate) fn resolve_with_env(
    cli: &RuntimeCli,
    env: &BTreeMap<String, String>,
) -> Result<RuntimeConfig, ConfigError> {
    let mode = preferred_string(cli.mode.as_deref(), env.get(MODE_ENV).map(String::as_str))
        .map(RuntimeMode::parse)
        .transpose()?
        .unwrap_or(RuntimeMode::Localdb);

    let runtime_profile = preferred_string(
        cli.runtime_profile.as_deref(),
        env.get(RUNTIME_PROFILE_ENV).map(String::as_str),
    )
    .map(RuntimeProfile::parse)
    .transpose()?
    .unwrap_or_else(|| mode.default_runtime_profile());

    let ResolvedConfigInputs {
        layered,
        resolved_config_dir,
        platform_profile,
    } = resolve_config_inputs(cli, env)?;

    let StartupNetworkConfig {
        bind_address,
        server_context_path,
    } = resolve_bind_address_and_context_path(cli, env, &layered)?;

    let derived_paths =
        resolve_derived_runtime_paths(cli, env, &layered, &resolved_config_dir, platform_profile);

    let oauth2_clients = resolve_oauth2_clients_for_startup_slice(&layered, env);
    let oauth2_account_creation = resolve_oauth2_account_creation(&layered, env)?;
    let oidc_email_verification = resolve_oidc_email_verification(&layered, env)?;
    let session_max_inactive_seconds = resolve_session_max_inactive_seconds(&layered, env);

    let writer_ownership_policy = resolve_writer_ownership_policy_for_startup_slice(cli, env)?;
    let demo_mode = active_profiles_contain_demo(&layered, env);

    let config = RuntimeConfig {
        bind_address,
        configuration_bind_address: bind_address,
        mode,
        demo_mode,
        oauth2_account_creation,
        oidc_email_verification,
        runtime_profile,
        platform_profile,
        config_dir: Some(resolved_config_dir),
        server_context_path: Some(server_context_path.clone()),
        configuration_server_context_path: Some(server_context_path),
        database_server_port: None,
        database_server_context_path: None,
        log_file: derived_paths.log_file,
        database_file: derived_paths.database_file,
        tasks_db_file: derived_paths.tasks_db_file,
        lucene_data_directory: derived_paths.lucene_data_directory,
        fonts_data_directory: derived_paths.fonts_data_directory,
        oauth2_clients,
        writer_ownership_policy,
        session_max_inactive_seconds,
        task_pool_size: 1,
    };

    config.validate_single_writer_storage_ownership(env)?;

    Ok(config)
}

pub(crate) fn resolve_admin_action_with_env(
    cli: &RuntimeCli,
    env: &BTreeMap<String, String>,
) -> Result<AdminActionConfig, ConfigError> {
    let ResolvedConfigInputs {
        layered,
        resolved_config_dir,
        platform_profile,
    } = resolve_config_inputs(cli, env)?;

    let derived_paths =
        resolve_derived_runtime_paths(cli, env, &layered, &resolved_config_dir, platform_profile);

    Ok(AdminActionConfig {
        database_file: derived_paths.database_file,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::cli_args::RuntimeCli;
    use crate::error::ConfigError;

    use super::resolve_with_env;

    struct TempConfigDir(PathBuf);

    impl TempConfigDir {
        fn new(case: &str) -> Self {
            let path = unique_config_dir(case);
            fs::create_dir_all(&path).expect("config dir should be created");
            Self(path)
        }
    }

    impl Drop for TempConfigDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn unique_config_dir(case: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "komga-runtime-config-{case}-{nanos}-{}",
            std::process::id()
        ))
    }

    #[test]
    fn rejects_invalid_boolean_from_application_config() {
        let config_dir = TempConfigDir::new("invalid-bool");
        fs::write(
            config_dir.0.join("application.yml"),
            "komga:\n  oidc-email-verification: maybe\n",
        )
        .expect("application config should be written");

        let cli = RuntimeCli {
            config_dir: Some(config_dir.0.clone()),
            ..RuntimeCli::default()
        };
        let error = resolve_with_env(&cli, &BTreeMap::new())
            .expect_err("invalid config boolean should fail startup config resolution");

        assert!(matches!(error, ConfigError::InvalidBoolean(value) if value == "maybe"));
    }
}
