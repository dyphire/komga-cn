use super::*;
use komga_infrastructure_base::DatabaseHandle;
use komga_infrastructure_base::{
    connect_task_pool, connect_task_write_pool, default_read_max_connections,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::Instrument;

#[test]
fn runtime_workers_observe_shutdown_signal_before_runtime_teardown() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("worker shutdown signal test runtime should build");
    let ctx = runtime.block_on(
        TestFixture::builder("worker-shutdown-signal")
            .without_runtime_workers()
            .build(),
    );
    let config = ctx.config().clone();

    let logs = capture_router_logs_async_result(&config, {
        let config = config.clone();
        async move {
            async move {
                let runtime = runtime_task_context_from_config(&config).await;
                let background =
                    komga_infrastructure_jobs::prepare_task_queue(runtime.clone(), None).await;
                let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
                let workers = background.spawn_workers(runtime, Some(shutdown_rx));
                tokio::time::sleep(Duration::from_millis(10)).await;
                shutdown_tx
                    .send(true)
                    .expect("worker shutdown signal should send");
                for worker in workers {
                    worker
                        .await
                        .expect("worker should exit after shutdown signal");
                }
                background.shutdown_executor().await;
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
    let marker_index = event_index(&events, "worker_shutdown_signal_marker", "", "");
    for worker in [
        "periodic_library_scan",
        "background_task",
        "authentication_activity_cleanup",
    ] {
        let started = worker_event(&events, worker, "started");
        assert_eq!(field_bool(started, "in_span"), Some(true));
        assert_eq!(field_bool(started, "consumes_queue"), Some(true));
        assert_eq!(field_bool(started, "owns_main_database"), Some(true));
        assert!(event_index(&events, "worker_shutdown", worker, "shutdown") < marker_index);
    }
}

#[tokio::test]
async fn router_keeps_runtime_workers_alive_for_http_enqueued_tasks() {
    let ctx = TestFixture::builder("router-worker-lifecycle-http-enqueue")
        .with_seed(|paths| async move {
            let empty_root = paths.config_dir.join("empty-library-root");
            std::fs::create_dir_all(&empty_root).expect("empty library root should be created");
            let pool = connect_test_pool(paths.main_db.as_path(), 1)
                .await
                .expect("library root update db should open");
            sqlx::query("UPDATE LIBRARY SET ROOT = ?, SCAN_STARTUP = ? WHERE ID = ?")
                .bind(empty_root.to_string_lossy().to_string())
                .bind(false)
                .bind("library-1")
                .execute(&pool)
                .await
                .expect("library root should be updated for runtime worker contract");
            pool.close().await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/libraries/library-1/scan")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("library scan request should build"),
        )
        .await
        .expect("library scan request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    wait_for_empty_task_queue(ctx.paths()).await;
}

#[test]
fn periodic_scan_iteration_logs_completion_only_when_due_and_stays_silent_when_idle() {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("periodic scan worker test runtime should build");
    let ctx = executor.block_on(TestFixture::new("worker-periodic-scan-lifecycle"));

    executor.block_on(async {
        let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
            .await
            .expect("periodic scan worker db should open");
        sqlx::query("UPDATE LIBRARY SET SCAN_INTERVAL = ? WHERE ID = ?")
            .bind("HOURLY")
            .bind("library-1")
            .execute(&pool)
            .await
            .expect("periodic scan worker library interval should be updated");
        pool.close().await;
    });
    let config = ctx.config().clone();
    let runtime = executor.block_on(runtime_task_context(ctx.paths()));
    let task_queue = Arc::new(Mutex::new(executor.block_on(
        TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main"),
    )));

    let idle_logs = capture_router_logs_async_result(&config, {
        let runtime = runtime.clone();
        let task_queue = task_queue.clone();
        async move {
            let mut last_run = HashMap::new();
            komga_infrastructure_jobs::run_periodic_library_scan_iteration(
                task_queue,
                None,
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

    let (run_logs, woke_scheduler) = capture_router_logs_async_result(&config, {
        let runtime = runtime.clone();
        let task_queue = task_queue.clone();
        let task_wakeup = Arc::new(tokio::sync::Notify::new());
        async move {
            // Drive elapsed scan time with Tokio's paused clock so the test does not depend on host uptime.
            tokio::time::pause();
            let last_run_at = tokio::time::Instant::now();
            tokio::time::advance(Duration::from_secs(3_700)).await;
            tokio::time::resume();
            let mut last_run = HashMap::from([("library-1".to_string(), last_run_at)]);
            let wakeup = task_wakeup.notified();
            komga_infrastructure_jobs::run_periodic_library_scan_iteration(
                task_queue,
                Some(task_wakeup.clone()),
                runtime,
                &mut last_run,
            )
            .await
            .expect("due periodic scan iteration should succeed");
            tokio::time::timeout(Duration::from_millis(100), wakeup)
                .await
                .is_ok()
        }
    });
    let run_events = parse_json_log_lines(&run_logs);
    let run = worker_event(&run_events, "periodic_library_scan", "running");
    let complete = worker_event(&run_events, "periodic_library_scan", "completed");

    println!("periodic_scan_run_logs {run_logs}");

    assert_eq!(field_str(run, "library_id"), Some("library-1"));
    assert_eq!(field_u64(complete, "enqueued"), Some(1));
    assert_eq!(field_u64(complete, "processed"), Some(0));
    assert!(
        woke_scheduler,
        "due periodic scan should wake the background scheduler"
    );
    assert!(
        matching_event_fields(&run_events, "task_process_available").is_empty(),
        "periodic scan iteration should only enqueue + wake, not process the queue inline",
    );

    let failure_logs = capture_router_logs_async_result(&config, {
        let runtime = runtime.clone();
        let task_queue = task_queue.clone();
        async move {
            let mut last_run = HashMap::new();
            let pool = connect_test_pool(runtime.job().database().main_db().database_file(), 1)
                .await
                .expect("periodic scan failure db should open");
            sqlx::query("UPDATE LIBRARY SET SCAN_INTERVAL = ? WHERE ID = ?")
                .bind("FUTURE_VALUE")
                .bind("library-1")
                .execute(&pool)
                .await
                .expect("periodic scan failure interval should be updated");
            pool.close().await;

            komga_infrastructure_jobs::run_periodic_library_scan_iteration(
                task_queue,
                None,
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
}

async fn wait_for_empty_task_queue(paths: &RuntimeDbPaths) {
    let pool = connect_test_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open while waiting for background worker");
    let drained = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let row = sqlx::query("SELECT COUNT(*) AS COUNT FROM TASK")
                .fetch_one(&pool)
                .await
                .expect("task count should be queryable");
            if row.get::<i64, _>("COUNT") == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .is_ok();
    let row = sqlx::query("SELECT COUNT(*) AS COUNT FROM TASK")
        .fetch_one(&pool)
        .await
        .expect("final task count should be queryable");
    let remaining = row.get::<i64, _>("COUNT");
    pool.close().await;

    assert!(
        drained,
        "runtime worker should drain HTTP-enqueued task; remaining={remaining}",
    );
}

#[test]
fn periodic_scan_iteration_drains_each_due_library_separately_and_cleans_stale_state() {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("periodic multi-library scan worker test runtime should build");
    let ctx = executor.block_on(TestFixture::new("worker-periodic-scan-multi-library"));

    executor.block_on(async {
        let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
            .await
            .expect("periodic multi-library scan worker db should open");
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
            .bind("library-2")
            .bind("Library 2")
            .bind(ctx.paths().config_dir.to_string_lossy().to_string())
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
    });
    let config = ctx.config().clone();
    let runtime = executor.block_on(runtime_task_context(ctx.paths()));
    let task_queue = Arc::new(Mutex::new(executor.block_on(
        TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main"),
    )));

    let (run_logs, (enqueued, last_run)) = capture_router_logs_async_result(&config, {
        let runtime = runtime.clone();
        let task_queue = task_queue.clone();
        async move {
            // Drive elapsed scan time with Tokio's paused clock so the test does not depend on host uptime.
            tokio::time::pause();
            let last_run_at = tokio::time::Instant::now();
            tokio::time::advance(Duration::from_secs(3_700)).await;
            tokio::time::resume();
            let mut last_run = HashMap::from([
                ("library-1".to_string(), last_run_at),
                ("library-2".to_string(), last_run_at),
                ("stale-library".to_string(), last_run_at),
            ]);
            let enqueued = komga_infrastructure_jobs::run_periodic_library_scan_iteration(
                task_queue,
                None,
                runtime,
                &mut last_run,
            )
            .await
            .expect("due periodic scan iteration should drain each library separately");
            (enqueued, last_run)
        }
    });
    let run_events = parse_json_log_lines(&run_logs);
    let complete = worker_event(&run_events, "periodic_library_scan", "completed");

    println!("periodic_scan_multi_library_logs {run_logs}");

    assert_eq!(field_u64(complete, "enqueued"), Some(2));
    assert_eq!(field_u64(complete, "processed"), Some(0));
    assert_eq!(enqueued, 2);
    assert!(
        matching_event_fields(&run_events, "task_process_available").is_empty(),
        "periodic scan iteration should not emit inline scheduler completion logs",
    );
    assert!(last_run.contains_key("library-1"));
    assert!(last_run.contains_key("library-2"));
    assert!(!last_run.contains_key("stale-library"));
}

#[test]
fn background_task_iteration_logs_completion_and_failure_without_empty_poll_noise() {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("background worker iteration test runtime should build");
    let ctx = executor.block_on(TestFixture::new("worker-background-task-lifecycle"));
    let config = ctx.config().clone();
    let runtime = executor.block_on(runtime_task_context(ctx.paths()));

    let idle_queue = Arc::new(Mutex::new(executor.block_on(
        TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main"),
    )));
    let idle_logs = capture_router_logs_async_result(&config, {
        let runtime = runtime.clone();
        let idle_queue = idle_queue.clone();
        async move {
            komga_infrastructure_jobs::run_background_task_iteration(idle_queue, runtime)
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

    let success_queue = Arc::new(Mutex::new(executor.block_on(
        TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main"),
    )));
    executor.block_on(async {
        let queue = success_queue.lock().await;
        queue
            .enqueue(TaskQueueRecord::new("RebuildIndex", 1_000, None))
            .await
            .expect("task enqueue should succeed");
    });
    let success_logs = capture_router_logs_async_result(&config, {
        let runtime = runtime.clone();
        let success_queue = success_queue.clone();
        async move {
            komga_infrastructure_jobs::run_background_task_iteration(success_queue, runtime)
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

    let failure_queue = Arc::new(Mutex::new(executor.block_on(
        TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main"),
    )));
    executor.block_on(async {
        let queue = failure_queue.lock().await;
        queue
            .enqueue(
                TaskQueueRecord::new("UNSUPPORTED_TASK:worker-failure", 1_000, None)
                    .with_simple_type("UNSUPPORTED_TASK"),
            )
            .await
            .expect("task enqueue should succeed");
    });
    let failure_logs = capture_router_logs_async_result(&config, {
        let runtime = runtime.clone();
        let failure_queue = failure_queue.clone();
        async move {
            komga_infrastructure_jobs::run_background_task_iteration(failure_queue, runtime)
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
}

#[test]
fn authentication_cleanup_logs_skip_complete_and_failure_boundaries() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("auth cleanup worker lifecycle test runtime should build");
    let ctx = runtime.block_on(TestFixture::new("worker-auth-cleanup-lifecycle"));
    let config = runtime.block_on(runtime_task_context(ctx.paths()));
    let log_config = ctx.config().clone();

    let complete_logs = capture_router_logs_async_result(&log_config, {
        let config = config.clone();
        async move {
            komga_infrastructure_jobs::cleanup_authentication_activity_once(&config)
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

    let skip_runtime = runtime.block_on(runtime_task_context_with_ownership(
        ctx.paths(),
        TaskRuntimeOwnership {
            owns_main_database: false,
            ..TaskRuntimeOwnership::all_owned()
        },
    ));
    let skip_logs = capture_router_logs_async_result(&log_config, async move {
        komga_infrastructure_jobs::cleanup_authentication_activity_once(&skip_runtime)
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
    let failure_runtime = runtime.block_on(async {
        let failure_root_db_path = invalid_root.join("database.sqlite");
        let task_write_pool = connect_task_write_pool(&failure_root_db_path)
            .await
            .expect("test private write pool should open");
        let task_read_pool =
            connect_task_pool(&failure_root_db_path, default_read_max_connections())
                .await
                .expect("test private read pool should open");
        TaskRuntimeContext::new(TaskRuntimeContextParams {
            main_db: DatabaseHandle::file_backed(failure_root_db_path)
                .await
                .expect("test db should open"),
            tasks_db_file: config.worker().tasks_db_file().to_path_buf(),
            lucene_data_directory: config.job().search().lucene_data_directory().to_path_buf(),
            consumes_queue: config.worker().consumes_queue(),
            ownership: TaskRuntimeOwnership {
                owns_main_database: config.job().database().owns_main_database(),
                owns_filesystem_scan_output: config
                    .job()
                    .filesystem()
                    .owns_filesystem_scan_output(),
                owns_sidecar_output: config.job().filesystem().owns_sidecar_output(),
                owns_search_index: config.job().search().owns_search_index(),
            },
            task_pool_size: config.worker().task_pool_size(),
            task_write_pool,
            task_read_pool,
            runtime_events: Arc::new(RuntimeSseEventStore::default()),
            riir_db: None,
        })
    });
    let failure_logs = capture_router_logs_async_result(&log_config, async move {
        komga_infrastructure_jobs::cleanup_authentication_activity_once(&failure_runtime)
            .await
            .expect_err("auth cleanup should fail when db has no schema")
            .to_string()
    })
    .0;
    let failure_events = parse_json_log_lines(&failure_logs);
    let failure = worker_event(&failure_events, "authentication_activity_cleanup", "failed");

    println!("auth_cleanup_failure_logs {failure_logs}");

    assert!(
        field_str(failure, "error")
            .is_some_and(|value| value.contains("no such table: AUTHENTICATION_ACTIVITY")),
        "auth cleanup failure should keep sqlite error detail: {failure:?}",
    );
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
