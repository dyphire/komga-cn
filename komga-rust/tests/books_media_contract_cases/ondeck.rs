use super::*;
use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};

async fn insert_ondeck_read_progress(
    paths: &RuntimeDbPaths,
    user_id: &str,
    book_id: &str,
    page: i64,
    completed: bool,
    read_date: &str,
) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("ondeck test db should open for read progress seed");

    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(user_id)
    .bind(page)
    .bind(completed)
    .bind(read_date)
    .execute(&pool)
    .await
    .expect("ondeck test read progress row should insert");

    pool.close().await;
}

async fn insert_ondeck_book(
    paths: &RuntimeDbPaths,
    book_id: &str,
    series_id: &str,
    library_id: &str,
    number: i64,
    title: &str,
) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("ondeck test db should open for book insert");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(format!("{book_id}.epub"))
    .bind(format!("books/{book_id}.epub"))
    .bind(series_id)
    .bind(2_048_i64)
    .bind(number)
    .bind(library_id)
    .execute(&pool)
    .await
    .expect("ondeck test book row should insert");

    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind(book_id)
        .bind(10_i64)
        .execute(&pool)
        .await
        .expect("ondeck test media row should insert");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(number.to_string())
    .bind(number as f64)
    .bind(title)
    .bind("2024-02-01")
    .bind(book_id)
    .execute(&pool)
    .await
    .expect("ondeck test metadata row should insert");

    pool.close().await;
}

async fn update_ondeck_series_book_count(paths: &RuntimeDbPaths, series_id: &str, book_count: i64) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("ondeck test db should open for series count update");

    sqlx::query("UPDATE SERIES SET BOOK_COUNT = ? WHERE ID = ?")
        .bind(book_count)
        .bind(series_id)
        .execute(&pool)
        .await
        .expect("ondeck test series count should update");

    sqlx::query("UPDATE SERIES_METADATA SET TOTAL_BOOK_COUNT = ? WHERE SERIES_ID = ?")
        .bind(book_count)
        .bind(series_id)
        .execute(&pool)
        .await
        .expect("ondeck test total book count should update");

    pool.close().await;
}

async fn seed_ondeck_series_progress(
    paths: &RuntimeDbPaths,
    user_id: &str,
    series_id: &str,
    read_count: i64,
    in_progress_count: i64,
    most_recent_read_date: &str,
) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("ondeck test db should open for series progress seed");

    sqlx::query(
        "INSERT INTO READ_PROGRESS_SERIES (SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT, MOST_RECENT_READ_DATE) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(series_id)
    .bind(user_id)
    .bind(read_count)
    .bind(in_progress_count)
    .bind(most_recent_read_date)
    .execute(&pool)
    .await
    .expect("ondeck test series progress row should insert");

    pool.close().await;
}

async fn seed_ondeck_progress(
    paths: &RuntimeDbPaths,
    user_id: &str,
    series_id: &str,
    completed_book_id: &str,
    most_recent_read_date: &str,
) {
    insert_ondeck_read_progress(
        paths,
        user_id,
        completed_book_id,
        10_i64,
        true,
        most_recent_read_date,
    )
    .await;
    seed_ondeck_series_progress(
        paths,
        user_id,
        series_id,
        1_i64,
        0_i64,
        most_recent_read_date,
    )
    .await;
}

#[allow(clippy::too_many_arguments)]
async fn seed_ondeck_user(
    paths: &RuntimeDbPaths,
    user_id: &str,
    email: &str,
    password: &str,
    shared_all_libraries: bool,
    age_restriction: Option<i64>,
    age_restriction_allow_only: bool,
    shared_library_ids: &[&str],
    labels_allow: &[&str],
    labels_exclude: &[&str],
) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("ondeck test user db should open");

    let hashed_password =
        hash_bcrypt_password(password, DEFAULT_COST).expect("bcrypt hash should be computed");

    sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(email)
    .bind(hashed_password)
    .bind(shared_all_libraries)
    .bind(age_restriction)
    .bind(age_restriction_allow_only)
    .execute(&pool)
    .await
    .expect("ondeck test user should insert");

    for role in ["USER", "PAGE_STREAMING"] {
        sqlx::query("INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
            .bind(user_id)
            .bind(role)
            .execute(&pool)
            .await
            .expect("ondeck test user role should insert");
    }

    for library_id in shared_library_ids {
        sqlx::query("INSERT INTO USER_LIBRARY_SHARING (USER_ID, LIBRARY_ID) VALUES (?, ?)")
            .bind(user_id)
            .bind(*library_id)
            .execute(&pool)
            .await
            .expect("ondeck test user library sharing should insert");
    }

    for label in labels_allow {
        sqlx::query("INSERT INTO USER_SHARING (LABEL, ALLOW, USER_ID) VALUES (?, ?, ?)")
            .bind(*label)
            .bind(true)
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("ondeck test user allowed label should insert");
    }

    for label in labels_exclude {
        sqlx::query("INSERT INTO USER_SHARING (LABEL, ALLOW, USER_ID) VALUES (?, ?, ?)")
            .bind(*label)
            .bind(false)
            .bind(user_id)
            .execute(&pool)
            .await
            .expect("ondeck test user excluded label should insert");
    }

    pool.close().await;
}

