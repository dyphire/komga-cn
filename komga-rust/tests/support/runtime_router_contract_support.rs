use std::collections::BTreeMap;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use komga_rust::config::{RuntimeCli, RuntimeConfig};
use komga_rust::infrastructure::sqlite::connect_pool;
use serde_json::Value;
use tower::util::ServiceExt;

#[path = "persistence_contract_fixture.rs"]
mod persistence_contract_fixture;

pub use persistence_contract_fixture::RuntimeDbPaths;

pub fn cleanup_router_fixture(paths: RuntimeDbPaths) {
    persistence_contract_fixture::cleanup(paths)
}

pub async fn new_router_fixture(case_id: &str) -> persistence_contract_fixture::RuntimeDbPaths {
    let paths = persistence_contract_fixture::new_runtime_db_paths(case_id)
        .expect("router contract fixture paths should be created");
    persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
        .await
        .expect("main db flyway fixture should be created");
    persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
        .await
        .expect("tasks db flyway fixture should be created");
    paths
}

pub fn runtime_config_for_paths(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
) -> RuntimeConfig {
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
    env.insert(
        "KOMGA_RUST_RUNTIME_PROFILE".to_string(),
        "snapshot-aligned".to_string(),
    );

    RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &env)
        .expect("runtime config should resolve fixture paths")
}

