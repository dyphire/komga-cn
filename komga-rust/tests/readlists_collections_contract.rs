use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use sqlx::Row;
use tower::util::ServiceExt;

#[path = "support/runtime_router_contract_support.rs"]
mod runtime_router_contract_support;

use runtime_router_contract_support::*;

#[test]
fn readlists_collections_contract_target_is_registered() {
    assert_required_target_declared("readlists/collections", "readlists_collections_contract");
}

#[tokio::test]
async fn router_readlist_tachiyomi_progress_marks_books_completed_at_real_page_count() {
    let paths = new_router_fixture("router-readlist-tachiyomi-progress-real-page-count").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "lastBookRead": 2 }).to_string()))
                .expect("readlist tachiyomi write request should build"),
        )
        .await
        .expect("readlist tachiyomi write request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for tachiyomi verification");
    let rows = sqlx::query(
        "SELECT BOOK_ID, PAGE, COMPLETED FROM READ_PROGRESS WHERE USER_ID = ? ORDER BY BOOK_ID ASC",
    )
    .bind("admin-user")
    .fetch_all(&pool)
    .await
    .expect("read progress rows should be queryable");
    pool.close().await;

    let persisted = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("BOOK_ID"),
                row.get::<i64, _>("PAGE"),
                row.get::<i64, _>("COMPLETED"),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        persisted,
        vec![("book-1".to_string(), 10, 1), ("book-2".to_string(), 11, 1),]
    );

    cleanup_router_fixture(paths);
}

async fn seed_extra_readlist(paths: &RuntimeDbPaths, readlist_id: &str, name: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("extra readlist db should open");

    sqlx::query("INSERT INTO READLIST (ID, NAME, BOOK_COUNT) VALUES (?, ?, ?)")
        .bind(readlist_id)
        .bind(name)
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("extra readlist row should be inserted");

    pool.close().await;
}

async fn load_selected_readlist_thumbnail_count(paths: &RuntimeDbPaths, readlist_id: &str) -> i64 {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("readlist thumbnail count db should open");

    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM THUMBNAIL_READLIST WHERE READLIST_ID = ? AND SELECTED = 1",
    )
    .bind(readlist_id)
    .fetch_one(&pool)
    .await
    .expect("selected readlist thumbnail count should load");

    pool.close().await;
    count
}

