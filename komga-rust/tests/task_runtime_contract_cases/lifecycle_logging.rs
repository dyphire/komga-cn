use super::*;

#[test]
fn scheduler_logs_truthful_success_lifecycle_at_commit_boundaries() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("success lifecycle test runtime should build");
    let paths = runtime.block_on(async {
        let paths = new_router_fixture("scheduler-logging-success-lifecycle").await;
        seed_router_contract_data(&paths).await;
        paths
    });

    let config = runtime_config_for_paths(&paths);
    let task = TaskQueueRecord::new(
        "UpgradeIndex:logging-success",
        1_000,
        Some("search-maintenance".to_string()),
    );

    let (logs, processed) = capture_router_logs_async_result(&config, {
        let config = config.clone();
        let task = task.clone();
        async move {
            let runtime = runtime_task_context_from_config(&config).await;
            let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
            scheduler.enqueue(task).await;
            scheduler
                .process_available(&runtime)
                .await
                .expect("upgrade-index lifecycle fixture should process successfully")
        }
    });

    let events = parse_json_log_lines(&logs);
    let enqueue =
        event_fields_with_task_id(&events, "task_enqueue", "UpgradeIndex:logging-success");
    let claim = event_fields_with_task_id(&events, "task_claim", "UpgradeIndex:logging-success");
    let start = event_fields_with_task_id(&events, "task_start", "UpgradeIndex:logging-success");
    let complete =
        event_fields_with_task_id(&events, "task_complete", "UpgradeIndex:logging-success");
    let process_start = event_fields_with_outcome(&events, "task_process_available", "started");
    let process_complete =
        event_fields_with_outcome(&events, "task_process_available", "completed");

    println!("scheduler_success_lifecycle_logs {logs}");

    assert_eq!(
        processed, 1,
        "success fixture should process exactly one task"
    );
    assert_task_fields(
        enqueue,
        "UpgradeIndex:logging-success",
        "UpgradeIndex",
        1_000,
    );
    assert_task_fields(claim, "UpgradeIndex:logging-success", "UpgradeIndex", 1_000);
    assert_task_fields(start, "UpgradeIndex:logging-success", "UpgradeIndex", 1_000);
    assert_task_fields(
        complete,
        "UpgradeIndex:logging-success",
        "UpgradeIndex",
        1_000,
    );
    assert_eq!(field_str(enqueue, "group"), Some("search-maintenance"));
    assert_eq!(field_str(claim, "consumer_owner"), Some("rust-main"));
    assert_eq!(field_str(claim, "outcome"), Some("claimed"));
    assert_eq!(field_str(start, "outcome"), Some("started"));
    assert_eq!(field_str(complete, "outcome"), Some("completed"));
    assert_eq!(
        field_str(process_start, "consumer_owner"),
        Some("rust-main")
    );
    assert_eq!(
        field_str(process_complete, "consumer_owner"),
        Some("rust-main")
    );
    assert_eq!(field_u64(process_complete, "processed"), Some(1));
}

