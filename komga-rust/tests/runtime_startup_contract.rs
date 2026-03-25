use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_millis();
    std::env::temp_dir().join(format!("{prefix}-{millis}"))
}

#[test]
fn runtime_config_precedence_matches_spring() {
    let config_dir = unique_temp_dir("komga-runtime-contract");
    let file_root = config_dir.join("from-file");
    let env_root = config_dir.join("from-env");
    fs::create_dir_all(&config_dir).expect("test config directory should be created");

    let yaml_path = config_dir.join("application.yml");
    fs::write(
        &yaml_path,
        r#"
server:
  port: 28080
  servlet:
    context-path: /from-file
logging:
  file:
    name: __FILE_ROOT__/logs/komga.log
komga:
  database:
    file: __FILE_ROOT__/database.sqlite
  tasks-db:
    file: __FILE_ROOT__/tasks.sqlite
  lucene:
    data-directory: __FILE_ROOT__/lucene
  fonts:
    data-directory: __FILE_ROOT__/fonts
  kobo:
    kepubify-path: __FILE_ROOT__/kepubify
"#
        .replace("__FILE_ROOT__", &file_root.to_string_lossy()),
    )
    .expect("application.yml should be written");

    let mut env = BTreeMap::new();
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        config_dir.to_string_lossy().to_string(),
    );
    env.insert("SERVER_PORT".to_string(), "28111".to_string());
    env.insert(
        "SERVER_SERVLET_CONTEXT_PATH".to_string(),
        "/from-env".to_string(),
    );
    env.insert(
        "KOMGA_DATABASE_FILE".to_string(),
        env_root
            .join("database.sqlite")
            .to_string_lossy()
            .to_string(),
    );

    let config = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli::default(),
        &env,
    )
    .expect("runtime config should resolve");

    assert_eq!(config.bind_address.port(), 28111);
    assert_eq!(config.server_context_path.as_deref(), Some("/from-env"));
    assert_eq!(config.database_file, env_root.join("database.sqlite"),);
    assert_eq!(config.tasks_db_file, file_root.join("tasks.sqlite"),);
    assert_eq!(config.lucene_data_directory, file_root.join("lucene"),);
    assert_eq!(config.fonts_data_directory, file_root.join("fonts"),);
    assert_eq!(config.kepubify_path, Some(file_root.join("kepubify")),);
    assert_eq!(config.log_file, file_root.join("logs").join("komga.log"));
}

#[test]
fn invalid_context_path_fails_startup() {
    let mut env = BTreeMap::new();
    env.insert(
        "SERVER_SERVLET_CONTEXT_PATH".to_string(),
        "noslash".to_string(),
    );

    let error = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli::default(),
        &env,
    )
    .expect_err("invalid context path must fail startup");

    assert_eq!(
        error.to_string(),
        "invalid SERVER_SERVLET_CONTEXT_PATH: must be empty or start with '/' and not end with '/'",
    );
}

#[test]
fn startup_config_expands_kotlin_style_placeholders_for_paths() {
    let home_dir = unique_temp_dir("komga-runtime-home");
    let config_dir = home_dir.join(".komga");
    fs::create_dir_all(&config_dir).expect("test config directory should be created");

    let literal_placeholder_dir = std::env::current_dir()
        .expect("current working directory should exist")
        .join("${komga.config-dir}");
    if literal_placeholder_dir.exists() {
        fs::remove_dir_all(&literal_placeholder_dir)
            .expect("stale literal placeholder directory should be removable");
    }

    fs::write(
        config_dir.join("application.yml"),
        r#"
komga:
  config-dir: ${user.home}/.komga
  database:
    file: ${komga.config-dir}/database.sqlite
  tasks-db:
    file: ${komga.config-dir}/tasks.sqlite
  lucene:
    data-directory: ${komga.config-dir}/lucene
  fonts:
    data-directory: ${komga.config-dir}/fonts
logging:
  file:
    name: ${komga.config-dir}/logs/komga.log
"#,
    )
    .expect("application.yml should be written");

    let mut env = BTreeMap::new();
    env.insert("HOME".to_string(), home_dir.to_string_lossy().to_string());

    let config = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli::default(),
        &env,
    )
    .expect("runtime config should resolve");

    assert_eq!(config.config_dir.as_deref(), Some(config_dir.as_path()));
    assert_eq!(config.log_file, config_dir.join("logs").join("komga.log"));
    assert_eq!(config.database_file, config_dir.join("database.sqlite"));
    assert_eq!(config.tasks_db_file, config_dir.join("tasks.sqlite"));
    assert_eq!(config.lucene_data_directory, config_dir.join("lucene"));
    assert_eq!(config.fonts_data_directory, config_dir.join("fonts"));
    assert!(
        !literal_placeholder_dir.exists(),
        "startup should not create literal ${{komga.config-dir}} directory",
    );
}

