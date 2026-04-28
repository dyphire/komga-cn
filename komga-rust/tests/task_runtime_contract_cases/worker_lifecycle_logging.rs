use super::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::Instrument;

#[test]
fn runtime_worker_spawns_log_started_and_shutdown_with_span_context() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("worker spawn lifecycle test runtime should build");
    let paths = runtime.block_on(async {
        let paths = new_router_fixture("worker-spawn-lifecycle").await;
        seed_router_contract_data(&paths).await;
        paths
    });
    let config = runtime_config_for_paths(&paths);
    let runtime = runtime_task_context(&paths);

    let logs = capture_router_logs_async_result(&config, {
        let config = config.clone();
        let runtime = runtime.clone();
        async move {
            async move {
                let background =
                    komga_infrastructure::task_queue::worker_runtime::prepare_task_queue(
                        runtime_task_context_from_config(&config),
                        None,
                    )
                    .await;
                komga_infrastructure::task_queue::worker_runtime::spawn_runtime_workers(
                    background.task_queue,
                    runtime,
                    background.task_wakeup,
                    None,
                );
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            .instrument(tracing::info_span!("worker_lifecycle_contract_parent"))
            .await;
        }
    })
    .0;

    let events = parse_json_log_lines(&logs);
    let periodic_start = worker_event(&events, "periodic_library_scan", "started");
    let background_start = worker_event(&events, "background_task", "started");
    let auth_start = worker_event(&events, "authentication_activity_cleanup", "started");
    let periodic_shutdown = worker_event(&events, "periodic_library_scan", "shutdown");
    let background_shutdown = worker_event(&events, "background_task", "shutdown");
    let auth_shutdown = worker_event(&events, "authentication_activity_cleanup", "shutdown");

    println!("runtime_worker_spawn_lifecycle_logs {logs}");

    assert_eq!(field_bool(periodic_start, "in_span"), Some(true));
    assert_eq!(field_bool(background_start, "in_span"), Some(true));
    assert_eq!(field_bool(auth_start, "in_span"), Some(true));
    assert_eq!(field_bool(periodic_start, "consumes_queue"), Some(true));
    assert_eq!(field_bool(auth_start, "owns_main_database"), Some(true));
    assert_eq!(
        field_str(periodic_shutdown, "worker_id"),
        Some("periodic_library_scan")
    );
    assert_eq!(
        field_str(background_shutdown, "worker_id"),
        Some("background_task")
    );
    assert_eq!(
        field_str(auth_shutdown, "worker_id"),
        Some("authentication_activity_cleanup")
    );

    cleanup_router_fixture(paths);
}

#[test]
fn runtime_workers_observe_shutdown_signal_before_runtime_teardown() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("worker shutdown signal test runtime should build");
    let paths = runtime.block_on(async {
        let paths = new_router_fixture("worker-shutdown-signal").await;
        seed_router_contract_data(&paths).await;
        paths
    });
    let config = runtime_config_for_paths(&paths);
    let runtime = runtime_task_context(&paths);

    let logs = capture_router_logs_async_result(&config, {
        let config = config.clone();
        let runtime = runtime.clone();
        async move {
            async move {
                let background =
                    komga_infrastructure::task_queue::worker_runtime::prepare_task_queue(
                        runtime_task_context_from_config(&config),
                        None,
                    )
                    .await;
                let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
                komga_infrastructure::task_queue::worker_runtime::spawn_runtime_workers(
                    background.task_queue,
                    runtime,
                    background.task_wakeup,
                    Some(shutdown_rx),
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
                shutdown_tx
                    .send(true)
                    .expect("worker shutdown signal should send");
                tokio::time::sleep(Duration::from_millis(10)).await;
                tracing::info!(
                    event = "worker_shutdown_signal_marker",
                    "worker shutdown marker"
                );
            }
            .instrument(tracing::info_span!(
                "worker_shutdown_signal_contract_parent"
            ))
            .await;
        }
    })
    .0;

    let events = parse_json_log_lines(&logs);
    let periodic_shutdown_index = event_index(
        &events,
        "worker_shutdown",
        "periodic_library_scan",
        "shutdown",
    );
    let background_shutdown_index =
        event_index(&events, "worker_shutdown", "background_task", "shutdown");
    let auth_shutdown_index = event_index(
        &events,
        "worker_shutdown",
        "authentication_activity_cleanup",
        "shutdown",
    );
    let marker_index = event_index(&events, "worker_shutdown_signal_marker", "", "");

    println!("runtime_worker_shutdown_signal_logs {logs}");

    assert!(
        periodic_shutdown_index < marker_index,
        "periodic worker should stop before marker: {events:?}"
    );
    assert!(
        background_shutdown_index < marker_index,
        "background worker should stop before marker: {events:?}"
    );
    assert!(
        auth_shutdown_index < marker_index,
        "auth cleanup worker should stop before marker: {events:?}"
    );

    cleanup_router_fixture(paths);
}

