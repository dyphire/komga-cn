use std::fs;
use std::path::Path;

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use komga_compat_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::config::{CompatProfile, RuntimeConfig};
use komga_rust::persistence::sqlite::connect_pool;
use serde_json::Value;
use sqlx::Row;
use tower::ServiceExt;

#[path = "compat/auth_env.rs"]
mod compat_auth_env;

#[path = "support/persistence_contract_fixture.rs"]
mod persistence_contract_fixture;

#[test]
fn opds_contract_target_is_registered() {
    assert_required_target_declared("OPDS", "opds_contract");
}

#[tokio::test]
async fn opds_v2_auth_returns_opds_authentication_document() {
    let fixture = OpdsContractFixture::new("opds-auth").await;

    let response = request(
        &fixture.app,
        "GET",
        "/opds/v2/auth",
        None,
        &[(header::HOST.as_str(), "opds-contract.local")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("OPDS auth response must include content-type"),
        "application/opds-authentication+json",
    );

    let payload = response_json(response).await;
    assert_eq!(
        payload["authentication"][0]["type"],
        Value::String("http://opds-spec.org/auth/basic".to_string()),
    );
    assert_eq!(
        payload["id"],
        Value::String("http://opds-contract.local/opds/v2/auth".to_string()),
    );
    assert_eq!(
        payload["authentication"][0]["labels"]["login"],
        Value::String("Email".to_string()),
        "OPDS auth contract requires Kotlin-visible basic-auth login label",
    );
    assert_eq!(
        payload["authentication"][0]["labels"]["password"],
        Value::String("Password".to_string()),
        "OPDS auth contract requires Kotlin-visible basic-auth password label",
    );
    let links = payload["links"]
        .as_array()
        .expect("OPDS auth payload must expose links array");
    assert!(
        links.iter().any(|link| {
            link["rel"] == Value::String("help".to_string())
                && link["href"] == Value::String("https://komga.org".to_string())
        }),
        "OPDS auth contract requires help link to komga.org",
    );
    assert!(
        links.iter().any(|link| {
            link["rel"] == Value::String("logo".to_string())
                && link["href"]
                    == Value::String(
                        "http://opds-contract.local/android-chrome-512x512.png".to_string(),
                    )
        }),
        "OPDS auth contract requires host-resolved logo link",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn opds_v2_catalog_returns_auth_challenge_headers_and_auth_document() {
    let fixture = OpdsContractFixture::new("opds-catalog").await;

    let response = request(
        &fixture.app,
        "GET",
        "/opds/v2/catalog",
        None,
        &[(header::HOST.as_str(), "opds-contract.local")],
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .expect("OPDS catalog must include WWW-Authenticate"),
        "Basic realm=\"Realm\"",
    );
    assert_eq!(
        response
            .headers()
            .get(header::LINK)
            .expect("OPDS catalog must include Link auth document")
            .to_str()
            .expect("OPDS catalog link header must be UTF-8"),
        "<http://opds-contract.local/opds/v2/auth>; rel=\"http://opds-spec.org/auth/document\"; type=\"application/opds-authentication+json\"",
    );
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("OPDS catalog must include content-type"),
        "application/opds-authentication+json;charset=UTF-8",
    );

    let payload = response_json(response).await;
    assert_eq!(
        payload["authentication"][0]["type"],
        Value::String("http://opds-spec.org/auth/basic".to_string()),
    );
    assert_eq!(
        payload["id"],
        Value::String("http://opds-contract.local/opds/v2/auth".to_string()),
        "OPDS catalog challenge body must point clients to OPDS auth document id",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn opds_v1_series_requires_auth_then_returns_atom_feed_for_admin_session() {
    let fixture = OpdsContractFixture::new("opds-v1-series").await;

    let unauthorized = request(&fixture.app, "GET", "/opds/v1.2/series", None, &[]).await;
    assert_eq!(
        unauthorized.status(),
        StatusCode::UNAUTHORIZED,
        "OPDS v1 series route must require authenticated session",
    );

    let token = admin_session_token(&fixture.app).await;
    let authorized = request(
        &fixture.app,
        "GET",
        "/opds/v1.2/series",
        Some(&token),
        &[(header::HOST.as_str(), "opds-contract.local")],
    )
    .await;
    assert_eq!(authorized.status(), StatusCode::OK);
    assert_eq!(
        authorized
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("OPDS v1 series must include content-type"),
        "application/atom+xml",
    );

    let body = response_body_string(authorized).await;
    assert!(
        body.contains("<feed") && body.contains("<id>allSeries</id>"),
        "OPDS v1 series contract requires Atom feed envelope semantics",
    );
    assert!(
        body.contains("rel=\"self\"") && body.contains("/opds/v1.2/series"),
        "OPDS v1 series contract requires self link with OPDS route path",
    );
    assert!(
        body.contains("rel=\"start\"") && body.contains("/opds/v1.2/catalog"),
        "OPDS v1 series contract requires start navigation link to OPDS catalog root",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn opds_v2_manifest_requires_auth_and_rejects_snapshot_only_payload_for_persisted_book() {
    let fixture = OpdsContractFixture::new("opds-manifest").await;
    seed_persisted_book_for_manifest(
        &fixture.paths.main_db,
        &fixture.library_root,
        "library-opds",
        "series-opds",
        "book-opds-persisted-1",
        "Persisted OPDS Contract Book",
        "persisted-opds-book.cbz",
    )
    .await;

    assert_eq!(
        persisted_book_count(&fixture.paths.main_db, "book-opds-persisted-1").await,
        1,
        "OPDS manifest fixture must persist target BOOK row before HTTP assertions",
    );
    assert_eq!(
        persisted_book_title(&fixture.paths.main_db, "book-opds-persisted-1").await,
        Some("Persisted OPDS Contract Book".to_string()),
        "OPDS manifest fixture must persist target BOOK_METADATA title before HTTP assertions",
    );

    let unauthorized = request(
        &fixture.app,
        "GET",
        "/opds/v2/books/book-opds-persisted-1/manifest",
        None,
        &[],
    )
    .await;
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let token = admin_session_token(&fixture.app).await;
    let response = request(
        &fixture.app,
        "GET",
        "/opds/v2/books/book-opds-persisted-1/manifest",
        Some(&token),
        &[(header::HOST.as_str(), "opds-contract.local")],
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .expect("OPDS manifest must include content-type"),
        "application/opds-publication+json",
    );

    let payload = response_json(response).await;
    assert_eq!(
        payload["metadata"]["title"],
        Value::String("Persisted OPDS Contract Book".to_string()),
        "OPDS manifest contract requires metadata.title to come from persisted BOOK/BOOK_METADATA rows, rejecting snapshot-only payloads",
    );
    assert_eq!(
        payload["links"][0]["href"],
        Value::String(
            "http://opds-contract.local/opds/v2/books/book-opds-persisted-1/manifest".to_string()
        ),
        "OPDS manifest contract requires self link href to target requested persisted book id",
    );
    assert_eq!(
        payload["links"][0]["rel"],
        Value::String("self".to_string()),
        "OPDS manifest contract requires first link to remain self relation",
    );
    assert_eq!(
        payload["links"][0]["properties"]["authenticate"]["href"],
        Value::String("http://opds-contract.local/opds/v2/auth".to_string()),
        "OPDS manifest contract requires self link authenticate endpoint to resolve host-aware OPDS auth document",
    );
    assert_eq!(
        payload["links"][1]["href"],
        Value::String(
            "http://opds-contract.local/opds/v2/books/book-opds-persisted-1/file".to_string()
        ),
        "OPDS manifest contract requires acquisition link to target requested persisted book id, not snapshot book-1",
    );
    assert_eq!(
        payload["links"][1]["type"],
        Value::String("application/vnd.comicbook+zip".to_string()),
        "OPDS manifest contract requires acquisition media type to come from persisted book/media state, rejecting snapshot-fixed PDF type",
    );
    assert_eq!(
        payload["links"][2]["href"],
        Value::String(
            "http://opds-contract.local/opds/v2/books/book-opds-persisted-1/progression"
                .to_string()
        ),
        "OPDS manifest contract requires progression link to target requested persisted book id",
    );
    assert_eq!(
        payload["readingOrder"][0]["href"],
        Value::String(
            "http://opds-contract.local/opds/v2/books/book-opds-persisted-1/pages/1?contentNegotiation=false"
                .to_string(),
        ),
        "OPDS manifest contract requires readingOrder href to target requested persisted book id",
    );
    assert_eq!(
        payload["resources"][0]["href"],
        Value::String(
            "http://opds-contract.local/opds/v2/books/book-opds-persisted-1/thumbnail".to_string()
        ),
        "OPDS manifest contract requires resources thumbnail href to target requested persisted book id",
    );
    assert_ne!(
        payload["metadata"]["title"],
        Value::String("book.cbr".to_string()),
        "OPDS manifest contract explicitly rejects snapshot-only placeholder title values",
    );
    assert_ne!(
        payload["links"][1]["href"],
        Value::String("http://opds-contract.local/opds/v2/books/book-1/file".to_string()),
        "OPDS manifest contract explicitly rejects snapshot-only acquisition href that still points to book-1",
    );

    fixture.cleanup();
}

struct OpdsContractFixture {
    paths: persistence_contract_fixture::LegacyDbPaths,
    app: axum::Router,
    library_root: std::path::PathBuf,
}

impl OpdsContractFixture {
    async fn new(case_id: &str) -> Self {
        compat_auth_env::ensure_compat_auth_env();

        let paths = persistence_contract_fixture::new_legacy_db_paths(case_id)
            .expect("opds contract db paths should be created");
        persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
            .await
            .expect("main db flyway fixture should be created");
        persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
            .await
            .expect("tasks db flyway fixture should be created");

        fs::create_dir_all(paths.config_dir.join("lucene"))
            .expect("lucene directory should be created for opds contract fixture");
        fs::create_dir_all(paths.config_dir.join("fonts"))
            .expect("fonts directory should be created for opds contract fixture");
        let library_root = paths.config_dir.join("opds-library-root");
        fs::create_dir_all(&library_root)
            .expect("library root directory should be created for opds contract fixture");

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
) -> axum::response::Response {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(token) = token {
        builder = builder.header("X-Auth-Token", token);
    }
    for (key, value) in extra_headers {
        builder = builder.header(*key, *value);
    }

    app.clone()
        .oneshot(
            builder
                .body(Body::empty())
                .expect("opds request should build"),
        )
        .await
        .expect("opds request should execute")
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
                    format!("Basic {}", compat_auth_env::COMPAT_ADMIN_BASIC_AUTH_BASE64,),
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

async fn seed_persisted_book_for_manifest(
    main_db: &Path,
    library_root: &Path,
    library_id: &str,
    series_id: &str,
    book_id: &str,
    title: &str,
    file_name: &str,
) {
    let media_file_path = library_root.join(file_name);
    fs::write(&media_file_path, b"opds-persisted-media-payload")
        .expect("opds fixture media file should be written");

    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for opds fixture seeding");

    sqlx::query(
        "INSERT INTO LIBRARY (ID, NAME, ROOT, SCAN_STARTUP, EMPTY_TRASH_AFTER_SCAN, ONESHOTS_DIRECTORY) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(library_id)
    .bind("OPDS Contract Library")
    .bind(library_root.to_string_lossy().to_string())
    .bind(false)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("opds fixture library row should insert");

    sqlx::query(
        "INSERT INTO SERIES (ID, CREATED_DATE, LAST_MODIFIED_DATE, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, ONESHOT, DELETED_DATE) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(series_id)
    .bind("2024-03-01T00:00:00")
    .bind("2024-03-03T00:00:00")
    .bind("2024-03-03T00:00:00")
    .bind("OPDS Contract Series")
    .bind(format!("/library/{library_id}/series/{series_id}"))
    .bind(library_id)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("opds fixture series row should insert");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (CREATED_DATE, LAST_MODIFIED_DATE, STATUS, TITLE, TITLE_SORT, SUMMARY, LANGUAGE, PUBLISHER, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("2024-03-01T00:00:00")
    .bind("2024-03-03T00:00:00")
    .bind("ONGOING")
    .bind("OPDS Contract Series")
    .bind("OPDS Contract Series")
    .bind("opds contract series summary")
    .bind("en")
    .bind("Komga Press")
    .bind(series_id)
    .execute(&pool)
    .await
    .expect("opds fixture series metadata row should insert");

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
    .expect("opds fixture book row should insert");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (CREATED_DATE, LAST_MODIFIED_DATE, NUMBER, NUMBER_SORT, TITLE, SUMMARY, BOOK_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("2024-03-01T00:00:00")
    .bind("2024-03-03T00:00:00")
    .bind("1")
    .bind(1.0_f64)
    .bind(title)
    .bind("opds contract summary")
    .bind(book_id)
    .execute(&pool)
    .await
    .expect("opds fixture book metadata row should insert");

    sqlx::query("INSERT INTO MEDIA (STATUS, MEDIA_TYPE, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("READY")
        .bind("application/vnd.comicbook+zip")
        .bind(book_id)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("opds fixture media row should insert");

    pool.close().await;
}

async fn persisted_book_count(main_db: &Path, book_id: &str) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for opds book count inspection");
    let count = sqlx::query("SELECT COUNT(*) AS COUNT FROM BOOK WHERE ID = ?")
        .bind(book_id)
        .fetch_one(&pool)
        .await
        .expect("opds fixture book count should be queryable")
        .get::<i64, _>("COUNT");
    pool.close().await;
    count
}

async fn persisted_book_title(main_db: &Path, book_id: &str) -> Option<String> {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for opds book title inspection");
    let title = sqlx::query("SELECT TITLE FROM BOOK_METADATA WHERE BOOK_ID = ?")
        .bind(book_id)
        .fetch_optional(&pool)
        .await
        .expect("opds fixture book title should be queryable")
        .map(|row| row.get::<String, _>("TITLE"));
    pool.close().await;
    title
}
