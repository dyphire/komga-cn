use super::*;
use http_body_util::BodyExt;
use komga_application::media_assets::{BooksImportEntry, ImportCopyMode, MediaImportPort};
use komga_infrastructure::filesystem::import::FilesystemImportPort;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

const MEMBER_PASSWORD: &str = "router-contract-member-123";

pub(super) async fn read_sse_until(
    mut body: axum::body::Body,
    predicate: impl Fn(&str) -> bool,
    timeout: Duration,
) -> String {
    let deadline = tokio::time::Instant::now() + timeout;
    let mut buffer = String::new();

    loop {
        if predicate(&buffer) {
            return buffer;
        }

        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        assert!(
            remaining > Duration::ZERO,
            "timed out waiting for SSE output: {buffer}"
        );

        let frame = tokio::time::timeout(remaining, body.frame())
            .await
            .expect("sse body should yield a frame before timeout")
            .expect("sse stream should stay open")
            .expect("sse frame should decode successfully");

        if let Ok(data) = frame.into_data() {
            buffer.push_str(&String::from_utf8_lossy(&data));
        }
    }
}

#[derive(Debug)]
pub(super) struct ParsedEventLog {
    pub events: Vec<ParsedEvent>,
}

#[derive(Debug)]
pub(super) struct ParsedEvent {
    pub name: String,
    pub payload: serde_json::Value,
}

pub(super) fn parse_event_log(input: &str) -> anyhow::Result<ParsedEventLog> {
    let mut events = Vec::new();
    let mut frame = SseFrame::default();

    for raw_line in input.lines() {
        let line = raw_line.trim_end_matches('\r');

        if line.is_empty() {
            if !frame.is_empty()
                && let Some(event) = frame.finish()?
            {
                events.push(event);
            }
            continue;
        }

        if line.starts_with(':') {
            frame.skipped = true;
            continue;
        }

        if let Some(value) = line.strip_prefix("event:") {
            frame.event_name = Some(value.trim_start().to_string());
            continue;
        }

        if let Some(value) = line.strip_prefix("data:") {
            frame.data_lines.push(value.trim_start().to_string());
        }
    }

    if !frame.is_empty()
        && let Some(event) = frame.finish()?
    {
        events.push(event);
    }

    Ok(ParsedEventLog { events })
}

#[derive(Default)]
struct SseFrame {
    event_name: Option<String>,
    data_lines: Vec<String>,
    skipped: bool,
}

impl SseFrame {
    fn is_empty(&self) -> bool {
        self.event_name.is_none() && self.data_lines.is_empty() && !self.skipped
    }

    fn finish(&mut self) -> anyhow::Result<Option<ParsedEvent>> {
        if self.skipped {
            self.clear();
            return Ok(None);
        }

        let event_name = self
            .event_name
            .take()
            .unwrap_or_else(|| "message".to_string());
        let data = self.data_lines.join("\n");
        self.clear();

        if matches!(event_name.as_str(), "heartbeat" | "keepalive" | "ping") {
            return Ok(None);
        }

        let payload = if data.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::from_str::<serde_json::Value>(&data)
                .unwrap_or(serde_json::Value::String(data))
        };

        Ok(Some(ParsedEvent {
            name: event_name,
            payload,
        }))
    }

    fn clear(&mut self) {
        self.event_name = None;
        self.data_lines.clear();
        self.skipped = false;
    }
}

fn temp_import_source_file(case_id: &str, file_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("komga-sse-import-{case_id}-{nanos}"));
    fs::create_dir_all(&root).expect("sse import temp directory should be created");
    let source_file = root.join(file_name);
    fs::write(&source_file, b"fixture").expect("sse import source fixture should be written");
    source_file
}

fn missing_import_source_file(case_id: &str, file_name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir()
        .join(format!("komga-sse-missing-import-{case_id}-{nanos}"))
        .join(file_name)
}

