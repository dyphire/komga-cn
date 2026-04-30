use super::*;

#[tokio::test]
async fn router_discovery_book_detail_includes_persisted_authors_tags_and_read_progress() {
    let ctx = TestFixture::builder("router-discovery-book-detail-persisted-metadata")
        .with_seed(|paths| async move {
            seed_router_read_progress(&paths, true).await;
        })
        .build()
        .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail request should build"),
        )
        .await
        .expect("book detail request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("authors"))
            .and_then(Value::as_array)
            .and_then(|authors| authors.first())
            .and_then(|author| author.get("name")),
        Some(&Value::String("Jane Writer".to_string())),
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("authors"))
            .and_then(Value::as_array)
            .and_then(|authors| authors.first())
            .and_then(|author| author.get("role")),
        Some(&Value::String("writer".to_string())),
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("tags"))
            .and_then(Value::as_array)
            .and_then(|tags| tags.first()),
        Some(&Value::String("favorite-tag".to_string())),
    );
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|progress| progress.get("page")),
        Some(&json!(10)),
    );
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|progress| progress.get("completed")),
        Some(&Value::Bool(true)),
    );
}

#[tokio::test]
async fn router_discovery_book_detail_accepts_basic_auth_like_kotlin_clients() {
    let ctx = TestFixture::new("router-discovery-book-detail-basic-auth-compat").await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header(
                    header::AUTHORIZATION,
                    basic_authorization_header_value(
                        "admin@example.org",
                        "router-contract-admin-123",
                    ),
                )
                .header("x-auth-token", "")
                .body(Body::empty())
                .expect("book detail basic-auth request should build"),
        )
        .await
        .expect("book detail basic-auth request should complete");

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn router_discovery_book_detail_exposes_oneshot_flag_from_persisted_book_rows() {
    let ctx = TestFixture::new("router-discovery-book-detail-oneshot-flag").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("book detail oneshot db should open");
    sqlx::query("UPDATE BOOK SET ONESHOT = ? WHERE ID = ?")
        .bind(1_i64)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book oneshot flag should update for detail contract");
    sqlx::query("UPDATE SERIES SET ONESHOT = ? WHERE ID = ?")
        .bind(1_i64)
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series oneshot flag should update for detail contract consistency");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail oneshot request should build"),
        )
        .await
        .expect("book detail oneshot request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(payload.get("oneshot"), Some(&Value::Bool(true)));
}

#[tokio::test]
async fn router_discovery_book_detail_preserves_empty_read_progress_device_fields() {
    let ctx = TestFixture::builder("router-discovery-book-detail-empty-read-progress-device")
        .with_seed(|paths| async move {
            seed_router_read_progress(&paths, false).await;
        })
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("book detail device parity db should open");
    sqlx::query(
        "UPDATE READ_PROGRESS SET DEVICE_ID = '', DEVICE_NAME = '' WHERE BOOK_ID = ? AND USER_ID = ?",
    )
    .bind("book-1")
    .bind("admin-user")
    .execute(&pool)
    .await
    .expect("read progress device fields should update");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail request should build"),
        )
        .await
        .expect("book detail request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|progress| progress.get("deviceId")),
        Some(&Value::String(String::new()))
    );
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|progress| progress.get("deviceName")),
        Some(&Value::String(String::new()))
    );
}

#[tokio::test]
async fn router_discovery_book_detail_converts_admin_url_to_file_path() {
    let ctx = TestFixture::new("router-discovery-book-detail-admin-url-path").await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("book detail url parity db should open");
    sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
        .bind("file:/library%20root/books/book%201.cbz")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book url should update");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail request should build"),
        )
        .await
        .expect("book detail request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload.get("url"),
        Some(&Value::String("/library root/books/book 1.cbz".to_string()))
    );
}

#[tokio::test]
async fn router_discovery_book_detail_formats_file_last_modified_as_utc_timestamp() {
    let ctx = TestFixture::new("router-discovery-book-detail-file-last-modified-utc").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail request should build"),
        )
        .await
        .expect("book detail request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload.get("fileLastModified"),
        Some(&Value::String("1970-01-01T00:00:00Z".to_string()))
    );
}

