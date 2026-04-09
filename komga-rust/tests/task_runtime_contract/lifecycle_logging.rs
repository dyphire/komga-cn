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
        "UPGRADE_INDEX:logging-success",
        1_000,
        Some("search-maintenance".to_string()),
    );

    let (logs, processed) = capture_router_logs_async_result(&config, {
        let config = config.clone();
        let task = task.clone();
        async move {
            let mut scheduler = TaskQueueScheduler::for_runtime(config.clone(), "rust-main");
            scheduler.enqueue(task);
            scheduler
                .process_available(&config)
                .expect("upgrade-index lifecycle fixture should process successfully")
        }
    });

    let events = parse_json_log_lines(&logs);
    let enqueue =
        event_fields_with_task_id(&events, "task_enqueue", "UPGRADE_INDEX:logging-success");
    let claim = event_fields_with_task_id(&events, "task_claim", "UPGRADE_INDEX:logging-success");
    let start = event_fields_with_task_id(&events, "task_start", "UPGRADE_INDEX:logging-success");
    let complete =
        event_fields_with_task_id(&events, "task_complete", "UPGRADE_INDEX:logging-success");
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
        "UPGRADE_INDEX:logging-success",
        "UPGRADE_INDEX",
        1_000,
    );
    assert_task_fields(
        claim,
        "UPGRADE_INDEX:logging-success",
        "UPGRADE_INDEX",
        1_000,
    );
    assert_task_fields(
        start,
        "UPGRADE_INDEX:logging-success",
        "UPGRADE_INDEX",
        1_000,
    );
    assert_task_fields(
        complete,
        "UPGRADE_INDEX:logging-success",
        "UPGRADE_INDEX",
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
fn scheduler_logs_failure_and_disown_without_fake_success_events() {
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
        "UPGRADE_INDEX:logging-disown",
        1_000,
        Some("search-maintenance".to_string()),
    );

    let (logs, error_text) = capture_router_logs_async_result(&config, {
        let config = config.clone();
        let failed_task = failed_task.clone();
        let disowned_task = disowned_task.clone();
        async move {
            let mut scheduler = TaskQueueScheduler::for_runtime(config.clone(), "rust-main");
            scheduler.set_task_pool_size(2);
            scheduler.enqueue(failed_task);
            scheduler.enqueue(disowned_task);

            scheduler
                .process_available(&config)
                .expect_err("unsupported task should fail process_available")
                .to_string()
        }
    });

    let events = parse_json_log_lines(&logs);
    let fail = event_fields_with_task_id(&events, "task_fail", "UNSUPPORTED_TASK:logging-failure");
    let disown = event_fields_with_task_id(&events, "task_disown", "UPGRADE_INDEX:logging-disown");
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
        disown,
        "UPGRADE_INDEX:logging-disown",
        "UPGRADE_INDEX",
        1_000,
    );
    assert_eq!(field_str(disown, "outcome"), Some("disowned"));
    assert_eq!(field_str(disown, "consumer_owner"), Some("rust-main"));
    assert_eq!(
        field_str(process_failed, "consumer_owner"),
        Some("rust-main")
    );
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
        "UPGRADE_INDEX:logging-recover",
        1_000,
        Some("search-maintenance".to_string()),
    );

    let (logs, processed) = capture_router_logs_async_result(&config, {
        let config = config.clone();
        let task = task.clone();
        async move {
            let mut scheduler = TaskQueueScheduler::for_runtime(config.clone(), "rust-main");
            scheduler.enqueue(task);

            let claimed = scheduler
                .take_next()
                .expect("recover fixture should claim the queued task before recovery");
            assert_eq!(claimed.id, "UPGRADE_INDEX:logging-recover");

            scheduler
                .recover_and_process(&config)
                .expect("recover fixture should reclaim and complete the disowned task")
        }
    });

    let events = parse_json_log_lines(&logs);
    let recover =
        event_fields_with_task_id(&events, "task_recover", "UPGRADE_INDEX:logging-recover");
    let disown = event_fields_with_task_id(&events, "task_disown", "UPGRADE_INDEX:logging-recover");
    let claim_events = task_events(&events, "task_claim", "UPGRADE_INDEX:logging-recover");
    let complete =
        event_fields_with_task_id(&events, "task_complete", "UPGRADE_INDEX:logging-recover");

    println!("scheduler_recover_logs {logs}");

    assert_eq!(
        processed, 1,
        "recover fixture should process exactly one reclaimed task"
    );
    assert_task_fields(
        recover,
        "UPGRADE_INDEX:logging-recover",
        "UPGRADE_INDEX",
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