#[test]
fn periodic_scan_iteration_logs_completion_only_when_due_and_stays_silent_when_idle() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("periodic scan worker test runtime should build");
    let paths = runtime.block_on(async {
        let paths = new_router_fixture("worker-periodic-scan-lifecycle").await;
        seed_router_contract_data(&paths).await;

        let pool = connect_test_pool(paths.main_db.as_path(), 1)
            .await
            .expect("periodic scan worker db should open");
        sqlx::query("UPDATE LIBRARY SET SCAN_INTERVAL = ? WHERE ID = ?")
            .bind("HOURLY")
            .bind("library-1")
            .execute(&pool)
            .await
            .expect("periodic scan worker library interval should be updated");
        pool.close().await;
        paths
    });
    let config = runtime_config_for_paths(&paths);
    let runtime = runtime_task_context(&paths);
    let task_queue = Arc::new(Mutex::new(TaskQueueScheduler::for_runtime(
        runtime.clone(),
        "rust-main",
    )));

    let idle_logs = capture_router_logs_async_result(&config, {
        let runtime = runtime.clone();
        let task_queue = task_queue.clone();
        async move {
            let mut last_run = HashMap::new();
            komga_infrastructure::task_queue::worker_runtime::run_periodic_library_scan_iteration(
                task_queue,
                runtime,
                &mut last_run,
            )
            .await
            .expect("idle periodic scan iteration should succeed");
        }
    })
    .0;
    let idle_events = parse_json_log_lines(&idle_logs);

    println!("periodic_scan_idle_logs {idle_logs}");

    assert_eq!(
        matching_event_fields(&idle_events, "worker_bootstrap").len(),
        0
    );

    let run_logs = capture_router_logs_async_result(&config, {
        let runtime = runtime.clone();
        let task_queue = task_queue.clone();
        async move {
            let mut last_run = HashMap::from([(
                "library-1".to_string(),
                tokio::time::Instant::now() - Duration::from_secs(3_700),
            )]);
            komga_infrastructure::task_queue::worker_runtime::run_periodic_library_scan_iteration(
                task_queue,
                runtime,
                &mut last_run,
            )
            .await
            .expect("due periodic scan iteration should succeed");
        }
    })
    .0;
    let run_events = parse_json_log_lines(&run_logs);
    let run = worker_event(&run_events, "periodic_library_scan", "running");
    let complete = worker_event(&run_events, "periodic_library_scan", "completed");
    let scheduler_complete = matching_event_fields(&run_events, "task_process_available")
        .into_iter()
        .find(|fields| field_str(fields, "outcome") == Some("completed"))
        .expect("periodic scan success path should emit scheduler completed boundary");

    println!("periodic_scan_run_logs {run_logs}");

    assert_eq!(field_str(run, "library_id"), Some("library-1"));
    assert_eq!(field_u64(complete, "enqueued"), Some(1));
    assert_eq!(
        field_u64(complete, "processed"),
        field_u64(scheduler_complete, "processed"),
        "periodic worker completed.processed should reflect actual scheduler processed count",
    );

    let failure_logs = capture_router_logs_async_result(&config, {
        let runtime = runtime.clone();
        let task_queue = task_queue.clone();
        async move {
            let mut last_run = HashMap::new();
            let pool = connect_test_pool(runtime.database_file.as_path(), 1)
                .await
                .expect("periodic scan failure db should open");
            sqlx::query("UPDATE LIBRARY SET SCAN_INTERVAL = ? WHERE ID = ?")
                .bind("FUTURE_VALUE")
                .bind("library-1")
                .execute(&pool)
                .await
                .expect("periodic scan failure interval should be updated");
            pool.close().await;

            komga_infrastructure::task_queue::worker_runtime::run_periodic_library_scan_iteration(
                task_queue,
                runtime,
                &mut last_run,
            )
            .await
            .expect_err("invalid periodic scan interval should fail worker iteration")
        }
    })
    .0;
    let failure_events = parse_json_log_lines(&failure_logs);
    let failure = worker_event(&failure_events, "periodic_library_scan", "failed");

    println!("periodic_scan_failure_logs {failure_logs}");

    assert!(
        field_str(failure, "error")
            .is_some_and(|value| value.contains("unsupported library scan interval: FUTURE_VALUE")),
        "periodic scan failure should emit actionable worker-level error context: {failure:?}",
    );

    cleanup_router_fixture(paths);
}

