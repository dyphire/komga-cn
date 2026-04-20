use super::support::*;
use std::fs;
use std::time::Duration;

#[test]
fn runtime_startup_real_server_path_emits_banner_runtime_search_and_bind_events() {
    let _guard = startup_contract_lock();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("startup lifecycle test runtime should build");
    let mut config = runtime_config_for_logging_contract("komga-runtime-startup-lifecycle");
    fs::create_dir_all(&config.lucene_data_directory)
        .expect("lucene directory should be created for startup lifecycle test");
    komga_infrastructure::search::index_lifecycle::SearchIndexLifecycle::bootstrap(
        config.lucene_data_directory.as_path(),
    )
    .expect("startup lifecycle test should bootstrap a valid search index");
    let listener = runtime.block_on(async {
        tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("startup lifecycle test listener should bind")
    });
    config.bind_address = listener
        .local_addr()
        .expect("startup lifecycle test listener should expose local addr");
    let expected_database_file = config.database_file.to_string_lossy().to_string();
    let expected_bind_address = config.bind_address.to_string();

    let logs = capture_contract_log_async(&config, {
        let config = config.clone();
        async move {
            let join = tokio::spawn(async move {
                komga_server::app::serve_with_config(listener, config).await
            });

            tokio::time::sleep(Duration::from_millis(75)).await;
            join.abort();
            let _ = join.await;
        }
    });

    let events = parse_json_log_lines(&logs);
    let banner = event_fields(&events, "startup_banner");
    let runtime = event_fields(&events, "startup_runtime");
    let search = event_fields(&events, "search_startup_decision");
    let bind = event_fields(&events, "server_bind");

    println!("runtime_startup_lifecycle_logs {logs}");

    assert_eq!(event_count(&events, "startup_banner"), 1);
    assert_eq!(field_str(banner, "product"), Some("komga-rust"));
    assert!(
        field_str(banner, "version").is_some_and(|value| !value.is_empty()),
        "startup banner should expose a non-empty product version: {banner:?}",
    );
    assert!(
        field_str(banner, "build_time").is_some_and(|value| !value.is_empty()),
        "startup banner should expose a non-empty build time: {banner:?}",
    );
    assert_eq!(field_str(runtime, "runtime_mode"), Some("snapshot"));
    assert_eq!(
        field_str(runtime, "runtime_profile"),
        Some("snapshot-aligned")
    );
    assert_eq!(
        field_str(runtime, "database_file"),
        Some(expected_database_file.as_str())
    );
    assert_eq!(
        field_str(runtime, "main_database_writer_decision"),
        Some("allowed")
    );
    assert_eq!(
        field_str(runtime, "tasks_database_writer_decision"),
        Some("allowed")
    );
    assert_eq!(
        field_str(runtime, "filesystem_scan_writer_decision"),
        Some("allowed")
    );
    assert_eq!(
        field_str(runtime, "sidecar_writer_decision"),
        Some("allowed")
    );
    assert_eq!(
        field_str(runtime, "search_writer_decision"),
        Some("allowed")
    );
    assert_eq!(field_bool(runtime, "consumes_queue"), Some(true));
    assert_eq!(field_bool(runtime, "owns_main_database"), Some(true));
    assert_eq!(
        field_bool(runtime, "owns_filesystem_scan_output"),
        Some(true)
    );
    assert_eq!(field_bool(runtime, "owns_sidecar_output"), Some(true));
    assert_eq!(field_bool(runtime, "owns_search_index"), Some(true));
    assert_eq!(field_str(search, "search_startup_lifecycle"), Some("ready"));
    assert_eq!(field_str(search, "outcome"), Some("ready"));
    assert_eq!(field_str(search, "startup_task"), Some(""));
    assert_eq!(
        field_str(bind, "bind_address"),
        Some(expected_bind_address.as_str())
    );
}

#[test]
fn runtime_schema_gate_logs_main_and_tasks_lifecycle_events() {
    let _guard = startup_contract_lock();
    let config = runtime_config_for_logging_contract("komga-runtime-schema-gate-success");
    let (logs, result) = capture_contract_log_async_result(&config, {
        let config = config.clone();
        async move { komga_server::app::validate_startup_schema_gate_for_contract(&config).await }
    });

    result.expect("schema gate contract should succeed for fresh sqlite files");
    let events = parse_json_log_lines(&logs);
    let schema_events = matching_event_fields(&events, "startup_schema_gate");

    println!("runtime_schema_gate_success_logs {logs}");

    assert!(
        schema_events.iter().any(|fields| {
            field_str(fields, "database_role") == Some("main")
                && field_str(fields, "outcome") == Some("checking")
        }),
        "schema gate logs should include main checking event: {schema_events:?}",
    );
    assert!(
        schema_events.iter().any(|fields| {
            field_str(fields, "database_role") == Some("main")
                && field_str(fields, "outcome") == Some("ready")
        }),
        "schema gate logs should include main ready event: {schema_events:?}",
    );
    assert!(
        schema_events.iter().any(|fields| {
            field_str(fields, "database_role") == Some("tasks")
                && field_str(fields, "outcome") == Some("checking")
        }),
        "schema gate logs should include tasks checking event: {schema_events:?}",
    );
    assert!(
        schema_events.iter().any(|fields| {
            field_str(fields, "database_role") == Some("tasks")
                && field_str(fields, "outcome") == Some("ready")
        }),
        "schema gate logs should include tasks ready event: {schema_events:?}",
    );
}

