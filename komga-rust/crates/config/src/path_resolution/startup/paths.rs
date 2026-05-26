use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DerivedRuntimePaths {
    pub(crate) log_file: PathBuf,
    pub(crate) database_file: PathBuf,
    pub(crate) tasks_db_file: PathBuf,
    pub(crate) lucene_data_directory: PathBuf,
    pub(crate) fonts_data_directory: PathBuf,
}

pub(crate) fn read_string(layered: &LayeredConfig, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| layered.get_string(key).ok())
}

pub(crate) fn path_to_string(path: impl AsRef<Path>) -> String {
    path.as_ref().to_string_lossy().to_string()
}

pub(crate) fn expand_path_placeholders(
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

pub(crate) fn resolve_derived_runtime_paths(
    cli: &RuntimeCli,
    env: &BTreeMap<String, String>,
    layered: &LayeredConfig,
    resolved_config_dir: &Path,
    platform_profile: PlatformProfile,
) -> DerivedRuntimePaths {
    let log_file = cli
        .log_file
        .as_ref()
        .map(path_to_string)
        .or_else(|| env.get(LOG_FILE_ENV).cloned())
        .or_else(|| read_string(layered, &["logging.file.name"]))
        .or_else(|| platform_profile.default_log_file(env).map(path_to_string))
        .map(|value| PathBuf::from(expand_path_placeholders(&value, resolved_config_dir, env)))
        .unwrap_or_else(|| default_log_file_for_config_dir(resolved_config_dir));

    let database_file = env
        .get(DATABASE_FILE_ENV)
        .cloned()
        .or_else(|| read_string(layered, &["komga.database.file"]))
        .map(|value| PathBuf::from(expand_path_placeholders(&value, resolved_config_dir, env)))
        .unwrap_or_else(|| resolved_config_dir.join("database.sqlite"));

    let tasks_db_file = env
        .get(TASKS_DB_FILE_ENV)
        .cloned()
        .or_else(|| read_string(layered, &["komga.tasks-db.file", "komga.tasks.db.file"]))
        .map(|value| PathBuf::from(expand_path_placeholders(&value, resolved_config_dir, env)))
        .unwrap_or_else(|| resolved_config_dir.join("tasks.sqlite"));

    let lucene_data_directory = env
        .get(LUCENE_DATA_DIRECTORY_ENV)
        .cloned()
        .or_else(|| {
            read_string(
                layered,
                &["komga.lucene.data-directory", "komga.lucene.data.directory"],
            )
        })
        .map(|value| PathBuf::from(expand_path_placeholders(&value, resolved_config_dir, env)))
        .unwrap_or_else(|| resolved_config_dir.join("lucene"));

    let fonts_data_directory = env
        .get(FONTS_DATA_DIRECTORY_ENV)
        .cloned()
        .or_else(|| {
            read_string(
                layered,
                &["komga.fonts.data-directory", "komga.fonts.data.directory"],
            )
        })
        .map(|value| PathBuf::from(expand_path_placeholders(&value, resolved_config_dir, env)))
        .unwrap_or_else(|| resolved_config_dir.join("fonts"));

    DerivedRuntimePaths {
        log_file,
        database_file,
        tasks_db_file,
        lucene_data_directory,
        fonts_data_directory,
    }
}

pub(crate) fn default_home_config_dir(env: &BTreeMap<String, String>) -> Option<PathBuf> {
    env.get("HOME")
        .or_else(|| env.get("USERPROFILE"))
        .map(|home| PathBuf::from(home).join(DEFAULT_CONFIG_DIR))
}

pub(crate) fn is_default_home_config_dir(path: &Path, env: &BTreeMap<String, String>) -> bool {
    default_home_config_dir(env)
        .as_ref()
        .is_some_and(|home_config_dir| path == home_config_dir)
}

pub(crate) fn preferred_string<'a>(cli: Option<&'a str>, env: Option<&'a str>) -> Option<&'a str> {
    cli.filter(|value| !value.trim().is_empty())
        .or_else(|| env.filter(|value| !value.trim().is_empty()))
}

pub(crate) fn default_log_file_for_config_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("logs").join(DEFAULT_LOG_FILE_NAME)
}