async fn import_book_for_sse(
    main_db: &Path,
    source_file: &Path,
    expected_success: bool,
) -> Result<(), String> {
    let pool = komga_infrastructure::sqlite::connect_test_pool(main_db, 1)
        .await
        .map_err(|error| format!("open import db for sse test: {error}"))?;
    let port = FilesystemImportPort::new(pool.clone(), pool.clone());
    let result = port
        .import_book(
            ImportCopyMode::Copy,
            BooksImportEntry {
                source_file: source_file.to_path_buf(),
                series_id: "series-1".to_string(),
                destination_name: None,
                upgrade_book_id: None,
            },
        )
        .await;
    pool.close().await;

    match (expected_success, result) {
        (true, Ok(Some(_))) => Ok(()),
        (true, Ok(None)) => Err("import unexpectedly returned no-op".to_string()),
        (true, Err(error)) => Err(error),
        (false, Err(_)) => Ok(()),
        (false, Ok(_)) => Err("import unexpectedly succeeded".to_string()),
    }
}

#[tokio::test]
async fn router_sse_events_requires_authenticated_user() {
    let ctx = TestFixture::new("router-sse-events-auth-required").await;

    let app = ctx.app().clone();
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/sse/v1/events")
                .body(Body::empty())
                .expect("sse unauthorized request should build"),
        )
        .await
        .expect("sse unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn router_sse_events_admin_stream_emits_task_queue_status_and_heartbeat() {
    let ctx = TestFixture::new("router-sse-events-admin-task-heartbeat").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/sse/v1/events")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("sse admin request should build"),
        )
        .await
        .expect("sse admin request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/event-stream")
    );

    let body = read_sse_until(
        response.into_body(),
        |raw| raw.contains("event: TaskQueueStatus") && raw.contains("heartbeat"),
        Duration::from_secs(17),
    )
    .await;
    let parsed = parse_event_log(&body).expect("admin sse body should parse");
    assert!(
        parsed
            .events
            .iter()
            .any(|event| event.name == "TaskQueueStatus"
                && event.payload.get("countByType").is_some()),
        "admin SSE should include TaskQueueStatus event: {body}"
    );
}

#[tokio::test]
async fn router_sse_events_emit_library_changed_without_five_second_poll_delay() {
    let ctx = TestFixture::new("router-sse-events-library-change").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/sse/v1/events")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("sse library change request should build"),
        )
        .await
        .expect("sse library change request should complete");

    let update_app = app.clone();
    let update_auth_token = auth_token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let response = update_app
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/libraries/library-1")
                    .header("x-auth-token", &update_auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "name": "Updated Library 1" }).to_string(),
                    ))
                    .expect("sse library patch request should build"),
            )
            .await
            .expect("sse library patch request should complete");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    });

    let body = read_sse_until(
        response.into_body(),
        |raw| raw.contains("event: LibraryChanged"),
        Duration::from_secs(3),
    )
    .await;
    let parsed = parse_event_log(&body).expect("library change sse body should parse");
    assert!(
        parsed.events.iter().any(|event| {
            event.name == "LibraryChanged"
                && event.payload.get("libraryId") == Some(&Value::String("library-1".to_string()))
        }),
        "SSE should emit LibraryChanged promptly after library update mutation: {body}"
    );
}