#[test]
fn scheduler_logs_failure_with_concurrent_success_without_fake_success_events() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failure lifecycle test runtime should build");
    let paths = runtime.block_on(async {
        let paths = new_router_fixture("scheduler-logging-failure-disown").await;
        seed_router_contract_data(&paths).await;
        paths
    });

    let config = runtime_config_for_paths(&paths);
    let failed_task = TaskQueueRecord::new(
        "UNSUPPORTED_TASK:logging-failure",
        2_000,
        Some("broken-group".to_string()),
    )
    .with_simple_type("UNSUPPORTED_TASK");
    let disowned_task = TaskQueueRecord::new(
        "UpgradeIndex:logging-disown",
        1_000,
        Some("search-maintenance".to_string()),
    );

    let (logs, error_text) = capture_router_logs_async_result(&config, {
        let config = config.clone();
        let failed_task = failed_task.clone();
        let disowned_task = disowned_task.clone();
        async move {
            let mut runtime = runtime_task_context_from_config(&config).await;
            runtime.task_pool_size = 2;
            let task_queue = std::sync::Arc::new(tokio::sync::Mutex::new(
                TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await,
            ));
            {
                let queue = task_queue.lock().await;
                queue.enqueue(failed_task).await;
                queue.enqueue(disowned_task).await;
            }

            komga_infrastructure::task_queue::worker_runtime::run_background_task_iteration(
                task_queue, runtime,
            )
            .await
            .expect_err("unsupported task should fail the background task iteration")
            .to_string()
        }
    });

    let events = parse_json_log_lines(&logs);
    let fail = event_fields_with_task_id(&events, "task_fail", "UNSUPPORTED_TASK:logging-failure");
    let complete =
        event_fields_with_task_id(&events, "task_complete", "UpgradeIndex:logging-disown");
    let process_failed = event_fields_with_outcome(&events, "task_process_available", "failed");

    println!("scheduler_failure_disown_logs {logs}");

    assert!(
        error_text.contains("unsupported runtime task type: UNSUPPORTED_TASK"),
        "failure fixture should surface unsupported-task context: {error_text}",
    );
    assert_task_fields(
        fail,
        "UNSUPPORTED_TASK:logging-failure",
        "UNSUPPORTED_TASK",
        2_000,
    );
    assert_eq!(field_str(fail, "outcome"), Some("failed"));
    assert!(
        field_str(fail, "error")
            .is_some_and(|value| value.contains("unsupported runtime task type: UNSUPPORTED_TASK")),
        "failed task should emit actionable error text: {fail:?}",
    );
    assert_task_fields(
        complete,
        "UpgradeIndex:logging-disown",
        "UpgradeIndex",
        1_000,
    );
    assert_eq!(field_str(complete, "outcome"), Some("completed"));
    assert_eq!(field_str(complete, "consumer_owner"), Some("rust-main"));
    assert_eq!(
        field_str(process_failed, "consumer_owner"),
        Some("rust-main")
    );
    assert_eq!(field_u64(process_failed, "processed"), Some(1));
    assert!(
        field_str(process_failed, "error")
            .is_some_and(|value| value.contains("unsupported runtime task type: UNSUPPORTED_TASK")),
        "failed process boundary should retain the task failure reason: {process_failed:?}",
    );
    assert!(
        matching_event_fields(&events, "task_complete")
            .into_iter()
            .all(|fields| field_str(fields, "task_id") != Some("UNSUPPORTED_TASK:logging-failure")),
        "failed task must not emit task_complete: {events:?}",
    );
    assert!(
        matching_event_fields(&events, "task_disown")
            .into_iter()
            .all(|fields| field_str(fields, "task_id") != Some("UpgradeIndex:logging-disown")),
        "concurrently completed tasks must not be logged as disowned: {events:?}",
    );
}

