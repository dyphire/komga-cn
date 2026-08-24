use super::support::*;
use super::*;
use sqlx::Row;

#[test]
fn startup_search_lifecycle_missing_index_enqueues_rebuild_contract() {
    let config = runtime_config_for_logging_contract("komga-runtime-startup-search-missing-index");
    let lucene_dir = config.lucene_data_directory.clone();
    fs::create_dir_all(&lucene_dir).expect("lucene directory should be created");

    let queued_rebuild_tasks = build_runtime_without_workers_and_count_rebuild_tasks(&config);

    assert_eq!(queued_rebuild_tasks, 1);
    assert!(
        !lucene_dir.join("meta.json").exists(),
        "startup lifecycle decision must not create an index before the rebuild task runs",
    );
}

#[test]
fn startup_search_lifecycle_existing_runtime_index_skips_startup_task_contract() {
    let config = runtime_config_for_logging_contract("komga-runtime-startup-search-existing-index");
    let lucene_dir = config.lucene_data_directory.clone();
    fs::create_dir_all(&lucene_dir).expect("lucene directory should be created");
    komga_infrastructure::search::SearchIndexLifecycle::bootstrap(lucene_dir.as_path())
        .expect("runtime index bootstrap should create an existing index");

    let queued_rebuild_tasks = build_runtime_without_workers_and_count_rebuild_tasks(&config);

    assert_eq!(queued_rebuild_tasks, 0);
}

#[test]
fn startup_search_lifecycle_stale_schema_forces_rebuild_contract() {
    let config = runtime_config_for_logging_contract("komga-runtime-startup-search-stale-schema");
    let lucene_dir = config.lucene_data_directory.clone();
    create_stale_schema_search_index(lucene_dir.as_path());
    let stale_meta_before = fs::read_to_string(lucene_dir.join("meta.json"))
        .expect("stale schema index should expose meta.json");

    let queued_rebuild_tasks = build_runtime_without_workers_and_count_rebuild_tasks(&config);

    assert_eq!(queued_rebuild_tasks, 1);
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
fn startup_search_lifecycle_stale_analyzer_version_forces_rebuild_contract() {
    let config =
        runtime_config_for_logging_contract("komga-runtime-startup-search-stale-analyzer-version");
    let lucene_dir = config.lucene_data_directory.clone();
    fs::create_dir_all(&lucene_dir).expect("lucene directory should be created");
    create_runtime_index_with_stale_analyzer_version(lucene_dir.as_path());

    let stale_meta_before = fs::read_to_string(lucene_dir.join("meta.json"))
        .expect("stale analyzer version fixture should expose meta.json");

    let queued_rebuild_tasks = build_runtime_without_workers_and_count_rebuild_tasks(&config);

    assert_eq!(queued_rebuild_tasks, 1);
    assert_eq!(
        fs::read_to_string(lucene_dir.join("meta.json")).ok(),
        None,
        "startup lifecycle must clear stale analyzer-version state without recreating the index before rebuild",
    );
    assert!(
        !stale_meta_before.is_empty(),
        "fixture sanity: stale analyzer version test should start from a real runtime meta.json",
    );
}

#[test]
fn startup_search_lifecycle_corrupt_index_forces_rebuild_contract() {
    let config = runtime_config_for_logging_contract("komga-runtime-startup-search-corrupt-index");
    let lucene_dir = config.lucene_data_directory.clone();
    fs::create_dir_all(&lucene_dir).expect("lucene directory should be created");
    fs::write(lucene_dir.join("meta.json"), b"not-valid-json")
        .expect("corrupted meta marker should be written");

    let queued_rebuild_tasks = build_runtime_without_workers_and_count_rebuild_tasks(&config);

    assert_eq!(queued_rebuild_tasks, 1);
    assert_eq!(
        fs::read_to_string(lucene_dir.join("meta.json")).ok(),
        None,
        "startup lifecycle must clear corrupt index state without recreating the index before rebuild",
    );
}

#[test]
fn startup_search_lifecycle_external_owned_index_skips_recovery_contract() {
    let mut config =
        runtime_config_for_logging_contract("komga-runtime-startup-search-external-owned");
    let config_root = config
        .config_dir
        .as_ref()
        .expect("config dir should be set")
        .clone();
    let lucene_dir = config.lucene_data_directory.clone();
    fs::create_dir_all(&lucene_dir).expect("lucene directory should be created");
    fs::write(lucene_dir.join("meta.json"), b"not-valid-json")
        .expect("corrupted meta marker should be written");

    config.mode = komga_config::profile::RuntimeMode::Isolated;
    config.writer_ownership_policy = komga_config::writer_ownership::WriterOwnershipPolicy {
        isolation_root: Some(config_root),
        allow_isolated_writes: true,
    };

    let queued_rebuild_tasks = build_runtime_without_workers_and_count_rebuild_tasks(&config);

    assert_eq!(queued_rebuild_tasks, 0);
    assert_eq!(
        fs::read_to_string(lucene_dir.join("meta.json"))
            .expect("external-owned meta should remain readable"),
        "not-valid-json",
        "startup must not rewrite external-owned search index",
    );
}

#[test]
fn startup_search_lifecycle_external_owned_stale_analyzer_version_skips_recovery_contract() {
    let mut config =
        runtime_config_for_logging_contract("komga-runtime-startup-search-external-owned-analyzer");
    let config_root = config
        .config_dir
        .as_ref()
        .expect("config dir should be set")
        .clone();
    let lucene_dir = config.lucene_data_directory.clone();
    fs::create_dir_all(&lucene_dir).expect("lucene directory should be created");
    create_runtime_index_with_stale_analyzer_version(lucene_dir.as_path());

    config.mode = komga_config::profile::RuntimeMode::Isolated;
    config.writer_ownership_policy = komga_config::writer_ownership::WriterOwnershipPolicy {
        isolation_root: Some(config_root),
        allow_isolated_writes: true,
    };

    let queued_rebuild_tasks = build_runtime_without_workers_and_count_rebuild_tasks(&config);

    assert_eq!(queued_rebuild_tasks, 0);
    assert_eq!(
        fs::read_to_string(lucene_dir.join(ANALYZER_VERSION_MARKER_FILE))
            .expect("external-owned analyzer marker should remain readable"),
        stale_analyzer_version().to_string(),
        "startup must not rewrite external-owned stale analyzer version markers",
    );
}

fn build_runtime_without_workers_and_count_rebuild_tasks(
    config: &komga_config::env_config::RuntimeConfig,
) -> i64 {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("startup search lifecycle runtime should build")
        .block_on(async {
            komga_server::app::validate_startup_schema_gate_for_contract(config)
                .await
                .expect("startup search lifecycle schema should initialize");

            let router =
                komga_server::app::build_router_without_runtime_workers_for_contract(config).await;
            let pool = connect_test_pool(config.tasks_db_file.as_path(), 1)
                .await
                .expect("startup search lifecycle tasks db should open");
            let row = sqlx::query(
                "SELECT COUNT(*) AS COUNT FROM TASK WHERE SIMPLE_TYPE = 'RebuildIndex'",
            )
            .fetch_one(&pool)
            .await
            .expect("startup search lifecycle task count should be readable");
            let count = row.get::<i64, _>("COUNT");
            pool.close().await;
            drop(router);
            count
        })
}