pub async fn seed_router_contract_data(paths: &persistence_contract_fixture::RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open");

    for sql in [
        "ALTER TABLE READ_PROGRESS ADD COLUMN DEVICE_ID varchar NOT NULL DEFAULT ''",
        "ALTER TABLE READ_PROGRESS ADD COLUMN DEVICE_NAME varchar NOT NULL DEFAULT ''",
        "ALTER TABLE READ_PROGRESS ADD COLUMN LOCATOR blob",
    ] {
        let _ = sqlx::query(sql).execute(&pool).await;
    }

    sqlx::query(
        "INSERT INTO LIBRARY (ID, NAME, ROOT) \
                 VALUES (?, ?, ?)",
    )
    .bind("library-1")
    .bind("Library 1")
    .bind(paths.config_dir.to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("library row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-1")
    .bind(0_i64)
    .bind("Series 1")
    .bind("series/series-1")
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, \
           SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series 1")
    .bind("Series 1")
    .bind("PubHouse")
    .bind("EN")
    .bind(16_i64)
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) \
                 VALUES (?, ?)",
    )
    .bind("series-1")
    .bind("SciFi")
    .execute(&pool)
    .await
    .expect("series metadata genre row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA_TAG (SERIES_ID, TAG) \
                 VALUES (?, ?)",
    )
    .bind("series-1")
    .bind("Favorite")
    .execute(&pool)
    .await
    .expect("series metadata tag row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA_SHARING (SERIES_ID, LABEL) \
                 VALUES (?, ?)",
    )
    .bind("series-1")
    .bind("Family")
    .execute(&pool)
    .await
    .expect("series metadata sharing row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION_AUTHOR (SERIES_ID, NAME, ROLE) \
         VALUES (?, ?, ?)",
    )
    .bind("series-1")
    .bind("John Doe")
    .bind("writer")
    .execute(&pool)
    .await
    .expect("book metadata aggregation author row should be inserted");

    sqlx::query(
        "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("collection-1")
    .bind("Collection 1")
    .bind(false)
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("collection row should be inserted");

    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) \
         VALUES (?, ?, ?)",
    )
    .bind("collection-1")
    .bind("series-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("collection series row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, \
           LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind(0_i64)
    .bind("book-1.epub")
    .bind("books/book-1.epub")
    .bind("series-1")
    .bind(1_024_i64)
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book row should be inserted");

    sqlx::query(
        "UPDATE BOOK \
                 SET FILE_HASH_KOREADER = ? \
                 WHERE ID = ?",
    )
    .bind("hash-book-1")
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("book koreader hash should be set");

    sqlx::query(
        "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("application/epub+zip")
    .bind("READY")
    .bind("book-1")
    .bind(10_i64)
    .execute(&pool)
    .await
    .expect("media row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("1")
    .bind(1.0_f64)
    .bind("Book 1")
    .bind("2024-01-15")
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("book metadata row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) \
                 VALUES (?, ?)",
    )
    .bind("book-1")
    .bind("favorite-tag")
    .execute(&pool)
    .await
    .expect("book metadata tag row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) \
                 VALUES (?, ?, ?)",
    )
    .bind("book-1")
    .bind("Jane Writer")
    .bind("writer")
    .execute(&pool)
    .await
    .expect("book metadata author row should be inserted");

    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, SELECTED) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("thumb-book-1")
    .bind("book-1")
    .bind("USER_UPLOADED")
    .bind(true)
    .execute(&pool)
    .await
    .expect("book thumbnail row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION (RELEASE_DATE, SUMMARY, SUMMARY_NUMBER, SERIES_ID) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("2024-01-15")
    .bind("")
    .bind("")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("book metadata aggregation row should be inserted");

    sqlx::query(
        "INSERT INTO READLIST (ID, NAME, BOOK_COUNT) \
                 VALUES (?, ?, ?)",
    )
    .bind("readlist-1")
    .bind("ReadList 1")
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("readlist row should be inserted");

    sqlx::query(
        "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) \
                 VALUES (?, ?, ?)",
    )
    .bind("readlist-1")
    .bind("book-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("readlist book row should be inserted");

    let hashed_password = hash_bcrypt_password("router-contract-admin-123", DEFAULT_COST)
        .expect("bcrypt hash should be computed");
    sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("admin-user")
    .bind("admin@example.org")
    .bind(hashed_password)
    .bind(true)
    .execute(&pool)
    .await
    .expect("admin user should be inserted");

    for role in ["USER", "ADMIN", "FILE_DOWNLOAD", "PAGE_STREAMING"] {
        sqlx::query(
            "INSERT INTO USER_ROLE (USER_ID, ROLE) \
                     VALUES (?, ?)",
        )
        .bind("admin-user")
        .bind(role)
        .execute(&pool)
        .await
        .expect("admin role should be inserted");
    }

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_contract_nullable_samples(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract nullable db should open");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Series 2")
    .bind("series/series-2")
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("nullable series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, \
           SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series 2")
    .bind("Series 2")
    .bind("NullPub")
    .bind("EN")
    .bind(18_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("nullable series metadata row should be inserted");

    sqlx::query(
        "UPDATE SERIES \
                 SET BOOK_COUNT = ? \
                 WHERE ID = ?",
    )
    .bind(1_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("nullable series book count should be updated");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, \
           LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-2")
    .bind(0_i64)
    .bind("book-2.epub")
    .bind("books/book-2.epub")
    .bind("series-2")
    .bind(2_048_i64)
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("nullable book row should be inserted");

    sqlx::query(
        "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("application/epub+zip")
    .bind("READY")
    .bind("book-2")
    .bind(12_i64)
    .execute(&pool)
    .await
    .expect("nullable media row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("2")
    .bind(2.0_f64)
    .bind("Book 2")
    .bind("2024-01-16")
    .bind("book-2")
    .execute(&pool)
    .await
    .expect("nullable book metadata row should be inserted");

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_read_progress(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    completed: bool,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract read-progress db should open");

    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(if completed { 10_i64 } else { 1_i64 })
    .bind(completed)
    .execute(&pool)
    .await
    .expect("router contract read-progress row should be inserted");

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_series_read_progress(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    read_count: i64,
    in_progress_count: i64,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract series read-progress db should open");

    sqlx::query(
        "INSERT INTO READ_PROGRESS_SERIES (SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT (SERIES_ID, USER_ID) DO UPDATE \
         SET READ_COUNT = excluded.READ_COUNT, IN_PROGRESS_COUNT = excluded.IN_PROGRESS_COUNT",
    )
    .bind("series-1")
    .bind("admin-user")
    .bind(read_count)
    .bind(in_progress_count)
    .execute(&pool)
    .await
    .expect("router contract series read-progress row should be upserted");

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_series_counts(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    book_count: i64,
    total_book_count: Option<i64>,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract series counts db should open");

    sqlx::query(
        "UPDATE SERIES \
                 SET BOOK_COUNT = ? \
                 WHERE ID = ?",
    )
    .bind(book_count)
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("router contract series book_count should be updated");

    sqlx::query(
        "UPDATE SERIES_METADATA \
                 SET TOTAL_BOOK_COUNT = ? \
                 WHERE SERIES_ID = ?",
    )
    .bind(total_book_count)
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("router contract series total_book_count should be updated");

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_age_exclude_user(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    user_id: &str,
    email: &str,
    password: &str,
    age_restriction: i64,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract restricted-user db should open");

    let hashed_password =
        hash_bcrypt_password(password, DEFAULT_COST).expect("bcrypt hash should be computed");

    sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(email)
    .bind(hashed_password)
    .bind(true)
    .bind(age_restriction)
    .bind(false)
    .execute(&pool)
    .await
    .expect("restricted user should be inserted");

    for role in ["USER", "PAGE_STREAMING"] {
        sqlx::query("INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
            .bind(user_id)
            .bind(role)
            .execute(&pool)
            .await
            .expect("restricted role should be inserted");
    }

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_series_title_sort(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    series_id: &str,
    title_sort: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract series title-sort db should open");

    sqlx::query(
        "UPDATE SERIES_METADATA \
         SET TITLE_SORT = ? \
         WHERE SERIES_ID = ?",
    )
    .bind(title_sort)
    .bind(series_id)
    .execute(&pool)
    .await
    .expect("series metadata title_sort should be updated for contract test");

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_series_aggregated_tag(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    series_id: &str,
    tag: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract series aggregated tag db should open");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION_TAG (SERIES_ID, TAG) \
         VALUES (?, ?)",
    )
    .bind(series_id)
    .bind(tag)
    .execute(&pool)
    .await
    .expect("series aggregated tag row should be inserted for contract test");

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_custom_series(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    series_id: &str,
    name: &str,
    library_id: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract custom series db should open");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(series_id)
    .bind(0_i64)
    .bind(name)
    .bind(format!("series/{series_id}"))
    .bind(library_id)
    .execute(&pool)
    .await
    .expect("custom series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, \
           SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind(name)
    .bind(name)
    .bind("PubHouse")
    .bind("EN")
    .bind(16_i64)
    .bind(series_id)
    .execute(&pool)
    .await
    .expect("custom series metadata row should be inserted");

    pool.close().await;
}

pub async fn login_with_basic_and_get_token(app: axum::Router) -> String {
    login_with_basic_credentials_and_get_token(
        app,
        "admin@example.org",
        "router-contract-admin-123",
    )
    .await
}

pub async fn login_with_basic_credentials_and_get_token(
    app: axum::Router,
    email: &str,
    password: &str,
) -> String {
    let basic_token = STANDARD.encode(format!("{email}:{password}"));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, format!("Basic {basic_token}"))
                .body(Body::empty())
                .expect("users/me request should build"),
        )
        .await
        .expect("users/me request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get("x-auth-token")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("users/me login should return x-auth-token")
}

pub async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&body).expect("response body should be valid json")
}
