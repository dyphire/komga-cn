use super::*;

async fn seed_book_thumbnail_bytes(
    paths: &RuntimeDbPaths,
    thumbnail_id: &str,
    media_type: &str,
    bytes: &[u8],
) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series contract db should open for book thumbnail seed");
    sqlx::query("UPDATE THUMBNAIL_BOOK SET MEDIA_TYPE = ?, THUMBNAIL = ? WHERE ID = ?")
        .bind(media_type)
        .bind(bytes)
        .bind(thumbnail_id)
        .execute(&pool)
        .await
        .expect("book thumbnail row should be updated for series contract");
    pool.close().await;
}

#[tokio::test]
async fn router_series_thumbnail_upload_parses_multipart_image_and_selected_flag() {
    let paths = new_router_fixture("router-series-thumbnail-upload-multipart").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "series.png", "image/png", false, &image_bytes);

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
    let payload = response_json(upload).await;
    assert_eq!(
        payload.get("seriesId"),
        Some(&Value::String("series-1".to_string()))
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
        .expect("series thumbnail upload should return thumbnail id");
    let stored = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/series/series-1/thumbnails/{thumbnail_id}"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series thumbnail fetch request should build"),
        )
        .await
        .expect("series thumbnail fetch request should complete");

    assert_eq!(stored.status(), StatusCode::OK);
    let stored_body = to_bytes(stored.into_body(), usize::MAX)
        .await
        .expect("series thumbnail fetch body should be readable");
    assert_eq!(stored_body.as_ref(), image_bytes.as_slice());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_thumbnail_upload_rejects_oneshot_series() {
    let paths = new_router_fixture("router-series-thumbnail-upload-oneshot-rejected").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series thumbnail oneshot db should open");
    sqlx::query("UPDATE SERIES SET ONESHOT = ? WHERE ID = ?")
        .bind(1_i64)
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series oneshot flag should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "series.png", "image/png", false, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/series-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("oneshot series thumbnail upload request should build"),
        )
        .await
        .expect("oneshot series thumbnail upload request should complete");

    assert_eq!(upload.status(), StatusCode::BAD_REQUEST);

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series thumbnail oneshot verify db should open");
    let count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM THUMBNAIL_SERIES WHERE SERIES_ID = ?")
            .bind("series-1")
            .fetch_one(&pool)
            .await
            .expect("series thumbnail count should load");
    pool.close().await;

    assert_eq!(count, 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_thumbnail_select_marks_uploaded_thumbnail_selected() {
    let paths = new_router_fixture("router-series-thumbnail-select-success").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "series.png", "image/png", false, &image_bytes);

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
    let thumbnail_id = response_json(upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("series thumbnail upload should return thumbnail id")
        .to_string();

    let select = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!(
                    "/api/v1/series/series-1/thumbnails/{thumbnail_id}/selected"
                ))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series thumbnail select request should build"),
        )
        .await
        .expect("series thumbnail select request should complete");
    assert_eq!(select.status(), StatusCode::ACCEPTED);

    let list = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series thumbnail list request should build"),
        )
        .await
        .expect("series thumbnail list request should complete");
    assert_eq!(list.status(), StatusCode::OK);
    let rows = response_json(list).await;
    let rows = rows
        .as_array()
        .expect("series thumbnail list response should be an array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("id"), Some(&Value::String(thumbnail_id)));
    assert_eq!(rows[0].get("selected"), Some(&Value::Bool(true)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_oneshot_series_thumbnail_falls_back_to_book_thumbnail() {
    let paths = new_router_fixture("router-oneshot-series-thumbnail-fallback-book-thumbnail").await;
    seed_router_contract_data(&paths).await;

    let png_bytes = fixture_png_bytes();
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("oneshot series thumbnail db should open");
    sqlx::query("UPDATE SERIES SET ONESHOT = ? WHERE ID = ?")
        .bind(1_i64)
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series oneshot flag should update for thumbnail fallback");
    sqlx::query("UPDATE BOOK SET ONESHOT = ? WHERE ID = ?")
        .bind(1_i64)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book oneshot flag should update for thumbnail fallback");
    sqlx::query("DELETE FROM THUMBNAIL_SERIES WHERE SERIES_ID = ?")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series thumbnails should be removed for oneshot fallback");
    pool.close().await;

    seed_book_thumbnail_bytes(&paths, "thumb-book-1", "image/png", &png_bytes).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("oneshot series thumbnail fallback request should build"),
        )
        .await
        .expect("oneshot series thumbnail fallback request should complete");

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
        .expect("oneshot series thumbnail fallback body should be readable");
    assert_eq!(body.as_ref(), png_bytes.as_slice());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_thumbnail_by_id_reads_sidecar_thumbnail_file() {
    let paths = new_router_fixture("router-series-thumbnail-by-id-sidecar-file").await;
    seed_router_contract_data(&paths).await;

    let sidecar_bytes = fixture_png_bytes();
    let sidecar_path = paths.config_dir.join("series-sidecar.png");
    std::fs::write(&sidecar_path, &sidecar_bytes).expect("series sidecar image should be written");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series sidecar db should open");
    sqlx::query(
        "INSERT INTO THUMBNAIL_SERIES (ID, SERIES_ID, URL, THUMBNAIL, TYPE, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT, SELECTED) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("thumb-sidecar-1")
    .bind("series-1")
    .bind(format!("file:{}", sidecar_path.display()))
    .bind(Option::<Vec<u8>>::None)
    .bind("USER_UPLOADED")
    .bind("image/png")
    .bind(i64::try_from(sidecar_bytes.len()).expect("sidecar length should fit i64"))
    .bind(1_i64)
    .bind(1_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("series sidecar thumbnail row should insert");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1/thumbnails/thumb-sidecar-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series sidecar thumbnail by-id request should build"),
        )
        .await
        .expect("series sidecar thumbnail by-id request should complete");

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
        .expect("series sidecar thumbnail by-id body should be readable");
    assert_eq!(body.as_ref(), sidecar_bytes.as_slice());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_thumbnail_by_id_returns_internal_server_error_when_sidecar_file_is_missing()
{
    let paths = new_router_fixture("router-series-thumbnail-by-id-missing-sidecar-file").await;
    seed_router_contract_data(&paths).await;

    let missing_sidecar_path = paths.config_dir.join("series-missing-sidecar.png");

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series missing-sidecar db should open");
    sqlx::query(
        "INSERT INTO THUMBNAIL_SERIES (ID, SERIES_ID, URL, THUMBNAIL, TYPE, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT, SELECTED) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("thumb-missing-sidecar-1")
    .bind("series-1")
    .bind(format!("file:{}", missing_sidecar_path.display()))
    .bind(Option::<Vec<u8>>::None)
    .bind("USER_UPLOADED")
    .bind("image/png")
    .bind(1_i64)
    .bind(1_i64)
    .bind(1_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("series missing-sidecar thumbnail row should insert");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1/thumbnails/thumb-missing-sidecar-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series missing-sidecar thumbnail by-id request should build"),
        )
        .await
        .expect("series missing-sidecar thumbnail by-id request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_thumbnail_by_id_allows_missing_path_series_for_unrestricted_user() {
    let paths = new_router_fixture("router-series-thumbnail-by-id-missing-path-series").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "series.png", "image/png", false, &image_bytes);

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

    let thumbnail_id = response_json(upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("series thumbnail upload should return thumbnail id")
        .to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/v1/series/missing-series/thumbnails/{thumbnail_id}"
                ))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series thumbnail missing path series request should build"),
        )
        .await
        .expect("series thumbnail missing path series request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("series thumbnail missing path series body should be readable");
    assert_eq!(body.as_ref(), image_bytes.as_slice());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_thumbnail_delete_rejects_non_user_uploaded_thumbnail() {
    let paths = new_router_fixture("router-series-thumbnail-delete-generated-rejected").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series generated thumbnail delete db should open");
    sqlx::query(
        "INSERT INTO THUMBNAIL_SERIES (ID, SERIES_ID, URL, THUMBNAIL, TYPE, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT, SELECTED) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("thumb-generated-1")
    .bind("series-1")
    .bind("")
    .bind(fixture_png_bytes())
    .bind("GENERATED")
    .bind("image/png")
    .bind(1_i64)
    .bind(1_i64)
    .bind(1_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("generated series thumbnail row should insert");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/series/series-1/thumbnails/thumb-generated-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("generated series thumbnail delete request should build"),
        )
        .await
        .expect("generated series thumbnail delete request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series generated thumbnail verify db should open");
    let remaining = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM THUMBNAIL_SERIES WHERE ID = ? AND SERIES_ID = ?",
    )
    .bind("thumb-generated-1")
    .bind("series-1")
    .fetch_one(&pool)
    .await
    .expect("generated series thumbnail count should load");
    pool.close().await;

    assert_eq!(remaining, 1);

    cleanup_router_fixture(paths);
}
