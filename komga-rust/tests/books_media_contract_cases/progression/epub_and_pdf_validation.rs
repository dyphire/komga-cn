use super::*;

#[tokio::test]
async fn router_book_progression_put_rejects_epub_locator_without_progression() {
    let paths = new_router_fixture("router-book-progression-put-epub-missing-progression").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "modified": "2024-01-04T05:06:07Z",
                        "device": { "id": "reader-1", "name": "KOReader" },
                        "locator": {
                            "href": "chapter.xhtml#frag",
                            "type": "application/xhtml+xml",
                            "locations": { "position": 15 }
                        }
                    })
                    .to_string(),
                ))
                .expect("epub progression without locator progression request should build"),
        )
        .await
        .expect("epub progression without locator progression request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "location.progression is required".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_rejects_epub_locator_when_extension_is_missing() {
    let paths = new_router_fixture("router-book-progression-put-epub-missing-extension").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "modified": "2024-01-04T05:06:07Z",
                        "device": { "id": "reader-1", "name": "KOReader" },
                        "locator": {
                            "href": "chapter.xhtml#frag",
                            "type": "application/xhtml+xml",
                            "locations": { "progression": 0.3 }
                        }
                    })
                    .to_string(),
                ))
                .expect("epub progression without extension request should build"),
        )
        .await
        .expect("epub progression without extension request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Epub extension not found".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_rejects_epub_locator_with_non_existing_href() {
    let paths = new_router_fixture("router-book-progression-put-epub-bad-href").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for progression bad-href seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for progression bad-href test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "modified": "2024-01-04T05:06:07Z",
                        "device": { "id": "reader-1", "name": "KOReader" },
                        "locator": {
                            "href": "ch5.xhtml#frag",
                            "type": "application/xhtml+xml",
                            "locations": { "progression": 0.3 }
                        }
                    })
                    .to_string(),
                ))
                .expect("epub progression bad href request should build"),
        )
        .await
        .expect("epub progression bad href request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "Resource does not exist in book: ch5.xhtml".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_accepts_pdf_position_payload() {
    let paths = new_router_fixture("router-book-progression-put-pdf-position").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": { "id": "reader-9", "name": "Kobo Libra" },
        "locator": {
            "href": "",
            "type": "",
            "locations": { "position": 1 }
        }
    });

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-pdf-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(progression.to_string()))
                .expect("pdf progression put request should build"),
        )
        .await
        .expect("pdf progression put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/progression")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("pdf progression get request should build"),
        )
        .await
        .expect("pdf progression get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);
    let payload = response_json(get_response).await;
    assert_eq!(payload.get("modified"), progression.get("modified"));
    assert_eq!(payload.get("device"), progression.get("device"));
    assert_eq!(payload.get("locator"), progression.get("locator"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_rejects_pdf_position_beyond_page_count() {
    let paths = new_router_fixture("router-book-progression-put-pdf-out-of-range").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-pdf-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "modified": "2024-01-04T05:06:07Z",
                        "device": { "id": "reader-9", "name": "Kobo Libra" },
                        "locator": {
                            "href": "",
                            "type": "",
                            "locations": { "position": 2 }
                        }
                    })
                    .to_string(),
                ))
                .expect("pdf progression out-of-range request should build"),
        )
        .await
        .expect("pdf progression out-of-range request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "Page argument (2) must be within 1 and book page count (1)".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}
