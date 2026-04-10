use super::*;

#[tokio::test]
async fn router_book_read_progress_requires_page_when_completed_is_false_or_missing() {
    let paths = new_router_fixture("router-book-read-progress-requires-page").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for body in [
        json!({}),
        json!({ "completed": false }),
        json!({ "page": Value::Null }),
        json!({ "page": Value::Null, "completed": false }),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/books/book-1/read-progress")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("book read-progress missing-page request should build"),
            )
            .await
            .expect("book read-progress missing-page request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let payload = response_json(response).await;
        assert_eq!(payload.get("violations"), Some(&json!([])));
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_read_progress_rejects_non_positive_page_with_validation_payload() {
    let paths = new_router_fixture("router-book-read-progress-non-positive-page").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for body in [
        json!({ "page": 0 }),
        json!({ "page": -1 }),
        json!({ "page": 0, "completed": true }),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/books/book-1/read-progress")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("book read-progress non-positive page request should build"),
            )
            .await
            .expect("book read-progress non-positive page request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let payload = response_json(response).await;
        assert_eq!(
            payload.get("violations"),
            Some(&json!([
                {
                    "fieldName": "page",
                    "message": "must be greater than 0"
                }
            ]))
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_read_progress_completed_true_ignores_positive_page_and_marks_completed() {
    let paths = new_router_fixture("router-book-read-progress-completed-true-ignores-page").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for page in [5, 999] {
        let update = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/books/book-1/read-progress")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({ "page": page, "completed": true }).to_string(),
                    ))
                    .expect("book read-progress completed-true with page request should build"),
            )
            .await
            .expect("book read-progress completed-true with page request should complete");

        assert_eq!(update.status(), StatusCode::NO_CONTENT, "page={page}");

        let detail = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/books/book-1")
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("book detail after completed-true with page request should build"),
            )
            .await
            .expect("book detail after completed-true with page request should complete");

        assert_eq!(detail.status(), StatusCode::OK, "page={page}");
        let payload = response_json(detail).await;
        assert_eq!(
            payload
                .get("readProgress")
                .and_then(|value| value.get("page")),
            Some(&Value::from(10)),
            "page={page}"
        );
        assert_eq!(
            payload
                .get("readProgress")
                .and_then(|value| value.get("completed")),
            Some(&Value::Bool(true)),
            "page={page}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_read_progress_rejects_page_beyond_page_count_with_specific_error() {
    let paths = new_router_fixture("router-book-read-progress-page-out-of-range").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "page": 999 }).to_string()))
                .expect("book read-progress out-of-range request should build"),
        )
        .await
        .expect("book read-progress out-of-range request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "Page argument (999) must be within 1 and book page count (10)".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_read_progress_marks_completed_when_page_equals_last_page() {
    let paths = new_router_fixture("router-book-read-progress-last-page-completes").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "page": 10 }).to_string()))
                .expect("book read-progress last-page request should build"),
        )
        .await
        .expect("book read-progress last-page request should complete");

    assert_eq!(update.status(), StatusCode::NO_CONTENT);

    let detail = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail after last-page read-progress request should build"),
        )
        .await
        .expect("book detail after last-page read-progress request should complete");

    assert_eq!(detail.status(), StatusCode::OK);
    let payload = response_json(detail).await;
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|value| value.get("page")),
        Some(&Value::from(10))
    );
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|value| value.get("completed")),
        Some(&Value::Bool(true))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_read_progress_persists_epub_locator_for_page_updates() {
    let paths = new_router_fixture("router-book-read-progress-persists-epub-locator").await;
    seed_router_contract_data(&paths).await;

    let positions = json!([
        {
            "href": "/book-1.xhtml#kobo.1.1",
            "type": "application/xhtml+xml",
            "locations": {
                "position": 1,
                "progression": 0.0,
                "totalProgression": 0.1
            }
        },
        {
            "href": "/book-1.xhtml#kobo.2.1",
            "type": "application/xhtml+xml",
            "locations": {
                "position": 2,
                "progression": 0.5,
                "totalProgression": 0.2
            }
        }
    ]);

    let extension_blob = fixture_epub_positions_extension_blob();

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub locator seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(extension_blob)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for read-progress locator test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "page": 2 }).to_string()))
                .expect("book read-progress epub locator request should build"),
        )
        .await
        .expect("book read-progress epub locator request should complete");

    assert_eq!(update.status(), StatusCode::NO_CONTENT);

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub locator verification");
    let locator_row =
        sqlx::query("SELECT LOCATOR FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1")
            .bind("book-1")
            .bind("admin-user")
            .fetch_one(&verify_pool)
            .await
            .expect("read progress locator should be queryable");
    let locator_blob = locator_row
        .try_get::<Option<Vec<u8>>, _>("LOCATOR")
        .or_else(|_| locator_row.try_get::<Option<Vec<u8>>, _>("locator"))
        .expect("read progress locator column should be readable");
    verify_pool.close().await;

    let locator = locator_blob.as_deref().map(|blob| {
        serde_json::from_slice::<Value>(blob).expect("locator blob should be valid JSON")
    });
    assert_eq!(
        locator,
        positions.as_array().and_then(|items| items.get(1)).cloned()
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_read_progress_delete_clears_persisted_progress_and_koreader_view() {
    for (fixture_name, oneshot) in [
        ("router-book-read-progress-delete-clears-progress", false),
        ("router-book-read-progress-delete-oneshot-book", true),
    ] {
        let paths = new_router_fixture(fixture_name).await;
        seed_router_contract_data(&paths).await;
        seed_read_progress_delete_fixture(&paths, fixture_epub_positions_extension_blob(), oneshot)
            .await;

        let app = build_router_with_config(&runtime_config_for_paths(&paths));
        let auth_token = login_with_basic_and_get_token(app.clone()).await;

        let update = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/books/book-1/read-progress")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({ "page": 2 }).to_string()))
                    .expect("book read-progress setup request should build"),
            )
            .await
            .expect("book read-progress setup request should complete");
        assert_eq!(
            update.status(),
            StatusCode::NO_CONTENT,
            "fixture={fixture_name}"
        );

        let delete = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/api/v1/books/book-1/read-progress")
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("book read-progress delete request should build"),
            )
            .await
            .expect("book read-progress delete request should complete");
        assert_eq!(
            delete.status(),
            StatusCode::NO_CONTENT,
            "fixture={fixture_name}"
        );

        let detail = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/books/book-1")
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("book detail after read-progress delete request should build"),
            )
            .await
            .expect("book detail after read-progress delete request should complete");
        assert_eq!(detail.status(), StatusCode::OK, "fixture={fixture_name}");
        let detail_payload = response_json(detail).await;
        assert_eq!(
            detail_payload.get("readProgress"),
            Some(&Value::Null),
            "fixture={fixture_name}"
        );

        let koreader = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/koreader/syncs/progress/hash-book-1")
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("koreader progress after delete request should build"),
            )
            .await
            .expect("koreader progress after delete request should complete");
        assert_eq!(koreader.status(), StatusCode::OK, "fixture={fixture_name}");

        let verify_pool = connect_pool(paths.main_db.as_path(), 1)
            .await
            .expect("main db should open for read-progress delete verification");
        let remaining = sqlx::query(
            "SELECT COUNT(*) AS COUNT FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ?",
        )
        .bind("book-1")
        .bind("admin-user")
        .fetch_one(&verify_pool)
        .await
        .expect("read-progress delete verification query should succeed")
        .get::<i64, _>("COUNT");
        verify_pool.close().await;
        assert_eq!(remaining, 0, "fixture={fixture_name}");

        cleanup_router_fixture(paths);
    }
}

async fn seed_read_progress_delete_fixture(
    paths: &RuntimeDbPaths,
    extension_blob: Vec<u8>,
    oneshot: bool,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for read-progress delete setup");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(extension_blob)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for read-progress delete test");
    if oneshot {
        sqlx::query("UPDATE BOOK SET ONESHOT = ? WHERE ID = ?")
            .bind(1_i64)
            .bind("book-1")
            .execute(&pool)
            .await
            .expect("book oneshot flag should update for read-progress delete contract");
        sqlx::query("UPDATE SERIES SET ONESHOT = ? WHERE ID = ?")
            .bind(1_i64)
            .bind("series-1")
            .execute(&pool)
            .await
            .expect(
                "series oneshot flag should update for read-progress delete contract consistency",
            );
    }
    pool.close().await;
}
