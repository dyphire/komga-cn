use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tantivy::Index;
use tantivy::schema::{STORED, STRING, Schema};

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_millis();
    std::env::temp_dir().join(format!("{prefix}-{millis}"))
}

fn create_stale_schema_search_index(index_dir: &std::path::Path) {
    fs::create_dir_all(index_dir).expect("stale schema index directory should be created");

    let mut schema_builder = Schema::builder();
    schema_builder.add_text_field("doc_key", STRING | STORED);
    schema_builder.add_text_field("entity_id", STRING | STORED);
    let stale_schema = schema_builder.build();

    Index::create_in_dir(index_dir, stale_schema)
        .expect("stale schema runtime index should be created");
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
fn startup_config_resolution_does_not_require_runtime_public_directory() {
    let config_dir = unique_temp_dir("komga-runtime-no-public-needed");
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
    .expect("runtime config should resolve without runtime public directory");

    assert!(
        !config_dir.join("public").exists(),
        "test contract should not create or require a runtime public directory",
    );
    assert_eq!(config.config_dir.as_deref(), Some(config_dir.as_path()));
    assert_eq!(config.database_file, config_dir.join("database.sqlite"));
    assert_eq!(config.tasks_db_file, config_dir.join("tasks.sqlite"));
    assert_eq!(config.lucene_data_directory, config_dir.join("lucene"));
    assert_eq!(config.fonts_data_directory, config_dir.join("fonts"));
    assert_eq!(config.log_file, config_dir.join("logs").join("komga.log"));
}

#[test]
fn obsolete_webui_env_var_is_silently_ignored_during_startup_resolution() {
    let config_dir = unique_temp_dir("komga-runtime-ignored-webui-env");
    fs::create_dir_all(&config_dir).expect("test config directory should be created");

    let mut env = BTreeMap::new();
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        config_dir.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_WEBUI_DIR".to_string(),
        "/tmp/ignored-webui".to_string(),
    );

    let config = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli::default(),
        &env,
    )
    .expect("obsolete WebUI env var should be ignored, not rejected");

    assert_eq!(config.config_dir.as_deref(), Some(config_dir.as_path()));
    assert_eq!(config.database_file, config_dir.join("database.sqlite"));
}

#[test]
fn runtime_config_prefers_cli_config_dir_over_env_and_file_defaults() {
    let cli_config_dir = unique_temp_dir("komga-runtime-cli-config-dir");
    fs::create_dir_all(&cli_config_dir).expect("cli config directory should be created");

    fs::write(
        cli_config_dir.join("application.yml"),
        r#"
komga:
  database:
    file: ${komga.config-dir}/from-cli/database.sqlite
"#,
    )
    .expect("application.yml should be written");

    let mut env = BTreeMap::new();
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        unique_temp_dir("komga-runtime-env-config-dir")
            .to_string_lossy()
            .to_string(),
    );

    let cli = komga_rust::config::RuntimeCli {
        config_dir: Some(cli_config_dir.clone()),
        ..Default::default()
    };

    let config = komga_rust::config::RuntimeConfig::resolve_with_env(&cli, &env)
        .expect("runtime config should resolve");

    assert_eq!(config.config_dir.as_deref(), Some(cli_config_dir.as_path()));
    assert_eq!(
        config.database_file,
        cli_config_dir.join("from-cli").join("database.sqlite"),
    );
}

#[test]
fn startup_config_dir_env_overrides_application_property_config_dir() {
    let bootstrap_config_dir = unique_temp_dir("komga-runtime-bootstrap-config-dir");
    let file_declared_config_dir = unique_temp_dir("komga-runtime-file-config-dir");
    fs::create_dir_all(&bootstrap_config_dir)
        .expect("bootstrap config directory should be created");
    fs::create_dir_all(&file_declared_config_dir)
        .expect("file-declared config directory should be created");

    fs::write(
        bootstrap_config_dir.join("application.yml"),
        format!(
            "komga:\n  config-dir: {}\nlogging:\n  file:\n    name: ${{komga.config-dir}}/logs/komga.log\n",
            file_declared_config_dir.to_string_lossy(),
        ),
    )
    .expect("application.yml should be written");

    let mut env = BTreeMap::new();
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        bootstrap_config_dir.to_string_lossy().to_string(),
    );

    let config = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli::default(),
        &env,
    )
    .expect("runtime config should resolve");

    assert_eq!(
        config.config_dir.as_deref(),
        Some(bootstrap_config_dir.as_path())
    );
    assert_eq!(
        config.log_file,
        bootstrap_config_dir.join("logs").join("komga.log"),
    );
}