#[tokio::test]
async fn router_sse_events_emit_book_import_for_successful_runtime_import() {
    let ctx = TestFixture::new("router-sse-events-book-import-success").await;
    let source_file = temp_import_source_file(
        "router-sse-events-book-import-success",
        "import-success.cbz",
    );

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/sse/v1/events")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("sse successful book import request should build"),
        )
        .await
        .expect("sse successful book import request should complete");

    let update_main_db = ctx.paths().main_db.clone();
    let import_source = source_file.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        import_book_for_sse(update_main_db.as_path(), import_source.as_path(), true)
            .await
            .expect("runtime import should succeed");
    });

    let body = read_sse_until(
        response.into_body(),
        |raw| raw.contains("event: BookImported") && raw.contains("\"success\":true"),
        Duration::from_secs(3),
    )
    .await;
    let parsed = parse_event_log(&body).expect("successful book import sse body should parse");
    assert!(
        parsed.events.iter().any(|event| {
            event.name == "BookImported"
                && event
                    .payload
                    .get("bookId")
                    .and_then(Value::as_str)
                    .is_some()
                && event.payload.get("sourceFile")
                    == Some(&Value::String(source_file.to_string_lossy().to_string()))
                && event.payload.get("success") == Some(&Value::Bool(true))
                && event.payload.get("message") == Some(&Value::Null)
        }),
        "admin SSE should emit BookImported for successful imports: {body}"
    );
}

#[tokio::test]
async fn router_sse_events_emit_book_import_failure_for_failed_runtime_import() {
    let ctx = TestFixture::new("router-sse-events-book-import-failure").await;
    let source_file = missing_import_source_file(
        "router-sse-events-book-import-failure",
        "missing-import.cbz",
    );

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/sse/v1/events")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("sse failed book import request should build"),
        )
        .await
        .expect("sse failed book import request should complete");

    let update_main_db = ctx.paths().main_db.clone();
    let import_source = source_file.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        import_book_for_sse(update_main_db.as_path(), import_source.as_path(), false)
            .await
            .expect("runtime import should fail");
    });

    let body = read_sse_until(
        response.into_body(),
        |raw| raw.contains("event: BookImported") && raw.contains("\"success\":false"),
        Duration::from_secs(3),
    )
    .await;
    let parsed = parse_event_log(&body).expect("failed book import sse body should parse");
    assert!(
        parsed.events.iter().any(|event| {
            event.name == "BookImported"
                && event.payload.get("bookId") == Some(&Value::Null)
                && event.payload.get("sourceFile")
                    == Some(&Value::String(source_file.to_string_lossy().to_string()))
                && event.payload.get("success") == Some(&Value::Bool(false))
                && event
                    .payload
                    .get("message")
                    .and_then(Value::as_str)
                    .is_some_and(|message| message.contains("source file does not exist"))
        }),
        "admin SSE should emit failed BookImported events with error details: {body}"
    );
}

#[tokio::test]
async fn router_sse_events_emit_session_expired_for_invalidated_user_sessions() {
    let member_user_id = "member-sse-password-reset";
    let member_email = "member-sse-password-reset@example.org";
    let ctx = TestFixture::new("router-sse-events-session-expired").await;
    seed_router_library_restricted_user(
        ctx.paths(),
        member_user_id,
        member_email,
        MEMBER_PASSWORD,
        &["library-1"],
    )
    .await;

    let app = ctx.app().clone();
    let admin_token = ctx.login_admin().await;
    let member_token = ctx
        .login_with_credentials(member_email, MEMBER_PASSWORD)
        .await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/sse/v1/events")
                .header("x-auth-token", &member_token)
                .body(Body::empty())
                .expect("member sse request should build"),
        )
        .await
        .expect("member sse request should complete");

    let admin_app = app.clone();
    let password_update_uri = format!("/api/v2/users/{member_user_id}/password");
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let response = admin_app
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(password_update_uri)
                    .header("x-auth-token", &admin_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "password": "updated-password-123" }).to_string(),
                    ))
                    .expect("admin password reset request should build"),
            )
            .await
            .expect("admin password reset request should complete");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    });

    let body = read_sse_until(
        response.into_body(),
        |raw| raw.contains("event: SessionExpired"),
        Duration::from_secs(3),
    )
    .await;
    let parsed = parse_event_log(&body).expect("session expired sse body should parse");
    assert!(
        parsed.events.iter().any(|event| {
            event.name == "SessionExpired"
                && event.payload.get("userId") == Some(&Value::String(member_user_id.to_string()))
        }),
        "SSE should emit SessionExpired for invalidated sessions: {body}"
    );
}