async fn seed_collection_listing_variants(paths: &RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("collection listing variants db should open");

    sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
        .bind("library-2")
        .bind("Library 2")
        .bind(
            paths
                .config_dir
                .join("library-2")
                .to_string_lossy()
                .to_string(),
        )
        .execute(&pool)
        .await
        .expect("secondary library row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Search Target")
    .bind("series/series-2")
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("secondary series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Search Target")
    .bind("Search Target")
    .bind("SecondPub")
    .bind("FR")
    .bind(12_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("secondary series metadata row should be inserted");

    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
        .bind("collection-2")
        .bind("Beta Collection")
        .bind(false)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("secondary collection row should be inserted");

    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-2")
    .bind("series-2")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("secondary collection series row should be inserted");

    pool.close().await;
}

async fn seed_collection_series_variants(paths: &RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("collection series variants db should open");

    sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
        .bind("library-2")
        .bind("Library 2")
        .bind(
            paths
                .config_dir
                .join("library-2")
                .to_string_lossy()
                .to_string(),
        )
        .execute(&pool)
        .await
        .expect("secondary library for collection series should be inserted");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Series 2")
    .bind("series/series-2")
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("secondary series for collection series should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ENDED")
    .bind("Series 2")
    .bind("Series 2")
    .bind("OtherPub")
    .bind("FR")
    .bind(18_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("secondary series metadata for collection series should be inserted");

    sqlx::query("INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) VALUES (?, ?)")
        .bind("series-2")
        .bind("Drama")
        .execute(&pool)
        .await
        .expect("secondary series genre should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION_AUTHOR (SERIES_ID, NAME, ROLE) VALUES (?, ?, ?)",
    )
    .bind("series-2")
    .bind("Alice Roe")
    .bind("editor")
    .execute(&pool)
    .await
    .expect("secondary series author should be inserted");

    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-1")
    .bind("series-2")
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("secondary series should be attached to collection-1");

    pool.close().await;
}

async fn seed_readlist_endpoint_variants(paths: &RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("readlist endpoint variants db should open");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Series 2")
    .bind("series/series-2")
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("secondary readlist series should be inserted");

    sqlx::query("INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind("ONGOING")
        .bind("Series 2")
        .bind("Series 2")
        .bind("PubHouse")
        .bind("EN")
        .bind(16_i64)
        .bind("series-2")
        .execute(&pool)
        .await
        .expect("secondary readlist series metadata should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION (RELEASE_DATE, SUMMARY, SUMMARY_NUMBER, SERIES_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("2024-01-01")
    .bind("")
    .bind("")
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("secondary readlist book metadata aggregation should be inserted");

    sqlx::query("INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind("book-2")
        .bind(0_i64)
        .bind("books/book-2.epub")
        .bind("books/book-2.epub")
        .bind("series-2")
        .bind(2_048_i64)
        .bind(2_i64)
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("secondary readlist book should be inserted");

    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind("book-2")
        .bind(11_i64)
        .execute(&pool)
        .await
        .expect("secondary readlist media should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)")
        .bind("2")
        .bind(2.0_f64)
        .bind("Book 2")
        .bind("2024-01-16")
        .bind("book-2")
        .execute(&pool)
        .await
        .expect("secondary readlist book metadata should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-2")
        .bind("Jane Writer")
        .bind("writer")
        .execute(&pool)
        .await
        .expect("secondary readlist book author should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
        .bind("book-2")
        .bind("library-one-tag")
        .execute(&pool)
        .await
        .expect("secondary readlist book tag should be inserted");

    sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
        .bind("library-2")
        .bind("Library 2")
        .bind(
            paths
                .config_dir
                .join("library-2")
                .to_string_lossy()
                .to_string(),
        )
        .execute(&pool)
        .await
        .expect("secondary readlist library should be inserted");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-3")
    .bind(0_i64)
    .bind("Series 3")
    .bind("series/series-3")
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("third readlist series should be inserted");

    sqlx::query("INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind("ONGOING")
        .bind("Series 3")
        .bind("Series 3")
        .bind("OtherPub")
        .bind("FR")
        .bind(12_i64)
        .bind("series-3")
        .execute(&pool)
        .await
        .expect("third readlist series metadata should be inserted");

    sqlx::query("INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind("book-3")
        .bind(0_i64)
        .bind("books/book-3.epub")
        .bind("books/book-3.epub")
        .bind("series-3")
        .bind(3_072_i64)
        .bind(3_i64)
        .bind("library-2")
        .execute(&pool)
        .await
        .expect("third readlist book should be inserted");

    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind("book-3")
        .bind(12_i64)
        .execute(&pool)
        .await
        .expect("third readlist media should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)")
        .bind("3")
        .bind(3.0_f64)
        .bind("Book 3")
        .bind("2024-01-17")
        .bind("book-3")
        .execute(&pool)
        .await
        .expect("third readlist book metadata should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-3")
        .bind("Guest Writer")
        .bind("writer")
        .execute(&pool)
        .await
        .expect("third readlist book author should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
        .bind("book-3")
        .bind("library-two-tag")
        .execute(&pool)
        .await
        .expect("third readlist book tag should be inserted");

    sqlx::query("UPDATE READLIST SET BOOK_COUNT = ? WHERE ID = ?")
        .bind(3_i64)
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist book count should be updated");

    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-1")
        .bind("book-2")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("second readlist book relation should be inserted");

    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-1")
        .bind("book-3")
        .bind(2_i64)
        .execute(&pool)
        .await
        .expect("third readlist book relation should be inserted");

    pool.close().await;
}

async fn mark_readlist_unordered(paths: &RuntimeDbPaths, readlist_id: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("unordered readlist db should open");

    sqlx::query("UPDATE READLIST SET ORDERED = ? WHERE ID = ?")
        .bind(false)
        .bind(readlist_id)
        .execute(&pool)
        .await
        .expect("readlist ordered flag should be updated");

    sqlx::query("UPDATE READLIST_BOOK SET NUMBER = ? WHERE READLIST_ID = ? AND BOOK_ID = ?")
        .bind(2_i64)
        .bind(readlist_id)
        .bind("book-2")
        .execute(&pool)
        .await
        .expect("unordered readlist should move book-2 after book-3 in relation order");

    sqlx::query("UPDATE READLIST_BOOK SET NUMBER = ? WHERE READLIST_ID = ? AND BOOK_ID = ?")
        .bind(1_i64)
        .bind(readlist_id)
        .bind("book-3")
        .execute(&pool)
        .await
        .expect("unordered readlist should move book-3 before book-2 in relation order");

    pool.close().await;
}

async fn seed_readlist_author_edge_case(paths: &RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("readlist author edge case db should open");

    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-3")
        .bind("Doe, John")
        .bind("")
        .execute(&pool)
        .await
        .expect("edge-case readlist author should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-3")
        .bind("Casey Role")
        .bind("CoWriter")
        .execute(&pool)
        .await
        .expect("mixed-case readlist author should be inserted");

    pool.close().await;
}

async fn seed_facet_scope_variants(paths: &RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("facet scope variants db should open");

    sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
        .bind("library-2")
        .bind("Library 2")
        .bind(
            paths
                .config_dir
                .join("library-2")
                .to_string_lossy()
                .to_string(),
        )
        .execute(&pool)
        .await
        .expect("facet secondary library should be inserted");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Series 2")
    .bind("series/series-2")
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("facet secondary series should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series 2")
    .bind("Series 2")
    .bind("OtherPub")
    .bind("FR")
    .bind(12_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("facet secondary series metadata should be inserted");

    sqlx::query("INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) VALUES (?, ?)")
        .bind("series-2")
        .bind("Drama")
        .execute(&pool)
        .await
        .expect("facet secondary genre should be inserted");

    sqlx::query("INSERT INTO SERIES_METADATA_TAG (SERIES_ID, TAG) VALUES (?, ?)")
        .bind("series-2")
        .bind("other-series-tag")
        .execute(&pool)
        .await
        .expect("facet secondary series tag should be inserted");

    sqlx::query("INSERT INTO SERIES_METADATA_SHARING (SERIES_ID, LABEL) VALUES (?, ?)")
        .bind("series-2")
        .bind("Friends")
        .execute(&pool)
        .await
        .expect("facet secondary sharing label should be inserted");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-2")
    .bind(0_i64)
    .bind("book-2.epub")
    .bind("books/book-2.epub")
    .bind("series-2")
    .bind(2_048_i64)
    .bind(2_i64)
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("facet secondary book should be inserted");

    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind("book-2")
        .bind(11_i64)
        .execute(&pool)
        .await
        .expect("facet secondary media should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)")
        .bind("2")
        .bind(2.0_f64)
        .bind("Book 2")
        .bind("2025-02-20")
        .bind("book-2")
        .execute(&pool)
        .await
        .expect("facet secondary book metadata should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
        .bind("book-2")
        .bind("other-book-tag")
        .execute(&pool)
        .await
        .expect("facet secondary book tag should be inserted");

    pool.close().await;
}

fn comicrack_multipart_body(xml: &str) -> (String, Vec<u8>) {
    let boundary = "komga-rust-comicrack-boundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"list.cbl\"\r\nContent-Type: application/xml\r\n\r\n{xml}\r\n--{boundary}--\r\n"
    );

    (
        format!("multipart/form-data; boundary={boundary}"),
        body.into_bytes(),
    )
}

fn comicrack_multipart_body_with_quoted_boundary(xml: &str) -> (String, Vec<u8>) {
    let boundary = "komga-rust-quoted-boundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"list.cbl\"\r\nContent-Type: application/xml\r\n\r\n{xml}\r\n--{boundary}--\r\n"
    );

    (
        format!("multipart/form-data; boundary=\"{boundary}\""),
        body.into_bytes(),
    )
}

#[tokio::test]
async fn router_collections_supports_search_library_id_and_unpaged() {
    let paths = new_router_fixture("router-collections-search-library-unpaged").await;
    seed_router_contract_data(&paths).await;
    seed_collection_listing_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections?search=beta&library_id=library-2&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collections search request should build"),
        )
        .await
        .expect("collections search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("collections payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id").and_then(Value::as_str),
        Some("collection-2")
    );
    assert_eq!(
        payload
            .get("pageable")
            .and_then(|pageable| pageable.get("unpaged"))
            .and_then(Value::as_bool),
        Some(true)
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_series_supports_kotlin_style_query_filters() {
    let paths = new_router_fixture("router-collection-series-query-filters").await;
    seed_router_contract_data(&paths).await;
    seed_collection_series_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections/collection-1/series?library_id=library-1&publisher=PubHouse&language=EN&genre=SciFi&age_rating=16&author=John+Doe,writer&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collection series filter request should build"),
        )
        .await
        .expect("collection series filter request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("collection series filter payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id").and_then(Value::as_str),
        Some("series-1")
    );
    assert_eq!(
        payload
            .get("pageable")
            .and_then(|pageable| pageable.get("unpaged"))
            .and_then(Value::as_bool),
        Some(true)
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_collections_filter_series_ids_for_partially_visible_user() {
    let paths = new_router_fixture("router-series-collections-partially-visible").await;
    seed_router_contract_data(&paths).await;
    seed_collection_series_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-1-user",
        "library1@example.org",
        "router-contract-library1-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library1@example.org",
        "router-contract-library1-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1/collections")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series collections partially visible request should build"),
        )
        .await
        .expect("series collections partially visible request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let collections = payload
        .as_array()
        .expect("series collections payload should be an array");
    assert_eq!(collections.len(), 1);
    assert_eq!(
        collections[0].get("id").and_then(Value::as_str),
        Some("collection-1")
    );
    assert_eq!(collections[0].get("filtered"), Some(&Value::Bool(true)));
    assert_eq!(collections[0].get("seriesIds"), Some(&json!(["series-1"])));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_collections_does_not_accept_sorted_position_series_alias() {
    let paths = new_router_fixture("router-series-collections-no-id-bridge").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for series alias test");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-real")
    .bind(0_i64)
    .bind("Series Real")
    .bind("series/series-real")
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("alias target series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series Real")
    .bind("ZZZ Series Real")
    .bind("PubHouse")
    .bind("EN")
    .bind(0_i64)
    .bind("series-real")
    .execute(&pool)
    .await
    .expect("alias target series metadata row should be inserted");

    sqlx::query(
        "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("collection-alias")
    .bind("Collection Alias")
    .bind(false)
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("alias collection row should be inserted");

    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) \
         VALUES (?, ?, ?)",
    )
    .bind("collection-alias")
    .bind("series-real")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("alias collection membership row should be inserted");

    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-2/collections")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series collections alias request should build"),
        )
        .await
        .expect("series collections alias request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload, json!([]));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_thumbnail_upload_parses_multipart_image_and_selected_flag() {
    let paths = new_router_fixture("router-readlist-thumbnail-upload-multipart").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "readlist.png", "image/png", false, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists/readlist-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("readlist thumbnail upload request should build"),
        )
        .await
        .expect("readlist thumbnail upload request should complete");

    assert_eq!(upload.status(), StatusCode::OK);
    let payload = response_json(upload).await;
    assert_eq!(
        payload.get("readListId"),
        Some(&Value::String("readlist-1".to_string()))
    );
    assert_eq!(
        payload.get("type"),
        Some(&Value::String("USER_UPLOADED".to_string()))
    );
    assert_eq!(payload.get("selected"), Some(&Value::Bool(false)));
    assert_eq!(
        payload.get("mediaType"),
        Some(&Value::String("image/png".to_string()))
    );
    assert_eq!(
        payload.get("fileSize"),
        Some(&json!(image_bytes.len() as i64))
    );
    assert_eq!(payload.get("width"), Some(&json!(1)));
    assert_eq!(payload.get("height"), Some(&json!(1)));

    let thumbnails = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist thumbnails request should build"),
        )
        .await
        .expect("readlist thumbnails request should complete");

    assert_eq!(thumbnails.status(), StatusCode::OK);
    let thumbnail_rows = response_json(thumbnails).await;
    let rows = thumbnail_rows
        .as_array()
        .expect("readlist thumbnails payload should be an array");
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("readListId"),
        Some(&Value::String("readlist-1".to_string()))
    );
    assert_eq!(
        rows[0].get("type"),
        Some(&Value::String("USER_UPLOADED".to_string()))
    );
    assert_eq!(rows[0].get("selected"), Some(&Value::Bool(false)));
    assert_eq!(
        rows[0].get("mediaType"),
        Some(&Value::String("image/png".to_string()))
    );
    assert_eq!(
        rows[0].get("fileSize"),
        Some(&json!(image_bytes.len() as i64))
    );
    assert_eq!(rows[0].get("width"), Some(&json!(1)));
    assert_eq!(rows[0].get("height"), Some(&json!(1)));

    let route_thumbnail = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist thumbnail route request should build"),
        )
        .await
        .expect("readlist thumbnail route request should complete");

    assert_eq!(route_thumbnail.status(), StatusCode::OK);
    assert_eq!(
        route_thumbnail
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    assert_eq!(
        route_thumbnail
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("max-age=3600, private")
    );
    let route_thumbnail_body = to_bytes(route_thumbnail.into_body(), usize::MAX)
        .await
        .expect("readlist thumbnail route body should be readable");
    assert_ne!(route_thumbnail_body.as_ref(), image_bytes.as_slice());
    assert_eq!(&route_thumbnail_body[..3], &[0xFF, 0xD8, 0xFF]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_thumbnail_select_returns_accepted_when_thumbnail_is_missing_but_readlist_exists() {
    let paths = new_router_fixture("router-readlist-thumbnail-select-missing-thumbnail").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/readlists/readlist-1/thumbnails/missing-thumbnail/selected")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist missing thumbnail select request should build"),
        )
        .await
        .expect("readlist missing thumbnail select request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_thumbnail_upload_parses_multipart_image_and_selected_flag() {
    let paths = new_router_fixture("router-collection-thumbnail-upload-multipart").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "collection.png", "image/png", false, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections/collection-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("collection thumbnail upload request should build"),
        )
        .await
        .expect("collection thumbnail upload request should complete");

    assert_eq!(upload.status(), StatusCode::OK);
    let payload = response_json(upload).await;
    assert_eq!(
        payload.get("collectionId"),
        Some(&Value::String("collection-1".to_string()))
    );
    assert_eq!(
        payload.get("type"),
        Some(&Value::String("USER_UPLOADED".to_string()))
    );
    assert_eq!(payload.get("selected"), Some(&Value::Bool(false)));
    assert_eq!(
        payload.get("mediaType"),
        Some(&Value::String("image/png".to_string()))
    );
    assert_eq!(
        payload.get("fileSize"),
        Some(&json!(image_bytes.len() as i64))
    );
    assert_eq!(payload.get("width"), Some(&json!(1)));
    assert_eq!(payload.get("height"), Some(&json!(1)));

    let thumbnail_id = payload
        .get("id")
        .and_then(Value::as_str)
        .expect("collection thumbnail upload should return thumbnail id");
    let stored = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/v1/collections/collection-1/thumbnails/{thumbnail_id}"
                ))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collection thumbnail fetch request should build"),
        )
        .await
        .expect("collection thumbnail fetch request should complete");

    assert_eq!(stored.status(), StatusCode::OK);
    assert_eq!(
        stored
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/png")
    );

    let route_thumbnail = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections/collection-1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collection thumbnail route request should build"),
        )
        .await
        .expect("collection thumbnail route request should complete");

    assert_eq!(route_thumbnail.status(), StatusCode::OK);
    assert_eq!(
        route_thumbnail
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    assert_eq!(
        route_thumbnail
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("max-age=3600, private")
    );
    let route_thumbnail_body = to_bytes(route_thumbnail.into_body(), usize::MAX)
        .await
        .expect("collection thumbnail route body should be readable");
    assert_ne!(route_thumbnail_body.as_ref(), image_bytes.as_slice());
    assert_eq!(&route_thumbnail_body[..3], &[0xFF, 0xD8, 0xFF]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_thumbnail_falls_back_to_dynamic_mosaic_when_no_persisted_thumbnail_exists()
{
    let paths = new_router_fixture("router-readlist-thumbnail-mosaic-fallback").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "book.png", "image/png", true, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/book-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("book thumbnail upload request should build"),
        )
        .await
        .expect("book thumbnail upload request should complete");
    assert_eq!(upload.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist thumbnail request should build"),
        )
        .await
        .expect("readlist thumbnail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("max-age=3600, private")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("readlist thumbnail body should be readable");
    assert!(
        !body.is_empty(),
        "readlist mosaic thumbnail should not be empty"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_thumbnails_allow_partially_visible_collection() {
    let paths = new_router_fixture("router-collection-thumbnails-partially-visible").await;
    seed_router_contract_data(&paths).await;
    seed_collection_series_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-1-user",
        "library1@example.org",
        "router-contract-library1-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library1@example.org",
        "router-contract-library1-123",
    )
    .await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "collection.png", "image/png", true, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections/collection-1/thumbnails")
                .header("x-auth-token", &admin_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("partially visible collection thumbnail upload request should build"),
        )
        .await
        .expect("partially visible collection thumbnail upload request should complete");
    assert_eq!(upload.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections/collection-1/thumbnails")
                .header("x-auth-token", &restricted_token)
                .body(Body::empty())
                .expect("partially visible collection thumbnails request should build"),
        )
        .await
        .expect("partially visible collection thumbnails request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload.as_array().map(Vec::len), Some(1));
    assert_eq!(
        payload[0].get("collectionId").and_then(Value::as_str),
        Some("collection-1")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_thumbnail_falls_back_to_dynamic_mosaic_when_no_persisted_thumbnail_exists()
 {
    let paths = new_router_fixture("router-collection-thumbnail-mosaic-fallback").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "series.png", "image/png", true, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/series-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("series thumbnail upload request should build"),
        )
        .await
        .expect("series thumbnail upload request should complete");
    assert_eq!(upload.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections/collection-1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collection thumbnail request should build"),
        )
        .await
        .expect("collection thumbnail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("max-age=3600, private")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("collection thumbnail body should be readable");
    assert!(
        !body.is_empty(),
        "collection mosaic thumbnail should not be empty"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_and_collection_media_assets_hide_age_restricted_content() {
    let paths = new_router_fixture("router-readlist-collection-media-assets-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
        &["USER", "FILE_DOWNLOAD"],
    )
    .await;
    write_router_epub_resource(
        &paths,
        "books/book-1.epub",
        "OEBPS/chapter.xhtml",
        br#"<html xmlns='http://www.w3.org/1999/xhtml'><body>Restricted</body></html>"#,
    );

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;
    let image_bytes = fixture_png_bytes();

    let (readlist_content_type, readlist_body) =
        multipart_image_upload_body("file", "readlist.png", "image/png", true, &image_bytes);
    let readlist_upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists/readlist-1/thumbnails")
                .header("x-auth-token", &admin_token)
                .header(header::CONTENT_TYPE, readlist_content_type)
                .body(Body::from(readlist_body))
                .expect("restricted readlist thumbnail upload request should build"),
        )
        .await
        .expect("restricted readlist thumbnail upload request should complete");
    assert_eq!(readlist_upload.status(), StatusCode::OK);
    let readlist_thumbnail_id = response_json(readlist_upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("restricted readlist upload should return thumbnail id")
        .to_string();

    let (collection_content_type, collection_body) =
        multipart_image_upload_body("file", "collection.png", "image/png", true, &image_bytes);
    let collection_upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections/collection-1/thumbnails")
                .header("x-auth-token", &admin_token)
                .header(header::CONTENT_TYPE, collection_content_type)
                .body(Body::from(collection_body))
                .expect("restricted collection thumbnail upload request should build"),
        )
        .await
        .expect("restricted collection thumbnail upload request should complete");
    assert_eq!(collection_upload.status(), StatusCode::OK);
    let collection_thumbnail_id = response_json(collection_upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("restricted collection upload should return thumbnail id")
        .to_string();

    for route in [
        "/api/v1/readlists/readlist-1/thumbnails",
        "/api/v1/readlists/readlist-1/file",
        "/api/v1/collections/collection-1/thumbnails",
        "/api/v1/collections/collection-1/thumbnail",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &restricted_token)
                    .body(Body::empty())
                    .expect("restricted readlist/collection get request should build"),
            )
            .await
            .expect("restricted readlist/collection get request should complete");

        assert_eq!(response.status(), StatusCode::NOT_FOUND, "route: {route}");
    }

    for route in [
        format!("/api/v1/readlists/readlist-1/thumbnails/{readlist_thumbnail_id}"),
        format!("/api/v1/collections/collection-1/thumbnails/{collection_thumbnail_id}"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route.as_str())
                    .header("x-auth-token", &restricted_token)
                    .body(Body::empty())
                    .expect("restricted readlist/collection by-id request should build"),
            )
            .await
            .expect("restricted readlist/collection by-id request should complete");

        assert_eq!(response.status(), StatusCode::NOT_FOUND, "route: {}", route);
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_detail_filters_books_for_partially_restricted_user() {
    let paths = new_router_fixture("router-readlist-detail-partially-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "partially-restricted-user",
        "partial@example.org",
        "router-contract-partial-123",
        15,
        &["USER", "FILE_DOWNLOAD"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "partial@example.org",
        "router-contract-partial-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1")
                .header("x-auth-token", &restricted_token)
                .body(Body::empty())
                .expect("partially restricted readlist detail request should build"),
        )
        .await
        .expect("partially restricted readlist detail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload.get("filtered"), Some(&Value::Bool(true)));
    assert_eq!(payload.get("bookIds"), Some(&json!(["book-3"])));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_media_assets_allow_partially_visible_restricted_readlist() {
    let paths = new_router_fixture("router-readlist-media-assets-partially-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "partially-restricted-user",
        "partial@example.org",
        "router-contract-partial-123",
        15,
        &["USER", "FILE_DOWNLOAD"],
    )
    .await;
    for (relative_path, chapter) in [
        ("books/book-1.epub", "book-1"),
        ("books/book-2.epub", "book-2"),
        ("library-2/books/book-3.epub", "book-3"),
    ] {
        write_router_epub_resource(
            &paths,
            relative_path,
            "OEBPS/chapter.xhtml",
            format!(
                "<html xmlns='http://www.w3.org/1999/xhtml'><body>{chapter}</body></html>"
            )
            .as_bytes(),
        );
    }

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "partial@example.org",
        "router-contract-partial-123",
    )
    .await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "readlist.png", "image/png", true, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists/readlist-1/thumbnails")
                .header("x-auth-token", &admin_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("partially restricted readlist thumbnail upload request should build"),
        )
        .await
        .expect("partially restricted readlist thumbnail upload request should complete");
    assert_eq!(upload.status(), StatusCode::OK);

    let thumbnails = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/thumbnails")
                .header("x-auth-token", &restricted_token)
                .body(Body::empty())
                .expect("partially restricted readlist thumbnails request should build"),
        )
        .await
        .expect("partially restricted readlist thumbnails request should complete");

    assert_eq!(thumbnails.status(), StatusCode::OK);
    let thumbnails_payload = response_json(thumbnails).await;
    assert_eq!(
        thumbnails_payload.as_array().map(Vec::len),
        Some(1),
        "partially visible readlist should still expose its thumbnail list"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_read_list_id_ops_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-read-list-id").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let read_list_is_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "ReadListId", "operator": "is", "value": "readlist-1"}})
                        .to_string(),
                ))
                .expect("strict books/list read-list is match request should build"),
        )
        .await
        .expect("strict books/list read-list is match request should complete");
    assert_eq!(read_list_is_match.status(), StatusCode::OK);
    let read_list_is_match_payload = response_json(read_list_is_match).await;
    let read_list_is_match_content = read_list_is_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books read-list is match payload should expose content array");
    assert_eq!(read_list_is_match_content.len(), 1);

    let read_list_is_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "ReadListId", "operator": "is", "value": "missing-readlist"}})
                        .to_string(),
                ))
                .expect("strict books/list read-list is miss request should build"),
        )
        .await
        .expect("strict books/list read-list is miss request should complete");
    assert_eq!(read_list_is_miss.status(), StatusCode::OK);
    let read_list_is_miss_payload = response_json(read_list_is_miss).await;
    let read_list_is_miss_content = read_list_is_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books read-list is miss payload should expose content array");
    assert_eq!(read_list_is_miss_content.len(), 0);

    let read_list_is_not_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "ReadListId", "operator": "isNot", "value": "missing-readlist"}})
                        .to_string(),
                ))
                .expect("strict books/list read-list isNot match request should build"),
        )
        .await
        .expect("strict books/list read-list isNot match request should complete");
    assert_eq!(read_list_is_not_match.status(), StatusCode::OK);
    let read_list_is_not_match_payload = response_json(read_list_is_not_match).await;
    let read_list_is_not_match_content = read_list_is_not_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books read-list isNot match payload should expose content array");
    assert_eq!(read_list_is_not_match_content.len(), 1);

    let read_list_is_not_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "ReadListId", "operator": "isNot", "value": "readlist-1"}})
                        .to_string(),
                ))
                .expect("strict books/list read-list isNot miss request should build"),
        )
        .await
        .expect("strict books/list read-list isNot miss request should complete");
    assert_eq!(read_list_is_not_miss.status(), StatusCode::OK);
    let read_list_is_not_miss_payload = response_json(read_list_is_not_miss).await;
    let read_list_is_not_miss_content = read_list_is_not_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books read-list isNot miss payload should expose content array");
    assert_eq!(read_list_is_not_miss_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_combined_read_list_id_filters_in_runtime_owned_mode()
{
    let paths =
        new_router_fixture("router-discovery-books-list-strict-read-list-id-combined").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let included_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AllOfBook",
                            "conditions": [
                                {"type": "ReadListId", "operator": "is", "value": "readlist-1"},
                                {"type": "ReadListId", "operator": "isNot", "value": "missing-readlist"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list combined read-list include request should build"),
        )
        .await
        .expect("strict books/list combined read-list include request should complete");
    assert_eq!(included_response.status(), StatusCode::OK);
    let included_payload = response_json(included_response).await;
    let included_content = included_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books combined read-list include payload should expose content array");
    assert_eq!(included_content.len(), 1);

    let excluded_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AllOfBook",
                            "conditions": [
                                {"type": "ReadListId", "operator": "is", "value": "readlist-1"},
                                {"type": "ReadListId", "operator": "isNot", "value": "readlist-1"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list combined read-list exclude request should build"),
        )
        .await
        .expect("strict books/list combined read-list exclude request should complete");
    assert_eq!(excluded_response.status(), StatusCode::OK);
    let excluded_payload = response_json(excluded_response).await;
    let excluded_content = excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books combined read-list exclude payload should expose content array");
    assert_eq!(excluded_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_referential_facets_support_repeated_library_id() {
    let paths = new_router_fixture("router-referential-facets-repeated-library-id").await;
    seed_router_contract_data(&paths).await;
    seed_facet_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let cases = [
        (
            "/api/v1/genres?library_id=library-1&library_id=library-2",
            json!(["Drama", "SciFi"]),
        ),
        (
            "/api/v1/tags?library_id=library-1&library_id=library-2",
            json!([
                "Favorite",
                "favorite-tag",
                "other-book-tag",
                "other-series-tag"
            ]),
        ),
        (
            "/api/v1/tags/series?library_id=library-1&library_id=library-2",
            json!(["Favorite", "other-series-tag"]),
        ),
        (
            "/api/v1/languages?library_id=library-1&library_id=library-2",
            json!(["EN", "FR"]),
        ),
        (
            "/api/v1/publishers?library_id=library-1&library_id=library-2",
            json!(["OtherPub", "PubHouse"]),
        ),
        (
            "/api/v1/age-ratings?library_id=library-1&library_id=library-2",
            json!([12, 16]),
        ),
        (
            "/api/v1/sharing-labels?library_id=library-1&library_id=library-2",
            json!(["Family", "Friends"]),
        ),
        (
            "/api/v1/series/release-dates?library_id=library-1&library_id=library-2",
            json!(["2024-01-15", "2025-02-20"]),
        ),
    ];

    for (route, expected) in cases {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("facet repeated-library request should build"),
            )
            .await
            .expect("facet repeated-library request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let payload = response_json(response).await;
        assert_eq!(payload, expected, "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_referential_facets_support_collection_id_scope() {
    let paths = new_router_fixture("router-referential-facets-collection-scope").await;
    seed_router_contract_data(&paths).await;
    seed_facet_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let cases = [
        (
            "/api/v1/genres?collection_id=collection-1",
            json!(["SciFi"]),
        ),
        (
            "/api/v1/tags?collection_id=collection-1",
            json!(["Favorite", "favorite-tag"]),
        ),
        (
            "/api/v1/tags/series?collection_id=collection-1",
            json!(["Favorite"]),
        ),
        (
            "/api/v1/languages?collection_id=collection-1",
            json!(["EN"]),
        ),
        (
            "/api/v1/publishers?collection_id=collection-1",
            json!(["PubHouse"]),
        ),
        (
            "/api/v1/age-ratings?collection_id=collection-1",
            json!([16]),
        ),
        (
            "/api/v1/sharing-labels?collection_id=collection-1",
            json!(["Family"]),
        ),
        (
            "/api/v1/series/release-dates?collection_id=collection-1",
            json!(["2024-01-15"]),
        ),
    ];

    for (route, expected) in cases {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("facet collection request should build"),
            )
            .await
            .expect("facet collection request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let payload = response_json(response).await;
        assert_eq!(payload, expected, "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_books_returns_paginated_content_and_library_filter() {
    let paths = new_router_fixture("router-readlist-books-paging-filter").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let paged_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books?page=1&size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist books paged request should build"),
        )
        .await
        .expect("readlist books paged request should complete");

    assert_eq!(paged_response.status(), StatusCode::OK);
    let paged_payload = response_json(paged_response).await;
    let paged_content = paged_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlist books paged payload should expose content array");
    assert_eq!(paged_content.len(), 1);
    assert_eq!(
        paged_content[0].get("id").and_then(Value::as_str),
        Some("book-2")
    );
    assert_eq!(
        paged_content[0].get("seriesTitle").and_then(Value::as_str),
        Some("Series 2"),
    );
    assert_eq!(
        paged_content[0].get("libraryId").and_then(Value::as_str),
        Some("library-1"),
    );
    assert_eq!(
        paged_content[0]
            .get("metadata")
            .and_then(|metadata| metadata.get("title"))
            .and_then(Value::as_str),
        Some("Book 2"),
    );
    assert_eq!(
        paged_payload.get("totalElements").and_then(Value::as_u64),
        Some(3)
    );

    let filtered_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books?library_id=library-1&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist books filtered request should build"),
        )
        .await
        .expect("readlist books filtered request should complete");

    assert_eq!(filtered_response.status(), StatusCode::OK);
    let filtered_payload = response_json(filtered_response).await;
    let filtered_content = filtered_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("readlist books filtered payload should expose content array");
    assert_eq!(filtered_content.len(), 2);
    assert_eq!(
        filtered_content[0].get("id").and_then(Value::as_str),
        Some("book-1")
    );
    assert_eq!(
        filtered_content[1].get("id").and_then(Value::as_str),
        Some("book-2")
    );
    assert_eq!(
        filtered_payload
            .get("pageable")
            .and_then(|pageable| pageable.get("unpaged"))
            .and_then(Value::as_bool),
        Some(true),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_tags_supports_readlist_scope() {
    let paths = new_router_fixture("router-book-tags-readlist-scope").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/tags/book?readlist_id=readlist-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book tags readlist scope request should build"),
        )
        .await
        .expect("book tags readlist scope request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload,
        json!(["favorite-tag", "library-one-tag", "library-two-tag"])
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_tags_supports_repeated_library_id_query() {
    let paths = new_router_fixture("router-book-tags-repeated-library-id").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/tags/book?library_id=library-1&library_id=library-2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book tags repeated library_id request should build"),
        )
        .await
        .expect("book tags repeated library_id request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload,
        json!(["favorite-tag", "library-one-tag", "library-two-tag"])
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_books_and_siblings_follow_release_date_when_unordered() {
    let paths = new_router_fixture("router-readlist-unordered-release-date").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    mark_readlist_unordered(&paths, "readlist-1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let books = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books?unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("unordered readlist books request should build"),
        )
        .await
        .expect("unordered readlist books request should complete");
    assert_eq!(books.status(), StatusCode::OK);
    let books_payload = response_json(books).await;
    let book_ids = books_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("unordered readlist books payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(book_ids, vec!["book-1", "book-2", "book-3"]);

    let previous = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books/book-2/previous")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("unordered previous request should build"),
        )
        .await
        .expect("unordered previous request should complete");
    assert_eq!(previous.status(), StatusCode::OK);
    let previous_payload = response_json(previous).await;
    assert_eq!(
        previous_payload.get("id").and_then(Value::as_str),
        Some("book-1")
    );

    let next = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books/book-2/next")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("unordered next request should build"),
        )
        .await
        .expect("unordered next request should complete");
    assert_eq!(next.status(), StatusCode::OK);
    let next_payload = response_json(next).await;
    assert_eq!(
        next_payload.get("id").and_then(Value::as_str),
        Some("book-3")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_books_author_filter_requires_matching_role() {
    let paths = new_router_fixture("router-readlist-author-role-filter").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let matching = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books?author=Jane+Writer,writer&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("author filter request should build"),
        )
        .await
        .expect("author filter request should complete");
    assert_eq!(matching.status(), StatusCode::OK);
    let matching_payload = response_json(matching).await;
    let matching_ids = matching_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("matching author payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(matching_ids, vec!["book-1", "book-2"]);

    let mismatching = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books?author=Jane+Writer,penciller&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("mismatching author filter request should build"),
        )
        .await
        .expect("mismatching author filter request should complete");
    assert_eq!(mismatching.status(), StatusCode::OK);
    let mismatching_payload = response_json(mismatching).await;
    let mismatching_ids = mismatching_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("mismatching author payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert!(mismatching_ids.is_empty());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_books_author_filter_accepts_empty_role_like_kotlin() {
    let paths = new_router_fixture("router-readlist-author-empty-role-filter").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_readlist_author_edge_case(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books?author=Doe,+John,&library_id=library-2&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("empty-role author filter request should build"),
        )
        .await
        .expect("empty-role author filter request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("empty-role author filter payload should expose content array");
    let ids = content
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["book-3"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_books_author_filter_ignores_bare_name_like_kotlin_http_query() {
    let paths = new_router_fixture("router-readlist-author-bare-name-ignored").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_readlist_author_edge_case(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books?author=Jane+Writer&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("bare-name author request should build"),
        )
        .await
        .expect("bare-name author request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("bare-name author payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["book-1", "book-2", "book-3"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_books_preserves_blank_and_comma_author_roles() {
    let paths = new_router_fixture("router-readlist-author-payload-fidelity").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;
    seed_readlist_author_edge_case(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books?library_id=library-2&unpaged=true")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist author fidelity request should build"),
        )
        .await
        .expect("readlist author fidelity request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let authors = payload
        .get("content")
        .and_then(Value::as_array)
        .and_then(|content| content.first())
        .and_then(|book| book.get("metadata"))
        .and_then(|metadata| metadata.get("authors"))
        .and_then(Value::as_array)
        .expect("readlist author fidelity payload should expose authors array");

    assert!(authors.iter().any(|author| {
        author.get("name").and_then(Value::as_str) == Some("Doe, John")
            && author.get("role").and_then(Value::as_str) == Some("")
    }));
    assert!(authors.iter().any(|author| {
        author.get("name").and_then(Value::as_str) == Some("Casey Role")
            && author.get("role").and_then(Value::as_str) == Some("CoWriter")
    }));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_book_siblings_follow_readlist_order() {
    let paths = new_router_fixture("router-readlist-book-siblings").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let previous = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books/book-2/previous")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist previous request should build"),
        )
        .await
        .expect("readlist previous request should complete");
    assert_eq!(previous.status(), StatusCode::OK);
    let previous_payload = response_json(previous).await;
    assert_eq!(
        previous_payload.get("id").and_then(Value::as_str),
        Some("book-1")
    );

    let next = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books/book-2/next")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist next request should build"),
        )
        .await
        .expect("readlist next request should complete");
    assert_eq!(next.status(), StatusCode::OK);
    let next_payload = response_json(next).await;
    assert_eq!(
        next_payload.get("id").and_then(Value::as_str),
        Some("book-3")
    );

    let missing = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/books/book-1/previous")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist missing previous request should build"),
        )
        .await
        .expect("readlist missing previous request should complete");
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_match_comicrack_rejects_invalid_xml_and_reports_matches() {
    let paths = new_router_fixture("router-readlist-match-comicrack").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let (invalid_content_type, invalid_body) = comicrack_multipart_body("<ReadingList>");
    let invalid = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists/match/comicrack")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, invalid_content_type)
                .body(Body::from(invalid_body))
                .expect("invalid comicrack request should build"),
        )
        .await
        .expect("invalid comicrack request should complete");
    assert_eq!(invalid.status(), StatusCode::BAD_REQUEST);
    let invalid_payload = response_json(invalid).await;
    assert_eq!(
        invalid_payload.get("errorCode").and_then(Value::as_str),
        Some("ERR_1015")
    );

    let xml = r#"<ReadingList><Name>ReadList 1</Name><Books><Book Series="Series 2" Number="002" /></Books></ReadingList>"#;
    let (valid_content_type, valid_body) = comicrack_multipart_body_with_quoted_boundary(xml);
    let valid = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists/match/comicrack")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, valid_content_type)
                .body(Body::from(valid_body))
                .expect("valid comicrack request should build"),
        )
        .await
        .expect("valid comicrack request should complete");
    assert_eq!(valid.status(), StatusCode::OK);
    let valid_payload = response_json(valid).await;
    assert_eq!(
        valid_payload
            .get("readListMatch")
            .and_then(|it| it.get("name"))
            .and_then(Value::as_str),
        Some("ReadList 1"),
    );
    assert_eq!(
        valid_payload
            .get("readListMatch")
            .and_then(|it| it.get("errorCode"))
            .and_then(Value::as_str),
        Some("ERR_1009"),
    );
    let requests = valid_payload
        .get("requests")
        .and_then(Value::as_array)
        .expect("valid comicrack payload should expose requests array");
    assert_eq!(requests.len(), 1);
    let matches = requests[0]
        .get("matches")
        .and_then(Value::as_array)
        .expect("valid comicrack request should expose matches array");
    assert_eq!(matches.len(), 1);
    assert_eq!(
        matches[0]
            .get("series")
            .and_then(|series| series.get("releaseDate"))
            .and_then(Value::as_str),
        Some("2024-01-01"),
    );
    assert_eq!(
        matches[0]
            .get("books")
            .and_then(Value::as_array)
            .and_then(|books| books.first())
            .and_then(|book| book.get("bookId"))
            .and_then(Value::as_str),
        Some("book-2"),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_metadata_and_collection_filters_in_runtime_owned_mode()
 {
    let paths =
        new_router_fixture("router-discovery-series-list-strict-metadata-and-collection").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let genre_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Genre", "operator": "contains", "value": "Sci"}})
                        .to_string(),
                ))
                .expect("strict series/list genre match request should build"),
        )
        .await
        .expect("strict series/list genre match request should complete");
    assert_eq!(genre_match.status(), StatusCode::OK);
    let genre_match_payload = response_json(genre_match).await;
    let genre_match_content = genre_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series genre match payload should expose content array");
    assert_eq!(genre_match_content.len(), 1);

    let genre_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Genre", "operator": "contains", "value": "Drama"}})
                        .to_string(),
                ))
                .expect("strict series/list genre miss request should build"),
        )
        .await
        .expect("strict series/list genre miss request should complete");
    assert_eq!(genre_miss.status(), StatusCode::OK);
    let genre_miss_payload = response_json(genre_miss).await;
    let genre_miss_content = genre_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series genre miss payload should expose content array");
    assert_eq!(genre_miss_content.len(), 0);

    let collection_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "CollectionId", "operator": "is", "value": "collection-1"}})
                        .to_string(),
                ))
                .expect("strict series/list collection match request should build"),
        )
        .await
        .expect("strict series/list collection match request should complete");
    assert_eq!(collection_match.status(), StatusCode::OK);
    let collection_match_payload = response_json(collection_match).await;
    let collection_match_content = collection_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series collection match payload should expose content array");
    assert_eq!(collection_match_content.len(), 1);

    let collection_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "CollectionId", "operator": "is", "value": "collection-missing"}})
                        .to_string(),
                ))
                .expect("strict series/list collection miss request should build"),
        )
        .await
        .expect("strict series/list collection miss request should complete");
    assert_eq!(collection_miss.status(), StatusCode::OK);
    let collection_miss_payload = response_json(collection_miss).await;
    let collection_miss_content = collection_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series collection miss payload should expose content array");
    assert_eq!(collection_miss_content.len(), 0);

    let language_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Language", "operator": "is", "value": "en"}})
                        .to_string(),
                ))
                .expect("strict series/list language match request should build"),
        )
        .await
        .expect("strict series/list language match request should complete");
    assert_eq!(language_match.status(), StatusCode::OK);
    let language_match_payload = response_json(language_match).await;
    let language_match_content = language_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series language match payload should expose content array");
    assert_eq!(language_match_content.len(), 1);

    let language_is_not_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Language", "operator": "isNot", "value": "en"}})
                        .to_string(),
                ))
                .expect("strict series/list language isNot excluded request should build"),
        )
        .await
        .expect("strict series/list language isNot excluded request should complete");
    assert_eq!(language_is_not_excluded.status(), StatusCode::OK);
    let language_is_not_excluded_payload = response_json(language_is_not_excluded).await;
    let language_is_not_excluded_content = language_is_not_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series language isNot excluded payload should expose content array");
    assert_eq!(language_is_not_excluded_content.len(), 0);

    let language_is_not_kept = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Language", "operator": "isNot", "value": "fr"}})
                        .to_string(),
                ))
                .expect("strict series/list language isNot kept request should build"),
        )
        .await
        .expect("strict series/list language isNot kept request should complete");
    assert_eq!(language_is_not_kept.status(), StatusCode::OK);
    let language_is_not_kept_payload = response_json(language_is_not_kept).await;
    let language_is_not_kept_content = language_is_not_kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series language isNot kept payload should expose content array");
    assert_eq!(language_is_not_kept_content.len(), 1);

    let publisher_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Publisher", "operator": "is", "value": "PubHouse"}})
                        .to_string(),
                ))
                .expect("strict series/list publisher match request should build"),
        )
        .await
        .expect("strict series/list publisher match request should complete");
    assert_eq!(publisher_match.status(), StatusCode::OK);
    let publisher_match_payload = response_json(publisher_match).await;
    let publisher_match_content = publisher_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series publisher match payload should expose content array");
    assert_eq!(publisher_match_content.len(), 1);

    let publisher_is_not_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Publisher", "operator": "isNot", "value": "PubHouse"}})
                        .to_string(),
                ))
                .expect("strict series/list publisher isNot excluded request should build"),
        )
        .await
        .expect("strict series/list publisher isNot excluded request should complete");
    assert_eq!(publisher_is_not_excluded.status(), StatusCode::OK);
    let publisher_is_not_excluded_payload = response_json(publisher_is_not_excluded).await;
    let publisher_is_not_excluded_content = publisher_is_not_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series publisher isNot excluded payload should expose content array");
    assert_eq!(publisher_is_not_excluded_content.len(), 0);

    let publisher_is_not_kept = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Publisher", "operator": "isNot", "value": "OtherPub"}})
                        .to_string(),
                ))
                .expect("strict series/list publisher isNot kept request should build"),
        )
        .await
        .expect("strict series/list publisher isNot kept request should complete");
    assert_eq!(publisher_is_not_kept.status(), StatusCode::OK);
    let publisher_is_not_kept_payload = response_json(publisher_is_not_kept).await;
    let publisher_is_not_kept_content = publisher_is_not_kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series publisher isNot kept payload should expose content array");
    assert_eq!(publisher_is_not_kept_content.len(), 1);

    let age_rating_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "AgeRating", "operator": "is", "value": 16}})
                        .to_string(),
                ))
                .expect("strict series/list age-rating match request should build"),
        )
        .await
        .expect("strict series/list age-rating match request should complete");
    assert_eq!(age_rating_match.status(), StatusCode::OK);
    let age_rating_match_payload = response_json(age_rating_match).await;
    let age_rating_match_content = age_rating_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series age-rating match payload should expose content array");
    assert_eq!(age_rating_match_content.len(), 1);

    for (operator, body, expected_count) in [
        (
            "isNot",
            json!({"condition": {"type": "AgeRating", "operator": "isNot", "value": 16}}),
            0_usize,
        ),
        (
            "isNot",
            json!({"condition": {"type": "AgeRating", "operator": "isNot", "value": 18}}),
            1_usize,
        ),
        (
            "greaterThan",
            json!({"condition": {"type": "AgeRating", "operator": "greaterThan", "value": 15}}),
            1_usize,
        ),
        (
            "greaterThan",
            json!({"condition": {"type": "AgeRating", "operator": "greaterThan", "value": 16}}),
            0_usize,
        ),
        (
            "lessThan",
            json!({"condition": {"type": "AgeRating", "operator": "lessThan", "value": 17}}),
            1_usize,
        ),
        (
            "lessThan",
            json!({"condition": {"type": "AgeRating", "operator": "lessThan", "value": 16}}),
            0_usize,
        ),
        (
            "isNull",
            json!({"condition": {"type": "AgeRating", "operator": "isNull"}}),
            0_usize,
        ),
        (
            "isNotNull",
            json!({"condition": {"type": "AgeRating", "operator": "isNotNull"}}),
            1_usize,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/series/list?page=0&size=20")
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("strict series/list age-rating operator request should build"),
            )
            .await
            .expect("strict series/list age-rating operator request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("strict series age-rating operator payload should expose content array");
        assert_eq!(
            content.len(),
            expected_count,
            "unexpected strict series age-rating count for operator={operator}",
        );
    }

    let sharing_label_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "SharingLabel", "operator": "contains", "value": "fam"}})
                        .to_string(),
                ))
                .expect("strict series/list sharing-label match request should build"),
        )
        .await
        .expect("strict series/list sharing-label match request should complete");
    assert_eq!(sharing_label_match.status(), StatusCode::OK);
    let sharing_label_match_payload = response_json(sharing_label_match).await;
    let sharing_label_match_content = sharing_label_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series sharing-label match payload should expose content array");
    assert_eq!(sharing_label_match_content.len(), 1);

    for (condition_type, operator, expected_count) in [
        ("Tag", "isNot", 0_usize),
        ("Tag", "isNull", 0_usize),
        ("Tag", "isNotNull", 1_usize),
        ("Genre", "isNot", 0_usize),
        ("Genre", "isNull", 0_usize),
        ("Genre", "isNotNull", 1_usize),
        ("SharingLabel", "isNot", 0_usize),
        ("SharingLabel", "isNull", 0_usize),
        ("SharingLabel", "isNotNull", 1_usize),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/series/list?page=0&size=20")
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(if operator == "isNot" {
                        json!({
                            "condition": {
                                "type": condition_type,
                                "operator": operator,
                                "value": match condition_type {
                                    "Tag" => "Favorite",
                                    "Genre" => "SciFi",
                                    _ => "Family",
                                }
                            }
                        })
                        .to_string()
                    } else {
                        json!({
                            "condition": {
                                "type": condition_type,
                                "operator": operator,
                            }
                        })
                        .to_string()
                    }))
                    .expect("strict series/list nullable string-op request should build"),
            )
            .await
            .expect("strict series/list nullable string-op request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("strict series nullable string-op payload should expose content array");
        assert_eq!(
            content.len(),
            expected_count,
            "unexpected series nullable result for type={condition_type}, operator={operator}",
        );
    }

    let author_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Author", "operator": "contains", "value": "john"}})
                        .to_string(),
                ))
                .expect("strict series/list author match request should build"),
        )
        .await
        .expect("strict series/list author match request should complete");
    assert_eq!(author_match.status(), StatusCode::OK);
    let author_match_payload = response_json(author_match).await;
    let author_match_content = author_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series author match payload should expose content array");
    assert_eq!(author_match_content.len(), 1);

    let author_role_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Author",
                            "operator": "is",
                            "value": {
                                "name": "John Doe",
                                "role": "writer"
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list author role match request should build"),
        )
        .await
        .expect("strict series/list author role match request should complete");
    assert_eq!(author_role_match.status(), StatusCode::OK);
    let author_role_match_payload = response_json(author_role_match).await;
    let author_role_match_content = author_role_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series author role match payload should expose content array");
    assert_eq!(author_role_match_content.len(), 1);

    let author_role_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Author",
                            "operator": "is",
                            "value": {
                                "name": "John Doe",
                                "role": "editor"
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list author role miss request should build"),
        )
        .await
        .expect("strict series/list author role miss request should complete");
    assert_eq!(author_role_miss.status(), StatusCode::OK);
    let author_role_miss_payload = response_json(author_role_miss).await;
    let author_role_miss_content = author_role_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series author role miss payload should expose content array");
    assert_eq!(author_role_miss_content.len(), 0);

    cleanup_router_fixture(paths);
}