#[test]
fn startup_config_expands_escaped_kotlin_placeholders_for_paths() {
    let config_dir = unique_temp_dir("komga-runtime-escaped-path-placeholder");
    fs::create_dir_all(&config_dir).expect("test config directory should be created");

    fs::write(
        config_dir.join("application.yml"),
        r#"
logging:
  file:
    name: \${komga.config-dir}/logs/komga.log
"#,
    )
    .expect("application.yml should be written");

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

    assert_eq!(config.log_file, config_dir.join("logs").join("komga.log"));
}

#[test]
fn invalid_context_path_from_config_file_fails_startup() {
    let config_dir = unique_temp_dir("komga-runtime-invalid-context-from-file");
    fs::create_dir_all(&config_dir).expect("test config directory should be created");

    fs::write(
        config_dir.join("application.yml"),
        "server:\n  servlet:\n    context-path: /trailing/\n",
    )
    .expect("application.yml should be written");

    let mut env = BTreeMap::new();
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        config_dir.to_string_lossy().to_string(),
    );

    let error = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli::default(),
        &env,
    )
    .expect_err("invalid context path from config file must fail startup");

    assert_eq!(
        error.to_string(),
        "invalid SERVER_SERVLET_CONTEXT_PATH: must be empty or start with '/' and not end with '/'",
    );
}

#[test]
fn isolated_mode_rejects_default_writer_targets_during_startup_resolution() {
    let config_dir = unique_temp_dir("komga-runtime-isolated-writer-ownership");
    fs::create_dir_all(&config_dir).expect("test config directory should be created");

    let mut env = BTreeMap::new();
    env.insert("KOMGA_RUST_MODE".to_string(), "isolated".to_string());
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        config_dir.to_string_lossy().to_string(),
    );

    let error = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli::default(),
        &env,
    )
    .expect_err("isolated mode must reject default writer targets by default");

    assert!(
        error
            .to_string()
            .contains("unsafe mixed-writer storage ownership detected"),
        "startup should fail with mixed-writer ownership error: {error}",
    );
    assert!(
        error.to_string().contains("database.sqlite"),
        "mixed-writer error should attribute blocked target details: {error}",
    );
}

#[test]
fn canary_mode_rejects_default_writer_targets_during_startup_resolution() {
    let config_dir = unique_temp_dir("komga-runtime-canary-default-writer-ownership");
    fs::create_dir_all(&config_dir).expect("test config directory should be created");

    let mut env = BTreeMap::new();
    env.insert("KOMGA_RUST_MODE".to_string(), "canary".to_string());
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        config_dir.to_string_lossy().to_string(),
    );

    let error = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli::default(),
        &env,
    )
    .expect_err("canary mode must reject default writer targets");

    assert!(
        error
            .to_string()
            .contains("unsafe mixed-writer storage ownership detected"),
        "startup should fail with mixed-writer ownership error: {error}",
    );
    assert!(
        error.to_string().contains("database.sqlite"),
        "mixed-writer error should attribute blocked target details: {error}",
    );
}

#[test]
fn canary_mode_accepts_non_default_writer_targets_during_startup_resolution() {
    let config_dir = unique_temp_dir("komga-runtime-canary-owned-writer-targets");
    let canary_root = config_dir.join("canary-owned");
    fs::create_dir_all(&config_dir).expect("test config directory should be created");

    let mut env = BTreeMap::new();
    env.insert("KOMGA_RUST_MODE".to_string(), "canary".to_string());
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        config_dir.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_DATABASE_FILE".to_string(),
        canary_root
            .join("database.sqlite")
            .to_string_lossy()
            .to_string(),
    );
    env.insert(
        "KOMGA_TASKS_DB_FILE".to_string(),
        canary_root
            .join("tasks.sqlite")
            .to_string_lossy()
            .to_string(),
    );
    env.insert(
        "KOMGA_LUCENE_DATA_DIRECTORY".to_string(),
        canary_root.join("lucene").to_string_lossy().to_string(),
    );

    let config = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli::default(),
        &env,
    )
    .expect("canary mode should accept non-default writer targets");

    assert_eq!(config.mode, komga_rust::config::RuntimeMode::Canary);
    assert_eq!(config.database_file, canary_root.join("database.sqlite"));
    assert_eq!(config.tasks_db_file, canary_root.join("tasks.sqlite"));
    assert_eq!(config.lucene_data_directory, canary_root.join("lucene"));
}

#[test]
fn startup_search_lifecycle_missing_index_enqueues_rebuild_contract() {
    let lucene_dir = unique_temp_dir("komga-runtime-startup-search-missing-index");
    fs::create_dir_all(&lucene_dir).expect("lucene directory should be created");

    let mut config = komga_rust::config::RuntimeConfig::for_runtime_profile(
        komga_rust::config::RuntimeProfile::SnapshotAligned,
    );
    config.lucene_data_directory = lucene_dir.clone();

    let startup_task = komga_server::app::prepare_startup_search_task_for_contract(&config)
        .expect("missing index startup preparation should succeed");

    assert_eq!(startup_task, Some("REBUILD_INDEX"));
    assert!(
        !lucene_dir.join("meta.json").exists(),
        "startup lifecycle decision must not create an index before the rebuild task runs",
    );
}

