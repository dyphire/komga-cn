use super::support::*;
use super::*;

#[test]
fn startup_search_lifecycle_missing_index_enqueues_rebuild_contract() {
    let lucene_dir = unique_temp_dir("komga-runtime-startup-search-missing-index");
    fs::create_dir_all(&lucene_dir).expect("lucene directory should be created");

    let mut config = komga_config::env_config::RuntimeConfig::for_runtime_profile(
        komga_config::profile::RuntimeProfile::SnapshotAligned,
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
    komga_infrastructure::search::index_lifecycle::SearchIndexLifecycle::bootstrap(
        lucene_dir.as_path(),
    )
    .expect("runtime index bootstrap should create an existing index");

    let mut config = komga_config::env_config::RuntimeConfig::for_runtime_profile(
        komga_config::profile::RuntimeProfile::SnapshotAligned,
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

    let mut config = komga_config::env_config::RuntimeConfig::for_runtime_profile(
        komga_config::profile::RuntimeProfile::SnapshotAligned,
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
fn startup_search_lifecycle_stale_analyzer_version_forces_rebuild_contract() {
    let lucene_dir = unique_temp_dir("komga-runtime-startup-search-stale-analyzer-version");
    fs::create_dir_all(&lucene_dir).expect("lucene directory should be created");
    create_runtime_index_with_stale_analyzer_version(lucene_dir.as_path());

    let stale_meta_before = fs::read_to_string(lucene_dir.join("meta.json"))
        .expect("stale analyzer version fixture should expose meta.json");

    let mut config = komga_config::env_config::RuntimeConfig::for_runtime_profile(
        komga_config::profile::RuntimeProfile::SnapshotAligned,
    );
    config.lucene_data_directory = lucene_dir.clone();

    let startup_task = komga_server::app::prepare_startup_search_task_for_contract(&config)
        .expect("stale analyzer version startup should recover through explicit rebuild");

    assert_eq!(startup_task, Some("REBUILD_INDEX"));
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
    let lucene_dir = unique_temp_dir("komga-runtime-startup-search-corrupt-index");
    fs::create_dir_all(&lucene_dir).expect("lucene directory should be created");
    fs::write(lucene_dir.join("meta.json"), b"not-valid-json")
        .expect("corrupted meta marker should be written");

    let mut config = komga_config::env_config::RuntimeConfig::for_runtime_profile(
        komga_config::profile::RuntimeProfile::SnapshotAligned,
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

    let mut config = komga_config::env_config::RuntimeConfig::for_runtime_profile(
        komga_config::profile::RuntimeProfile::SnapshotAligned,
    );
    config.mode = komga_config::profile::RuntimeMode::Isolated;
    config.writer_ownership_policy = komga_config::writer_ownership::WriterOwnershipPolicy {
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

#[test]
fn startup_search_lifecycle_external_owned_stale_analyzer_version_skips_recovery_contract() {
    let config_root = unique_temp_dir("komga-runtime-startup-search-external-owned-analyzer");
    let lucene_dir = config_root.join("lucene");
    fs::create_dir_all(&lucene_dir).expect("lucene directory should be created");
    create_runtime_index_with_stale_analyzer_version(lucene_dir.as_path());

    let mut config = komga_config::env_config::RuntimeConfig::for_runtime_profile(
        komga_config::profile::RuntimeProfile::SnapshotAligned,
    );
    config.mode = komga_config::profile::RuntimeMode::Isolated;
    config.writer_ownership_policy = komga_config::writer_ownership::WriterOwnershipPolicy {
        isolation_root: Some(config_root.clone()),
        allow_isolated_writes: true,
    };
    config.lucene_data_directory = lucene_dir.clone();

    let startup_task = komga_server::app::prepare_startup_search_task_for_contract(&config)
        .expect("external-owned stale analyzer version startup should skip recovery");

    assert_eq!(startup_task, None);
    assert_eq!(
        fs::read_to_string(lucene_dir.join(ANALYZER_VERSION_MARKER_FILE))
            .expect("external-owned analyzer marker should remain readable"),
        stale_analyzer_version().to_string(),
        "startup must not rewrite external-owned stale analyzer version markers",
    );
}
