use super::*;

#[tokio::test]
async fn router_book_media_asset_routes_forbid_age_restricted_user() {
    let paths = new_router_fixture("router-book-media-asset-restricted-user").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
        &["USER", "PAGE_STREAMING", "FILE_DOWNLOAD"],
    )
    .await;
    write_router_epub_resource(
        &paths,
        "books/book-1.epub",
        "OEBPS/chapter.xhtml",
        br#"<html xmlns='http://www.w3.org/1999/xhtml'><body>Restricted</body></html>"#,
    );

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    for route in [
        "/api/v1/books/book-1/file",
        "/api/v1/books/book-1/thumbnails",
        "/api/v1/books/book-1/manifest",
        "/api/v1/books/book-1/resource/OEBPS/chapter.xhtml",
        "/api/v1/books/book-1/progression",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("restricted media asset get request should build"),
            )
            .await
            .expect("restricted media asset get request should complete");

        assert_eq!(response.status(), StatusCode::FORBIDDEN, "route: {route}");
    }

    for route in ["/api/v1/books/book-1/progression"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "locator": {
                                "locations": {
                                    "progression": 0.25
                                }
                            }
                        })
                        .to_string(),
                    ))
                    .expect("restricted media asset put request should build"),
            )
            .await
            .expect("restricted media asset put request should complete");

        assert_eq!(response.status(), StatusCode::FORBIDDEN, "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_file_delete_enqueues_delete_book_even_when_book_is_missing() {
    let paths = new_router_fixture("router-book-file-delete-missing-book").await;
    seed_router_contract_data(&paths).await;

    // This contract inspects the queued TASK row itself, so runtime workers must stay off or the
    // background consumer can claim and delete the missing-book delete task before verification.
    let app = komga_server::app::build_router_without_runtime_workers_for_contract(
        &runtime_config_for_paths(&paths),
    )
    .await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/books/missing-book/file")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing book file delete request should build"),
        )
        .await
        .expect("missing book file delete request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let tasks_pool = connect_test_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for missing book file delete verification");
    let rows = sqlx::query(
        "SELECT ID, SIMPLE_TYPE, GROUP_ID, PRIORITY, PAYLOAD FROM TASK ORDER BY ID ASC",
    )
    .fetch_all(&tasks_pool)
    .await
    .expect("missing book delete task rows should be queryable");
    tasks_pool.close().await;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String, _>("ID"), "DELETE_BOOK_missing-book");
    assert_eq!(rows[0].get::<String, _>("SIMPLE_TYPE"), "DeleteBook");
    assert_eq!(rows[0].get::<Option<String>, _>("GROUP_ID"), None);
    assert_eq!(rows[0].get::<i32, _>("PRIORITY"), 8);
    assert_eq!(
        serde_json::from_str::<Value>(&rows[0].get::<String, _>("PAYLOAD"))
            .expect("missing book delete payload should be valid json"),
        json!({
            "bookId": "missing-book",
            "priority": 8,
            "groupId": Value::Null,
            "uniqueId": "DELETE_BOOK_missing-book"
        }),
    );

    cleanup_router_fixture(paths);
}
