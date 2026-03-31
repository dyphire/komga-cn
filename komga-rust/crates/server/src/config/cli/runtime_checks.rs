use super::*;

pub(crate) fn ensure_startup_runtime_layout(config: &RuntimeConfig) -> Result<(), ConfigError> {
    if let Some(config_dir) = config.config_dir.as_ref() {
        ensure_runtime_directories(
            config_dir,
            &config.log_file,
            &config.database_file,
            &config.tasks_db_file,
            &config.lucene_data_directory,
            &config.fonts_data_directory,
        )?;
    }
    validate_temp_directory()
}

pub(crate) fn validate_single_writer_storage_ownership(
    config: &RuntimeConfig,
    env: &BTreeMap<String, String>,
) -> Result<(), ConfigError> {
    if !matches!(config.mode, RuntimeMode::Isolated | RuntimeMode::Canary) {
        return Ok(());
    }

    let Some(config_dir) = config.config_dir.as_ref() else {
        return Ok(());
    };

    let default_main_db = config_dir.join("database.sqlite");
    let default_tasks_db = config_dir.join("tasks.sqlite");
    let default_search_dir = config_dir.join("lucene");

    let mut mixed_targets = Vec::new();
    if config.database_file == default_main_db {
        mixed_targets.push("database.sqlite");
    }
    if config.tasks_db_file == default_tasks_db {
        mixed_targets.push("tasks.sqlite");
    }
    if config.lucene_data_directory == default_search_dir {
        mixed_targets.push("search directory");
    }

    if !mixed_targets.is_empty() {
        return Err(ConfigError::MixedWriterStorageOwnership {
            details: format!(
                "startup mode '{}' would write ownership targets [{}] under {}",
                config.mode.as_str(),
                mixed_targets.join(", "),
                config_dir.display(),
            ),
        });
    }

    if config.writer_ownership_policy.allow_isolated_writes
        && let Some(isolation_root) = config.writer_ownership_policy.isolation_root.as_ref()
    {
        let mut outside_isolation = Vec::new();
        if !config.database_file.starts_with(isolation_root) {
            outside_isolation.push(config.database_file.display().to_string());
        }
        if !config.tasks_db_file.starts_with(isolation_root) {
            outside_isolation.push(config.tasks_db_file.display().to_string());
        }
        if !config.lucene_data_directory.starts_with(isolation_root) {
            outside_isolation.push(config.lucene_data_directory.display().to_string());
        }

        if !outside_isolation.is_empty() {
            return Err(ConfigError::MixedWriterStorageOwnership {
                details: format!(
                    "isolated writer root '{}' does not own [{}]",
                    isolation_root.display(),
                    outside_isolation.join(", "),
                ),
            });
        }
    }

    if is_default_home_config_dir(config_dir, env)
        && (config.database_file == default_main_db
            || config.tasks_db_file == default_tasks_db
            || config.lucene_data_directory == default_search_dir)
    {
        return Err(ConfigError::MixedWriterStorageOwnership {
            details: format!(
                "default config-dir '{}' stays external-owned during isolated startup",
                config_dir.display(),
            ),
        });
    }

    Ok(())
}