#[test]
fn startup_search_lifecycle_existing_runtime_index_skips_startup_task_contract() {
    let lucene_dir = unique_temp_dir("komga-runtime-startup-search-existing-index");
    fs::create_dir_all(&lucene_dir).expect("lucene directory should be created");
    komga_rust::SearchIndexLifecycle::bootstrap(lucene_dir.as_path())
        .expect("runtime index bootstrap should create an existing index");

    let mut config = komga_rust::config::RuntimeConfig::for_runtime_profile(
        komga_rust::config::RuntimeProfile::SnapshotAligned,
    );
    config.lucene_data_directory = lucene_dir.clone();

    let startup_task = komga_server::app::prepare_startup_search_task_for_contract(&config)
        .expect("existing runtime index startup preparation should succeed");

    assert_eq!(startup_task, None);
}

#[test]
fn startup_search_lifecycle_stale_schema_forces_rebuild_contract() {
    let lucene_dir = unique_temp_dir("komga-runtime-startup-search-stale-schema");
    create_stale_schema_search_index(lucene_dir.as_path());
    let stale_meta_before = fs::read_to_string(lucene_dir.join("meta.json"))
        .expect("stale schema index should expose meta.json");

    let mut config = komga_rust::config::RuntimeConfig::for_runtime_profile(
        komga_rust::config::RuntimeProfile::SnapshotAligned,
    );
    config.lucene_data_directory = lucene_dir.clone();

    let startup_task = komga_server::app::prepare_startup_search_task_for_contract(&config)
        .expect("stale schema startup preparation should recover through explicit rebuild");

    assert_eq!(startup_task, Some("REBUILD_INDEX"));
    assert_eq!(
        fs::read_to_string(lucene_dir.join("meta.json")).ok(),
        None,
        "startup lifecycle must clear stale schema state without recreating the index before rebuild",
    );
    assert!(
        !stale_meta_before.is_empty(),
        "fixture sanity: stale schema test should start from a real legacy meta.json",
    );
}

#[test]
fn startup_search_lifecycle_corrupt_index_forces_rebuild_contract() {
    let lucene_dir = unique_temp_dir("komga-runtime-startup-search-corrupt-index");
    fs::create_dir_all(&lucene_dir).expect("lucene directory should be created");
    fs::write(lucene_dir.join("meta.json"), b"not-valid-json")
        .expect("corrupted meta marker should be written");

    let mut config = komga_rust::config::RuntimeConfig::for_runtime_profile(
        komga_rust::config::RuntimeProfile::SnapshotAligned,
    );
    config.lucene_data_directory = lucene_dir.clone();

    let startup_task = komga_server::app::prepare_startup_search_task_for_contract(&config)
        .expect("corrupt index startup preparation should recover through explicit rebuild");

    assert_eq!(startup_task, Some("REBUILD_INDEX"));
    assert_eq!(
        fs::read_to_string(lucene_dir.join("meta.json")).ok(),
        None,
        "startup lifecycle must clear corrupt index state without recreating the index before rebuild",
    );
}

#[test]
fn startup_search_lifecycle_external_owned_index_skips_recovery_contract() {
    let config_root = unique_temp_dir("komga-runtime-startup-search-external-owned");
    let lucene_dir = config_root.join("lucene");
    fs::create_dir_all(&lucene_dir).expect("lucene directory should be created");
    fs::write(lucene_dir.join("meta.json"), b"not-valid-json")
        .expect("corrupted meta marker should be written");

    let mut config = komga_rust::config::RuntimeConfig::for_runtime_profile(
        komga_rust::config::RuntimeProfile::SnapshotAligned,
    );
    config.mode = komga_rust::config::RuntimeMode::Isolated;
    config.writer_ownership_policy = komga_rust::config::WriterOwnershipPolicy {
        isolation_root: Some(config_root.clone()),
        allow_isolated_writes: true,
    };
    config.lucene_data_directory = lucene_dir.clone();

    let startup_task = komga_server::app::prepare_startup_search_task_for_contract(&config)
        .expect("external-owned search index startup should skip recovery");

    assert_eq!(startup_task, None);
    assert_eq!(
        fs::read_to_string(lucene_dir.join("meta.json"))
            .expect("external-owned meta should remain readable"),
        "not-valid-json",
        "startup must not rewrite external-owned search index",
    );
}
