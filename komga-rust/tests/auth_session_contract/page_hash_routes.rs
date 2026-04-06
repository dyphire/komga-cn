use super::*;

fn large_png_bytes(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_fn(width.max(1), height.max(1), |x, y| {
        image::Rgba([(x % 255) as u8, (y % 255) as u8, ((x + y) % 255) as u8, 255])
    });
    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .expect("png fixture should encode");
    output.into_inner()
}

async fn seed_unknown_page_hash_source(
    paths: &RuntimeDbPaths,
    book_id: &str,
    hash: &str,
    relative_book_path: &str,
    file_name: &str,
    media_type: &str,
    bytes: &[u8],
) -> std::path::PathBuf {
    let source_path = paths.config_dir.join(relative_book_path);
    if let Some(parent) = source_path.parent() {
        std::fs::create_dir_all(parent).expect("unknown page hash source parent should be created");
    }
    std::fs::write(&source_path, bytes).expect("unknown page hash source should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("unknown page hash source db should open");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(file_name)
    .bind(relative_book_path)
    .bind("series-1")
    .bind(i64::try_from(bytes.len()).expect("source bytes length should fit i64"))
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("unknown page hash source book row should be inserted");

    sqlx::query(
        "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(hash)
    .bind(file_name)
    .bind(media_type)
    .bind(i64::try_from(bytes.len()).expect("source bytes length should fit i64"))
    .execute(&pool)
    .await
    .expect("unknown page hash source media page row should be inserted");

    pool.close().await;
    source_path
}

async fn seed_unknown_page_hash_pdf_match(paths: &RuntimeDbPaths, book_id: &str, hash: &str) {
    seed_router_pdf_book(
        paths,
        book_id,
        "series-1",
        "unknown-page-hash-source.pdf",
        "Unknown PDF Page",
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("unknown page hash pdf db should open");
    sqlx::query(
        "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(hash)
    .bind("1")
    .bind("image/jpeg")
    .bind(4_096_i64)
    .execute(&pool)
    .await
    .expect("unknown page hash pdf media page row should be inserted");
    pool.close().await;
}

async fn seed_known_page_hash_samples(paths: &RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("known page hash sample db should open");

    for (book_id, name, url, hash, page_number, file_size) in [
        (
            "book-known-1",
            "book-known-1.epub",
            "books/book-known-1.epub",
            "alpha-hash",
            10_i64,
            111_i64,
        ),
        (
            "book-known-2",
            "book-known-2.epub",
            "books/book-known-2.epub",
            "alpha-hash",
            11_i64,
            111_i64,
        ),
        (
            "book-known-3",
            "book-known-3.epub",
            "books/book-known-3.epub",
            "beta-hash",
            12_i64,
            222_i64,
        ),
        (
            "book-known-4",
            "book-known-4.epub",
            "books/book-known-4.epub",
            "gamma-hash",
            13_i64,
            333_i64,
        ),
        (
            "book-known-5",
            "book-known-5.epub",
            "books/book-known-5.epub",
            "gamma-hash",
            14_i64,
            333_i64,
        ),
        (
            "book-known-6",
            "book-known-6.epub",
            "books/book-known-6.epub",
            "gamma-hash",
            15_i64,
            333_i64,
        ),
    ] {
        sqlx::query(
            "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(0_i64)
        .bind(name)
        .bind(url)
        .bind("series-1")
        .bind(2_048_i64)
        .bind(page_number)
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("known page hash sample book row should be inserted");

        sqlx::query(
            "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(0_i64)
        .bind(hash)
        .bind(format!("{book_id}.png"))
        .bind("image/png")
        .bind(file_size)
        .execute(&pool)
        .await
        .expect("known page hash sample media page row should be inserted");
    }

    for (hash, size, action, delete_count, created_date, last_modified_date) in [
        (
            "alpha-hash",
            Some(120_i64),
            "IGNORE",
            1_i64,
            "2024-01-01 00:00:00",
            "2024-01-05 00:00:00",
        ),
        (
            "beta-hash",
            Some(220_i64),
            "DELETE_AUTO",
            2_i64,
            "2024-01-02 00:00:00",
            "2024-01-03 00:00:00",
        ),
        (
            "gamma-hash",
            Some(320_i64),
            "DELETE_MANUAL",
            0_i64,
            "2024-01-03 00:00:00",
            "2024-01-04 00:00:00",
        ),
    ] {
        sqlx::query(
            "INSERT INTO PAGE_HASH (HASH, SIZE, ACTION, DELETE_COUNT, CREATED_DATE, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(hash)
        .bind(size)
        .bind(action)
        .bind(delete_count)
        .bind(created_date)
        .bind(last_modified_date)
        .execute(&pool)
        .await
        .expect("known page hash row should be inserted");
    }

    pool.close().await;
}

fn delete_match_payload(
    book_id: &str,
    url: &str,
    page_number: i64,
    file_name: &str,
    file_size: i64,
    media_type: &str,
) -> String {
    json!({
        "bookId": book_id,
        "url": url,
        "pageNumber": page_number,
        "fileName": file_name,
        "fileSize": file_size,
        "mediaType": media_type,
    })
    .to_string()
}

#[tokio::test]
async fn router_put_page_hash_normalizes_negative_size_to_null() {
    let paths = new_router_fixture("router-put-page-hash-negative-size-null").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"negative-size-hash","size":-1,"action":"IGNORE"}"#,
                ))
                .expect("page hash put request should build"),
        )
        .await
        .expect("page hash put request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        load_page_hash_size(&paths, "negative-size-hash").await,
        None
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_preserves_whitespace_padded_hash() {
    let paths = new_router_fixture("router-put-page-hash-whitespace-hash").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":" negative-size-hash ","size":1,"action":"IGNORE"}"#,
                ))
                .expect("page hash put request with padded hash should build"),
        )
        .await
        .expect("page hash put request with padded hash should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        load_page_hash_size(&paths, " negative-size-hash ").await,
        Some(1)
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_rejects_blank_only_hash() {
    let paths = new_router_fixture("router-put-page-hash-blank-only-hash").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"hash":"   ","size":1,"action":"IGNORE"}"#))
                .expect("page hash put request with blank-only hash should build"),
        )
        .await
        .expect("page hash put request with blank-only hash should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_rejects_whitespace_padded_action() {
    let paths = new_router_fixture("router-put-page-hash-whitespace-action").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"negative-size-hash","size":1,"action":" IGNORE "}"#,
                ))
                .expect("page hash put request with whitespace action should build"),
        )
        .await
        .expect("page hash put request with whitespace action should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_rejects_non_integer_size_values() {
    let paths = new_router_fixture("router-put-page-hash-non-integer-size").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"typed-size-hash","size":true,"action":"IGNORE"}"#,
                ))
                .expect("page hash put request with non-integer size should build"),
        )
        .await
        .expect("page hash put request with non-integer size should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        load_page_hash_record(&paths, "typed-size-hash")
            .await
            .is_none()
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_preserves_existing_size_on_update() {
    let paths = new_router_fixture("router-put-page-hash-preserve-existing-size").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_row(&paths, "existing-size-hash", Some(5), "IGNORE").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"existing-size-hash","size":99,"action":"DELETE_AUTO"}"#,
                ))
                .expect("page hash update request should build"),
        )
        .await
        .expect("page hash update request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        load_page_hash_record(&paths, "existing-size-hash").await,
        Some((Some(5), "DELETE_AUTO".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_page_hash_persists_known_thumbnail_so_it_survives_source_removal() {
    let paths = new_router_fixture("router-put-page-hash-persists-thumbnail").await;
    seed_router_contract_data(&paths).await;
    let source_path = seed_page_hash_image_source(
        &paths,
        "book-page-hash-thumb",
        "known-thumb-hash",
        "images/known-thumb-source.png",
        "known-thumb-source.png",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/page-hashes")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"hash":"known-thumb-hash","size":64,"action":"IGNORE"}"#,
                ))
                .expect("page hash put request for thumbnail persistence should build"),
        )
        .await
        .expect("page hash put request for thumbnail persistence should complete");

    assert_eq!(put_response.status(), StatusCode::ACCEPTED);
    std::fs::remove_file(&source_path).expect("source image should be removable after put");

    let thumbnail_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/known-thumb-hash/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("known page hash thumbnail request should build"),
        )
        .await
        .expect("known page hash thumbnail request should complete");

    assert_eq!(thumbnail_response.status(), StatusCode::OK);
    assert_eq!(
        thumbnail_response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    let body = to_bytes(thumbnail_response.into_body(), usize::MAX)
        .await
        .expect("known page hash thumbnail response body should be readable");
    assert!(body.starts_with(&[0xFF, 0xD8]));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_post_page_hash_delete_all_enqueues_remove_hashed_pages_tasks_without_touching_media_rows()
 {
    let paths = new_router_fixture("router-page-hash-delete-all-enqueue-only").await;
    seed_router_contract_data(&paths).await;
    seed_known_page_hash_samples(&paths).await;

    let setup_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for page hash delete-all setup");
    sqlx::query(
        "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("book-known-1")
    .bind(1_i64)
    .bind("alpha-hash")
    .bind("book-known-1-page-2.png")
    .bind("image/png")
    .bind(222_i64)
    .execute(&setup_pool)
    .await
    .expect("second duplicate page row should be inserted");
    setup_pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/page-hashes/alpha-hash/delete-all")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash delete-all request should build"),
        )
        .await
        .expect("page hash delete-all request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for page hash delete-all verification");
    let remaining_media_rows =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM MEDIA_PAGE WHERE FILE_HASH = ?")
            .bind("alpha-hash")
            .fetch_one(&verify_pool)
            .await
            .expect("remaining page hash match rows should be queryable")
            .get::<i64, _>("COUNT");
    let delete_count =
        sqlx::query("SELECT DELETE_COUNT AS DELETE_COUNT FROM PAGE_HASH WHERE HASH = ?")
            .bind("alpha-hash")
            .fetch_one(&verify_pool)
            .await
            .expect("page hash delete count should be queryable")
            .get::<i64, _>("DELETE_COUNT");
    verify_pool.close().await;

    assert_eq!(remaining_media_rows, 3);
    assert_eq!(delete_count, 1);

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for page hash delete-all verification");
    let rows = sqlx::query("SELECT ID, SIMPLE_TYPE, GROUP_ID, PAYLOAD FROM TASK ORDER BY ID ASC")
        .fetch_all(&tasks_pool)
        .await
        .expect("page hash delete-all task rows should be queryable");
    tasks_pool.close().await;

    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].get::<String, _>("ID"),
        "REMOVE_HASHED_PAGES_book-known-1"
    );
    assert_eq!(rows[0].get::<String, _>("SIMPLE_TYPE"), "RemoveHashedPages");
    assert_eq!(rows[0].get::<Option<String>, _>("GROUP_ID"), None);
    let first_payload: Value = serde_json::from_str(&rows[0].get::<String, _>("PAYLOAD"))
        .expect("first delete-all payload should be valid json");
    assert_eq!(
        first_payload,
        json!({
            "bookId": "book-known-1",
            "pages": [
                {
                    "fileHash": "alpha-hash",
                    "fileSize": 111,
                    "fileName": "book-known-1.png",
                    "mediaType": "image/png",
                    "pageNumber": 1
                },
                {
                    "fileHash": "alpha-hash",
                    "fileSize": 222,
                    "fileName": "book-known-1-page-2.png",
                    "mediaType": "image/png",
                    "pageNumber": 2
                }
            ],
            "priority": 4,
            "groupId": Value::Null,
            "uniqueId": "REMOVE_HASHED_PAGES_book-known-1"
        })
    );

    assert_eq!(
        rows[1].get::<String, _>("ID"),
        "REMOVE_HASHED_PAGES_book-known-2"
    );
    assert_eq!(rows[1].get::<String, _>("SIMPLE_TYPE"), "RemoveHashedPages");
    assert_eq!(rows[1].get::<Option<String>, _>("GROUP_ID"), None);
    let second_payload: Value = serde_json::from_str(&rows[1].get::<String, _>("PAYLOAD"))
        .expect("second delete-all payload should be valid json");
    assert_eq!(
        second_payload,
        json!({
            "bookId": "book-known-2",
            "pages": [
                {
                    "fileHash": "alpha-hash",
                    "fileSize": 111,
                    "fileName": "book-known-2.png",
                    "mediaType": "image/png",
                    "pageNumber": 1
                }
            ],
            "priority": 4,
            "groupId": Value::Null,
            "uniqueId": "REMOVE_HASHED_PAGES_book-known-2"
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_post_page_hash_delete_all_accepts_missing_hash_without_enqueuing_tasks_like_kotlin()
{
    let paths = new_router_fixture("router-page-hash-delete-all-missing-hash").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/page-hashes/missing-hash/delete-all")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing-hash page hash delete-all request should build"),
        )
        .await
        .expect("missing-hash page hash delete-all request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for missing-hash delete-all verification");
    let queued_count = sqlx::query("SELECT COUNT(*) AS COUNT FROM TASK")
        .fetch_one(&tasks_pool)
        .await
        .expect("missing-hash delete-all task count should be queryable")
        .get::<i64, _>("COUNT");
    tasks_pool.close().await;

    assert_eq!(queued_count, 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_post_page_hash_delete_match_enqueues_remove_hashed_pages_task_without_touching_media_rows_like_kotlin()
 {
    let paths = new_router_fixture("router-page-hash-delete-match-enqueue-only").await;
    seed_router_contract_data(&paths).await;
    seed_known_page_hash_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/page-hashes/alpha-hash/delete-match")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(delete_match_payload(
                    "book-known-1",
                    "books/book-known-1.epub",
                    1,
                    "book-known-1.png",
                    111,
                    "image/png",
                )))
                .expect("page hash delete-match request should build"),
        )
        .await
        .expect("page hash delete-match request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for page hash delete-match verification");
    let remaining_media_rows =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM MEDIA_PAGE WHERE FILE_HASH = ?")
            .bind("alpha-hash")
            .fetch_one(&verify_pool)
            .await
            .expect("remaining delete-match media rows should be queryable")
            .get::<i64, _>("COUNT");
    let delete_count =
        sqlx::query("SELECT DELETE_COUNT AS DELETE_COUNT FROM PAGE_HASH WHERE HASH = ?")
            .bind("alpha-hash")
            .fetch_one(&verify_pool)
            .await
            .expect("delete-match page hash delete count should be queryable")
            .get::<i64, _>("DELETE_COUNT");
    verify_pool.close().await;

    assert_eq!(remaining_media_rows, 2);
    assert_eq!(delete_count, 1);

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for page hash delete-match verification");
    let rows = sqlx::query("SELECT ID, SIMPLE_TYPE, GROUP_ID, PAYLOAD FROM TASK ORDER BY ID ASC")
        .fetch_all(&tasks_pool)
        .await
        .expect("page hash delete-match task rows should be queryable");
    tasks_pool.close().await;

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get::<String, _>("ID"),
        "REMOVE_HASHED_PAGES_book-known-1"
    );
    assert_eq!(rows[0].get::<String, _>("SIMPLE_TYPE"), "RemoveHashedPages");
    assert_eq!(rows[0].get::<Option<String>, _>("GROUP_ID"), None);
    let payload: Value = serde_json::from_str(&rows[0].get::<String, _>("PAYLOAD"))
        .expect("delete-match payload should be valid json");
    assert_eq!(
        payload,
        json!({
            "bookId": "book-known-1",
            "pages": [
                {
                    "fileHash": "alpha-hash",
                    "fileSize": 111,
                    "fileName": "book-known-1.png",
                    "mediaType": "image/png",
                    "pageNumber": 1
                }
            ],
            "priority": 4,
            "groupId": Value::Null,
            "uniqueId": "REMOVE_HASHED_PAGES_book-known-1"
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_post_page_hash_delete_match_accepts_missing_hash_and_still_enqueues_task_like_kotlin()
 {
    let paths = new_router_fixture("router-page-hash-delete-match-missing-hash").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/page-hashes/missing-hash/delete-match")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(delete_match_payload(
                    "book-missing-hash",
                    "books/book-missing-hash.epub",
                    1,
                    "book-missing-hash.png",
                    111,
                    "image/png",
                )))
                .expect("missing-hash page hash delete-match request should build"),
        )
        .await
        .expect("missing-hash page hash delete-match request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for missing-hash delete-match verification");
    let rows = sqlx::query("SELECT ID, SIMPLE_TYPE, GROUP_ID, PAYLOAD FROM TASK ORDER BY ID ASC")
        .fetch_all(&tasks_pool)
        .await
        .expect("missing-hash delete-match task rows should be queryable");
    tasks_pool.close().await;

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get::<String, _>("ID"),
        "REMOVE_HASHED_PAGES_book-missing-hash"
    );
    assert_eq!(rows[0].get::<String, _>("SIMPLE_TYPE"), "RemoveHashedPages");
    assert_eq!(rows[0].get::<Option<String>, _>("GROUP_ID"), None);
    let payload: Value = serde_json::from_str(&rows[0].get::<String, _>("PAYLOAD"))
        .expect("missing-hash delete-match payload should be valid json");
    assert_eq!(payload["bookId"], json!("book-missing-hash"));
    assert_eq!(payload["pages"][0]["fileHash"], json!("missing-hash"));
    assert_eq!(payload["pages"][0]["pageNumber"], json!(1));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_unknown_thumbnail_returns_original_image_without_resize_like_kotlin()
{
    let paths = new_router_fixture("router-page-hash-unknown-thumbnail-original-image").await;
    seed_router_contract_data(&paths).await;
    let expected = large_png_bytes(640, 320);
    seed_unknown_page_hash_source(
        &paths,
        "book-unknown-thumb-image",
        "unknown-thumb-image-hash",
        "images/unknown-thumb-image.png",
        "unknown-thumb-image.png",
        "image/png",
        &expected,
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown/unknown-thumb-image-hash/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("unknown page hash thumbnail request should build"),
        )
        .await
        .expect("unknown page hash thumbnail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/png")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("unknown page hash thumbnail body should be readable");
    assert_eq!(body.as_ref(), expected.as_slice());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_unknown_thumbnail_renders_pdf_page_without_resize_like_kotlin() {
    let paths = new_router_fixture("router-page-hash-unknown-thumbnail-pdf-original").await;
    seed_router_contract_data(&paths).await;
    seed_unknown_page_hash_pdf_match(
        &paths,
        "book-unknown-thumb-pdf-original",
        "unknown-thumb-pdf-original-hash",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown/unknown-thumb-pdf-original-hash/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("pdf unknown page hash thumbnail request should build"),
        )
        .await
        .expect("pdf unknown page hash thumbnail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("pdf unknown page hash thumbnail body should be readable");
    let image = image::load_from_memory(&body)
        .expect("pdf unknown page hash thumbnail should decode as image");
    assert!(image.width().max(image.height()) > 300);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_unknown_thumbnail_honors_resize_and_renders_jpeg_for_pdf_like_kotlin()
{
    let paths = new_router_fixture("router-page-hash-unknown-thumbnail-pdf-resize").await;
    seed_router_contract_data(&paths).await;
    seed_unknown_page_hash_pdf_match(&paths, "book-unknown-thumb-pdf", "unknown-thumb-pdf-hash")
        .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown/unknown-thumb-pdf-hash/thumbnail?resize=300")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("resized unknown page hash pdf thumbnail request should build"),
        )
        .await
        .expect("resized unknown page hash pdf thumbnail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("resized unknown page hash pdf thumbnail body should be readable");
    let image = image::load_from_memory(&body)
        .expect("resized unknown page hash pdf thumbnail should decode as image");
    assert_eq!(image.width().max(image.height()), 300);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_unknown_thumbnail_returns_not_found_when_match_is_missing_like_kotlin()
 {
    let paths = new_router_fixture("router-page-hash-unknown-thumbnail-missing-match").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown/missing-match-hash/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing-match unknown page hash thumbnail request should build"),
        )
        .await
        .expect("missing-match unknown page hash thumbnail request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_unknown_thumbnail_returns_not_found_when_page_source_is_missing_like_kotlin()
 {
    let paths = new_router_fixture("router-page-hash-unknown-thumbnail-missing-source").await;
    seed_router_contract_data(&paths).await;
    let source_path = seed_unknown_page_hash_source(
        &paths,
        "book-missing-source",
        "missing-source-hash",
        "images/missing-source.png",
        "missing-source.png",
        "image/png",
        &large_png_bytes(64, 64),
    )
    .await;
    std::fs::remove_file(&source_path).expect("missing-source fixture should be removable");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown/missing-source-hash/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing-source unknown page hash thumbnail request should build"),
        )
        .await
        .expect("missing-source unknown page hash thumbnail request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hashes_honors_match_count_desc_sort_like_kotlin() {
    let paths = new_router_fixture("router-page-hashes-known-match-count-desc").await;
    seed_router_contract_data(&paths).await;
    seed_known_page_hash_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes?sort=matchCount,desc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("sorted known page hashes request should build"),
        )
        .await
        .expect("sorted known page hashes request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("known page hashes content should be an array");
    let hashes = content
        .iter()
        .map(|entry| {
            entry["hash"]
                .as_str()
                .expect("known page hash entry should contain hash")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(hashes, vec!["gamma-hash", "alpha-hash", "beta-hash"]);
    assert_eq!(payload["sort"]["sorted"], true);
    assert_eq!(payload["sort"]["unsorted"], false);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hashes_filters_by_action_query_like_kotlin() {
    let paths = new_router_fixture("router-page-hashes-known-action-filter").await;
    seed_router_contract_data(&paths).await;
    seed_known_page_hash_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes?action=IGNORE,DELETE_AUTO&sort=hash,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("filtered known page hashes request should build"),
        )
        .await
        .expect("filtered known page hashes request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("filtered known page hashes content should be an array");
    let hashes = content
        .iter()
        .map(|entry| {
            entry["hash"]
                .as_str()
                .expect("filtered known page hash entry should contain hash")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(hashes, vec!["alpha-hash", "beta-hash"]);
    assert_eq!(payload["totalElements"], json!(2));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hashes_rejects_invalid_action_query_like_kotlin() {
    let paths = new_router_fixture("router-page-hashes-known-invalid-action").await;
    seed_router_contract_data(&paths).await;
    seed_known_page_hash_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes?action=IGNORE,NOT_REAL")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("known page hashes invalid-action request should build"),
        )
        .await
        .expect("known page hashes invalid-action request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hashes_unknown_honors_hash_desc_sort_query() {
    let paths = new_router_fixture("router-page-hashes-unknown-hash-desc-sort").await;
    seed_router_contract_data(&paths).await;
    seed_unknown_page_hash_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown?sort=hash,desc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("sorted unknown page hashes request should build"),
        )
        .await
        .expect("sorted unknown page hashes request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("unknown page hashes content should be an array");
    let hashes = content
        .iter()
        .map(|entry| {
            entry["hash"]
                .as_str()
                .expect("page hash unknown entry should contain hash")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(hashes, vec!["z-hash".to_string(), "a-hash".to_string()]);
    assert_eq!(payload["sort"]["sorted"], true);
    assert_eq!(payload["sort"]["unsorted"], false);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hashes_unknown_honors_kotlin_legacy_sort_keys() {
    let paths = new_router_fixture("router-page-hashes-unknown-legacy-sort-keys").await;
    seed_router_contract_data(&paths).await;
    seed_unknown_page_hash_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for sort in ["url,desc", "bookId,desc", "pageNumber,desc"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/page-hashes/unknown?sort={sort}"))
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("legacy-sorted unknown page hashes request should build"),
            )
            .await
            .expect("legacy-sorted unknown page hashes request should complete");

        assert_eq!(response.status(), StatusCode::OK, "sort={sort}");
        let payload = response_json(response).await;
        let content = payload["content"]
            .as_array()
            .expect("unknown page hashes content should be an array");
        let hashes = content
            .iter()
            .map(|entry| {
                entry["hash"]
                    .as_str()
                    .expect("page hash unknown entry should contain hash")
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            hashes,
            vec!["z-hash".to_string(), "a-hash".to_string()],
            "sort={sort}"
        );
        assert_eq!(payload["sort"]["sorted"], true, "sort={sort}");
        assert_eq!(payload["sort"]["unsorted"], false, "sort={sort}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hashes_unknown_groups_same_hash_even_when_file_sizes_differ() {
    let paths = new_router_fixture("router-page-hashes-unknown-groups-by-hash-only").await;
    seed_router_contract_data(&paths).await;
    seed_unknown_page_hash_samples_with_mixed_sizes(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("mixed-size unknown page hashes request should build"),
        )
        .await
        .expect("mixed-size unknown page hashes request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("unknown page hashes content should be an array");
    assert_eq!(payload["totalElements"], json!(1));
    assert_eq!(content.len(), 1);
    assert_eq!(content[0]["hash"], json!("mixed-size-hash"));
    assert_eq!(content[0]["matchCount"], json!(2));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_honors_page_number_desc_sort_query() {
    let paths = new_router_fixture("router-page-hash-matches-page-number-desc").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=pageNumber,desc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("sorted page hash matches request should build"),
        )
        .await
        .expect("sorted page hash matches request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("page hash matches content should be an array");
    let page_numbers = content
        .iter()
        .map(|entry| {
            entry["pageNumber"]
                .as_i64()
                .expect("page hash match entry should contain page number")
        })
        .collect::<Vec<_>>();
    assert_eq!(page_numbers, vec![5, 3, 1]);
    assert_eq!(payload["sort"]["sorted"], true);
    assert_eq!(payload["sort"]["unsorted"], false);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_rejects_match_count_and_total_size_sort_keys() {
    let paths = new_router_fixture("router-page-hash-matches-unsupported-aggregate-sort").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for sort in ["matchCount,desc", "totalSize,desc"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/page-hashes/match-sort-hash?sort={sort}"))
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("page hash matches aggregate sort request should build"),
            )
            .await
            .expect("page hash matches aggregate sort request should complete");

        assert_eq!(
            response.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "sort={sort}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_converts_file_url_to_path_string() {
    let paths = new_router_fixture("router-page-hash-matches-url-to-path").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;
    update_book_url(
        &paths,
        "book-match-1",
        "file:/library-root/books/book-match-1.cbz",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches url path request should build"),
        )
        .await
        .expect("page hash matches url path request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("page hash matches content should be an array");
    assert_eq!(content[0]["url"], "/library-root/books/book-match-1.cbz");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_returns_internal_error_for_unparseable_book_url() {
    let paths = new_router_fixture("router-page-hash-matches-invalid-url").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;
    update_book_url(&paths, "book-match-1", "::not-a-valid-url::").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches invalid url request should build"),
        )
        .await
        .expect("page hash matches invalid url request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_decodes_percent_encoded_file_url_path() {
    let paths = new_router_fixture("router-page-hash-matches-decodes-file-url-path").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;
    update_book_url(
        &paths,
        "book-match-1",
        "file:/library%20root/books/book%20match%201.cbz",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches encoded file url request should build"),
        )
        .await
        .expect("page hash matches encoded file url request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("page hash matches content should be an array");
    assert_eq!(content[0]["url"], "/library root/books/book match 1.cbz");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_returns_internal_error_for_null_file_size() {
    let paths = new_router_fixture("router-page-hash-matches-null-file-size").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;
    update_media_page_file_size_to_null(&paths, "book-match-1", 0).await;
    assert_eq!(
        load_media_page_file_size(&paths, "book-match-1", 0).await,
        None
    );

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches null file size request should build"),
        )
        .await
        .expect("page hash matches null file size request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_page_hash_matches_returns_internal_error_for_non_file_url() {
    let paths = new_router_fixture("router-page-hash-matches-http-url").await;
    seed_router_contract_data(&paths).await;
    seed_page_hash_match_samples(&paths, "match-sort-hash").await;
    update_book_url(
        &paths,
        "book-match-1",
        "https://example.com/books/book-match-1.cbz",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches non-file url request should build"),
        )
        .await
        .expect("page hash matches non-file url request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
}
