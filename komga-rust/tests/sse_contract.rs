use std::collections::BTreeMap;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::Engine;
use komga_compat_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::config::{RuntimeCli, RuntimeConfig};
use komga_rust::persistence::sqlite::connect_pool;
use serde_json::{Value, json};
use tower::ServiceExt;

#[path = "support/persistence_contract_fixture.rs"]
mod persistence_contract_fixture;

#[test]
fn sse_contract_target_is_registered() {
    assert_required_target_declared("SSE", "sse_contract");
}

#[tokio::test]
async fn sse_requires_authentication() {
    let fixture = SseContractFixture::new("sse-auth-required").await;
    let app = fixture.app();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/sse/v1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    fixture.cleanup();
}

#[tokio::test]
async fn admin_stream_requires_non_heartbeat_domain_payload_and_exposes_persisted_task_queue_status() {
    let fixture = SseContractFixture::new("sse-admin-task-queue").await;
    let app = fixture.app();
    let admin_token = login_x_auth_token(&app, fixture.admin_email, fixture.admin_password).await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/sse/v1/events")
                .header("X-Auth-Token", &admin_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/event-stream"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    let events = parse_domain_events(&text);

    assert!(
        !events.is_empty(),
        "SSE contract rejects heartbeat-only streams when persisted task/content/auth state exists",
    );

    let task_queue_status = events
        .iter()
        .find(|event| event.name == "TaskQueueStatus")
        .expect("admin stream must expose TaskQueueStatus from persisted TASK rows");

    assert_eq!(
        task_queue_status.payload,
        json!({
            "count": 2,
            "countByType": {
                "scanLibrary": 1,
                "analyzeBook": 1,
            },
        }),
        "TaskQueueStatus payload must mirror Kotlin-visible task-type keys from persisted queued/running task rows",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn rejects_heartbeat_only_stream_when_persisted_domain_state_exists() {
    let fixture = SseContractFixture::new("sse-reject-heartbeat-only").await;
    let app = fixture.app();
    let admin_token = login_x_auth_token(&app, fixture.admin_email, fixture.admin_password).await;

    let events = sse_events(app, &admin_token, None).await;
    assert!(
        !events.is_empty(),
        "SSE contract requires at least one non-heartbeat domain event when persisted auth/task/content state exists",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn non_admin_stream_hides_task_queue_status_and_requires_non_heartbeat_content_domain_payload() {
    let fixture = SseContractFixture::new("sse-user-content-visibility").await;
    let app = fixture.app();
    let user_token = login_x_auth_token(&app, fixture.user_email, fixture.user_password).await;

    let content_transition = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("X-Auth-Token", &user_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"page":1}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        content_transition.status(),
        StatusCode::NO_CONTENT,
        "fixture sanity: persisted read-progress transition must succeed before SSE assertion",
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/sse/v1/events")
                .header("X-Auth-Token", &user_token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    let events = parse_domain_events(&text);

    assert!(
        !events.iter().any(|event| event.name == "TaskQueueStatus"),
        "non-admin SSE stream must not expose admin-only TaskQueueStatus",
    );

    let read_progress_changed = events
        .iter()
        .find(|event| event.name == "ReadProgressChanged")
        .expect("SSE stream must carry a non-heartbeat content domain payload from persisted read-progress transitions");
    assert_eq!(
        read_progress_changed.payload,
        json!({
            "bookId": "book-1",
            "userId": fixture.user_id,
        }),
    );

    fixture.cleanup();
}

#[tokio::test]
async fn session_expired_visibility_is_user_scoped_for_admin_and_non_admin_streams() {
    let fixture = SseContractFixture::new("sse-session-expired-visibility").await;
    let app = fixture.app();
    let admin_token = login_x_auth_token(&app, fixture.admin_email, fixture.admin_password).await;
    let user_token = login_x_auth_token(&app, fixture.user_email, fixture.user_password).await;

    let admin_for_admin_event = sse_events(
        app.clone(),
        &admin_token,
        Some(("x-komga-session-expired-user-id", fixture.admin_id)),
    )
    .await;
    assert!(
        admin_for_admin_event
            .iter()
            .any(|event| event.name == "SessionExpired" && event.payload == json!({ "userId": fixture.admin_id })),
        "admin SSE stream should receive SessionExpired only when the expired user id is the authenticated admin id",
    );

    let admin_for_user_event = sse_events(
        app.clone(),
        &admin_token,
        Some(("x-komga-session-expired-user-id", fixture.user_id)),
    )
    .await;
    assert!(
        !admin_for_user_event
            .iter()
            .any(|event| event.name == "SessionExpired"),
        "admin SSE stream must not receive SessionExpired events scoped to another user id",
    );

    let user_for_self_event = sse_events(
        app.clone(),
        &user_token,
        Some(("x-komga-session-expired-user-id", fixture.user_id)),
    )
    .await;
    assert!(
        user_for_self_event
            .iter()
            .any(|event| event.name == "SessionExpired" && event.payload == json!({ "userId": fixture.user_id })),
        "non-admin SSE stream should receive SessionExpired only for its own user id",
    );

    let user_for_admin_event = sse_events(
        app,
        &user_token,
        Some(("x-komga-session-expired-user-id", fixture.admin_id)),
    )
    .await;
    assert!(
        !user_for_admin_event
            .iter()
            .any(|event| event.name == "SessionExpired"),
        "non-admin SSE stream must not receive SessionExpired events scoped to admin ids",
    );

    fixture.cleanup();
}

struct SseContractFixture {
    paths: persistence_contract_fixture::LegacyDbPaths,
    config: RuntimeConfig,
    admin_id: &'static str,
    admin_email: &'static str,
    admin_password: &'static str,
    user_id: &'static str,
    user_email: &'static str,
    user_password: &'static str,
}

impl SseContractFixture {
    async fn new(case_id: &str) -> Self {
        let paths = persistence_contract_fixture::new_legacy_db_paths(case_id)
            .expect("sse contract db paths should be created");
        persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
            .await
            .expect("main db flyway fixture should be created");
        persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
            .await
            .expect("tasks db flyway fixture should be created");

        let fixture = Self {
            config: runtime_config_for_paths(&paths),
            paths,
            admin_id: "db-admin-1",
            admin_email: "db-admin@example.org",
            admin_password: "db-admin-password",
            user_id: "db-user-1",
            user_email: "db-user@example.org",
            user_password: "db-user-password",
        };

        fixture.seed_auth_rows().await;
        fixture.seed_task_rows().await;
        fixture.seed_content_rows().await;

        fixture
    }

    fn app(&self) -> axum::Router {
        komga_rust::app::build_router_with_config(&self.config)
    }

    fn cleanup(self) {
        persistence_contract_fixture::cleanup(self.paths);
    }

    async fn seed_auth_rows(&self) {
        let pool = connect_pool(&self.paths.main_db, 1)
            .await
            .expect("sqlite pool should open for sse auth fixture seeding");

        sqlx::query("INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) VALUES (?, ?, ?, ?)")
            .bind(self.admin_id)
            .bind(self.admin_email)
            .bind("$2a$10$x7NyXzncFgR/Nd/VR8eYde9njk/JaWz1X05C1wkk1G89dZnmVpw3e")
            .bind(true)
            .execute(&pool)
            .await
            .expect("sse auth fixture should seed admin user row");

        sqlx::query("INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) VALUES (?, ?, ?, ?)")
            .bind(self.user_id)
            .bind(self.user_email)
            .bind("$2a$10$6uBfM3Iovphyo.x1KDYFa.kdgG/Wth9mRYP9wQDTwYF0ShEXc6/4m")
            .bind(true)
            .execute(&pool)
            .await
            .expect("sse auth fixture should seed non-admin user row");

        for role in ["ADMIN", "FILE_DOWNLOAD", "PAGE_STREAMING", "USER"] {
            sqlx::query("INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
                .bind(self.admin_id)
                .bind(role)
                .execute(&pool)
                .await
                .expect("sse auth fixture should seed admin roles");
        }

        sqlx::query("INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
            .bind(self.user_id)
            .bind("USER")
            .execute(&pool)
            .await
            .expect("sse auth fixture should seed user role");

        pool.close().await;
    }

    async fn seed_task_rows(&self) {
        let pool = connect_pool(&self.paths.tasks_db, 1)
            .await
            .expect("sqlite pool should open for sse task fixture seeding");

        sqlx::query("INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind("SCAN_LIBRARY:sse-queued")
            .bind(30_i64)
            .bind("library-1")
            .bind("org.gotson.komga.domain.task.ScanLibrary")
            .bind("SCAN_LIBRARY")
            .bind("{\"libraryId\":\"library-1\"}")
            .bind(Option::<String>::None)
            .execute(&pool)
            .await
            .expect("sse task fixture should seed queued scan task row");

        sqlx::query("INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind("ANALYZE_BOOK:sse-running")
            .bind(10_i64)
            .bind("book-1")
            .bind("org.gotson.komga.domain.task.AnalyzeBook")
            .bind("ANALYZE_BOOK")
            .bind("{\"bookId\":\"book-1\"}")
            .bind("rust-main")
            .execute(&pool)
            .await
            .expect("sse task fixture should seed running analyze task row");

        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM TASK")
            .fetch_one(&pool)
            .await
            .expect("sse task fixture rows should be queryable");
        assert_eq!(count, 2, "fixture sanity: expected exactly two seeded TASK rows");

        pool.close().await;
    }

    async fn seed_content_rows(&self) {
        let pool = connect_pool(&self.paths.main_db, 1)
            .await
            .expect("sqlite pool should open for sse content fixture seeding");

        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES ('library-1', 'SSE Contract Library', '/library-1')")
            .execute(&pool)
            .await
            .expect("sse content fixture should seed library row");
        sqlx::query("INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES ('series-1', CURRENT_TIMESTAMP, 'SSE Series', '/series-1', 'library-1')")
            .execute(&pool)
            .await
            .expect("sse content fixture should seed series row");
        sqlx::query("INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, SERIES_ID) VALUES ('ENDED', 'SSE Series', 'SSE Series', 'series-1')")
            .execute(&pool)
            .await
            .expect("sse content fixture should seed series metadata row");
        sqlx::query("INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, LIBRARY_ID) VALUES ('book-1', CURRENT_TIMESTAMP, 'SSE Book', '/book-1', 'series-1', 'library-1')")
            .execute(&pool)
            .await
            .expect("sse content fixture should seed book row");
        sqlx::query("INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, BOOK_ID) VALUES ('1', 1, 'SSE Book', 'book-1')")
            .execute(&pool)
            .await
            .expect("sse content fixture should seed book metadata row");
        sqlx::query("INSERT INTO MEDIA (STATUS, BOOK_ID) VALUES ('READY', 'book-1')")
            .execute(&pool)
            .await
            .expect("sse content fixture should seed media row");
        sqlx::query("INSERT INTO MEDIA_FILE (FILE_NAME, BOOK_ID) VALUES ('book-1.cbz', 'book-1')")
            .execute(&pool)
            .await
            .expect("sse content fixture should seed media file row");
        sqlx::query("INSERT INTO MEDIA_PAGE (FILE_NAME, MEDIA_TYPE, NUMBER, BOOK_ID) VALUES ('1.jpg', 'image/jpeg', 1, 'book-1')")
            .execute(&pool)
            .await
            .expect("sse content fixture should seed media page row");

        let book_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM BOOK WHERE ID = 'book-1'")
            .fetch_one(&pool)
            .await
            .expect("sse content fixture rows should be queryable");
        assert_eq!(book_count, 1, "fixture sanity: expected seeded book row");

        pool.close().await;
    }
}

#[derive(Debug)]
struct ParsedSseEvent {
    name: String,
    payload: Value,
}

async fn sse_events(
    app: axum::Router,
    auth_token: &str,
    extra_header: Option<(&str, &str)>,
) -> Vec<ParsedSseEvent> {
    let mut request = Request::builder()
        .uri("/sse/v1/events")
        .header("X-Auth-Token", auth_token);
    if let Some((name, value)) = extra_header {
        request = request.header(name, value);
    }

    let response = app
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let text = String::from_utf8(body.to_vec()).unwrap();
    parse_domain_events(&text)
}

fn parse_domain_events(raw: &str) -> Vec<ParsedSseEvent> {
    raw.split("\n\n")
        .filter_map(|frame| {
            let mut name = None;
            let mut payload = None;

            for line in frame.lines() {
                if line.starts_with(':') {
                    continue;
                }
                if let Some(value) = line.strip_prefix("event: ") {
                    name = Some(value.to_string());
                    continue;
                }
                if let Some(value) = line.strip_prefix("data: ") {
                    payload = Some(
                        serde_json::from_str::<Value>(value)
                            .expect("SSE event payload should be valid JSON"),
                    );
                }
            }

            let name = name?;
            if name.eq_ignore_ascii_case("heartbeat") {
                return None;
            }

            Some(ParsedSseEvent {
                name,
                payload: payload.unwrap_or(Value::Null),
            })
        })
        .collect()
}

fn basic_auth(credentials: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(credentials)
    )
}

async fn login_x_auth_token(app: &axum::Router, email: &str, password: &str) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, basic_auth(&format!("{email}:{password}")))
                .header("X-Auth-Token", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "fixture sanity: persisted basic-auth credentials should issue X-Auth-Token",
    );

    response
        .headers()
        .get("x-auth-token")
        .expect("login response should include x-auth-token")
        .to_str()
        .expect("x-auth-token should be valid UTF-8")
        .to_string()
}

fn runtime_config_for_paths(paths: &persistence_contract_fixture::LegacyDbPaths) -> RuntimeConfig {
    let mut env = BTreeMap::new();
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        paths.config_dir.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_DATABASE_FILE".to_string(),
        paths.main_db.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_TASKS_DB_FILE".to_string(),
        paths.tasks_db.to_string_lossy().to_string(),
    );

    RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &env)
        .expect("runtime config should resolve persistence fixture paths")
}
