use super::*;
use std::sync::OnceLock;
use tokio::sync::Mutex;

fn book_thumbnail_runtime_sse_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[tokio::test]
async fn router_book_thumbnail_upload_parses_multipart_image_and_selected_flag() {
    let paths = new_router_fixture("router-book-thumbnail-upload-multipart").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "cover.png", "image/png", false, &image_bytes);

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
    let payload = response_json(upload).await;
    assert_eq!(
        payload.get("bookId"),
        Some(&Value::String("book-1".to_string()))
    );
    assert_eq!(
        payload.get("type"),
        Some(&Value::String("USER_UPLOADED".to_string()))
    );
    assert!(
        payload.get("id").and_then(Value::as_str).is_some(),
        "book thumbnail upload should return thumbnail id"
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_upload_selects_thumbnail_when_none_was_selected() {
    let paths = new_router_fixture("router-book-thumbnail-upload-auto-selects-first").await;
    seed_router_contract_data(&paths).await;

    let cleanup_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for book thumbnail cleanup");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&cleanup_pool)
        .await
        .expect("existing book-1 thumbnails should be deleted before upload test");
    cleanup_pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "cover.png", "image/png", false, &image_bytes);

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

    let verify_pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for selected thumbnail verification");
    let selected_count = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND SELECTED = 1",
    )
    .bind("book-1")
    .fetch_one(&verify_pool)
    .await
    .expect("selected book thumbnails should be queryable")
    .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(selected_count, 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_upload_accepts_oneshot_book() {
    let paths = new_router_fixture("router-book-thumbnail-upload-oneshot-book").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for oneshot book thumbnail setup");
    sqlx::query("UPDATE BOOK SET ONESHOT = ? WHERE ID = ?")
        .bind(1_i64)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book oneshot flag should update for thumbnail upload contract");
    sqlx::query("UPDATE SERIES SET ONESHOT = ? WHERE ID = ?")
        .bind(1_i64)
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series oneshot flag should update for thumbnail upload contract consistency");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "cover.png", "image/png", false, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/book-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("oneshot book thumbnail upload request should build"),
        )
        .await
        .expect("oneshot book thumbnail upload request should complete");

    assert_eq!(upload.status(), StatusCode::OK);
    let payload = response_json(upload).await;
    assert_eq!(
        payload.get("bookId"),
        Some(&Value::String("book-1".to_string()))
    );
    assert_eq!(
        payload.get("type"),
        Some(&Value::String("USER_UPLOADED".to_string()))
    );
    assert_eq!(payload.get("selected"), Some(&Value::Bool(false)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_upload_rejects_invalid_selected_flag() {
    let paths = new_router_fixture("router-book-thumbnail-upload-invalid-selected").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let boundary = "komga-rust-invalid-selected-boundary";
    let mut body = Vec::new();
    use std::io::Write as _;
    write!(
        &mut body,
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"cover.png\"\r\nContent-Type: image/png\r\n\r\n"
    )
    .expect("multipart invalid-selected file prelude should be written");
    body.extend_from_slice(&image_bytes);
    write!(
        &mut body,
        "\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"selected\"\r\n\r\nmaybe\r\n--{boundary}--\r\n"
    )
    .expect("multipart invalid-selected field should be written");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/book-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .expect("invalid selected thumbnail upload request should build"),
        )
        .await
        .expect("invalid selected thumbnail upload request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "book thumbnail selected field must be true or false".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_admin_routes_accept_basic_auth_like_kotlin_clients() {
    let paths = new_router_fixture("router-book-thumbnail-admin-basic-auth-compat").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let authorization =
        basic_authorization_header_value("admin@example.org", "router-contract-admin-123");
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "cover.png", "image/png", false, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/book-1/thumbnails")
                .header(header::AUTHORIZATION, authorization.as_str())
                .header("x-auth-token", "")
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("book thumbnail basic-auth upload request should build"),
        )
        .await
        .expect("book thumbnail basic-auth upload request should complete");
    assert_eq!(upload.status(), StatusCode::OK);
    let thumbnail_id = response_json(upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("book thumbnail basic-auth upload should return thumbnail id")
        .to_string();

    let select = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!(
                    "/api/v1/books/book-1/thumbnails/{thumbnail_id}/selected"
                ))
                .header(header::AUTHORIZATION, authorization.as_str())
                .header("x-auth-token", "")
                .body(Body::empty())
                .expect("book thumbnail basic-auth select request should build"),
        )
        .await
        .expect("book thumbnail basic-auth select request should complete");
    assert_eq!(select.status(), StatusCode::ACCEPTED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_upload_emits_thumbnail_book_added_event() {
    let _guard = book_thumbnail_runtime_sse_lock().lock().await;
    let paths = new_router_fixture("router-book-thumbnail-upload-sse").await;
    seed_router_contract_data(&paths).await;

    let cursor = komga_application::runtime_sse::current_runtime_sse_event_cursor();
    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "cover.png", "image/png", false, &image_bytes);

    let upload = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/book-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("book thumbnail upload sse request should build"),
        )
        .await
        .expect("book thumbnail upload sse request should complete");
    assert_eq!(upload.status(), StatusCode::OK);

    let (_, events) = komga_application::runtime_sse::pending_runtime_sse_events(
        cursor,
        "runtime-contract-admin",
        true,
    );
    let thumbnail_event = events
        .iter()
        .find(|event| event.name == "ThumbnailBookAdded")
        .expect("book thumbnail upload should emit ThumbnailBookAdded SSE");

    assert_eq!(
        thumbnail_event.payload.get("bookId"),
        Some(&Value::String("book-1".to_string()))
    );
    assert_eq!(
        thumbnail_event.payload.get("seriesId"),
        Some(&Value::String("series-1".to_string()))
    );
    assert_eq!(
        thumbnail_event.payload.get("selected"),
        Some(&Value::Bool(false))
    );

    cleanup_router_fixture(paths);
}
