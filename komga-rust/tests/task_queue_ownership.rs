use std::collections::BTreeMap;

use komga_rust::config::{RuntimeCli, RuntimeConfig};
use komga_rust::task_queue::{TaskQueueAdmin, TaskQueueRecord, TaskQueueScheduler};

#[test]
fn shadow_mode_scheduler_cannot_consume_shared_queue() {
    let config = RuntimeConfig::resolve_with_env(
        &RuntimeCli {
            mode: Some("shadow".to_string()),
            ..Default::default()
        },
        &BTreeMap::new(),
    )
    .expect("runtime config should resolve");

    let mut scheduler = TaskQueueScheduler::for_runtime(config, "rust-shadow");
    scheduler.enqueue(TaskQueueRecord::new("task-1", 50, None));

    assert_eq!(scheduler.take_next(), None);
}

#[test]
fn scheduler_enforces_priority_and_group_ownership() {
    let config = RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &BTreeMap::new())
        .expect("runtime config should resolve");

    let mut scheduler = TaskQueueScheduler::for_runtime(config, "rust-main");
    scheduler.enqueue(TaskQueueRecord::new("low", 1, None));
    scheduler.enqueue(TaskQueueRecord::new(
        "high-g1",
        100,
        Some("group-1".to_string()),
    ));
    scheduler.enqueue(TaskQueueRecord::new(
        "mid-g1",
        80,
        Some("group-1".to_string()),
    ));

    let first = scheduler
        .take_next()
        .expect("first task should be available");
    assert_eq!(first.id, "high-g1");
    assert_eq!(first.owner.as_deref(), Some("rust-main"));

    let second = scheduler
        .take_next()
        .expect("next task should skip locked group and take low");
    assert_eq!(second.id, "low");

    scheduler.complete("high-g1");

    let third = scheduler
        .take_next()
        .expect("group task should become available after owner task completion");
    assert_eq!(third.id, "mid-g1");
}

#[test]
fn admin_read_groups_by_owner_and_clear_only_unowned() {
    let mut admin = TaskQueueAdmin::default();
    admin.enqueue(TaskQueueRecord::new(
        "owned",
        10,
        Some("group-a".to_string()),
    ));
    admin.enqueue(TaskQueueRecord::new(
        "queued",
        9,
        Some("group-a".to_string()),
    ));
    admin.claim("owned", "java-main");

    let grouped = admin.read_grouped_by_owner();
    assert_eq!(
        grouped.get(&Some("java-main".to_string())).map(Vec::len),
        Some(1)
    );
    assert_eq!(grouped.get(&None).map(Vec::len), Some(1));

    let removed = admin.clear_unowned();
    assert_eq!(removed, 1);

    let grouped_after = admin.read_grouped_by_owner();
    assert_eq!(
        grouped_after
            .get(&Some("java-main".to_string()))
            .map(Vec::len),
        Some(1)
    );
    assert!(grouped_after.get(&None).is_none());
}

#[test]
fn shadow_mode_with_isolation_can_consume_isolated_queue() {
    let config = RuntimeConfig::resolve_with_env(
        &RuntimeCli {
            mode: Some("shadow".to_string()),
            shadow_isolation_root: Some("/tmp/komga-shadow".into()),
            allow_shadow_writes: true,
            ..Default::default()
        },
        &BTreeMap::new(),
    )
    .expect("runtime config should resolve");

    let mut scheduler = TaskQueueScheduler::for_runtime(config, "rust-shadow");
    scheduler.enqueue(TaskQueueRecord::new("isolated-task", 20, None));

    let task = scheduler
        .take_next()
        .expect("isolated shadow queue should allow consumption");
    assert_eq!(task.id, "isolated-task");
}
