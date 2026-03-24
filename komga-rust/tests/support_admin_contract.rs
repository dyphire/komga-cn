use std::fs;
use std::path::Path;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use komga_compat_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::config::{CompatProfile, RuntimeConfig};
use komga_rust::persistence::sqlite::connect_pool;
use serde_json::{Value, json};
use sqlx::Row;
use tower::ServiceExt;

#[path = "compat/auth_env.rs"]
mod compat_auth_env;

#[path = "support/persistence_contract_fixture.rs"]
mod persistence_contract_fixture;

#[test]
fn support_admin_contract_target_is_registered() {
    assert_required_target_declared("support/admin", "support_admin_contract");
}

#[tokio::test]
async fn support_admin_endpoint_matrix_requires_all_promised_families() {
    let fixture = SupportAdminContractFixture::new("support-admin-matrix", true).await;

    let cases = [
        EndpointCase::unauthorized("GET", "/api/v1/announcements", None),
        EndpointCase::unauthorized("GET", "/api/v1/releases", None),
        EndpointCase::unauthorized("POST", "/api/v1/filesystem", Some(json!({ "path": "" }))),
        EndpointCase::ok("GET", "/api/v1/fonts/families", None),
        EndpointCase::unauthorized("GET", "/api/v1/history", None),
        EndpointCase::unauthorized("GET", "/api/v1/page-hashes", None),
        EndpointCase::unauthorized(
            "POST",
            "/api/v1/transient-books",
            Some(json!({ "path": "/tmp" })),
        ),
        EndpointCase::unauthorized("DELETE", "/api/v1/syncpoints/me", None),
        EndpointCase::ok("GET", "/api/v1/claim", None),
    ];

    for case in cases {
        let response = request(&fixture.app, case.method, case.path, None, case.body).await;
        assert_eq!(
            response.status(),
            case.expected_status,
            "support/admin endpoint matrix mismatch for {} {}",
            case.method,
            case.path,
        );
    }

    fixture.cleanup();
}

#[tokio::test]
async fn support_admin_new_family_routes_reject_missing_path_placeholders() {
    let fixture = SupportAdminContractFixture::new("support-admin-missing-paths", true).await;

    let cases = [
        ("GET", "/api/v1/releases", None, "releases"),
        (
            "POST",
            "/api/v1/filesystem",
            Some(json!({ "path": "" })),
            "filesystem",
        ),
        ("GET", "/api/v1/fonts/families", None, "fonts"),
        ("GET", "/api/v1/history", None, "history"),
        ("GET", "/api/v1/page-hashes", None, "page-hashes"),
        (
            "POST",
            "/api/v1/transient-books",
            Some(json!({ "path": "/tmp" })),
            "transient-books",
        ),
    ];

    for (method, path, body, family) in cases {
        let response = request(&fixture.app, method, path, None, body).await;
        assert_ne!(
            response.status(),
            StatusCode::NOT_FOUND,
            "support/admin {family} family must not be missing from router declarations; 404 here indicates missing-path placeholder behavior instead of a real handler contract",
        );
    }

    fixture.cleanup();
}

