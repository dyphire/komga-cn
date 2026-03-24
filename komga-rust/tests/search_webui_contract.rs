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

const SEARCH_OWNERSHIP_HEADER: &str = "x-komga-compat-search-ownership";
const NATIVE_OWNERSHIP_MARKER: &str = "native-rust-owned";

#[test]
fn search_webui_contract_target_is_registered() {
    assert_required_target_declared("search/WebUI", "search_webui_contract");
}

#[tokio::test]
async fn search_books_list_native_owned_filters_persisted_rows_and_rejects_snapshot_only_payload() {
    let fixture = SearchWebUiContractFixture::new("search-webui-books-list").await;

    seed_search_fixture(
        &fixture.paths.main_db,
        &fixture.library_root,
        "library-search",
        "series-search-a",
        "series-search-b",
        "book-search-hit",
        "book-search-miss",
    )
    .await;

    assert_eq!(
        persisted_book_count(&fixture.paths.main_db).await,
        2,
        "search contract fixture must persist two books before querying books/list",
    );

    let token = admin_session_token(&fixture.app).await;
    let response = request(
        &fixture.app,
        "POST",
        "/api/v1/books/list?page=0&size=20&sort=metadata.title,asc",
        Some(&token),
        &[
            (SEARCH_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER),
            (header::CONTENT_TYPE.as_str(), "application/json"),
        ],
        Some(json!({ "fullTextSearch": "Needle" })),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
        "search API parity requires native ownership marker on native-owned books/list responses",
    );

    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("books/list response must expose content array");
    assert_eq!(
        content.len(),
        1,
        "books/list fullTextSearch must filter persisted rows; returning unfiltered canned payload breaks parity",
    );
    assert_eq!(
        content[0]["id"],
        Value::String("book-search-hit".to_string()),
        "books/list search result should target the persisted matching book id",
    );
    assert_eq!(
        content[0]["metadata"]["title"],
        Value::String("Needle in Rust Stack".to_string()),
        "books/list search result must surface persisted BOOK_METADATA.title",
    );
    assert_ne!(
        content[0]["id"],
        Value::String("book-1".to_string()),
        "books/list search contract explicitly rejects snapshot-only placeholder book identifiers",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn search_series_list_native_owned_filters_persisted_series_by_full_text_search() {
    let fixture = SearchWebUiContractFixture::new("search-webui-series-list").await;

    seed_search_fixture(
        &fixture.paths.main_db,
        &fixture.library_root,
        "library-search",
        "series-search-a",
        "series-search-b",
        "book-search-hit",
        "book-search-miss",
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let response = request(
        &fixture.app,
        "POST",
        "/api/v1/series/list?page=0&size=20&sort=metadata.titleSort,asc",
        Some(&token),
        &[
            (SEARCH_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER),
            (header::CONTENT_TYPE.as_str(), "application/json"),
        ],
        Some(json!({ "fullTextSearch": "Needle" })),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
        "series/list native-owned search responses must keep native ownership marker",
    );

    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("series/list response must expose content array");
    assert_eq!(content.len(), 1, "series fullTextSearch must filter persisted titles");
    assert_eq!(
        content[0]["id"],
        Value::String("series-search-a".to_string()),
        "series/list search should return persisted matching series only",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn series_list_shadow_mode_maps_snapshot_library_filter_to_persisted_library_ids() {
    let fixture = SearchWebUiContractFixture::new("search-webui-series-shadow-library-filter").await;

    seed_search_fixture(
        &fixture.paths.main_db,
        &fixture.library_root,
        "library-search",
        "series-search-a",
        "series-search-b",
        "book-search-hit",
        "book-search-miss",
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let response = request(
        &fixture.app,
        "POST",
        "/api/v1/series/list?page=0&size=20&sort=metadata.titleSort,asc",
        Some(&token),
        &[(header::CONTENT_TYPE.as_str(), "application/json")],
        Some(json!({
            "condition": {
                "allOf": [
                    {
                        "libraryId": {
                            "operator": "is",
                            "value": "1"
                        }
                    }
                ]
            }
        })),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        None,
        "shadow-mode series/list must not emit native ownership marker",
    );

    let payload = response_json(response).await;
    assert_eq!(
        payload["totalElements"],
        Value::Number(2u64.into()),
        "legacy snapshot library id filter should map to persisted library ids in shadow-mode browse",
    );
    let ids = payload["content"]
        .as_array()
        .expect("series/list shadow-mode response must expose content array")
        .iter()
        .filter_map(|entry| entry.get("id"))
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        ids.iter().any(|id| id == &Value::String("series-search-a".to_string())),
        "series/list shadow-mode should return persisted ids, not only snapshot placeholders",
    );
    assert!(
        ids.iter().all(|id| id != &Value::String("series-1".to_string())),
        "series/list shadow-mode must not regress to snapshot series-1 placeholder ids",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn books_list_shadow_mode_maps_snapshot_library_filter_to_persisted_library_ids() {
    let fixture = SearchWebUiContractFixture::new("search-webui-books-shadow-library-filter").await;

    seed_search_fixture(
        &fixture.paths.main_db,
        &fixture.library_root,
        "library-search",
        "series-search-a",
        "series-search-b",
        "book-search-hit",
        "book-search-miss",
    )
    .await;

    let token = admin_session_token(&fixture.app).await;
    let response = request(
        &fixture.app,
        "POST",
        "/api/v1/books/list?page=0&size=20&sort=metadata.title,asc",
        Some(&token),
        &[(header::CONTENT_TYPE.as_str(), "application/json")],
        Some(json!({
            "condition": {
                "allOf": [
                    {
                        "libraryId": {
                            "operator": "is",
                            "value": "1"
                        }
                    }
                ]
            }
        })),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        None,
        "shadow-mode books/list must not emit native ownership marker",
    );

    let payload = response_json(response).await;
    assert_eq!(
        payload["totalElements"],
        Value::Number(2u64.into()),
        "legacy snapshot library id filter should map to persisted library ids in shadow-mode books browse",
    );
    let ids = payload["content"]
        .as_array()
        .expect("books/list shadow-mode response must expose content array")
        .iter()
        .filter_map(|entry| entry.get("id"))
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        ids.iter().any(|id| id == &Value::String("book-search-hit".to_string())),
        "books/list shadow-mode should return persisted ids, not only snapshot placeholders",
    );
    assert!(
        ids.iter().all(|id| id != &Value::String("book-1".to_string())),
        "books/list shadow-mode must not regress to snapshot book-1 placeholder ids",
    );

    fixture.cleanup();
}

#[tokio::test]
async fn rejects_missing_webui_dependencies() {
    let fixture = SearchWebUiContractFixture::new("search-webui-dependencies").await;
    let token = admin_session_token(&fixture.app).await;

    let global_settings = request(
        &fixture.app,
        "GET",
        "/api/v1/client-settings/global/list",
        None,
        &[],
        None,
    )
    .await;
    assert_ne!(
        global_settings.status(),
        StatusCode::NOT_FOUND,
        "unchanged WebUI depends on /api/v1/client-settings/global/list route existence",
    );
    assert_eq!(global_settings.status(), StatusCode::OK);
    let global_payload = response_json(global_settings).await;
    assert!(
        global_payload.get("webui.oauth2.hide_login").is_some(),
        "unchanged WebUI bootstrap depends on webui.oauth2.hide_login client setting",
    );

    let user_settings_unauth = request(
        &fixture.app,
        "GET",
        "/api/v1/client-settings/user/list",
        None,
        &[],
        None,
    )
    .await;
    assert_eq!(user_settings_unauth.status(), StatusCode::UNAUTHORIZED);

    let user_settings_auth = request(
        &fixture.app,
        "GET",
        "/api/v1/client-settings/user/list",
        Some(&token),
        &[],
        None,
    )
    .await;
    assert_eq!(user_settings_auth.status(), StatusCode::OK);

    let login_set_cookie = request(
        &fixture.app,
        "GET",
        "/api/v1/login/set-cookie",
        Some(&token),
        &[],
        None,
    )
    .await;
    assert_ne!(
        login_set_cookie.status(),
        StatusCode::NOT_FOUND,
        "unchanged WebUI login flow depends on /api/v1/login/set-cookie",
    );
    assert_eq!(login_set_cookie.status(), StatusCode::NO_CONTENT);
    assert!(
        login_set_cookie
            .headers()
            .get(header::SET_COOKIE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|cookie| cookie.contains("KOMGA-SESSION=")),
        "login/set-cookie must set KOMGA-SESSION cookie for unchanged WebUI session bootstrap",
    );

    let books_list_unauth = request(
        &fixture.app,
        "POST",
        "/api/v1/books/list?page=0&size=20&sort=metadata.title,asc",
        None,
        &[(header::CONTENT_TYPE.as_str(), "application/json")],
        Some(json!({ "fullTextSearch": "Needle" })),
    )
    .await;
    assert_eq!(books_list_unauth.status(), StatusCode::UNAUTHORIZED);

    fixture.cleanup();
}

struct SearchWebUiContractFixture {
    paths: persistence_contract_fixture::LegacyDbPaths,
    app: axum::Router,
    library_root: std::path::PathBuf,
}

impl SearchWebUiContractFixture {
    async fn new(case_id: &str) -> Self {
        compat_auth_env::ensure_compat_auth_env();

        let paths = persistence_contract_fixture::new_legacy_db_paths(case_id)
            .expect("search/webui contract db paths should be created");
        persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
            .await
            .expect("main db flyway fixture should be created");
        persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
            .await
            .expect("tasks db flyway fixture should be created");

        fs::create_dir_all(paths.config_dir.join("lucene"))
            .expect("lucene directory should be created for search/webui contract fixture");
        fs::create_dir_all(paths.config_dir.join("fonts"))
            .expect("fonts directory should be created for search/webui contract fixture");
        let library_root = paths.config_dir.join("search-webui-library-root");
        fs::create_dir_all(&library_root)
            .expect("library root directory should be created for search/webui fixture");

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
        .oneshot(
            builder
                .body(body)
                .expect("search/webui contract request should build"),
        )
        .await
        .expect("search/webui contract request should execute")
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&body).expect("response body should contain valid JSON")
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

async fn seed_search_fixture(
    main_db: &Path,
    library_root: &Path,
    library_id: &str,
    matching_series_id: &str,
    non_matching_series_id: &str,
    matching_book_id: &str,
    non_matching_book_id: &str,
) {
    fs::write(library_root.join("search-hit.cbz"), b"search-hit")
        .expect("search fixture hit media file should be written");
    fs::write(library_root.join("search-miss.cbz"), b"search-miss")
        .expect("search fixture miss media file should be written");

    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for search/webui fixture seeding");

    sqlx::query(
        "INSERT INTO LIBRARY (ID, NAME, ROOT, SCAN_STARTUP, EMPTY_TRASH_AFTER_SCAN, ONESHOTS_DIRECTORY) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(library_id)
    .bind("Search WebUI Contract Library")
    .bind(library_root.to_string_lossy().to_string())
    .bind(false)
    .bind(false)
    .bind(None::<String>)
    .execute(&pool)
    .await
    .expect("search/webui fixture library row should insert");

    seed_series_and_book(
        &pool,
        library_id,
        matching_series_id,
        matching_book_id,
        "Needle in Rust Series",
        "Needle in Rust Stack",
        "search-hit.cbz",
        1,
    )
    .await;

    seed_series_and_book(
        &pool,
        library_id,
        non_matching_series_id,
        non_matching_book_id,
        "Background Series",
        "Background Volume",
        "search-miss.cbz",
        2,
    )
    .await;

    pool.close().await;
}

async fn seed_series_and_book(
    pool: &sqlx::SqlitePool,
    library_id: &str,
    series_id: &str,
    book_id: &str,
    series_title: &str,
    book_title: &str,
    file_name: &str,
    number: i32,
) {
    sqlx::query(
        "INSERT INTO SERIES (ID, CREATED_DATE, LAST_MODIFIED_DATE, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, ONESHOT, DELETED_DATE) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(series_id)
    .bind("2024-03-01T00:00:00")
    .bind("2024-03-03T00:00:00")
    .bind("2024-03-03T00:00:00")
    .bind(series_title)
    .bind(format!("/library/{library_id}/series/{series_id}"))
    .bind(library_id)
    .bind(false)
    .bind(None::<String>)
    .execute(pool)
    .await
    .expect("search/webui fixture series row should insert");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (CREATED_DATE, LAST_MODIFIED_DATE, STATUS, TITLE, TITLE_SORT, SUMMARY, LANGUAGE, PUBLISHER, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("2024-03-01T00:00:00")
    .bind("2024-03-03T00:00:00")
    .bind("ONGOING")
    .bind(series_title)
    .bind(series_title)
    .bind("search contract series summary")
    .bind("en")
    .bind("Komga Press")
    .bind(series_id)
    .execute(pool)
    .await
    .expect("search/webui fixture series metadata row should insert");

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
    .bind(40_i64)
    .bind(number)
    .bind(library_id)
    .bind(false)
    .bind(None::<String>)
    .execute(pool)
    .await
    .expect("search/webui fixture book row should insert");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (CREATED_DATE, LAST_MODIFIED_DATE, NUMBER, NUMBER_SORT, TITLE, SUMMARY, BOOK_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("2024-03-01T00:00:00")
    .bind("2024-03-03T00:00:00")
    .bind(number.to_string())
    .bind(number as f64)
    .bind(book_title)
    .bind("search contract book summary")
    .bind(book_id)
    .execute(pool)
    .await
    .expect("search/webui fixture book metadata row should insert");

    sqlx::query("INSERT INTO MEDIA (STATUS, MEDIA_TYPE, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("READY")
        .bind("application/vnd.comicbook+zip")
        .bind(book_id)
        .bind(1_i64)
        .execute(pool)
        .await
        .expect("search/webui fixture media row should insert");
}

async fn persisted_book_count(main_db: &Path) -> i64 {
    let pool = connect_pool(main_db, 1)
        .await
        .expect("sqlite pool should open for persisted search fixture inspection");
    let count = sqlx::query("SELECT COUNT(*) AS COUNT FROM BOOK")
        .fetch_one(&pool)
        .await
        .expect("search/webui fixture book count should be queryable")
        .get::<i64, _>("COUNT");
    pool.close().await;
    count
}
