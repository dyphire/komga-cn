use super::*;
use http_body_util::BodyExt;
use komga_application::media_assets::{
    BookImportService, BooksImportEntry, BooksImportPayload, ImportCopyMode,
};
use komga_application::runtime_sse::RuntimeSseEventSink;
use komga_infrastructure::FilesystemBookImport;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

const MEMBER_PASSWORD: &str = "router-contract-member-123";

pub(super) async fn read_sse_until(
    body: axum::body::Body,
    predicate: impl Fn(&str) -> bool,
    timeout: Duration,
) -> String {
    let mut body = body;
    let mut buffer = String::new();
    read_sse_until_buffered(&mut body, &mut buffer, predicate, timeout).await;
    buffer
}

pub(super) async fn read_sse_until_buffered(
    body: &mut axum::body::Body,
    buffer: &mut String,
    predicate: impl Fn(&str) -> bool,
    timeout: Duration,
) {
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        if predicate(buffer.as_str()) {
            return;
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

pub(super) async fn read_initial_sse_heartbeat(body: &mut axum::body::Body) -> String {
    let mut buffer = String::new();
    read_sse_until_buffered(
        body,
        &mut buffer,
        |raw| raw.contains(": heartbeat"),
        Duration::from_secs(1),
    )
    .await;
    buffer
}

pub(super) async fn read_sse_until_after_clock_advance(
    body: axum::body::Body,
    predicate: impl Fn(&str) -> bool + Send + 'static,
    timeout: Duration,
    advance: Duration,
) -> String {
    tokio::time::pause();
    let reader = tokio::spawn(read_sse_until(body, predicate, timeout));
    tokio::task::yield_now().await;
    tokio::time::advance(advance).await;
    reader.await.expect("sse reader should complete")
}

pub(super) async fn read_sse_until_after_clock_advance_buffered(
    body: &mut axum::body::Body,
    buffer: &mut String,
    predicate: impl Fn(&str) -> bool,
    timeout: Duration,
    advance: Duration,
) {
    tokio::time::pause();
    let reader = read_sse_until_buffered(body, buffer, predicate, timeout);
    tokio::pin!(reader);
    tokio::select! {
        () = &mut reader => {}
        () = async {
            tokio::task::yield_now().await;
            tokio::time::advance(advance).await;
        } => {
            reader.await;
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
    runtime_events: Arc<dyn RuntimeSseEventSink>,
    source_file: &Path,
    expected_success: bool,
) -> Result<(), String> {
    let pool = crate::support::sqlite::connect_test_pool(main_db, 1)
        .await
        .map_err(|error| format!("open import db for sse test: {error}"))?;
    let service = BookImportService::new(
        Arc::new(FilesystemBookImport::new(pool.clone(), pool.clone())),
        runtime_events,
    );
    let result = service
        .process_books_payload(
            BooksImportPayload {
                copy_mode: ImportCopyMode::Copy,
                books: vec![BooksImportEntry {
                    source_file: source_file.to_path_buf(),
                    series_id: "series-1".to_string(),
                    destination_name: None,
                    upgrade_book_id: None,
                }],
            },
            100,
        )
        .await;
    pool.close().await;

    match (expected_success, result) {
        (true, Ok(_)) => Ok(()),
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

    let body = read_sse_until_after_clock_advance(
        response.into_body(),
        |raw| raw.contains("event: TaskQueueStatus") && raw.contains("heartbeat"),
        Duration::from_secs(17),
        Duration::from_secs(11),
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

    let mut body = response.into_body();
    let mut body_buffer = read_initial_sse_heartbeat(&mut body).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "name": "Updated Library 1" }).to_string(),
                ))
                .expect("sse library patch request should build"),
        )
        .await
        .expect("sse library patch request should complete");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    read_sse_until_buffered(
        &mut body,
        &mut body_buffer,
        |raw| raw.contains("event: LibraryChanged"),
        Duration::from_secs(3),
    )
    .await;
    let body = body_buffer;
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

    let mut body = response.into_body();
    let mut body_buffer = read_initial_sse_heartbeat(&mut body).await;
    import_book_for_sse(
        ctx.paths().main_db.as_path(),
        ctx.runtime_events_arc(),
        source_file.as_path(),
        true,
    )
    .await
    .expect("runtime import should succeed");

    read_sse_until_buffered(
        &mut body,
        &mut body_buffer,
        |raw| raw.contains("event: BookImported") && raw.contains("\"success\":true"),
        Duration::from_secs(3),
    )
    .await;
    let body = body_buffer;
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

    let mut body = response.into_body();
    let mut body_buffer = read_initial_sse_heartbeat(&mut body).await;
    import_book_for_sse(
        ctx.paths().main_db.as_path(),
        ctx.runtime_events_arc(),
        source_file.as_path(),
        false,
    )
    .await
    .expect("runtime import should fail");

    read_sse_until_buffered(
        &mut body,
        &mut body_buffer,
        |raw| raw.contains("event: BookImported") && raw.contains("\"success\":false"),
        Duration::from_secs(3),
    )
    .await;
    let body = body_buffer;
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
async fn router_sse_events_keep_collection_thumbnail_wire_names() {
    let ctx = TestFixture::new("router-sse-events-collection-thumbnail-wire-name").await;

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
                .expect("sse collection thumbnail request should build"),
        )
        .await
        .expect("sse collection thumbnail request should complete");

    let mut body = response.into_body();
    let mut body_buffer = read_initial_sse_heartbeat(&mut body).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, upload_body) =
        multipart_image_upload_body("file", "collection.png", "image/png", true, &image_bytes);
    let upload = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections/collection-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(upload_body))
                .expect("collection thumbnail SSE upload request should build"),
        )
        .await
        .expect("collection thumbnail SSE upload request should complete");
    assert_eq!(upload.status(), StatusCode::OK);

    read_sse_until_buffered(
        &mut body,
        &mut body_buffer,
        |raw| raw.contains("event: ThumbnailSeriesCollectionAdded"),
        Duration::from_secs(3),
    )
    .await;
    let body = body_buffer;
    let parsed =
        parse_event_log(&body).expect("collection thumbnail wire-name SSE body should parse");
    assert!(
        parsed.events.iter().any(|event| {
            event.name == "ThumbnailSeriesCollectionAdded"
                && event.payload.get("collectionId")
                    == Some(&Value::String("collection-1".to_string()))
                && event.payload.get("selected") == Some(&Value::Bool(true))
        }),
        "SSE should preserve collection thumbnail wire event names expected by the WebUI: {body}"
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

    let password_update_uri = format!("/api/v2/users/{member_user_id}/password");
    let mut body = response.into_body();
    let mut body_buffer = read_initial_sse_heartbeat(&mut body).await;
    let response = app
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

    read_sse_until_buffered(
        &mut body,
        &mut body_buffer,
        |raw| raw.contains("event: SessionExpired"),
        Duration::from_secs(3),
    )
    .await;
    let body = body_buffer;
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

    let user_delete_uri = format!("/api/v2/users/{member_user_id}");
    let mut body = response.into_body();
    let mut body_buffer = read_initial_sse_heartbeat(&mut body).await;
    let response = app
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

    read_sse_until_buffered(
        &mut body,
        &mut body_buffer,
        |raw| raw.contains("event: SessionExpired"),
        Duration::from_secs(3),
    )
    .await;
    let body = body_buffer;
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

    let mut body = response.into_body();
    let mut body_buffer = read_initial_sse_heartbeat(&mut body).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v2/users/me/password")
                .header("x-auth-token", &member_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "password": "router-contract-member-456" }).to_string(),
                ))
                .expect("self password update request should build"),
        )
        .await
        .expect("self password update request should complete");
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    read_sse_until_after_clock_advance_buffered(
        &mut body,
        &mut body_buffer,
        |raw| raw.lines().filter(|line| *line == ": heartbeat").count() >= 2,
        Duration::from_secs(17),
        Duration::from_secs(16),
    )
    .await;
    let body = body_buffer;
    let parsed = parse_event_log(&body).expect("self-password sse body should parse");
    assert!(
        parsed
            .events
            .iter()
            .all(|event| event.name != "SessionExpired"),
        "self password change must not emit SessionExpired SSE: {body}"
    );
}
