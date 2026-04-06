use std::collections::BTreeMap;

use komga_rust::config::{RuntimeCli, RuntimeConfig};

use super::{RuntimeDbPaths, persistence_contract_fixture};

pub fn cleanup_router_fixture(paths: RuntimeDbPaths) {
    persistence_contract_fixture::cleanup(paths)
}

pub async fn new_router_fixture(case_id: &str) -> RuntimeDbPaths {
    let paths = persistence_contract_fixture::new_runtime_db_paths(case_id)
        .expect("router contract fixture paths should be created");
    persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
        .await
        .expect("main db flyway fixture should be created");
    persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
        .await
        .expect("tasks db flyway fixture should be created");
    paths
}

pub fn runtime_config_for_paths(paths: &RuntimeDbPaths) -> RuntimeConfig {
    let mut env = BTreeMap::new();
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        paths.config_dir.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_DATABASE_FILE".to_string(),
        paths.main_db.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_TASKS_DB_FILE".to_string(),
        paths.tasks_db.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_RUST_RUNTIME_PROFILE".to_string(),
        "snapshot-aligned".to_string(),
    );

    RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &env)
        .expect("runtime config should resolve fixture paths")
}

pub fn runtime_demo_config_for_paths(paths: &RuntimeDbPaths) -> RuntimeConfig {
    let mut env = BTreeMap::new();
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        paths.config_dir.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_DATABASE_FILE".to_string(),
        paths.main_db.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_TASKS_DB_FILE".to_string(),
        paths.tasks_db.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_RUST_RUNTIME_PROFILE".to_string(),
        "snapshot-aligned".to_string(),
    );
    env.insert("SPRING_PROFILES_ACTIVE".to_string(), "demo".to_string());

    RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &env)
        .expect("demo runtime config should resolve fixture paths")
}