#[test]
fn startup_discovers_application_properties_and_relaxed_komga_keys() {
    let config_dir = unique_temp_dir("komga-runtime-properties");
    fs::create_dir_all(&config_dir).expect("test config directory should be created");

    fs::write(
        config_dir.join("application.properties"),
        "komga.config-dir=${user.home}/.komga\n\
komga.database.file=${komga.config-dir}/database.sqlite\n\
komga.tasks-db.file=${komga.config-dir}/tasks.sqlite\n\
komga.lucene.data-directory=${komga.config-dir}/lucene\n\
komga.fonts.data-directory=${komga.config-dir}/fonts\n\
logging.file.name=${komga.config-dir}/logs/komga.log\n\
server.port=28123\n",
    )
    .expect("application.properties should be written");

    let mut env = BTreeMap::new();
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        config_dir.to_string_lossy().to_string(),
    );

    let config = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli::default(),
        &env,
    )
    .expect("runtime config should resolve from application.properties");

    let resolved_config_dir = config_dir;
    assert_eq!(
        config.config_dir.as_deref(),
        Some(resolved_config_dir.as_path())
    );
    assert_eq!(config.bind_address.port(), 28123);
    assert_eq!(
        config.database_file,
        resolved_config_dir.join("database.sqlite")
    );
    assert_eq!(
        config.tasks_db_file,
        resolved_config_dir.join("tasks.sqlite")
    );
    assert_eq!(
        config.lucene_data_directory,
        resolved_config_dir.join("lucene"),
    );
    assert_eq!(
        config.fonts_data_directory,
        resolved_config_dir.join("fonts")
    );
    assert_eq!(
        config.log_file,
        resolved_config_dir.join("logs").join("komga.log"),
    );
}

#[test]
fn startup_webui_layout_resolves_legacy_public_directory_with_index() {
    let config_dir = unique_temp_dir("komga-runtime-webui-layout");
    fs::create_dir_all(&config_dir).expect("test config directory should be created");
    let public_dir = config_dir.join("public");
    fs::create_dir_all(&public_dir).expect("public directory should be created");
    fs::write(public_dir.join("index.html"), "<html>legacy-index</html>")
        .expect("legacy index.html should be written");

    let mut env = BTreeMap::new();
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        config_dir.to_string_lossy().to_string(),
    );

    let config = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli::default(),
        &env,
    )
    .expect("runtime config should resolve");

    let webui_root = config
        .resolve_webui_assets_layout()
        .expect("startup should resolve legacy public layout when index exists");
    assert_eq!(webui_root, public_dir);
}

#[test]
fn startup_webui_layout_fails_closed_when_legacy_public_layout_missing() {
    let config_dir = unique_temp_dir("komga-runtime-webui-layout-missing");
    fs::create_dir_all(&config_dir).expect("test config directory should be created");

    let mut env = BTreeMap::new();
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        config_dir.to_string_lossy().to_string(),
    );

    let config = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli::default(),
        &env,
    )
    .expect("runtime config should resolve");

    let error = config
        .resolve_webui_assets_layout()
        .expect_err("startup must fail closed when no legacy public/index.html layout exists");
    assert!(
        error
            .to_string()
            .contains("missing WebUI runtime assets layout"),
        "startup error should deterministically explain missing WebUI layout: {error}",
    );
}
