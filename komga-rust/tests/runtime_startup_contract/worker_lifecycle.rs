use super::support::*;
use super::*;
use komga_rust::infrastructure::sqlite::connect_pool;

#[test]
fn runtime_startup_prepare_task_queue_logs_search_and_library_bootstrap_boundaries() {
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

    let logs = capture_contract_log_async(&config, {
        let config = config.clone();
        async move {
            let _background = komga_rust::infrastructure::task_queue::prepare_task_queue(
                config,
                Some("REBUILD_INDEX"),
            );
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
    assert!(
        field_u64(search_complete, "processed").is_some_and(|value| value >= 1),
        "startup search bootstrap should process at least the requested startup task: {search_complete:?}",
    );
    assert_eq!(
        field_str(search_complete, "startup_task"),
        Some("REBUILD_INDEX")
    );
}

#[test]
fn runtime_startup_prepare_task_queue_logs_truthful_skip_boundaries_for_external_owned_runtime() {
    let _guard = startup_contract_lock();
    let root = unique_temp_dir("komga-runtime-startup-worker-bootstrap-skip");
    fs::create_dir_all(&root).expect("startup worker bootstrap skip root should exist");
    let runtime = komga_rust::application::task_processing::TaskRuntimeContext {
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
        let _background = komga_rust::infrastructure::task_queue::prepare_task_queue(
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
fn runtime_startup_prepare_task_queue_logs_failed_search_bootstrap_without_fake_completion() {
    let _guard = startup_contract_lock();
    let config =
        runtime_config_for_logging_contract("komga-runtime-startup-worker-bootstrap-failed-search");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("startup failed search bootstrap test runtime should build");
    runtime.block_on(async {
        komga_server::app::validate_startup_schema_gate_for_contract(&config)
            .await
            .expect("startup failed search bootstrap schema should initialize");
    });

    let (logs, panic_text) = capture_contract_log_async_result(&config, {
        let config = config.clone();
        async move {
            std::panic::catch_unwind(|| {
                let _background = komga_rust::infrastructure::task_queue::prepare_task_queue(
                    config,
                    Some("UNSUPPORTED_TASK"),
                );
            })
            .expect_err("unsupported startup search task should panic")
            .downcast::<String>()
            .map(|message| *message)
            .or_else(|payload| {
                payload
                    .downcast::<&'static str>()
                    .map(|message| (*message).to_string())
            })
            .unwrap_or_else(|_| "non-string panic payload".to_string())
        }
    });

    let events = parse_json_log_lines(&logs);
    let search_start = runtime_event_with_component(
        &events,
        "worker_bootstrap",
        "startup_search_task",
        "started",
    );
    let search_failed =
        runtime_event_with_component(&events, "worker_bootstrap", "startup_search_task", "failed");

    println!("runtime_startup_prepare_task_queue_failed_search_logs {logs}");

    assert_eq!(
        field_str(search_start, "startup_task"),
        Some("UNSUPPORTED_TASK")
    );
    assert!(
        field_str(search_failed, "error")
            .is_some_and(|value| value.contains("unsupported runtime task type: UNSUPPORTED_TASK")),
        "startup failed search bootstrap should preserve processing failure details: {search_failed:?}",
    );
    assert!(
        matching_event_fields(&events, "worker_bootstrap")
            .into_iter()
            .filter(|fields| field_str(fields, "component") == Some("startup_search_task"))
            .all(|fields| field_str(fields, "outcome") != Some("completed")),
        "startup failed search bootstrap must not emit fake completed lifecycle events: {events:?}",
    );
    assert!(
        panic_text.contains("unsupported runtime task type: UNSUPPORTED_TASK"),
        "startup failed search bootstrap should still surface the real panic reason: {panic_text}",
    );
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
            komga_rust::infrastructure::task_queue::process_startup_library_scans(config);
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

    let skip_root = unique_temp_dir("komga-runtime-startup-library-scan-processing-skip");
    let skip_runtime = komga_rust::application::task_processing::TaskRuntimeContext {
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
        komga_rust::infrastructure::task_queue::process_startup_library_scans(skip_runtime);
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