#[test]
fn scheduler_logs_recover_before_reclaiming_owned_work() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("recover lifecycle test runtime should build");
    let paths = runtime.block_on(async {
        let paths = new_router_fixture("scheduler-logging-recover").await;
        seed_router_contract_data(&paths).await;
        paths
    });

    let config = runtime_config_for_paths(&paths);
    let task = TaskQueueRecord::new(
        "UpgradeIndex:logging-recover",
        1_000,
        Some("search-maintenance".to_string()),
    );

    let (logs, processed) = capture_router_logs_async_result(&config, {
        let config = config.clone();
        let task = task.clone();
        async move {
            let runtime = runtime_task_context_from_config(&config).await;
            let scheduler = TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await;
            scheduler.enqueue(task).await;

            let claimed = scheduler
                .take_next()
                .await
                .expect("recover fixture should claim the queued task before recovery");
            assert_eq!(claimed.id, "UpgradeIndex:logging-recover");

            scheduler
                .recover_and_process(&runtime)
                .await
                .expect("recover fixture should reclaim and complete the disowned task")
        }
    });

    let events = parse_json_log_lines(&logs);
    let recover =
        event_fields_with_task_id(&events, "task_recover", "UpgradeIndex:logging-recover");
    let disown = event_fields_with_task_id(&events, "task_disown", "UpgradeIndex:logging-recover");
    let claim_events = task_events(&events, "task_claim", "UpgradeIndex:logging-recover");
    let complete =
        event_fields_with_task_id(&events, "task_complete", "UpgradeIndex:logging-recover");

    println!("scheduler_recover_logs {logs}");

    assert_eq!(
        processed, 1,
        "recover fixture should process exactly one reclaimed task"
    );
    assert_task_fields(
        recover,
        "UpgradeIndex:logging-recover",
        "UpgradeIndex",
        1_000,
    );
    assert_eq!(field_str(recover, "outcome"), Some("recovered"));
    assert_eq!(field_str(disown, "outcome"), Some("disowned"));
    assert_eq!(
        claim_events.len(),
        2,
        "task should be claimed before and after recovery"
    );
    assert_eq!(field_str(complete, "outcome"), Some("completed"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn scheduler_take_next_respects_priority_order_group_locks_and_owner_persistence() {
    let paths = new_router_fixture("scheduler-batch-claim-ordering").await;
    seed_router_contract_data(&paths).await;

    let tasks_pool = connect_test_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for batch claim ordering setup");
    for (id, priority, group_id, class_name, simple_type) in [
        (
            "UpgradeIndex:shared-high",
            1_000_i64,
            Some("shared-group"),
            "org.gotson.komga.application.tasks.Task$UpgradeIndex",
            "UpgradeIndex",
        ),
        (
            "UpgradeIndex:shared-low",
            950_i64,
            Some("shared-group"),
            "org.gotson.komga.application.tasks.Task$UpgradeIndex",
            "UpgradeIndex",
        ),
        (
            "RebuildIndex:free-middle",
            900_i64,
            Some("independent-group"),
            "org.gotson.komga.application.tasks.Task$RebuildIndex",
            "RebuildIndex",
        ),
        (
            "UpgradeIndex:free-low",
            800_i64,
            None,
            "org.gotson.komga.application.tasks.Task$UpgradeIndex",
            "UpgradeIndex",
        ),
    ] {
        sqlx::query(
            "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, NULL)",
        )
        .bind(id)
        .bind(priority)
        .bind(group_id)
        .bind(class_name)
        .bind(simple_type)
        .bind(json!({
            "priority": priority,
            "groupId": group_id,
            "uniqueId": id,
        }).to_string())
        .execute(&tasks_pool)
        .await
        .expect("batch claim ordering task row should insert");
    }
    tasks_pool.close().await;

    let config = runtime_config_for_paths(&paths);
    let runtime = runtime_task_context_from_config(&config).await;
    let scheduler = TaskQueueScheduler::for_runtime(runtime, "rust-main").await;
    let claimed = [
        scheduler
            .take_next()
            .await
            .expect("first claim should exist"),
        scheduler
            .take_next()
            .await
            .expect("second claim should exist"),
        scheduler
            .take_next()
            .await
            .expect("third claim should exist"),
    ];

    assert_eq!(
        claimed
            .iter()
            .map(|task| task.id.clone())
            .collect::<Vec<_>>(),
        vec![
            "UpgradeIndex:shared-high".to_string(),
            "RebuildIndex:free-middle".to_string(),
            "UpgradeIndex:free-low".to_string(),
        ],
        "batch claim should keep priority order while skipping same-group work behind an already claimed head task",
    );
    assert!(
        claimed
            .iter()
            .all(|task| task.owner.as_deref() == Some("rust-main")),
        "claimed tasks should expose their persisted owner after batch selection",
    );

    let verify_pool = connect_test_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should reopen for batch claim ordering verification");
    let rows = sqlx::query("SELECT ID, OWNER FROM TASK ORDER BY PRIORITY DESC, ID ASC")
        .fetch_all(&verify_pool)
        .await
        .expect("batch claim ordering rows should be queryable");
    verify_pool.close().await;

    let owners = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("ID"),
                row.get::<Option<String>, _>("OWNER"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        owners,
        vec![
            (
                "UpgradeIndex:shared-high".to_string(),
                Some("rust-main".to_string()),
            ),
            ("UpgradeIndex:shared-low".to_string(), None),
            (
                "RebuildIndex:free-middle".to_string(),
                Some("rust-main".to_string()),
            ),
            (
                "UpgradeIndex:free-low".to_string(),
                Some("rust-main".to_string()),
            ),
        ],
        "batch claim should persist rust-main ownership only for the tasks that were actually claimable",
    );

    cleanup_router_fixture(paths);
}

fn task_events<'a>(
    events: &'a [Value],
    event: &str,
    task_id: &str,
) -> Vec<&'a serde_json::Map<String, Value>> {
    matching_event_fields(events, event)
        .into_iter()
        .filter(|fields| field_str(fields, "task_id") == Some(task_id))
        .collect()
}

fn event_fields_with_task_id<'a>(
    events: &'a [Value],
    event: &str,
    task_id: &str,
) -> &'a serde_json::Map<String, Value> {
    task_events(events, event, task_id)
        .into_iter()
        .next()
        .unwrap_or_else(|| {
            panic!("expected {event:?} for task {task_id:?} in captured logs: {events:?}")
        })
}

fn event_fields_with_outcome<'a>(
    events: &'a [Value],
    event: &str,
    outcome: &str,
) -> &'a serde_json::Map<String, Value> {
    matching_event_fields(events, event)
        .into_iter()
        .find(|fields| field_str(fields, "outcome") == Some(outcome))
        .unwrap_or_else(|| {
            panic!("expected {event:?} with outcome {outcome:?} in captured logs: {events:?}")
        })
}

fn assert_task_fields(
    fields: &serde_json::Map<String, Value>,
    task_id: &str,
    task_type: &str,
    priority: u64,
) {
    assert_eq!(field_str(fields, "task_id"), Some(task_id));
    assert_eq!(field_str(fields, "task_type"), Some(task_type));
    assert_eq!(field_u64(fields, "priority"), Some(priority));
}