#[tokio::test]
async fn filesystem_post_lists_fixture_config_directories_for_admin_session() {
    let fixture = SupportAdminContractFixture::new("support-admin-filesystem-runtime", true).await;
    assert!(
        fixture.paths.config_dir.join("fonts").is_dir(),
        "filesystem fixture must create fonts directory under config_dir",
    );
    assert!(
        fixture.paths.config_dir.join("lucene").is_dir(),
        "filesystem fixture must create lucene directory under config_dir",
    );
    let token = admin_session_token(&fixture.app).await;

    let response = request(
        &fixture.app,
        "POST",
        "/api/v1/filesystem",
        Some(&token),
        Some(json!({
            "path": fixture.paths.config_dir.to_string_lossy().to_string(),
            "showFiles": false,
        })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;

    let directories = payload["directories"]
        .as_array()
        .expect("filesystem response must expose a directories array derived from fixture config_dir");
    assert!(
        directories
            .iter()
            .any(|entry| entry["name"] == Value::String("fonts".to_string())
                && entry["type"] == Value::String("directory".to_string())
                && entry["path"]
                    == Value::String(fixture.paths.config_dir.join("fonts").to_string_lossy().to_string())),
        "filesystem contract requires concrete directory entry for fixture fonts path",
    );
    assert!(
        directories
            .iter()
            .any(|entry| entry["name"] == Value::String("lucene".to_string())
                && entry["type"] == Value::String("directory".to_string())
                && entry["path"]
                    == Value::String(fixture.paths.config_dir.join("lucene").to_string_lossy().to_string())),
        "filesystem contract requires concrete directory entry for fixture lucene path",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn fonts_families_includes_fixture_created_family_directory() {
    let fixture = SupportAdminContractFixture::new("support-admin-fonts-runtime", true).await;
    assert!(
        fixture
            .paths
            .config_dir
            .join("fonts")
            .join("FixtureSans")
            .join("FixtureSans-Regular.ttf")
            .is_file(),
        "fonts fixture must seed a readable FixtureSans font file before requesting families",
    );

    let response = request(&fixture.app, "GET", "/api/v1/fonts/families", None, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let families = payload
        .as_array()
        .expect("fonts families response must be an array");

    assert!(
        families.contains(&Value::String("FixtureSans".to_string())),
        "fonts contract must expose fixture-created family from runtime fonts directory",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn claim_get_reflects_unclaimed_state_before_any_user_exists() {
    let fixture = SupportAdminContractFixture::new("support-admin-claim-get", false).await;

    assert_eq!(
        user_count(&fixture.paths.main_db).await,
        0,
        "claim contract fixture must start with zero persisted users",
    );

    let response = request(&fixture.app, "GET", "/api/v1/claim", None, None).await;
    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;

    assert_eq!(
        payload["isClaimed"],
        Value::Bool(false),
        "GET /api/v1/claim must derive claim state from persisted USER rows; returning true while USER is empty is placeholder success behavior",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn announcements_put_persists_read_ids_for_authenticated_admin_and_rejects_no_op_success() {
    let fixture = SupportAdminContractFixture::new("support-admin-announcements-put", true).await;
    let (token, admin_user_id, admin_email) = admin_session_identity(&fixture.app).await;

    ensure_user_row_exists(&fixture.paths.main_db, &admin_user_id, &admin_email).await;
    assert_eq!(
        announcement_read_count_for_user(&fixture.paths.main_db, &admin_user_id).await,
        0,
        "announcements fixture must start with no ANNOUNCEMENTS_READ rows for the authenticated admin user",
    );

    let response = request(
        &fixture.app,
        "PUT",
        "/api/v1/announcements",
        Some(&token),
        Some(json!(["ann-contract-1", "ann-contract-2"])),
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "PUT /api/v1/announcements must keep unchanged-client semantics by returning 204 for authenticated admin requests",
    );

    let persisted = announcement_read_ids_for_user(&fixture.paths.main_db, &admin_user_id).await;
    assert_eq!(
        persisted.len(),
        2,
        "announcements PUT contract rejects no-op placeholder success: 204 responses must persist ANNOUNCEMENTS_READ rows for each submitted announcement id",
    );
    assert!(
        persisted.contains(&"ann-contract-1".to_string())
            && persisted.contains(&"ann-contract-2".to_string()),
        "announcements PUT contract requires persisted ANNOUNCEMENTS_READ rows to match submitted IDs",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn releases_get_for_authenticated_admin_rejects_placeholder_empty_array_and_requires_release_shape() {
    let fixture = SupportAdminContractFixture::new("support-admin-releases-get", true).await;
    let token = admin_session_token(&fixture.app).await;

    let response = request(&fixture.app, "GET", "/api/v1/releases", Some(&token), None).await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "GET /api/v1/releases must keep unchanged-client semantics by returning 200 for authenticated admin requests",
    );

    let payload = response_json(response).await;
    let releases = payload
        .as_array()
        .expect("GET /api/v1/releases must return a JSON array payload");
    assert!(
        !releases.is_empty(),
        "releases GET contract rejects placeholder [] success: authenticated admin calls must return at least one release object",
    );

    let first = releases.first().expect(
        "releases GET contract requires at least one release-shaped object with Kotlin-visible keys",
    );
    for key in [
        "version",
        "releaseDate",
        "url",
        "latest",
        "preRelease",
        "description",
    ] {
        assert!(
            first.get(key).is_some(),
            "releases GET contract requires Kotlin-visible key '{key}' in each release object",
        );
    }

    assert!(
        first["version"].is_string() && !first["version"].as_str().unwrap_or_default().is_empty(),
        "releases GET contract requires non-empty string version",
    );
    assert!(
        first["url"].is_string() && !first["url"].as_str().unwrap_or_default().is_empty(),
        "releases GET contract requires non-empty string url",
    );
    assert!(
        first["latest"].is_boolean(),
        "releases GET contract requires boolean latest",
    );
    assert!(
        first["preRelease"].is_boolean(),
        "releases GET contract requires boolean preRelease",
    );
    assert!(
        first["description"].is_string() || first["description"].is_null(),
        "releases GET contract requires description to be string or null",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn claim_post_creates_first_admin_user_and_blocks_second_claim() {
    let fixture = SupportAdminContractFixture::new("support-admin-claim-post", false).await;

    assert_eq!(user_count(&fixture.paths.main_db).await, 0);

    let create_response = request_with_claim_headers(
        &fixture.app,
        "owner@example.org",
        "owner-password",
    )
    .await;
    assert_eq!(
        create_response.status(),
        StatusCode::OK,
        "POST /api/v1/claim must create the initial admin user",
    );
    let created = response_json(create_response).await;
    assert_eq!(created["email"], Value::String("owner@example.org".to_string()));
    assert_eq!(
        user_count(&fixture.paths.main_db).await,
        1,
        "claim contract requires persisted user creation, not only request/response success",
    );

    let second_response = request_with_claim_headers(
        &fixture.app,
        "second-owner@example.org",
        "second-owner-password",
    )
    .await;
    assert_eq!(
        second_response.status(),
        StatusCode::BAD_REQUEST,
        "POST /api/v1/claim must reject re-claim attempts once at least one persisted user exists",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn syncpoints_delete_by_key_id_removes_only_targeted_rows_and_rejects_no_op_success() {
    let fixture = SupportAdminContractFixture::new("support-admin-syncpoints", true).await;
    let token = admin_session_token(&fixture.app).await;
    seed_syncpoint_rows(&fixture.paths.main_db).await;

    assert_eq!(
        syncpoint_count_for_key(&fixture.paths.main_db, "api-key-target").await,
        1,
        "syncpoints fixture must seed a targeted key row",
    );
    assert_eq!(
        syncpoint_count_for_key(&fixture.paths.main_db, "api-key-keep").await,
        1,
        "syncpoints fixture must seed a retained key row",
    );

    let response = request(
        &fixture.app,
        "DELETE",
        "/api/v1/syncpoints/me?key_id=api-key-target",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::NO_CONTENT,
        "DELETE /api/v1/syncpoints/me must return 204 after deleting targeted syncpoints",
    );

    assert_eq!(
        syncpoint_count_for_key(&fixture.paths.main_db, "api-key-target").await,
        0,
        "syncpoint delete contract rejects no-op success: targeted syncpoint rows must be deleted from SYNC_POINT",
    );
    assert_eq!(
        syncpoint_count_for_key(&fixture.paths.main_db, "api-key-keep").await,
        1,
        "syncpoint delete contract must not remove rows for other api keys",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn history_get_returns_paged_events_from_persisted_historical_tables() {
    let fixture = SupportAdminContractFixture::new("support-admin-history-runtime", true).await;
    let token = admin_session_token(&fixture.app).await;

    seed_historical_events(&fixture.paths.main_db).await;
    assert_eq!(
        historical_event_count(&fixture.paths.main_db).await,
        2,
        "history contract fixture must persist two HISTORICAL_EVENT rows before hitting the API",
    );

    let response = request(
        &fixture.app,
        "GET",
        "/api/v1/history?page=0&size=20",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    let content = payload["content"].as_array().expect(
        "GET /api/v1/history must return a Spring-like paged object with content derived from HISTORICAL_EVENT rows; placeholder [] body means runtime persistence integration is missing",
    );

    assert_eq!(
        payload["totalElements"],
        Value::from(2),
        "history contract requires totalElements to match persisted HISTORICAL_EVENT row count",
    );
    assert_eq!(
        content.len(),
        2,
        "history contract requires content entries for each persisted event on the requested page",
    );
    assert_eq!(
        content[0]["id"],
        Value::String("hist-newer".to_string()),
        "history contract requires default ordering by timestamp desc",
    );
    assert_eq!(
        content[1]["id"],
        Value::String("hist-older".to_string()),
        "history contract requires deterministic ordering across persisted events",
    );
    assert_eq!(
        content[0]["type"],
        Value::String("BOOK_IMPORTED".to_string()),
        "history contract requires type to come from persisted HISTORICAL_EVENT.TYPE",
    );
    assert_eq!(
        content[0]["bookId"],
        Value::String("book-2".to_string()),
        "history contract requires bookId to come from persisted HISTORICAL_EVENT.BOOK_ID",
    );
    assert_eq!(
        content[0]["properties"]["source"],
        Value::String("scan".to_string()),
        "history contract requires properties map to include HISTORICAL_EVENT_PROPERTIES entries",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn page_hashes_get_returns_paged_known_hashes_from_persisted_rows() {
    let fixture = SupportAdminContractFixture::new("support-admin-page-hashes-runtime", true).await;
    let token = admin_session_token(&fixture.app).await;

    seed_page_hash_rows(&fixture.paths.main_db).await;
    assert_eq!(
        page_hash_count(&fixture.paths.main_db).await,
        2,
        "page-hashes contract fixture must persist two PAGE_HASH rows before hitting the API",
    );

    let response = request(
        &fixture.app,
        "GET",
        "/api/v1/page-hashes?page=0&size=20",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    let content = payload["content"].as_array().expect(
        "GET /api/v1/page-hashes must return a Spring-like paged object with content derived from PAGE_HASH rows; placeholder [] body means runtime persistence integration is missing",
    );

    assert_eq!(
        payload["totalElements"],
        Value::from(2),
        "page-hashes contract requires totalElements to match persisted PAGE_HASH row count",
    );
    assert_eq!(
        content.len(),
        2,
        "page-hashes contract requires content entries for each persisted PAGE_HASH row on requested page",
    );
    assert!(
        content.iter().any(|entry| {
            entry["hash"] == Value::String("hash-known-delete-auto".to_string())
                && entry["size"] == Value::from(4096)
                && entry["action"] == Value::String("DELETE_AUTO".to_string())
                && entry["deleteCount"] == Value::from(3)
        }),
        "page-hashes contract requires one content entry mapped from persisted hash-known-delete-auto PAGE_HASH row values",
    );
    assert!(
        content.iter().any(|entry| {
            entry["hash"] == Value::String("hash-known-ignore".to_string())
                && entry["size"] == Value::from(2048)
                && entry["action"] == Value::String("IGNORE".to_string())
                && entry["deleteCount"] == Value::from(0)
        }),
        "page-hashes contract requires one content entry mapped from persisted hash-known-ignore PAGE_HASH row values",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn page_hashes_thumbnail_get_returns_persisted_thumbnail_bytes_for_known_hash() {
    let fixture = SupportAdminContractFixture::new("support-admin-page-hashes-thumbnail-runtime", true).await;
    let token = admin_session_token(&fixture.app).await;

    seed_page_hash_rows(&fixture.paths.main_db).await;
    seed_page_hash_thumbnail_row(
        &fixture.paths.main_db,
        "hash-known-delete-auto",
        b"fixture-page-hash-thumbnail-jpeg",
    )
    .await;
    assert_eq!(
        page_hash_thumbnail_count(&fixture.paths.main_db).await,
        1,
        "page-hashes thumbnail fixture must persist one PAGE_HASH_THUMBNAIL row before hitting the API",
    );

    let response = request(
        &fixture.app,
        "GET",
        "/api/v1/page-hashes/hash-known-delete-auto/thumbnail",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "GET /api/v1/page-hashes/{{page_hash}}/thumbnail must be a concrete support/admin handler; 404 here indicates missing-path placeholder behavior",
    );

    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .unwrap_or_default();
    assert_eq!(
        content_type,
        "image/jpeg",
        "page-hashes thumbnail contract requires JPEG content type for known persisted thumbnail",
    );

    let body = response_bytes(response).await;
    assert_eq!(
        body,
        b"fixture-page-hash-thumbnail-jpeg",
        "page-hashes thumbnail contract requires body bytes sourced from persisted PAGE_HASH_THUMBNAIL row",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn transient_books_post_scans_fixture_path_and_returns_detected_book_entries() {
    let fixture = SupportAdminContractFixture::new("support-admin-transient-books-runtime", true).await;
    let token = admin_session_token(&fixture.app).await;

    let scan_root = fixture.paths.config_dir.join("transient-books-fixture").join("Series One");
    fs::create_dir_all(&scan_root)
        .expect("transient-books fixture series directory should be created under config_dir");
    let scanned_file = scan_root.join("Fixture Volume 01.cbz");
    fs::write(&scanned_file, b"fixture-transient-book-data")
        .expect("transient-books fixture file should be created before scanning");
    assert!(
        scanned_file.is_file(),
        "transient-books fixture must contain at least one readable file before scanning",
    );

    let response = request(
        &fixture.app,
        "POST",
        "/api/v1/transient-books",
        Some(&token),
        Some(json!({ "path": scan_root.to_string_lossy().to_string() })),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    let scanned_books = payload
        .as_array()
        .expect("POST /api/v1/transient-books must return an array payload");

    assert!(
        !scanned_books.is_empty(),
        "transient-books contract rejects placeholder [] success: runtime must return entries derived from scanned fixture files",
    );
    assert!(
        scanned_books
            .iter()
            .any(|entry| entry["name"] == Value::String("Fixture Volume 01".to_string())),
        "transient-books contract requires an entry tied to fixture file Fixture Volume 01.cbz",
    );

    fixture.cleanup();
}

struct SupportAdminContractFixture {
    paths: persistence_contract_fixture::LegacyDbPaths,
    app: axum::Router,
}

impl SupportAdminContractFixture {
    async fn new(case_id: &str, with_compat_auth_env: bool) -> Self {
        if with_compat_auth_env {
            compat_auth_env::ensure_compat_auth_env();
        }

        let paths = persistence_contract_fixture::new_legacy_db_paths(case_id)
            .expect("support/admin contract db paths should be created");
        persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
            .await
            .expect("main db flyway fixture should be created");
        persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
            .await
            .expect("tasks db flyway fixture should be created");

        fs::create_dir_all(paths.config_dir.join("lucene"))
            .expect("lucene directory should be created for support/admin contract fixture");
        fs::create_dir_all(paths.config_dir.join("fonts"))
            .expect("fonts directory should be created for support/admin contract fixture");
        let fixture_font_dir = paths.config_dir.join("fonts").join("FixtureSans");
        fs::create_dir_all(&fixture_font_dir)
            .expect("fixture font family directory should be created for support/admin contract fixture");
        fs::write(fixture_font_dir.join("FixtureSans-Regular.ttf"), b"fixture-font-data")
            .expect("fixture font file should be created for support/admin contract fixture");

        let mut config = RuntimeConfig::for_compat_profile(CompatProfile::SnapshotAligned);
        config.config_dir = Some(paths.config_dir.clone());
        config.log_file = paths.config_dir.join("komga.log");
        config.database_file = paths.main_db.clone();
        config.tasks_db_file = paths.tasks_db.clone();
        config.lucene_data_directory = paths.config_dir.join("lucene");
        config.fonts_data_directory = paths.config_dir.join("fonts");

        let app = komga_rust::app::build_router_with_config(&config);

        Self { paths, app }
    }

    fn cleanup(self) {
        persistence_contract_fixture::cleanup(self.paths);
    }
}

#[derive(Clone)]
struct EndpointCase {
    method: &'static str,
    path: &'static str,
    expected_status: StatusCode,
    body: Option<Value>,
}

impl EndpointCase {
    fn unauthorized(method: &'static str, path: &'static str, body: Option<Value>) -> Self {
        Self {
            method,
            path,
            expected_status: StatusCode::UNAUTHORIZED,
            body,
        }
    }

    fn ok(method: &'static str, path: &'static str, body: Option<Value>) -> Self {
        Self {
            method,
            path,
            expected_status: StatusCode::OK,
            body,
        }
    }
}

async fn request(
    app: &axum::Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
) -> axum::response::Response {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = token {
        builder = builder.header("X-Auth-Token", token);
    }

    let body = if let Some(body) = body {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
        Body::from(body.to_string())
    } else {
        Body::empty()
    };

    app.clone()
        .oneshot(builder.body(body).expect("support/admin request should build"))
        .await
        .expect("support/admin request should execute")
}

async fn request_with_claim_headers(
    app: &axum::Router,
    email: &str,
    password: &str,
) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/claim")
                .header("X-Komga-Email", email)
                .header("X-Komga-Password", password)
                .body(Body::empty())
                .expect("claim request should build"),
        )
        .await
        .expect("claim request should execute")
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&body).expect("response body should contain valid JSON")
}

async fn response_bytes(response: axum::response::Response) -> Vec<u8> {
    axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable")
        .to_vec()
}

async fn admin_session_token(app: &axum::Router) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(
                    header::AUTHORIZATION,
                    format!(
                        "Basic {}",
                        compat_auth_env::COMPAT_ADMIN_BASIC_AUTH_BASE64,
                    ),
                )
                .header("X-Auth-Token", "")
                .body(Body::empty())
                .expect("users/me request should build"),
        )
        .await
        .expect("users/me request should execute");

    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get("X-Auth-Token")
        .expect("users/me response should provide X-Auth-Token")
        .to_str()
        .expect("session token should be valid utf-8")
        .to_string()
}

async fn admin_session_identity(app: &axum::Router) -> (String, String, String) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(
                    header::AUTHORIZATION,
                    format!(
                        "Basic {}",
                        compat_auth_env::COMPAT_ADMIN_BASIC_AUTH_BASE64,
                    ),
                )
                .header("X-Auth-Token", "")
                .body(Body::empty())
                .expect("users/me request should build"),
        )
        .await
        .expect("users/me request should execute");

    assert_eq!(response.status(), StatusCode::OK);
    let token = response
        .headers()
        .get("X-Auth-Token")
        .expect("users/me response should provide X-Auth-Token")
        .to_str()
        .expect("session token should be valid utf-8")
        .to_string();

    let payload = response_json(response).await;
    let user_id = payload["id"]
        .as_str()
        .expect("users/me payload should include persisted-compatible id")
        .to_string();
    let email = payload["email"]
        .as_str()
        .expect("users/me payload should include persisted-compatible email")
        .to_string();

    (token, user_id, email)
}

async fn user_count(main_db: &Path) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for support/admin user count inspection");
    let count = sqlx::query("SELECT COUNT(*) AS COUNT FROM USER")
        .fetch_one(&pool)
        .await
        .expect("user count should be queryable")
        .get::<i64, _>("COUNT");
    pool.close().await;
    count
}

async fn ensure_user_row_exists(main_db: &Path, user_id: &str, email: &str) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for support/admin user fixture seeding");

    sqlx::query(
        "INSERT OR IGNORE INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(email)
    .bind("$2a$10$x7NyXzncFgR/Nd/VR8eYde9njk/JaWz1X05C1wkk1G89dZnmVpw3e")
    .bind(true)
    .bind(None::<i64>)
    .bind(None::<bool>)
    .execute(&pool)
    .await
    .expect("support/admin user fixture row should insert or already exist");

    pool.close().await;
}

async fn announcement_read_count_for_user(main_db: &Path, user_id: &str) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for announcements read count inspection");
    let count = sqlx::query("SELECT COUNT(*) AS COUNT FROM ANNOUNCEMENTS_READ WHERE USER_ID = ?")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("announcements read count should be queryable")
        .get::<i64, _>("COUNT");
    pool.close().await;
    count
}

async fn announcement_read_ids_for_user(main_db: &Path, user_id: &str) -> Vec<String> {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for announcements read id inspection");
    let rows = sqlx::query(
        "SELECT ANNOUNCEMENT_ID FROM ANNOUNCEMENTS_READ WHERE USER_ID = ? ORDER BY ANNOUNCEMENT_ID ASC",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .expect("announcements read ids should be queryable");
    pool.close().await;

    rows.into_iter()
        .map(|row| row.get::<String, _>("ANNOUNCEMENT_ID"))
        .collect()
}

async fn seed_syncpoint_rows(main_db: &Path) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for syncpoint fixture seeding");

    sqlx::query(
        "INSERT OR IGNORE INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("admin-1")
    .bind("admin@example.org")
    .bind("$2a$10$x7NyXzncFgR/Nd/VR8eYde9njk/JaWz1X05C1wkk1G89dZnmVpw3e")
    .bind(true)
    .bind(None::<i64>)
    .bind(None::<bool>)
    .execute(&pool)
    .await
    .expect("support/admin syncpoint fixture admin user row should insert");

    for (id, key_id) in [
        ("syncpoint-target", Some("api-key-target")),
        ("syncpoint-keep", Some("api-key-keep")),
    ] {
        sqlx::query("INSERT INTO SYNC_POINT (ID, USER_ID, API_KEY_ID) VALUES (?, ?, ?)")
            .bind(id)
            .bind("admin-1")
            .bind(key_id)
            .execute(&pool)
            .await
            .expect("support/admin syncpoint fixture row should insert");
    }

    pool.close().await;
}

async fn syncpoint_count_for_key(main_db: &Path, key_id: &str) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for syncpoint count inspection");
    let count = sqlx::query("SELECT COUNT(*) AS COUNT FROM SYNC_POINT WHERE API_KEY_ID = ?")
        .bind(key_id)
        .fetch_one(&pool)
        .await
        .expect("syncpoint count should be queryable")
        .get::<i64, _>("COUNT");
    pool.close().await;
    count
}

async fn seed_historical_events(main_db: &Path) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for historical event fixture seeding");

    for (id, event_type, book_id, series_id, timestamp) in [
        (
            "hist-older",
            "BOOK_ADDED",
            Some("book-1"),
            Some("series-1"),
            "2024-03-01 08:00:00",
        ),
        (
            "hist-newer",
            "BOOK_IMPORTED",
            Some("book-2"),
            Some("series-2"),
            "2024-03-02 08:00:00",
        ),
    ] {
        sqlx::query(
            "INSERT INTO HISTORICAL_EVENT (ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(event_type)
        .bind(book_id)
        .bind(series_id)
        .bind(timestamp)
        .execute(&pool)
        .await
        .expect("support/admin history fixture event row should insert");
    }

    for (id, key, value) in [
        ("hist-older", "source", "manual"),
        ("hist-newer", "source", "scan"),
    ] {
        sqlx::query(
            "INSERT INTO HISTORICAL_EVENT_PROPERTIES (ID, \"KEY\", VALUE) VALUES (?, ?, ?)",
        )
        .bind(id)
        .bind(key)
        .bind(value)
        .execute(&pool)
        .await
        .expect("support/admin history fixture properties row should insert");
    }

    pool.close().await;
}

async fn historical_event_count(main_db: &Path) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for historical event count inspection");
    let count = sqlx::query("SELECT COUNT(*) AS COUNT FROM HISTORICAL_EVENT")
        .fetch_one(&pool)
        .await
        .expect("historical event count should be queryable")
        .get::<i64, _>("COUNT");
    pool.close().await;
    count
}

async fn seed_page_hash_rows(main_db: &Path) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for page-hash fixture seeding");

    for (hash, size, action, delete_count) in [
        ("hash-known-delete-auto", 4096_i64, "DELETE_AUTO", 3_i64),
        ("hash-known-ignore", 2048_i64, "IGNORE", 0_i64),
    ] {
        sqlx::query(
            "INSERT INTO PAGE_HASH (HASH, SIZE, ACTION, DELETE_COUNT) VALUES (?, ?, ?, ?)",
        )
        .bind(hash)
        .bind(size)
        .bind(action)
        .bind(delete_count)
        .execute(&pool)
        .await
        .expect("support/admin page-hash fixture row should insert");
    }

    pool.close().await;
}

async fn page_hash_count(main_db: &Path) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for page-hash count inspection");
    let count = sqlx::query("SELECT COUNT(*) AS COUNT FROM PAGE_HASH")
        .fetch_one(&pool)
        .await
        .expect("page-hash count should be queryable")
        .get::<i64, _>("COUNT");
    pool.close().await;
    count
}

async fn seed_page_hash_thumbnail_row(main_db: &Path, hash: &str, thumbnail: &[u8]) {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for page-hash thumbnail fixture seeding");

    sqlx::query("INSERT INTO PAGE_HASH_THUMBNAIL (HASH, THUMBNAIL) VALUES (?, ?)")
        .bind(hash)
        .bind(thumbnail)
        .execute(&pool)
        .await
        .expect("support/admin page-hash thumbnail fixture row should insert");

    pool.close().await;
}

async fn page_hash_thumbnail_count(main_db: &Path) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for page-hash thumbnail count inspection");
    let count = sqlx::query("SELECT COUNT(*) AS COUNT FROM PAGE_HASH_THUMBNAIL")
        .fetch_one(&pool)
        .await
        .expect("page-hash thumbnail count should be queryable")
        .get::<i64, _>("COUNT");
    pool.close().await;
    count
}