#[test]
fn runtime_schema_gate_failure_logs_actionable_context_before_returning_error() {
    let _guard = startup_contract_lock();
    let config = runtime_config_for_logging_contract("komga-runtime-schema-gate-failure");
    fs::create_dir_all(&config.database_file)
        .expect("schema gate failure fixture should create a directory at main db path");
    let expected_database_file = config.database_file.to_string_lossy().to_string();

    let (logs, result) = capture_contract_log_async_result(&config, {
        let config = config.clone();
        async move { komga_server::app::validate_startup_schema_gate_for_contract(&config).await }
    });

    let error = result.expect_err("schema gate should fail when main db path is a directory");
    let events = parse_json_log_lines(&logs);
    let schema_events = matching_event_fields(&events, "startup_schema_gate");

    println!("runtime_schema_gate_failure_logs {logs}");

    assert!(
        error
            .to_string()
            .contains("failed to open main sqlite database"),
        "schema gate failure should retain open-db context: {error}",
    );
    assert!(
        schema_events.iter().any(|fields| {
            field_str(fields, "database_role") == Some("main")
                && field_str(fields, "outcome") == Some("failed")
                && field_str(fields, "database_file") == Some(expected_database_file.as_str())
                && field_str(fields, "error")
                    .is_some_and(|value| value.contains("failed to open main sqlite database"))
        }),
        "schema gate failure should log actionable main-db failure context: {schema_events:?}",
    );
}

#[test]
fn runtime_search_startup_failure_logs_actionable_context_before_returning_error() {
    let _guard = startup_contract_lock();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("startup failure test runtime should build");
    let mut config = runtime_config_for_logging_contract("komga-runtime-search-startup-failure");
    fs::write(&config.lucene_data_directory, b"not-a-directory")
        .expect("search startup failure fixture should place a file at lucene path");
    let expected_lucene_dir = config.lucene_data_directory.to_string_lossy().to_string();
    let listener = runtime.block_on(async {
        tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("startup failure test listener should bind")
    });
    config.bind_address = listener
        .local_addr()
        .expect("startup failure test listener should expose local addr");

    let (logs, result) = capture_contract_log_async_result(&config, {
        let config = config.clone();
        async move { komga_server::app::serve_with_config(listener, config).await }
    });

    let error = result.expect_err("serve_with_config should fail when lucene path is a file");
    let events = parse_json_log_lines(&logs);
    let search = event_fields(&events, "search_startup_decision");

    println!("runtime_search_startup_failure_logs {logs}");

    assert!(
        error
            .to_string()
            .contains("search startup lifecycle decision failed"),
        "search startup failure should preserve planning context: {error}",
    );
    assert_eq!(field_str(search, "outcome"), Some("failed"));
    assert_eq!(
        field_str(search, "lucene_data_directory"),
        Some(expected_lucene_dir.as_str())
    );
    assert!(
        field_str(search, "error")
            .is_some_and(|value| value.contains("search startup lifecycle decision failed")),
        "search startup failure should emit actionable error details: {search:?}",
    );
    assert_eq!(event_count(&events, "server_bind"), 0);
}

#[test]
fn runtime_shutdown_lifecycle_logs_shutdown_and_shared_pool_close_events() {
    let _guard = startup_contract_lock();
    let config = runtime_config_for_logging_contract("komga-runtime-shutdown-lifecycle");
    let logs = capture_contract_log_async(&config, async move {
        komga_server::app::shutdown_runtime_for_contract().await;
    });

    let events = parse_json_log_lines(&logs);
    let shutdown = event_fields(&events, "server_shutdown");
    let pools = event_fields(&events, "shared_pool_close");

    println!("runtime_shutdown_lifecycle_logs {logs}");

    assert_eq!(field_str(shutdown, "outcome"), Some("graceful"));
    assert_eq!(field_str(pools, "outcome"), Some("closed"));
}
