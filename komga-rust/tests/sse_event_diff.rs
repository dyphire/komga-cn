mod compat;

use compat::sse::{
    compare_event_logs, parse_event_log, write_diff_report, SseAudience, SseHarnessConfig,
};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn sse_configuration_loads() {
    let config = SseHarnessConfig::load_default().expect("default SSE compat cases should load");
    let case_ids: Vec<&str> = config.cases.iter().map(|case| case.id.as_str()).collect();
    let connect = config
        .cases
        .iter()
        .find(|case| case.id == "P0-SSE-CONNECT")
        .expect("connect case should exist");
    let admin = config
        .cases
        .iter()
        .find(|case| case.id == "P0-SSE-ADMIN-TASKQUEUE")
        .expect("admin case should exist");

    assert_eq!(config.output_dir, "target/compat-diff");
    assert!(case_ids.contains(&"P0-SSE-CONNECT"));
    assert!(case_ids.contains(&"P0-SSE-ADMIN-TASKQUEUE"));
    assert!(case_ids.contains(&"P0-SSE-SESSION-EXPIRED"));
    assert_eq!(connect.path, "/sse/v1/events");
    assert_eq!(connect.audience, SseAudience::Any);
    assert_eq!(admin.audience, SseAudience::Admin);
    assert!(admin
        .headers
        .as_ref()
        .and_then(|headers| headers.get("X-Auth-Token"))
        .is_some());
}

#[test]
fn heartbeat_and_comments_are_filtered() {
    let log = parse_event_log(
        r#": keep-alive

event: heartbeat
data: {"ignored": true}

event: SessionExpired
data: {"userId": "user-1"}

event: TaskQueueStatus
data: {"counts": {"active": 1, "pending": 2}}

"#,
    )
    .expect("SSE log should parse");

    assert_eq!(log.events.len(), 2);
    assert_eq!(log.events[0].name, "SessionExpired");
    assert_eq!(
        log.events[0].payload,
        serde_json::json!({"userId": "user-1"})
    );
    assert_eq!(log.events[1].name, "TaskQueueStatus");
}

#[test]
fn diff_detects_name_and_payload_mismatches() {
    let java = parse_event_log(
        r#"event: SessionExpired
data: {"userId": "user-1"}

event: TaskQueueStatus
data: {"counts": {"active": 1, "pending": 2}}

"#,
    )
    .expect("java SSE log should parse");
    let rust = parse_event_log(
        r#"event: SessionExpired
data: {"userId": "user-2"}

event: TaskQueueChanged
data: {"counts": {"active": 1, "pending": 2}}

"#,
    )
    .expect("rust SSE log should parse");

    let report = compare_event_logs("P0-SSE-ADMIN-TASKQUEUE", SseAudience::Admin, &java, &rust);

    assert!(!report.matches);
    assert!(report
        .differences
        .iter()
        .any(|difference| difference.contains("payload mismatch")));
    assert!(report
        .differences
        .iter()
        .any(|difference| difference.contains("name mismatch")));

    let output_root = temp_output_root();
    write_diff_report(&output_root, &report).expect("diff report should be written");

    let written = fs::read_to_string(output_root.join("P0-SSE-ADMIN-TASKQUEUE.json"))
        .expect("report file should exist");
    assert!(written.contains("P0-SSE-ADMIN-TASKQUEUE"));
    assert!(written.contains("audience"));
}

#[test]
fn diff_reports_filtering_relevant_extra_events() {
    let java = parse_event_log(
        r#"event: SessionExpired
data: {"userId": "user-1"}

"#,
    )
    .expect("java SSE log should parse");
    let rust = parse_event_log(
        r#"event: SessionExpired
data: {"userId": "user-1"}

event: TaskQueueStatus
data: {"counts": {"active": 1, "pending": 2}}

"#,
    )
    .expect("rust SSE log should parse");

    let report = compare_event_logs("P0-SSE-ADMIN-TASKQUEUE", SseAudience::Admin, &java, &rust);

    assert!(!report.matches);
    assert!(report
        .differences
        .iter()
        .any(|difference| difference.contains("filtered event count mismatch for admin audience")));
    assert!(report
        .differences
        .iter()
        .any(|difference| difference.contains("present only in rust")));
}

fn temp_output_root() -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_millis();
    let path = std::env::temp_dir().join(format!("komga-sse-compat-diff-{millis}"));
    fs::create_dir_all(&path).expect("temp output root should be creatable");
    path
}