#[tokio::test]
async fn router_discovery_book_detail_does_not_bridge_missing_book_n_ids() {
    let ctx = TestFixture::new("router-discovery-book-detail-no-bridge-id").await;
    seed_router_primary_series_cbz_book(
        ctx.paths(),
        "book-z-2",
        "book-z-2.cbz",
        "Second Real Book",
    )
    .await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail bridge-id request should build"),
        )
        .await
        .expect("book detail bridge-id request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn router_book_metadata_batch_update_persists_title_and_updates_book_snapshot() {
    let ctx = TestFixture::new("router-book-metadata-batch-update-persists-and-touches-book").await;

    let pool_before = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open before metadata batch update");
    let last_modified_before = sqlx::query(
        "SELECT COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM BOOK WHERE ID = ? LIMIT 1",
    )
    .bind("book-1")
    .fetch_one(&pool_before)
    .await
    .expect("book last modified should be queryable before metadata batch update")
    .get::<String, _>("LAST_MODIFIED");
    pool_before.close().await;

    let auth_token = ctx.login_admin().await;

    let patch = json!({
        "book-1": {
            "title": "Updated Batch Title"
        }
    });

    let update = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(patch.to_string()))
                .expect("book metadata batch update request should build"),
        )
        .await
        .expect("book metadata batch update request should complete");

    assert_eq!(update.status(), StatusCode::NO_CONTENT);

    let detail = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail after metadata batch update request should build"),
        )
        .await
        .expect("book detail after metadata batch update request should complete");

    assert_eq!(detail.status(), StatusCode::OK);
    let payload = response_json(detail).await;
    assert_eq!(
        payload.get("metadata").and_then(|value| value.get("title")),
        Some(&Value::String("Updated Batch Title".to_string()))
    );

    let pool_after = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("main db should open after metadata batch update");
    let last_modified_after = sqlx::query(
        "SELECT COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM BOOK WHERE ID = ? LIMIT 1",
    )
    .bind("book-1")
    .fetch_one(&pool_after)
    .await
    .expect("book last modified should be queryable after metadata batch update")
    .get::<String, _>("LAST_MODIFIED");
    pool_after.close().await;
    assert_ne!(last_modified_after, last_modified_before);
}

#[tokio::test]
async fn router_book_metadata_batch_update_refreshes_book_search_results() {
    let ctx = TestFixture::new("router-book-metadata-batch-update-refreshes-search").await;

    let auth_token = ctx.login_admin().await;

    let initial_search = ctx
        .app()
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
                            "type": "Title",
                            "operator": "is",
                            "value": "Book 1"
                        }
                    })
                    .to_string(),
                ))
                .expect("initial books/list title search request should build"),
        )
        .await
        .expect("initial books/list title search request should complete");
    assert_eq!(initial_search.status(), StatusCode::OK);
    let initial_payload = response_json(initial_search).await;
    let initial_content = initial_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("initial books/list title search should expose content array");
    assert_eq!(initial_content.len(), 1);
    assert_eq!(
        initial_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    let patch = json!({
        "book-1": {
            "title": "Updated Batch Title"
        }
    });
    let update = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(patch.to_string()))
                .expect("book metadata batch update request should build"),
        )
        .await
        .expect("book metadata batch update request should complete");
    assert_eq!(update.status(), StatusCode::NO_CONTENT);

    let updated_search = ctx
        .app()
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
                            "type": "Title",
                            "operator": "is",
                            "value": "Updated Batch Title"
                        }
                    })
                    .to_string(),
                ))
                .expect("updated books/list title search request should build"),
        )
        .await
        .expect("updated books/list title search request should complete");
    assert_eq!(updated_search.status(), StatusCode::OK);
    let updated_payload = response_json(updated_search).await;
    let updated_content = updated_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("updated books/list title search should expose content array");
    assert_eq!(updated_content.len(), 1);
    assert_eq!(
        updated_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );
}