#[test]
fn periodic_scan_iteration_drains_each_due_library_separately_and_cleans_stale_state() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("periodic multi-library scan worker test runtime should build");
    let paths = runtime.block_on(async {
        let paths = new_router_fixture("worker-periodic-scan-multi-library").await;
        seed_router_contract_data(&paths).await;

        let pool = connect_test_pool(paths.main_db.as_path(), 1)
            .await
            .expect("periodic multi-library scan worker db should open");
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
            .bind("library-2")
            .bind("Library 2")
            .bind(paths.config_dir.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .expect("periodic multi-library scan worker second library should be inserted");
        sqlx::query("UPDATE LIBRARY SET SCAN_INTERVAL = ? WHERE ID IN (?, ?)")
            .bind("HOURLY")
            .bind("library-1")
            .bind("library-2")
            .execute(&pool)
            .await
            .expect("periodic multi-library scan intervals should be updated");
        pool.close().await;
        paths
    });
    let config = runtime_config_for_paths(&paths);
    let runtime = runtime_task_context(&paths);
    let task_queue = Arc::new(Mutex::new(TaskQueueScheduler::for_runtime(
        runtime.clone(),
        "rust-main",
    )));

    let (run_logs, (processed, last_run)) = capture_router_logs_async_result(&config, {
        let runtime = runtime.clone();
        let task_queue = task_queue.clone();
        async move {
            let mut last_run = HashMap::from([
                (
                    "library-1".to_string(),
                    tokio::time::Instant::now() - Duration::from_secs(3_700),
                ),
                (
                    "library-2".to_string(),
                    tokio::time::Instant::now() - Duration::from_secs(3_700),
                ),
                (
                    "stale-library".to_string(),
                    tokio::time::Instant::now() - Duration::from_secs(3_700),
                ),
            ]);
            let processed =
                komga_infrastructure::task_queue::worker_runtime::run_periodic_library_scan_iteration(
                    task_queue,
                    runtime,
                    &mut last_run,
                )
                .await
                .expect("due periodic scan iteration should drain each library separately");
            (processed, last_run)
        }
    });
    let run_events = parse_json_log_lines(&run_logs);
    let complete = worker_event(&run_events, "periodic_library_scan", "completed");
    let scheduler_completions = matching_event_fields(&run_events, "task_process_available")
        .into_iter()
        .filter(|fields| field_str(fields, "outcome") == Some("completed"))
        .collect::<Vec<_>>();

    println!("periodic_scan_multi_library_logs {run_logs}");

    assert_eq!(field_u64(complete, "enqueued"), Some(2));
    assert_eq!(field_u64(complete, "processed"), Some(processed as u64));
    assert_eq!(scheduler_completions.len(), 2);
    assert!(last_run.contains_key("library-1"));
    assert!(last_run.contains_key("library-2"));
    assert!(!last_run.contains_key("stale-library"));

    cleanup_router_fixture(paths);
}

