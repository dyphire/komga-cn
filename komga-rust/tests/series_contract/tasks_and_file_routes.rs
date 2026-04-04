use super::*;

#[tokio::test]
async fn router_series_media_assets_forbid_age_restricted_user() {
    let paths = new_router_fixture("router-series-media-assets-restricted-user").await;
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
    let (content_type, body) =
        multipart_image_upload_body("file", "series.png", "image/png", true, &image_bytes);
    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/series-1/thumbnails")
                .header("x-auth-token", &admin_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("restricted series thumbnail upload request should build"),
        )
        .await
        .expect("restricted series thumbnail upload request should complete");
    assert_eq!(upload.status(), StatusCode::OK);
    let thumbnail_id = response_json(upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("restricted series upload should return thumbnail id")
        .to_string();

    for route in [
        "/api/v1/series/series-1/thumbnails",
        "/api/v1/series/series-1/thumbnail",
        "/api/v1/series/series-1/file",
        "/api/v2/series/series-1/read-progress/tachiyomi",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &restricted_token)
                    .body(Body::empty())
                    .expect("restricted series get request should build"),
            )
            .await
            .expect("restricted series get request should complete");

        assert_eq!(response.status(), StatusCode::FORBIDDEN, "route: {route}");
    }

    let by_id = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/series/series-1/thumbnails/{thumbnail_id}"))
                .header("x-auth-token", &restricted_token)
                .body(Body::empty())
                .expect("restricted series thumbnail by-id request should build"),
        )
        .await
        .expect("restricted series thumbnail by-id request should complete");
    assert_eq!(by_id.status(), StatusCode::FORBIDDEN);

    for route in ["/api/v1/series/series-1/read-progress"] {
        for method in ["POST", "DELETE"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(route)
                        .header("x-auth-token", &restricted_token)
                        .body(Body::empty())
                        .expect("restricted series read-progress request should build"),
                )
                .await
                .expect("restricted series read-progress request should complete");
            assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method} {route}");
        }
    }

    let tachiyomi_put = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v2/series/series-1/read-progress/tachiyomi")
                .header("x-auth-token", &restricted_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "lastBookNumberSortRead": 1.0
                    })
                    .to_string(),
                ))
                .expect("restricted series tachiyomi put request should build"),
        )
        .await
        .expect("restricted series tachiyomi put request should complete");
    assert_eq!(tachiyomi_put.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_tachiyomi_progress_returns_zero_dto_for_missing_series() {
    let paths = new_router_fixture("router-series-tachiyomi-progress-missing-series").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/series/missing-series/read-progress/tachiyomi")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing series tachiyomi progress request should build"),
        )
        .await
        .expect("missing series tachiyomi progress request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload,
        json!({
            "booksCount": 0,
            "booksReadCount": 0,
            "booksUnreadCount": 0,
            "booksInProgressCount": 0,
            "lastReadContinuousNumberSort": 0.0,
            "maxNumberSort": 0.0,
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_tachiyomi_progress_counts_deleted_books_like_kotlin() {
    let paths = new_router_fixture("router-series-tachiyomi-progress-counts-deleted-books").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series tachiyomi deleted-book db should open");
    sqlx::query("UPDATE BOOK SET DELETED_DATE = ? WHERE ID = ?")
        .bind("2024-03-01T00:00:00")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("series tachiyomi deleted-book row should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/series/series-1/read-progress/tachiyomi")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series tachiyomi deleted-book request should build"),
        )
        .await
        .expect("series tachiyomi deleted-book request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "booksCount": 1,
            "booksReadCount": 0,
            "booksUnreadCount": 1,
            "booksInProgressCount": 0,
            "lastReadContinuousNumberSort": 0.0,
            "maxNumberSort": 1.0,
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_tachiyomi_progress_counts_completed_false_page_zero_as_in_progress() {
    let paths = new_router_fixture("router-series-tachiyomi-progress-page-zero-in-progress").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series tachiyomi page-zero db should open");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) VALUES (?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(0_i64)
    .bind(false)
    .execute(&pool)
    .await
    .expect("series tachiyomi page-zero read progress row should insert");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/series/series-1/read-progress/tachiyomi")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series tachiyomi page-zero request should build"),
        )
        .await
        .expect("series tachiyomi page-zero request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "booksCount": 1,
            "booksReadCount": 0,
            "booksUnreadCount": 0,
            "booksInProgressCount": 1,
            "lastReadContinuousNumberSort": 0.0,
            "maxNumberSort": 1.0,
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_file_returns_empty_zip_when_all_series_files_are_missing() {
    let paths = new_router_fixture("router-series-file-empty-zip").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1/file")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series file request should build"),
        )
        .await
        .expect("series file request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/zip")
    );
    let zip_body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("series file body should be readable");
    let archive = ZipArchive::new(Cursor::new(zip_body.to_vec()))
        .expect("series file body should be a readable zip archive");
    assert_eq!(archive.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_file_delete_enqueues_delete_series_without_group_id() {
    let paths = new_router_fixture("router-series-file-delete-group-null").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/series/series-1/file")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series file delete request should build"),
        )
        .await
        .expect("series file delete request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for series delete verification");
    let rows = sqlx::query("SELECT SIMPLE_TYPE FROM TASK ORDER BY ID ASC")
        .fetch_all(&tasks_pool)
        .await
        .expect("series delete task rows should be queryable");
    tasks_pool.close().await;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String, _>("SIMPLE_TYPE"), "DELETE_SERIES");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_analyze_enqueues_book_tasks_grouped_by_series_id() {
    let paths = new_router_fixture("router-series-analyze-group-series-id").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/series-1/analyze")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series analyze request should build"),
        )
        .await
        .expect("series analyze request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for series analyze verification");
    let rows = sqlx::query("SELECT SIMPLE_TYPE, GROUP_ID FROM TASK ORDER BY ID ASC")
        .fetch_all(&tasks_pool)
        .await
        .expect("series analyze task rows should be queryable");
    tasks_pool.close().await;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String, _>("SIMPLE_TYPE"), "ANALYZE_BOOK");
    assert_eq!(
        rows[0].get::<Option<String>, _>("GROUP_ID"),
        Some("series-1".to_string())
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_metadata_refresh_enqueues_kotlin_style_task_groups() {
    let paths = new_router_fixture("router-series-metadata-refresh-groups").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/series-1/metadata/refresh")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series metadata refresh request should build"),
        )
        .await
        .expect("series metadata refresh request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for series metadata refresh verification");
    let rows = sqlx::query("SELECT SIMPLE_TYPE, GROUP_ID FROM TASK ORDER BY ID ASC")
        .fetch_all(&tasks_pool)
        .await
        .expect("series metadata refresh task rows should be queryable");
    tasks_pool.close().await;

    assert_eq!(rows.len(), 3);
    assert_eq!(
        rows[0].get::<String, _>("SIMPLE_TYPE"),
        "REFRESH_BOOK_LOCAL_ARTWORK"
    );
    assert_eq!(
        rows[1].get::<String, _>("SIMPLE_TYPE"),
        "REFRESH_BOOK_METADATA"
    );
    assert_eq!(
        rows[1].get::<Option<String>, _>("GROUP_ID"),
        Some("series-1".to_string())
    );
    assert_eq!(
        rows[2].get::<String, _>("SIMPLE_TYPE"),
        "REFRESH_SERIES_LOCAL_ARTWORK"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_series_metadata_refresh_does_not_canonicalize_series_id() {
    let paths = new_router_fixture("router-series-metadata-refresh-no-canonicalize").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "custom-series-2", "Series 2", "library-1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/series-2/metadata/refresh")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series metadata refresh alias request should build"),
        )
        .await
        .expect("series metadata refresh alias request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for series metadata refresh alias verification");
    let rows = sqlx::query("SELECT ID, SIMPLE_TYPE, GROUP_ID FROM TASK ORDER BY ID ASC")
        .fetch_all(&tasks_pool)
        .await
        .expect("series metadata refresh alias task rows should be queryable");
    tasks_pool.close().await;

    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get::<String, _>("ID"),
        "REFRESH_SERIES_LOCAL_ARTWORK:series-2"
    );
    assert_eq!(
        rows[0].get::<String, _>("SIMPLE_TYPE"),
        "REFRESH_SERIES_LOCAL_ARTWORK"
    );
    assert_eq!(rows[0].get::<Option<String>, _>("GROUP_ID"), None);

    cleanup_router_fixture(paths);
}
