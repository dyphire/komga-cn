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
fn device_sync_contract_target_is_registered() {
    assert_required_target_declared("device sync", "device_sync_contract");
}

#[tokio::test]
async fn kobo_ping_requires_auth_but_accepts_authenticated_session_without_path_api_key() {
    let fixture = DeviceSyncContractFixture::new("device-sync-kobo-ping").await;

    let unauthorized = request(
        &fixture.app,
        "GET",
        "/kobo/not-the-api-key/ping",
        None,
        &[],
        None,
    )
    .await;
    assert_eq!(
        unauthorized.status(),
        StatusCode::UNAUTHORIZED,
        "Kobo ping must fail closed for unauthenticated invalid path token requests",
    );

    let session_token = admin_session_token(&fixture.app).await;
    let authorized = request(
        &fixture.app,
        "GET",
        "/kobo/not-the-api-key/ping",
        Some(&session_token),
        &[],
        None,
    )
    .await;
    assert_eq!(authorized.status(), StatusCode::OK);
    assert_eq!(
        response_body_string(authorized).await,
        "pong",
        "Kobo ping contract requires authenticated sessions to authorize device ping even when path token is not API key",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn kobo_initialization_advertises_requested_token_scoped_resource_links() {
    let fixture = DeviceSyncContractFixture::new("device-sync-kobo-init").await;

    let response = request(
        &fixture.app,
        "GET",
        "/kobo/compat-api-key/v1/initialization",
        None,
        &[],
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("x-kobo-apitoken")
            .expect("kobo initialization must include x-kobo-apitoken header"),
        "e30=",
    );

    let payload = response_json(response).await;
    assert_eq!(
        payload["Resources"]["device_auth"],
        Value::String("/kobo/compat-api-key/v1/auth/device".to_string()),
    );
    assert_eq!(
        payload["Resources"]["library_sync"],
        Value::String("/kobo/compat-api-key/v1/library/sync".to_string()),
    );

    fixture.cleanup();
}

#[tokio::test]
async fn kobo_auth_device_rejects_fixed_placeholder_token_triplet() {
    let fixture = DeviceSyncContractFixture::new("device-sync-kobo-auth-device").await;

    let first = request(
        &fixture.app,
        "POST",
        "/kobo/compat-api-key/v1/auth/device",
        None,
        &[(header::CONTENT_TYPE.as_str(), "application/json")],
        Some(json!({ "UserKey": "kobo-user-key-1" })),
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    let first_payload = response_json(first).await;

    let second = request(
        &fixture.app,
        "POST",
        "/kobo/compat-api-key/v1/auth/device",
        None,
        &[(header::CONTENT_TYPE.as_str(), "application/json")],
        Some(json!({ "UserKey": "kobo-user-key-2" })),
    )
    .await;
    assert_eq!(second.status(), StatusCode::OK);
    let second_payload = response_json(second).await;

    assert_eq!(first_payload["TokenType"], Value::String("Bearer".to_string()));
    assert_eq!(first_payload["UserKey"], Value::String("kobo-user-key-1".to_string()));
    assert_eq!(second_payload["UserKey"], Value::String("kobo-user-key-2".to_string()));

    assert_ne!(
        first_payload["AccessToken"],
        second_payload["AccessToken"],
        "Kobo device auth contract rejects fixed AccessToken placeholders reused across different user keys",
    );
    assert_ne!(
        first_payload["RefreshToken"],
        second_payload["RefreshToken"],
        "Kobo device auth contract rejects fixed RefreshToken placeholders reused across different user keys",
    );
    assert_ne!(
        first_payload["TrackingId"],
        second_payload["TrackingId"],
        "Kobo device auth contract rejects fixed TrackingId placeholders reused across different user keys",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn kobo_library_sync_for_persisted_book_rejects_empty_canned_metadata_payload() {
    let fixture = DeviceSyncContractFixture::new("device-sync-kobo-library-sync").await;
    seed_persisted_book_for_device_sync(
        &fixture.paths.main_db,
        &fixture.library_root,
        "library-device",
        "series-device",
        "book-device-1",
        "Persisted Device Sync Book",
        "device-sync-book.cbz",
    )
    .await;
    assert_eq!(
        persisted_book_count(&fixture.paths.main_db, "book-device-1").await,
        1,
        "device sync fixture must persist one book row before sync assertions",
    );

    let session_token = admin_session_token(&fixture.app).await;
    let response = request(
        &fixture.app,
        "GET",
        "/kobo/not-the-api-key/v1/library/sync",
        Some(&session_token),
        &[("x-kobo-userkey", "kobo-user-key-sync")],
        None,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;

    let has_payload = payload["NewBookMetadata"]
        .as_array()
        .is_some_and(|items| !items.is_empty())
        || payload["NewEntitlement"]
            .as_array()
            .is_some_and(|items| !items.is_empty());
    assert!(
        has_payload,
        "Kobo library sync contract rejects empty canned payloads: persisted books must surface as metadata/entitlement deltas",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn koreader_auth_and_progress_round_trip_uses_authenticated_session_without_api_key_header() {
    let fixture = DeviceSyncContractFixture::new("device-sync-koreader-progress").await;
    let session_token = admin_session_token(&fixture.app).await;

    let unauthorized_auth = request(
        &fixture.app,
        "GET",
        "/koreader/users/auth",
        None,
        &[],
        None,
    )
    .await;
    assert_eq!(unauthorized_auth.status(), StatusCode::UNAUTHORIZED);

    let authorized_auth = request(
        &fixture.app,
        "GET",
        "/koreader/users/auth",
        Some(&session_token),
        &[],
        None,
    )
    .await;
    assert_eq!(authorized_auth.status(), StatusCode::OK);
    assert_eq!(
        authorized_auth
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("KOReader auth must include content-type"),
        "application/vnd.koreader.v1+json",
    );

    let missing_before_put = request(
        &fixture.app,
        "GET",
        "/koreader/syncs/progress/book-device-hash",
        Some(&session_token),
        &[],
        None,
    )
    .await;
    assert_eq!(
        missing_before_put.status(),
        StatusCode::NOT_FOUND,
        "KOReader progress must return 404 before any persisted in-memory sync payload exists",
    );

    let put_progress = request(
        &fixture.app,
        "PUT",
        "/koreader/syncs/progress",
        Some(&session_token),
        &[(header::CONTENT_TYPE.as_str(), "application/json")],
        Some(json!({
            "document": "book-device-hash",
            "percentage": 0.61,
            "progress": "/body/DocFragment[8].0",
            "device": "KOReader",
            "device_id": "dev-koreader-1",
        })),
    )
    .await;
    assert_eq!(put_progress.status(), StatusCode::NO_CONTENT);

    let get_progress = request(
        &fixture.app,
        "GET",
        "/koreader/syncs/progress/book-device-hash",
        Some(&session_token),
        &[],
        None,
    )
    .await;
    assert_eq!(get_progress.status(), StatusCode::OK);
    assert_eq!(
        get_progress
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("KOReader progress GET must include content-type"),
        "application/vnd.koreader.v1+json",
    );
    let payload = response_json(get_progress).await;
    assert_eq!(payload["document"], Value::String("book-device-hash".to_string()));
    assert_eq!(payload["percentage"], Value::from(0.61_f64));
    assert_eq!(
        payload["progress"],
        Value::String("/body/DocFragment[8].0".to_string())
    );

    fixture.cleanup();
}

struct DeviceSyncContractFixture {
    paths: persistence_contract_fixture::LegacyDbPaths,
    app: axum::Router,
    library_root: std::path::PathBuf,
}

impl DeviceSyncContractFixture {
    async fn new(case_id: &str) -> Self {
        compat_auth_env::ensure_compat_auth_env();

        let paths = persistence_contract_fixture::new_legacy_db_paths(case_id)
            .expect("device sync contract db paths should be created");
        persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
            .await
            .expect("main db flyway fixture should be created");
        persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
            .await
            .expect("tasks db flyway fixture should be created");

        fs::create_dir_all(paths.config_dir.join("lucene"))
            .expect("lucene directory should be created for device sync contract fixture");
        fs::create_dir_all(paths.config_dir.join("fonts"))
            .expect("fonts directory should be created for device sync contract fixture");
        let library_root = paths.config_dir.join("device-sync-library-root");
        fs::create_dir_all(&library_root)
            .expect("library root directory should be created for device sync contract fixture");

        let mut config = RuntimeConfig::for_compat_profile(CompatProfile::SnapshotAligned);
        config.config_dir = Some(paths.config_dir.clone());
        config.log_file = paths.config_dir.join("komga.log");
        config.database_file = paths.main_db.clone();
        config.tasks_db_file = paths.tasks_db.clone();
        config.lucene_data_directory = paths.config_dir.join("lucene");
        config.fonts_data_directory = paths.config_dir.join("fonts");

        let app = komga_rust::app::build_router_with_config(&config);

        Self {
            paths,
            app,
            library_root,
        }
    }

    fn cleanup(self) {
        persistence_contract_fixture::cleanup(self.paths);
    }
}

async fn request(
    app: &axum::Router,
    method: &str,
    path: &str,
    token: Option<&str>,
    extra_headers: &[(&str, &str)],
    json_body: Option<Value>,
) -> axum::response::Response {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = token {
        builder = builder.header("X-Auth-Token", token);
    }
    for (key, value) in extra_headers {
        builder = builder.header(*key, *value);
    }

    let body = match json_body {
        Some(payload) => Body::from(
            serde_json::to_vec(&payload).expect("json request payload must serialize in tests"),
        ),
        None => Body::empty(),
    };

    app.clone()
        .oneshot(builder.body(body).expect("device sync request should build"))
        .await
        .expect("device sync request should execute")
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&body).expect("response body should contain valid JSON")
}

async fn response_body_string(response: axum::response::Response) -> String {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    String::from_utf8(body.to_vec()).expect("response body should be UTF-8")
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

async fn seed_persisted_book_for_device_sync(
    main_db: &Path,
    library_root: &Path,
    library_id: &str,
    series_id: &str,
    book_id: &str,
    title: &str,
    file_name: &str,
) {
    let media_file_path = library_root.join(file_name);
    fs::write(&media_file_path, b"device-sync-persisted-media-payload")
        .expect("device sync fixture media file should be written");

    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for device sync fixture seeding");

    sqlx::query(
        "INSERT INTO LIBRARY (ID, NAME, ROOT, SCAN_STARTUP, EMPTY_TRASH_AFTER_SCAN, ONESHOTS_DIRECTORY) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(library_id)
    .bind("Device Sync Contract Library")
    .bind(library_root.to_string_lossy().to_string())
    .bind(false)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("device sync fixture library row should insert");

    sqlx::query(
        "INSERT INTO SERIES (ID, CREATED_DATE, LAST_MODIFIED_DATE, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, ONESHOT, DELETED_DATE) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(series_id)
    .bind("2024-03-01T00:00:00")
    .bind("2024-03-03T00:00:00")
    .bind("2024-03-03T00:00:00")
    .bind("Device Sync Series")
    .bind(format!("/library/{library_id}/series/{series_id}"))
    .bind(library_id)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("device sync fixture series row should insert");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (CREATED_DATE, LAST_MODIFIED_DATE, STATUS, TITLE, TITLE_SORT, SUMMARY, LANGUAGE, PUBLISHER, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("2024-03-01T00:00:00")
    .bind("2024-03-03T00:00:00")
    .bind("ONGOING")
    .bind("Device Sync Series")
    .bind("Device Sync Series")
    .bind("device sync series summary")
    .bind("en")
    .bind("Komga Press")
    .bind(series_id)
    .execute(&pool)
    .await
    .expect("device sync fixture series metadata row should insert");

    sqlx::query(
        "INSERT INTO BOOK (ID, CREATED_DATE, LAST_MODIFIED_DATE, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, ONESHOT, DELETED_DATE) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind("2024-03-01T00:00:00")
    .bind("2024-03-03T00:00:00")
    .bind("2024-03-03T00:00:00")
    .bind(file_name)
    .bind(format!("/library/{library_id}/books/{file_name}"))
    .bind(series_id)
    .bind(22_i64)
    .bind(1_i32)
    .bind(library_id)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("device sync fixture book row should insert");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (CREATED_DATE, LAST_MODIFIED_DATE, NUMBER, NUMBER_SORT, TITLE, SUMMARY, BOOK_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("2024-03-01T00:00:00")
    .bind("2024-03-03T00:00:00")
    .bind("1")
    .bind(1.0_f64)
    .bind(title)
    .bind("device sync contract summary")
    .bind(book_id)
    .execute(&pool)
    .await
    .expect("device sync fixture book metadata row should insert");

    sqlx::query("INSERT INTO MEDIA (STATUS, MEDIA_TYPE, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("READY")
        .bind("application/vnd.comicbook+zip")
        .bind(book_id)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("device sync fixture media row should insert");

    pool.close().await;
}

async fn persisted_book_count(main_db: &Path, book_id: &str) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for device sync fixture count inspection");
    let count = sqlx::query("SELECT COUNT(*) AS COUNT FROM BOOK WHERE ID = ?")
        .bind(book_id)
        .fetch_one(&pool)
        .await
        .expect("device sync fixture book count should be queryable")
        .get::<i64, _>("COUNT");
    pool.close().await;
    count
}