async fn kotlin_equivalent_ondeck_ids(paths: &RuntimeDbPaths, user_id: &str) -> Vec<String> {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("ondeck kotlin-equivalent db should open");

    let rows = sqlx::query(
        "WITH cte_series AS MATERIALIZED ( \
             SELECT s.ID AS SERIES_ID, rps.MOST_RECENT_READ_DATE AS MOST_RECENT_READ_DATE \
             FROM SERIES s \
             JOIN READ_PROGRESS_SERIES rps ON s.ID = rps.SERIES_ID AND rps.USER_ID = ? \
             JOIN SERIES_METADATA sd ON s.ID = sd.SERIES_ID \
             WHERE rps.IN_PROGRESS_COUNT = 0 \
             AND rps.READ_COUNT != s.BOOK_COUNT \
         ), \
         cte_books AS MATERIALIZED ( \
             SELECT b.ID AS BOOK_ID, b.SERIES_ID AS SERIES_ID, bm.NUMBER_SORT AS NUMBER_SORT \
             FROM BOOK b \
             JOIN BOOK_METADATA bm ON b.ID = bm.BOOK_ID \
             LEFT JOIN READ_PROGRESS r ON b.ID = r.BOOK_ID AND r.USER_ID = ? \
             WHERE r.COMPLETED IS NULL \
             AND b.SERIES_ID IN (SELECT SERIES_ID FROM cte_series) \
         ) \
         SELECT b.ID AS ID \
         FROM cte_series \
         JOIN cte_books b1 ON cte_series.SERIES_ID = b1.SERIES_ID \
         LEFT JOIN cte_books b2 ON b1.SERIES_ID = b2.SERIES_ID \
             AND (b1.NUMBER_SORT > b2.NUMBER_SORT \
                 OR (b1.NUMBER_SORT = b2.NUMBER_SORT AND b1.BOOK_ID > b2.BOOK_ID)) \
         JOIN BOOK b ON b1.BOOK_ID = b.ID \
         JOIN MEDIA m ON b.ID = m.BOOK_ID \
         JOIN BOOK_METADATA d ON b.ID = d.BOOK_ID \
         JOIN SERIES_METADATA sd ON b.SERIES_ID = sd.SERIES_ID \
         LEFT JOIN READ_PROGRESS r ON 0 \
         WHERE b2.BOOK_ID IS NULL \
         ORDER BY cte_series.MOST_RECENT_READ_DATE DESC",
    )
    .bind(user_id)
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .expect("ondeck kotlin-equivalent query should succeed");

    pool.close().await;

    rows.into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect()
}

async fn books_ondeck_response(
    app: &axum::Router,
    auth_token: Option<&str>,
    uri: &str,
) -> axum::response::Response {
    app.clone()
        .oneshot({
            let mut builder = Request::builder().method("GET").uri(uri);
            if let Some(auth_token) = auth_token {
                builder = builder.header("x-auth-token", auth_token);
            }
            builder
                .body(Body::empty())
                .expect("books/ondeck request should build")
        })
        .await
        .expect("books/ondeck request should complete")
}

