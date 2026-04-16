use super::support::*;
use super::*;
use komga_infrastructure::sqlite::connect_pool;

#[test]
fn runtime_startup_prepare_task_queue_enqueues_search_rebuild_without_processing_it_inline() {
    let _guard = startup_contract_lock();
    let config = runtime_config_for_logging_contract("komga-runtime-startup-worker-bootstrap");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("startup worker bootstrap test runtime should build");
    runtime.block_on(async {
        komga_server::app::validate_startup_schema_gate_for_contract(&config)
            .await
            .expect("startup worker bootstrap schema should initialize");

        let pool = connect_pool(config.database_file.as_path(), 1)
            .await
            .expect("startup worker bootstrap db should open");
        sqlx::query(
            "INSERT INTO LIBRARY (ID, NAME, ROOT, SCAN_STARTUP, SCAN_INTERVAL) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("library-1")
        .bind("Library 1")
        .bind(config.config_dir.as_ref().expect("config dir should exist").to_string_lossy().to_string())
        .bind(true)
        .bind("DAILY")
        .execute(&pool)
        .await
        .expect("startup worker bootstrap library row should be inserted");
        pool.close().await;
    });

    let (logs, queued_rebuild_tasks) = capture_contract_log_async_result(&config, {
        let config = config.clone();
        async move {
            let background = komga_infrastructure::task_queue::worker_runtime::prepare_task_queue(
                runtime_task_context(&config),
                Some("REBUILD_INDEX"),
            );
            background
                .task_queue
                .lock()
                .expect("startup worker bootstrap queue lock should not be poisoned")
                .count_by_simple_type()
                .get("REBUILD_INDEX")
                .copied()
                .unwrap_or(0)
        }
    });

    let events = parse_json_log_lines(&logs);
    let library_start = runtime_event_with_component(
        &events,
        "worker_bootstrap",
        "startup_library_scans",
        "started",
    );
    let library_complete = runtime_event_with_component(
        &events,
        "worker_bootstrap",
        "startup_library_scans",
        "completed",
    );
    let search_start = runtime_event_with_component(
        &events,
        "worker_bootstrap",
        "startup_search_task",
        "started",
    );
    let search_complete = runtime_event_with_component(
        &events,
        "worker_bootstrap",
        "startup_search_task",
        "completed",
    );

    println!("runtime_startup_prepare_task_queue_worker_logs {logs}");

    assert_eq!(field_bool(library_start, "owns_main_database"), Some(true));
    assert_eq!(field_bool(search_start, "consumes_queue"), Some(true));
    assert_eq!(field_bool(search_start, "owns_search_index"), Some(true));
    assert_eq!(field_u64(library_complete, "enqueued"), Some(1));
    assert_eq!(field_u64(search_complete, "enqueued"), Some(1));
    assert_eq!(field_u64(search_complete, "processed"), Some(0));
    assert_eq!(
        field_str(search_complete, "startup_task"),
        Some("REBUILD_INDEX")
    );
    assert_eq!(queued_rebuild_tasks, 1);
}

#[test]
fn runtime_startup_prepare_task_queue_logs_truthful_skip_boundaries_for_external_owned_runtime() {
    let _guard = startup_contract_lock();
    let root = unique_temp_dir("komga-runtime-startup-worker-bootstrap-skip");
    fs::create_dir_all(&root).expect("startup worker bootstrap skip root should exist");
    let runtime = komga_application::task_processing::TaskRuntimeContext {
        database_file: root.join("database.sqlite"),
        tasks_db_file: root.join("tasks.sqlite"),
        lucene_data_directory: root.join("lucene"),
        consumes_queue: false,
        owns_main_database: false,
        owns_filesystem_scan_output: false,
        owns_sidecar_output: false,
        owns_search_index: false,
    };

    let mut config =
        runtime_config_for_logging_contract("komga-runtime-startup-worker-bootstrap-skip-logs");
    config.log_file = root.join("logs").join("komga.log");

    let logs = capture_contract_log_async(&config, async move {
        let _background = komga_infrastructure::task_queue::worker_runtime::prepare_task_queue(
            runtime,
            Some("REBUILD_INDEX"),
        );
    });

    let events = parse_json_log_lines(&logs);
    let library_skip = runtime_event_with_component(
        &events,
        "worker_bootstrap",
        "startup_library_scans",
        "skipped",
    );
    let search_skip = runtime_event_with_component(
        &events,
        "worker_bootstrap",
        "startup_search_task",
        "skipped",
    );

    println!("runtime_startup_prepare_task_queue_worker_skip_logs {logs}");

    assert_eq!(field_bool(library_skip, "owns_main_database"), Some(false));
    assert_eq!(field_bool(search_skip, "consumes_queue"), Some(false));
    assert_eq!(
        field_str(search_skip, "skip_reason"),
        Some("queue_consumption_disabled")
    );
}

#[test]
fn runtime_startup_prepare_task_queue_skips_search_rebuild_when_search_index_not_owned() {
    let _guard = startup_contract_lock();
    let root = unique_temp_dir("komga-runtime-startup-worker-bootstrap-search-not-owned");
    fs::create_dir_all(&root).expect("mixed ownership startup worker root should exist");
    let runtime = komga_application::task_processing::TaskRuntimeContext {
        database_file: root.join("database.sqlite"),
        tasks_db_file: root.join("tasks.sqlite"),
        lucene_data_directory: root.join("lucene"),
        consumes_queue: true,
        owns_main_database: true,
        owns_filesystem_scan_output: false,
        owns_sidecar_output: false,
        owns_search_index: false,
    };

    let mut config = runtime_config_for_logging_contract(
        "komga-runtime-startup-worker-bootstrap-search-not-owned-logs",
    );
    config.log_file = root.join("logs").join("komga.log");

    let (logs, queued_rebuild_tasks) = capture_contract_log_async_result(&config, async move {
        let background = komga_infrastructure::task_queue::worker_runtime::prepare_task_queue(
            runtime,
            Some("REBUILD_INDEX"),
        );
        background
            .task_queue
            .lock()
            .expect("mixed ownership startup queue lock should not be poisoned")
            .count_by_simple_type()
            .get("REBUILD_INDEX")
            .copied()
            .unwrap_or(0)
    });

    let events = parse_json_log_lines(&logs);
    let search_skip = runtime_event_with_component(
        &events,
        "worker_bootstrap",
        "startup_search_task",
        "skipped",
    );

    assert_eq!(field_bool(search_skip, "consumes_queue"), Some(true));
    assert_eq!(field_bool(search_skip, "owns_search_index"), Some(false));
    assert_eq!(
        field_str(search_skip, "skip_reason"),
        Some("search_index_not_owned")
    );
    assert_eq!(queued_rebuild_tasks, 0);
}

#[test]
fn runtime_startup_prepare_task_queue_logs_no_startup_library_scan_skip_when_no_profiles_request_it()
 {
    let _guard = startup_contract_lock();
    let config =
        runtime_config_for_logging_contract("komga-runtime-startup-worker-bootstrap-no-startup");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("startup no-startup-profile test runtime should build");
    runtime.block_on(async {
        komga_server::app::validate_startup_schema_gate_for_contract(&config)
            .await
            .expect("startup no-startup-profile schema should initialize");

        let pool = connect_pool(config.database_file.as_path(), 1)
            .await
            .expect("startup no-startup-profile db should open");
        sqlx::query(
            "INSERT INTO LIBRARY (ID, NAME, ROOT, SCAN_STARTUP, SCAN_INTERVAL) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("library-1")
        .bind("Library 1")
        .bind(config.config_dir.as_ref().expect("config dir should exist").to_string_lossy().to_string())
        .bind(false)
        .bind("DISABLED")
        .execute(&pool)
        .await
        .expect("startup no-startup-profile library row should be inserted");
        pool.close().await;
    });

    let (logs, queued_scan_tasks) = capture_contract_log_async_result(&config, {
        let config = config.clone();
        async move {
            let background = komga_infrastructure::task_queue::worker_runtime::prepare_task_queue(
                runtime_task_context(&config),
                None,
            );
            background
                .task_queue
                .lock()
                .expect("startup no-startup-profile queue lock should not be poisoned")
                .count_by_simple_type()
                .get("SCAN_LIBRARY")
                .copied()
                .unwrap_or(0)
        }
    });

    let events = parse_json_log_lines(&logs);
    let library_skip = runtime_event_with_component(
        &events,
        "worker_bootstrap",
        "startup_library_scans",
        "skipped",
    );
    let search_skip = runtime_event_with_component(
        &events,
        "worker_bootstrap",
        "startup_search_task",
        "skipped",
    );

    assert_eq!(
        field_str(library_skip, "skip_reason"),
        Some("no_startup_library_scans")
    );
    assert_eq!(
        field_str(search_skip, "skip_reason"),
        Some("startup_task_not_requested")
    );
    assert_eq!(queued_scan_tasks, 0);
}

#[test]
fn runtime_startup_library_scan_processing_logs_run_complete_and_skip_boundaries() {
    let _guard = startup_contract_lock();
    let config =
        runtime_config_for_logging_contract("komga-runtime-startup-library-scan-processing");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("startup scan processing test runtime should build");
    runtime.block_on(async {
        komga_server::app::validate_startup_schema_gate_for_contract(&config)
            .await
            .expect("startup scan processing schema should initialize");

        let pool = connect_pool(config.database_file.as_path(), 1)
            .await
            .expect("startup scan processing db should open");
        sqlx::query(
            "INSERT INTO LIBRARY (ID, NAME, ROOT, SCAN_STARTUP, SCAN_INTERVAL) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("library-1")
        .bind("Library 1")
        .bind(config.config_dir.as_ref().expect("config dir should exist").to_string_lossy().to_string())
        .bind(true)
        .bind("DAILY")
        .execute(&pool)
        .await
        .expect("startup scan processing library row should be inserted");
        pool.close().await;
    });

    let (run_logs, ()) = capture_contract_log_async_result(&config, {
        let config = config.clone();
        async move {
            komga_infrastructure::task_queue::worker_runtime::process_startup_library_scans(
                runtime_task_context(&config),
            );
        }
    });
    let run_events = parse_json_log_lines(&run_logs);
    let run_start = runtime_event_with_component(
        &run_events,
        "worker_bootstrap",
        "startup_library_scan_processing",
        "started",
    );
    let run_complete = runtime_event_with_component(
        &run_events,
        "worker_bootstrap",
        "startup_library_scan_processing",
        "completed",
    );

    println!("runtime_startup_library_scan_processing_logs {run_logs}");

    assert_eq!(field_bool(run_start, "owns_main_database"), Some(true));
    assert_eq!(field_u64(run_complete, "processed"), Some(1));

    let disabled_startup_config = runtime_config_for_logging_contract(
        "komga-runtime-startup-library-scan-processing-disabled-startup",
    );
    runtime.block_on(async {
        komga_server::app::validate_startup_schema_gate_for_contract(&disabled_startup_config)
            .await
            .expect("startup disabled-startup processing schema should initialize");

        let pool = connect_pool(disabled_startup_config.database_file.as_path(), 1)
            .await
            .expect("startup disabled-startup processing db should open");
        sqlx::query(
            "INSERT INTO LIBRARY (ID, NAME, ROOT, SCAN_STARTUP, SCAN_INTERVAL) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("library-2")
        .bind("Library 2")
        .bind(
            disabled_startup_config
                .config_dir
                .as_ref()
                .expect("config dir should exist")
                .to_string_lossy()
                .to_string(),
        )
        .bind(false)
        .bind("DAILY")
        .execute(&pool)
        .await
        .expect("startup disabled-startup processing library row should be inserted");
        pool.close().await;
    });

    let disabled_startup_logs = capture_contract_log_async(&disabled_startup_config, {
        let config = disabled_startup_config.clone();
        async move {
            komga_infrastructure::task_queue::worker_runtime::process_startup_library_scans(
                runtime_task_context(&config),
            );
        }
    });
    let disabled_startup_events = parse_json_log_lines(&disabled_startup_logs);
    let disabled_startup_skip = runtime_event_with_component(
        &disabled_startup_events,
        "worker_bootstrap",
        "startup_library_scan_processing",
        "skipped",
    );

    assert_eq!(
        field_bool(disabled_startup_skip, "owns_main_database"),
        Some(true)
    );
    assert_eq!(
        field_str(disabled_startup_skip, "skip_reason"),
        Some("no_startup_library_scans")
    );

    let skip_root = unique_temp_dir("komga-runtime-startup-library-scan-processing-skip");
    let skip_runtime = komga_application::task_processing::TaskRuntimeContext {
        database_file: skip_root.join("database.sqlite"),
        tasks_db_file: skip_root.join("tasks.sqlite"),
        lucene_data_directory: skip_root.join("lucene"),
        consumes_queue: false,
        owns_main_database: false,
        owns_filesystem_scan_output: false,
        owns_sidecar_output: false,
        owns_search_index: false,
    };
    let mut skip_config = runtime_config_for_logging_contract(
        "komga-runtime-startup-library-scan-processing-skip-logs",
    );
    skip_config.log_file = skip_root.join("logs").join("komga.log");

    let skip_logs = capture_contract_log_async(&skip_config, async move {
        komga_infrastructure::task_queue::worker_runtime::process_startup_library_scans(skip_runtime);
    });
    let skip_events = parse_json_log_lines(&skip_logs);
    let skip = runtime_event_with_component(
        &skip_events,
        "worker_bootstrap",
        "startup_library_scan_processing",
        "skipped",
    );

    println!("runtime_startup_library_scan_processing_skip_logs {skip_logs}");

    assert_eq!(field_bool(skip, "owns_main_database"), Some(false));
    assert_eq!(
        field_str(skip, "skip_reason"),
        Some("main_database_not_owned")
    );
}

#[test]
fn runtime_startup_library_scan_processing_logs_no_libraries_skip_boundary() {
    let _guard = startup_contract_lock();
    let config =
        runtime_config_for_logging_contract("komga-runtime-startup-library-scan-processing-empty");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("startup empty scan processing test runtime should build");
    runtime.block_on(async {
        komga_server::app::validate_startup_schema_gate_for_contract(&config)
            .await
            .expect("startup empty scan processing schema should initialize");
    });

    let logs = capture_contract_log_async(&config, {
        let config = config.clone();
        async move {
            komga_infrastructure::task_queue::worker_runtime::process_startup_library_scans(
                runtime_task_context(&config),
            );
        }
    });
    let events = parse_json_log_lines(&logs);
    let skip = runtime_event_with_component(
        &events,
        "worker_bootstrap",
        "startup_library_scan_processing",
        "skipped",
    );

    assert_eq!(field_bool(skip, "owns_main_database"), Some(true));
    assert_eq!(field_str(skip, "skip_reason"), Some("no_libraries"));
}

fn runtime_event_with_component<'a>(
    events: &'a [serde_json::Value],
    event: &str,
    component: &str,
    outcome: &str,
) -> &'a serde_json::Map<String, serde_json::Value> {
    matching_event_fields(events, event)
        .into_iter()
        .find(|fields| {
            field_str(fields, "component") == Some(component)
                && field_str(fields, "outcome") == Some(outcome)
        })
        .unwrap_or_else(|| {
            panic!(
                "expected event {event:?} component {component:?} outcome {outcome:?} in captured logs: {events:?}"
            )
        })
}

fn field_u64(fields: &serde_json::Map<String, serde_json::Value>, field: &str) -> Option<u64> {
    fields.get(field).and_then(serde_json::Value::as_u64)
}