#[test]
fn background_task_iteration_logs_completion_and_failure_without_empty_poll_noise() {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("background worker iteration test runtime should build");
    let paths = executor.block_on(async {
        let paths = new_router_fixture("worker-background-task-lifecycle").await;
        seed_router_contract_data(&paths).await;
        paths
    });
    let config = runtime_config_for_paths(&paths);
    let runtime = runtime_task_context(&paths);

    let idle_queue = Arc::new(Mutex::new(TaskQueueScheduler::for_runtime(
        runtime.clone(),
        "rust-main",
    )));
    let idle_logs = capture_router_logs_async_result(&config, {
        let runtime = runtime.clone();
        let idle_queue = idle_queue.clone();
        async move {
            komga_infrastructure::task_queue::worker_runtime::run_background_task_iteration(
                idle_queue, runtime,
            )
            .await
            .expect("idle background task iteration should succeed");
        }
    })
    .0;
    let idle_events = parse_json_log_lines(&idle_logs);

    println!("background_worker_idle_logs {idle_logs}");

    assert_eq!(
        matching_event_fields(&idle_events, "worker_bootstrap").len(),
        0
    );

    let success_queue = Arc::new(Mutex::new(TaskQueueScheduler::for_runtime(
        runtime.clone(),
        "rust-main",
    )));
    executor.block_on(async {
        let mut queue = success_queue.lock().await;
        queue
            .enqueue(TaskQueueRecord::new("REBUILD_INDEX", 1_000, None))
            .await;
    });
    let success_logs = capture_router_logs_async_result(&config, {
        let runtime = runtime.clone();
        let success_queue = success_queue.clone();
        async move {
            komga_infrastructure::task_queue::worker_runtime::run_background_task_iteration(
                success_queue,
                runtime,
            )
            .await
            .expect("background task iteration should process queued task");
        }
    })
    .0;
    let success_events = parse_json_log_lines(&success_logs);
    let success_run = worker_event(&success_events, "background_task", "running");
    let success_complete = worker_event(&success_events, "background_task", "completed");

    println!("background_worker_success_logs {success_logs}");

    assert_eq!(field_u64(success_run, "queued_tasks"), Some(1));
    assert_eq!(field_u64(success_complete, "processed"), Some(1));

    let failure_queue = Arc::new(Mutex::new(TaskQueueScheduler::for_runtime(
        runtime.clone(),
        "rust-main",
    )));
    executor.block_on(async {
        let mut queue = failure_queue.lock().await;
        queue
            .enqueue(
                TaskQueueRecord::new("UNSUPPORTED_TASK:worker-failure", 1_000, None)
                    .with_simple_type("UNSUPPORTED_TASK"),
            )
            .await;
    });
    let failure_logs = capture_router_logs_async_result(&config, {
        let runtime = runtime.clone();
        let failure_queue = failure_queue.clone();
        async move {
            komga_infrastructure::task_queue::worker_runtime::run_background_task_iteration(
                failure_queue,
                runtime,
            )
            .await
            .expect_err("unsupported task should fail background worker iteration")
            .to_string()
        }
    })
    .0;
    let failure_events = parse_json_log_lines(&failure_logs);
    let failure = worker_event(&failure_events, "background_task", "failed");

    println!("background_worker_failure_logs {failure_logs}");

    assert!(
        field_str(failure, "error")
            .is_some_and(|value| value.contains("unsupported runtime task type: UNSUPPORTED_TASK")),
        "background worker failure should retain task processing error: {failure:?}",
    );

    cleanup_router_fixture(paths);
}