async fn books_ondeck_ids(app: &axum::Router, auth_token: &str, uri: &str) -> Vec<String> {
    let response = books_ondeck_response(app, Some(auth_token), uri).await;

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    payload
        .get("content")
        .and_then(Value::as_array)
        .expect("books/ondeck payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn router_discovery_books_ondeck_requires_auth() {
    let paths = new_router_fixture("router-discovery-books-ondeck-requires-auth").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let response = books_ondeck_response(&app, None, "/api/v1/books/ondeck").await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_ondeck_returns_representative_book_detail_fields_and_kotlin_unpaged_shape()
 {
    let paths = new_router_fixture("router-discovery-books-ondeck-full-payload").await;
    seed_router_contract_data(&paths).await;
    insert_ondeck_book(&paths, "book-2", "series-1", "library-1", 2, "Book 2").await;
    update_ondeck_series_book_count(&paths, "series-1", 2).await;
    seed_ondeck_progress(
        &paths,
        "admin-user",
        "series-1",
        "book-1",
        "2024-02-01T00:00:00",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response =
        books_ondeck_response(&app, Some(&auth_token), "/api/v1/books/ondeck?unpaged=true").await;

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("books/ondeck payload should expose content array");

    assert_eq!(content.len(), 1);
    let first = &content[0];
    assert_eq!(first.get("id"), Some(&json!("book-2")));
    assert_eq!(first.get("seriesId"), Some(&json!("series-1")));
    assert_eq!(first.get("seriesTitle"), Some(&json!("Series 1")));
    assert_eq!(first.get("libraryId"), Some(&json!("library-1")));
    assert_eq!(first.get("url"), Some(&json!("books/book-2.epub")));
    assert_eq!(first.get("number"), Some(&json!(2)));
    assert_eq!(first.pointer("/media/status"), Some(&json!("READY")));
    assert_eq!(first.pointer("/media/mediaProfile"), Some(&json!("EPUB")));
    assert_eq!(first.pointer("/metadata/title"), Some(&json!("Book 2")));
    assert_eq!(first.pointer("/metadata/numberSort"), Some(&json!(2.0)));
    assert_eq!(
        first.pointer("/metadata/releaseDate"),
        Some(&json!("2024-02-01"))
    );
    assert_eq!(first.get("readProgress"), Some(&Value::Null));
    assert_eq!(payload.pointer("/sort/unsorted"), Some(&json!(true)));
    assert_eq!(
        payload.pointer("/pageable/sort/unsorted"),
        Some(&json!(true))
    );
    assert_eq!(payload.pointer("/pageable/paged"), Some(&json!(true)));
    assert_eq!(payload.pointer("/pageable/unpaged"), Some(&json!(false)));
    assert_eq!(payload.get("size"), Some(&json!(20)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_ondeck_accepts_basic_auth_like_kotlin_clients() {
    let paths = new_router_fixture("router-discovery-books-ondeck-basic-auth-compat").await;
    seed_router_contract_data(&paths).await;
    insert_ondeck_book(&paths, "book-2", "series-1", "library-1", 2, "Book 2").await;
    update_ondeck_series_book_count(&paths, "series-1", 2).await;
    seed_ondeck_progress(
        &paths,
        "admin-user",
        "series-1",
        "book-1",
        "2024-02-01T00:00:00",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/ondeck?unpaged=true")
                .header(
                    header::AUTHORIZATION,
                    basic_authorization_header_value(
                        "admin@example.org",
                        "router-contract-admin-123",
                    ),
                )
                .header("x-auth-token", "")
                .body(Body::empty())
                .expect("books/ondeck basic-auth request should build"),
        )
        .await
        .expect("books/ondeck basic-auth request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_ondeck_honors_label_allow_restriction() {
    let paths = new_router_fixture("router-discovery-books-ondeck-label-allow").await;
    seed_router_contract_data(&paths).await;
    seed_router_contract_nullable_samples(&paths).await;
    insert_ondeck_book(&paths, "book-3", "series-1", "library-1", 3, "Book 3").await;
    insert_ondeck_book(&paths, "book-4", "series-2", "library-1", 4, "Book 4").await;
    update_ondeck_series_book_count(&paths, "series-1", 2).await;
    update_ondeck_series_book_count(&paths, "series-2", 2).await;
    seed_ondeck_user(
        &paths,
        "label-allow-user",
        "label-allow@example.org",
        "router-contract-label-allow-123",
        true,
        None,
        false,
        &[],
        &["family"],
        &[],
    )
    .await;
    seed_ondeck_progress(
        &paths,
        "label-allow-user",
        "series-1",
        "book-1",
        "2024-02-01T00:00:00",
    )
    .await;
    seed_ondeck_progress(
        &paths,
        "label-allow-user",
        "series-2",
        "book-2",
        "2024-02-02T00:00:00",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "label-allow@example.org",
        "router-contract-label-allow-123",
    )
    .await;

    let ids = books_ondeck_ids(&app, &auth_token, "/api/v1/books/ondeck").await;
    assert_eq!(ids, vec!["book-3"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_ondeck_filters_to_requested_library_ids() {
    let paths = new_router_fixture("router-discovery-books-ondeck-library-filter").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    insert_ondeck_book(&paths, "book-4", "series-3", "library-2", 4, "Book 4").await;
    update_ondeck_series_book_count(&paths, "series-3", 2).await;
    seed_ondeck_progress(
        &paths,
        "admin-user",
        "series-3",
        "book-3",
        "2024-02-03T00:00:00",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let ids = books_ondeck_ids(
        &app,
        &auth_token,
        "/api/v1/books/ondeck?library_id=library-1",
    )
    .await;
    assert!(ids.is_empty());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_ondeck_filters_to_authorized_library_intersection() {
    let paths = new_router_fixture("router-discovery-books-ondeck-authorized-library-filter").await;
    seed_router_contract_data(&paths).await;
    insert_ondeck_book(&paths, "book-2", "series-1", "library-1", 2, "Book 2").await;
    update_ondeck_series_book_count(&paths, "series-1", 2).await;
    seed_ondeck_user(
        &paths,
        "library-restricted-user",
        "library-restricted@example.org",
        "router-contract-library-restricted-123",
        false,
        None,
        false,
        &["library-1"],
        &[],
        &[],
    )
    .await;
    seed_ondeck_progress(
        &paths,
        "library-restricted-user",
        "series-1",
        "book-1",
        "2024-02-01T00:00:00",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library-restricted@example.org",
        "router-contract-library-restricted-123",
    )
    .await;

    let forbidden_ids = books_ondeck_ids(
        &app,
        &auth_token,
        "/api/v1/books/ondeck?library_id=library-2",
    )
    .await;
    assert!(forbidden_ids.is_empty());

    let allowed_ids = books_ondeck_ids(
        &app,
        &auth_token,
        "/api/v1/books/ondeck?library_id=library-1",
    )
    .await;
    assert_eq!(allowed_ids, vec!["book-2"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_ondeck_hides_age_restricted_series() {
    let paths = new_router_fixture("router-discovery-books-ondeck-age-restricted").await;
    seed_router_contract_data(&paths).await;
    insert_ondeck_book(&paths, "book-2", "series-1", "library-1", 2, "Book 2").await;
    update_ondeck_series_book_count(&paths, "series-1", 2).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;
    seed_ondeck_progress(
        &paths,
        "restricted-user",
        "series-1",
        "book-1",
        "2024-02-01T00:00:00",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let ids = books_ondeck_ids(&app, &auth_token, "/api/v1/books/ondeck").await;
    assert!(ids.is_empty());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_ondeck_hides_allow_only_age_mismatch_series() {
    let paths = new_router_fixture("router-discovery-books-ondeck-age-allow-only").await;
    seed_router_contract_data(&paths).await;
    insert_ondeck_book(&paths, "book-2", "series-1", "library-1", 2, "Book 2").await;
    update_ondeck_series_book_count(&paths, "series-1", 2).await;
    seed_ondeck_user(
        &paths,
        "allow-only-user",
        "allow-only@example.org",
        "router-contract-allow-only-123",
        true,
        Some(12),
        true,
        &[],
        &[],
        &[],
    )
    .await;
    seed_ondeck_progress(
        &paths,
        "allow-only-user",
        "series-1",
        "book-1",
        "2024-02-01T00:00:00",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "allow-only@example.org",
        "router-contract-allow-only-123",
    )
    .await;

    let ids = books_ondeck_ids(&app, &auth_token, "/api/v1/books/ondeck").await;
    assert!(ids.is_empty());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_ondeck_hides_label_restricted_series() {
    let paths = new_router_fixture("router-discovery-books-ondeck-label-restricted").await;
    seed_router_contract_data(&paths).await;
    insert_ondeck_book(&paths, "book-2", "series-1", "library-1", 2, "Book 2").await;
    update_ondeck_series_book_count(&paths, "series-1", 2).await;
    seed_ondeck_user(
        &paths,
        "label-restricted-user",
        "label-restricted@example.org",
        "router-contract-label-restricted-123",
        true,
        None,
        false,
        &[],
        &[],
        &["family"],
    )
    .await;
    seed_ondeck_progress(
        &paths,
        "label-restricted-user",
        "series-1",
        "book-1",
        "2024-02-01T00:00:00",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "label-restricted@example.org",
        "router-contract-label-restricted-123",
    )
    .await;

    let ids = books_ondeck_ids(&app, &auth_token, "/api/v1/books/ondeck").await;
    assert!(ids.is_empty());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_ondeck_orders_by_most_recent_read_date_desc() {
    let paths = new_router_fixture("router-discovery-books-ondeck-most-recent-order").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    insert_ondeck_book(&paths, "book-4", "series-1", "library-1", 4, "Book 4").await;
    insert_ondeck_book(&paths, "book-5", "series-2", "library-1", 5, "Book 5").await;
    update_ondeck_series_book_count(&paths, "series-1", 2).await;
    update_ondeck_series_book_count(&paths, "series-2", 2).await;
    seed_ondeck_progress(
        &paths,
        "admin-user",
        "series-1",
        "book-1",
        "2024-02-01T00:00:00",
    )
    .await;
    seed_ondeck_progress(
        &paths,
        "admin-user",
        "series-2",
        "book-2",
        "2024-02-03T00:00:00",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let ids = books_ondeck_ids(&app, &auth_token, "/api/v1/books/ondeck?page=0&size=20").await;
    assert_eq!(ids, vec!["book-5", "book-4"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_ondeck_equal_read_dates_match_kotlin_sql_order() {
    let paths = new_router_fixture("router-discovery-books-ondeck-equal-read-date-order").await;
    seed_router_contract_data(&paths).await;
    seed_router_contract_nullable_samples(&paths).await;
    insert_ondeck_book(&paths, "book-3", "series-1", "library-1", 3, "Book 3").await;
    insert_ondeck_book(&paths, "book-4", "series-2", "library-1", 4, "Book 4").await;
    update_ondeck_series_book_count(&paths, "series-1", 2).await;
    update_ondeck_series_book_count(&paths, "series-2", 2).await;
    seed_ondeck_progress(
        &paths,
        "admin-user",
        "series-1",
        "book-1",
        "2024-02-03T00:00:00",
    )
    .await;
    seed_ondeck_progress(
        &paths,
        "admin-user",
        "series-2",
        "book-2",
        "2024-02-03T00:00:00",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let route_ids =
        books_ondeck_ids(&app, &auth_token, "/api/v1/books/ondeck?page=0&size=20").await;
    let kotlin_ids = kotlin_equivalent_ondeck_ids(&paths, "admin-user").await;
    assert_eq!(route_ids, kotlin_ids);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_ondeck_returns_first_unread_book_per_series() {
    let paths = new_router_fixture("router-discovery-books-ondeck-first-unread").await;
    seed_router_contract_data(&paths).await;
    insert_ondeck_book(&paths, "book-2", "series-1", "library-1", 2, "Book 2").await;
    insert_ondeck_book(&paths, "book-3", "series-1", "library-1", 3, "Book 3").await;
    update_ondeck_series_book_count(&paths, "series-1", 3).await;
    seed_ondeck_progress(
        &paths,
        "admin-user",
        "series-1",
        "book-1",
        "2024-02-01T00:00:00",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let ids = books_ondeck_ids(&app, &auth_token, "/api/v1/books/ondeck?page=0&size=20").await;
    assert_eq!(ids, vec!["book-2"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_ondeck_excludes_series_with_in_progress_books() {
    let paths = new_router_fixture("router-discovery-books-ondeck-in-progress-series").await;
    seed_router_contract_data(&paths).await;
    insert_ondeck_book(&paths, "book-2", "series-1", "library-1", 2, "Book 2").await;
    update_ondeck_series_book_count(&paths, "series-1", 2).await;
    seed_ondeck_progress(
        &paths,
        "admin-user",
        "series-1",
        "book-1",
        "2024-02-01T00:00:00",
    )
    .await;
    insert_ondeck_read_progress(
        &paths,
        "admin-user",
        "book-2",
        3_i64,
        false,
        "2024-02-02T00:00:00",
    )
    .await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("ondeck in-progress db should open");
    sqlx::query(
        "UPDATE READ_PROGRESS_SERIES SET IN_PROGRESS_COUNT = ?, MOST_RECENT_READ_DATE = ? WHERE SERIES_ID = ? AND USER_ID = ?",
    )
    .bind(1_i64)
    .bind("2024-02-02T00:00:00")
    .bind("series-1")
    .bind("admin-user")
    .execute(&pool)
    .await
    .expect("ondeck in-progress series counters should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let ids = books_ondeck_ids(&app, &auth_token, "/api/v1/books/ondeck?page=0&size=20").await;
    assert!(ids.is_empty());

    cleanup_router_fixture(paths);
}
