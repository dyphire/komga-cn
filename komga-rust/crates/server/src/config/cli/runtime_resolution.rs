use super::*;

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

    let resolved_config_dir_raw = cli
        .config_dir
        .as_ref()
        .map(path_to_string)
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            preferred_string(None, env.get(CONFIG_DIR_ENV).map(String::as_str)).map(str::to_string)
        })
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

    let (bind_address, server_context_path) =
        resolve_bind_address_and_context_path(cli, env, &layered)?;

    let derived_paths =
        resolve_derived_runtime_paths(cli, env, &layered, &resolved_config_dir, platform_profile);

    let webui_dir = cli
        .webui_dir
        .as_ref()
        .map(path_to_string)
        .or_else(|| {
            preferred_string(None, env.get(WEBUI_DIRECTORY_ENV).map(String::as_str))
                .map(str::to_string)
        })
        .or_else(|| {
            read_string(
                &layered,
                &[
                    "komga.webui.directory",
                    "komga.webui.dir",
                    "komga.webui.path",
                ],
            )
        })
        .map(|value| PathBuf::from(expand_path_placeholders(&value, &resolved_config_dir, env)))
        .or_else(|| platform_profile.default_webui_directory());

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

    let oauth2_clients = resolve_oauth2_clients_for_startup_slice(&layered, env);

    let writer_ownership_policy = resolve_writer_ownership_policy_for_startup_slice(cli, env)?;

    let config = RuntimeConfig {
        bind_address,
        mode,
        runtime_profile,
        platform_profile,
        config_dir: Some(resolved_config_dir),
        server_context_path: Some(server_context_path),
        log_file: derived_paths.log_file,
        database_file: derived_paths.database_file,
        tasks_db_file: derived_paths.tasks_db_file,
        lucene_data_directory: derived_paths.lucene_data_directory,
        fonts_data_directory: derived_paths.fonts_data_directory,
        kepubify_path,
        webui_dir,
        oauth2_clients,
        writer_ownership_policy,
    };

    config.validate_single_writer_storage_ownership(env)?;

    Ok(config)
}