#[test]
fn authentication_cleanup_logs_skip_complete_and_failure_boundaries() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("auth cleanup worker lifecycle test runtime should build");
    let paths = runtime.block_on(async {
        let paths = new_router_fixture("worker-auth-cleanup-lifecycle").await;
        seed_router_contract_data(&paths).await;
        paths
    });
    let config = runtime_task_context(&paths);
    let log_config = runtime_config_for_paths(&paths);

    let complete_logs = capture_router_logs_async_result(&log_config, {
        let config = config.clone();
        async move {
            komga_infrastructure::task_queue::worker_runtime::cleanup_authentication_activity_once(
                &config,
            )
            .await
            .expect("auth cleanup should complete when main db is owned");
        }
    })
    .0;
    let complete_events = parse_json_log_lines(&complete_logs);
    let run = worker_event(
        &complete_events,
        "authentication_activity_cleanup",
        "running",
    );
    let complete = worker_event(
        &complete_events,
        "authentication_activity_cleanup",
        "completed",
    );

    println!("auth_cleanup_complete_logs {complete_logs}");

    assert_eq!(field_bool(run, "owns_main_database"), Some(true));
    assert_eq!(
        field_str(complete, "worker_id"),
        Some("authentication_activity_cleanup")
    );

    let skip_runtime = TaskRuntimeContext {
        owns_main_database: false,
        ..config.clone()
    };
    let skip_logs = capture_router_logs_async_result(&log_config, async move {
        komga_infrastructure::task_queue::worker_runtime::cleanup_authentication_activity_once(
            &skip_runtime,
        )
        .await
        .expect("auth cleanup skip path should return ok");
    })
    .0;
    let skip_events = parse_json_log_lines(&skip_logs);
    let skip = worker_event(&skip_events, "authentication_activity_cleanup", "skipped");

    println!("auth_cleanup_skip_logs {skip_logs}");

    assert_eq!(
        field_str(skip, "skip_reason"),
        Some("main_database_not_owned")
    );

    let invalid_root = std::env::temp_dir().join(format!(
        "komga-worker-auth-cleanup-invalid-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&invalid_root).expect("auth cleanup invalid fixture root should exist");
    let failure_runtime = TaskRuntimeContext {
        database_file: invalid_root.clone(),
        ..config
    };
    let failure_logs = capture_router_logs_async_result(&log_config, async move {
        komga_infrastructure::task_queue::worker_runtime::cleanup_authentication_activity_once(
            &failure_runtime,
        )
        .await
        .expect_err("auth cleanup should fail when db path is a directory")
        .to_string()
    })
    .0;
    let failure_events = parse_json_log_lines(&failure_logs);
    let failure = worker_event(&failure_events, "authentication_activity_cleanup", "failed");

    println!("auth_cleanup_failure_logs {failure_logs}");

    assert!(
        field_str(failure, "error")
            .is_some_and(|value| value.contains("failed to open sqlite database")),
        "auth cleanup failure should keep sqlite error context: {failure:?}",
    );

    cleanup_router_fixture(paths);
}

fn worker_event<'a>(
    events: &'a [Value],
    worker: &str,
    outcome: &str,
) -> &'a serde_json::Map<String, Value> {
    let event = if outcome == "shutdown" {
        "worker_shutdown"
    } else {
        "worker_bootstrap"
    };

    matching_event_fields(events, event)
        .into_iter()
        .find(|fields| {
            field_str(fields, "worker_id") == Some(worker)
                && field_str(fields, "outcome") == Some(outcome)
        })
        .unwrap_or_else(|| {
            panic!(
                "expected {event} for worker {worker:?} outcome {outcome:?} in captured logs: {events:?}"
            )
        })
}

fn field_bool(fields: &serde_json::Map<String, Value>, field: &str) -> Option<bool> {
    fields.get(field).and_then(Value::as_bool)
}

fn event_index(events: &[Value], event: &str, worker: &str, outcome: &str) -> usize {
    events
        .iter()
        .enumerate()
        .find_map(|(index, entry)| {
            let fields = entry.get("fields")?.as_object()?;
            let matches_event = field_str(fields, "event") == Some(event);
            let matches_worker = worker.is_empty() || field_str(fields, "worker_id") == Some(worker);
            let matches_outcome = outcome.is_empty() || field_str(fields, "outcome") == Some(outcome);
            (matches_event && matches_worker && matches_outcome).then_some(index)
        })
        .unwrap_or_else(|| {
            panic!(
                "expected event {event:?} worker {worker:?} outcome {outcome:?} in captured logs: {events:?}"
            )
        })
}