#[tokio::test]
async fn router_book_metadata_update_refreshes_book_full_text_search_results() {
    let ctx = TestFixture::new("router-book-metadata-update-refreshes-search").await;

    let auth_token = ctx.login_admin().await;

    let initial_search = ctx
        .app()
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
                        "fullTextSearch": "Updated Single Title"
                    })
                    .to_string(),
                ))
                .expect("initial single metadata search request should build"),
        )
        .await
        .expect("initial single metadata search request should complete");
    assert_eq!(initial_search.status(), StatusCode::OK);
    let initial_payload = response_json(initial_search).await;
    let initial_content = initial_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("initial single metadata search should expose content array");
    assert!(initial_content.is_empty());

    let update = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "title": "Updated Single Title"
                    })
                    .to_string(),
                ))
                .expect("single book metadata update request should build"),
        )
        .await
        .expect("single book metadata update request should complete");
    assert_eq!(update.status(), StatusCode::NO_CONTENT);

    let updated_search = ctx
        .app()
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
                        "fullTextSearch": "Updated Single Title"
                    })
                    .to_string(),
                ))
                .expect("updated single metadata search request should build"),
        )
        .await
        .expect("updated single metadata search request should complete");
    assert_eq!(updated_search.status(), StatusCode::OK);
    let updated_payload = response_json(updated_search).await;
    let updated_content = updated_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("updated single metadata search should expose content array");
    assert_eq!(updated_content.len(), 1);
    assert_eq!(
        updated_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );
}

#[tokio::test]
async fn router_book_metadata_update_rejects_invalid_isbn_values() {
    let ctx = TestFixture::new("router-book-metadata-update-invalid-isbn").await;

    let auth_token = ctx.login_admin().await;

    for invalid_isbn in ["1617290459", "978-123-456-789-6"] {
        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/books/book-1/metadata")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "isbn": invalid_isbn,
                        })
                        .to_string(),
                    ))
                    .expect("invalid isbn metadata update request should build"),
            )
            .await
            .expect("invalid isbn metadata update request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn router_book_metadata_update_rejects_blank_title_and_number_values() {
    let ctx = TestFixture::new("router-book-metadata-update-blank-title-number").await;

    let auth_token = ctx.login_admin().await;

    for payload in [json!({ "title": "" }), json!({ "number": "" })] {
        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/books/book-1/metadata")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(payload.to_string()))
                    .expect("blank title/number metadata update request should build"),
            )
            .await
            .expect("blank title/number metadata update request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn router_book_metadata_update_returns_not_found_for_missing_book() {
    let ctx = TestFixture::new("router-book-metadata-update-missing-book").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/missing/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "summary": "ignored" }).to_string()))
                .expect("missing book metadata update request should build"),
        )
        .await
        .expect("missing book metadata update request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn router_book_metadata_update_rejects_invalid_link_urls() {
    let ctx = TestFixture::new("router-book-metadata-update-invalid-link-url").await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "links": [
                            {
                                "label": "AniList",
                                "url": "not-a-url"
                            }
                        ]
                    })
                    .to_string(),
                ))
                .expect("invalid link url metadata update request should build"),
        )
        .await
        .expect("invalid link url metadata update request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn router_book_metadata_update_accepts_normalized_valid_isbn_13() {
    let ctx = TestFixture::new("router-book-metadata-update-valid-isbn-13").await;

    let auth_token = ctx.login_admin().await;

    let update = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "isbn": "978-161-729-045-9abc xxxoefj",
                    })
                    .to_string(),
                ))
                .expect("valid isbn metadata update request should build"),
        )
        .await
        .expect("valid isbn metadata update request should complete");
    assert_eq!(update.status(), StatusCode::NO_CONTENT);

    let detail = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail after valid isbn metadata update request should build"),
        )
        .await
        .expect("book detail after valid isbn metadata update request should complete");
    assert_eq!(detail.status(), StatusCode::OK);

    let payload = response_json(detail).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("isbn")),
        Some(&Value::String("9781617290459".to_string()))
    );
}