#[tokio::test]
async fn router_sse_events_rejects_new_connections_after_shutdown_with_internal_server_error() {
    let ctx = TestFixture::new("router-sse-events-shutdown-rejects-new-connections").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let shutdown_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/actuator/shutdown")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("actuator shutdown request should build"),
        )
        .await
        .expect("actuator shutdown request should complete");
    assert_eq!(shutdown_response.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/sse/v1/events")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("sse shutdown rejection request should build"),
        )
        .await
        .expect("sse shutdown rejection request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn router_sse_events_emit_session_expired_when_admin_deletes_user() {
    let member_user_id = "member-sse-user-delete";
    let member_email = "member-sse-user-delete@example.org";
    let ctx = TestFixture::new("router-sse-events-session-expired-user-delete").await;
    seed_router_library_restricted_user(
        ctx.paths(),
        member_user_id,
        member_email,
        MEMBER_PASSWORD,
        &["library-1"],
    )
    .await;

    let app = ctx.app().clone();
    let admin_token = ctx.login_admin().await;
    let member_token = ctx
        .login_with_credentials(member_email, MEMBER_PASSWORD)
        .await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/sse/v1/events")
                .header("x-auth-token", &member_token)
                .body(Body::empty())
                .expect("member delete-target sse request should build"),
        )
        .await
        .expect("member delete-target sse request should complete");

    let admin_app = app.clone();
    let user_delete_uri = format!("/api/v2/users/{member_user_id}");
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let response = admin_app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(user_delete_uri)
                    .header("x-auth-token", &admin_token)
                    .body(Body::empty())
                    .expect("admin user delete request should build"),
            )
            .await
            .expect("admin user delete request should complete");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    });

    let body = read_sse_until(
        response.into_body(),
        |raw| raw.contains("event: SessionExpired"),
        Duration::from_secs(3),
    )
    .await;
    let parsed = parse_event_log(&body).expect("delete session expired sse body should parse");
    assert!(
        parsed.events.iter().any(|event| {
            event.name == "SessionExpired"
                && event.payload.get("userId") == Some(&Value::String(member_user_id.to_string()))
        }),
        "SSE should emit SessionExpired when admin deletes a user: {body}"
    );
}

#[tokio::test]
async fn router_sse_events_do_not_emit_session_expired_when_user_changes_own_password() {
    let member_user_id = "member-sse-self-password";
    let member_email = "member-sse-self-password@example.org";
    let ctx = TestFixture::new("router-sse-events-self-password-no-session-expired").await;
    seed_router_library_restricted_user(
        ctx.paths(),
        member_user_id,
        member_email,
        MEMBER_PASSWORD,
        &["library-1"],
    )
    .await;

    let app = ctx.app().clone();
    let member_token = ctx
        .login_with_credentials(member_email, MEMBER_PASSWORD)
        .await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/sse/v1/events")
                .header("x-auth-token", &member_token)
                .body(Body::empty())
                .expect("self-password sse request should build"),
        )
        .await
        .expect("self-password sse request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let update_app = app.clone();
    let update_token = member_token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        let response = update_app
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v2/users/me/password")
                    .header("x-auth-token", &update_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "password": "router-contract-member-456" }).to_string(),
                    ))
                    .expect("self password update request should build"),
            )
            .await
            .expect("self password update request should complete");
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    });

    let body = read_sse_until(
        response.into_body(),
        |raw| raw.lines().filter(|line| *line == ": heartbeat").count() >= 2,
        Duration::from_secs(17),
    )
    .await;
    let parsed = parse_event_log(&body).expect("self-password sse body should parse");
    assert!(
        parsed
            .events
            .iter()
            .all(|event| event.name != "SessionExpired"),
        "self password change must not emit SessionExpired SSE: {body}"
    );
}
