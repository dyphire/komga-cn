#![allow(dead_code)]

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use axum::Router;
use komga_config::cli_args::RuntimeCli;
use komga_config::env_config::RuntimeConfig;
use komga_infrastructure::search::index_lifecycle::SearchIndexLifecycle;
use komga_infrastructure::search::runtime_tasks::rebuild_index_from_database;
use komga_infrastructure::sqlite::connect_task_write_pool;

use super::persistence_contract_fixture::{self, RuntimeDbPaths};
use super::runtime_router_contract_support::contract_seed::seed_router_contract_data;
use super::runtime_router_contract_support::user_auth::login_with_basic_credentials_and_get_token;

pub struct TestDbFixture {
    paths: Option<RuntimeDbPaths>,
}

impl TestDbFixture {
    pub async fn new(case_id: &str) -> Self {
        let paths = persistence_contract_fixture::new_runtime_db_paths(case_id)
            .expect("fixture paths should be created");
        persistence_contract_fixture::seed_runtime_dbs_from_flyway_template(&paths)
            .await
            .expect("runtime db template seed should succeed");
        TestDbFixture { paths: Some(paths) }
    }

    pub fn new_raw(case_id: &str) -> Self {
        let paths = persistence_contract_fixture::new_runtime_db_paths(case_id)
            .expect("fixture paths should be created");
        TestDbFixture { paths: Some(paths) }
    }

    pub fn paths(&self) -> &RuntimeDbPaths {
        self.paths
            .as_ref()
            .expect("fixture paths should be present")
    }

    pub fn config(&self) -> RuntimeConfig {
        build_snapshot_config(self.paths())
    }

    pub async fn close(mut self) {
        if let Some(paths) = self.paths.take() {
            persistence_contract_fixture::cleanup_async(paths).await;
        }
    }
}

impl Drop for TestDbFixture {
    fn drop(&mut self) {
        if let Some(paths) = self.paths.take() {
            persistence_contract_fixture::cleanup(paths);
        }
    }
}

enum RouterMode {
    WithRuntimeWorkers,
    WithoutRuntimeWorkers,
}

enum ConfigMode {
    SnapshotAligned,
    Demo,
}

type SeedFn = Box<dyn FnOnce(RuntimeDbPaths) -> Pin<Box<dyn Future<Output = ()> + Send>>>;
type ConfigOverrideFn = Box<dyn FnOnce(&mut RuntimeConfig)>;

pub struct TestFixtureBuilder {
    case_id: String,
    router_mode: RouterMode,
    config_mode: ConfigMode,
    with_search_index: bool,
    with_standard_seed: bool,
    seeds: Vec<SeedFn>,
    config_overrides: Vec<ConfigOverrideFn>,
}

impl TestFixtureBuilder {
    pub fn new(case_id: &str) -> Self {
        TestFixtureBuilder {
            case_id: case_id.to_string(),
            router_mode: RouterMode::WithRuntimeWorkers,
            config_mode: ConfigMode::SnapshotAligned,
            with_search_index: false,
            with_standard_seed: true,
            seeds: Vec::new(),
            config_overrides: Vec::new(),
        }
    }

    pub fn with_search_index(mut self) -> Self {
        self.with_search_index = true;
        self
    }

    pub fn without_standard_seed(mut self) -> Self {
        self.with_standard_seed = false;
        self
    }

    pub fn without_runtime_workers(mut self) -> Self {
        self.router_mode = RouterMode::WithoutRuntimeWorkers;
        self
    }

    pub fn demo_mode(mut self) -> Self {
        self.config_mode = ConfigMode::Demo;
        self
    }

    pub fn with_seed<F, Fut>(mut self, f: F) -> Self
    where
        F: FnOnce(RuntimeDbPaths) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        self.seeds.push(Box::new(move |paths| Box::pin(f(paths))));
        self
    }

    pub fn with_config<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut RuntimeConfig) + 'static,
    {
        self.config_overrides.push(Box::new(f));
        self
    }

    pub async fn build(self) -> TestFixture {
        let paths = persistence_contract_fixture::new_runtime_db_paths(&self.case_id)
            .expect("fixture paths should be created");
        persistence_contract_fixture::seed_runtime_dbs_from_flyway_template(&paths)
            .await
            .expect("runtime db template seed should succeed");

        let mut config = match self.config_mode {
            ConfigMode::SnapshotAligned => build_snapshot_config(&paths),
            ConfigMode::Demo => build_demo_config(&paths),
        };

        for override_fn in self.config_overrides {
            override_fn(&mut config);
        }

        if self.with_standard_seed {
            seed_router_contract_data(&paths).await;
        }

        for seed_fn in self.seeds {
            seed_fn(paths.clone()).await;
        }

        if self.with_search_index {
            rebuild_search_index(&paths, &config).await;
        } else {
            bootstrap_empty_search_index(&config);
        }

        let app = match self.router_mode {
            RouterMode::WithRuntimeWorkers => {
                komga_server::app::build_router_with_config(&config).await
            }
            RouterMode::WithoutRuntimeWorkers => {
                komga_server::app::build_router_without_runtime_workers_for_contract(&config).await
            }
        };

        TestFixture {
            db: TestDbFixture { paths: Some(paths) },
            config,
            app,
        }
    }
}

pub struct TestFixture {
    db: TestDbFixture,
    config: RuntimeConfig,
    app: Router,
}

impl TestFixture {
    pub async fn new(case_id: &str) -> Self {
        TestFixtureBuilder::new(case_id).build().await
    }

    pub fn builder(case_id: &str) -> TestFixtureBuilder {
        TestFixtureBuilder::new(case_id)
    }

    pub fn app(&self) -> &Router {
        &self.app
    }

    pub fn config(&self) -> &RuntimeConfig {
        &self.config
    }

    pub fn paths(&self) -> &RuntimeDbPaths {
        self.db.paths()
    }

    pub async fn login_admin(&self) -> String {
        login_with_basic_credentials_and_get_token(
            self.app.clone(),
            "admin@example.org",
            "router-contract-admin-123",
        )
        .await
    }

    pub async fn login_with_credentials(&self, email: &str, password: &str) -> String {
        login_with_basic_credentials_and_get_token(self.app.clone(), email, password).await
    }

    pub async fn close(self) {
        self.db.close().await;
    }

    pub async fn close_shared_pools(&self) {
        persistence_contract_fixture::close_shared_pools(self.paths()).await;
    }
}

fn build_snapshot_config(paths: &RuntimeDbPaths) -> RuntimeConfig {
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

fn build_demo_config(paths: &RuntimeDbPaths) -> RuntimeConfig {
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

fn bootstrap_empty_search_index(config: &RuntimeConfig) {
    SearchIndexLifecycle::bootstrap(config.lucene_data_directory.as_path())
        .expect("search-ready fixture should bootstrap an empty search index");
}

async fn rebuild_search_index(paths: &RuntimeDbPaths, config: &RuntimeConfig) {
    let pool = connect_task_write_pool(paths.main_db.as_path())
        .await
        .expect("search-ready fixture should open pool");
    rebuild_index_from_database(&pool, config.lucene_data_directory.as_path())
        .await
        .expect("search-ready fixture should rebuild the search index");
    pool.close().await;
}
