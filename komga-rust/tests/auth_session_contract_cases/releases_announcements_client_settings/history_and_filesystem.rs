use super::*;

async fn insert_history_event(
    paths: &RuntimeDbPaths,
    id: &str,
    event_type: &str,
    book_id: Option<&str>,
    series_id: Option<&str>,
    timestamp: &str,
) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("history event db should open");
    sqlx::query(
        "INSERT INTO HISTORICAL_EVENT (ID, TYPE, BOOK_ID, SERIES_ID, TIMESTAMP) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(event_type)
    .bind(book_id)
    .bind(series_id)
    .bind(timestamp)
    .execute(&pool)
    .await
    .expect("history event should be inserted");
    pool.close().await;
}

#[tokio::test]
async fn router_get_history_honors_type_sort_override_like_kotlin() {
    let ctx = TestFixture::new("router-get-history-type-sort-override").await;
    insert_history_event(
        ctx.paths(),
        "event-series",
        "SERIES_ADDED",
        None,
        Some("series-1"),
        "2024-02-01T00:00:00Z",
    )
    .await;
    insert_history_event(
        ctx.paths(),
        "event-book",
        "BOOK_ADDED",
        Some("book-1"),
        None,
        "2024-01-01T00:00:00Z",
    )
    .await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/history?sort=type,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get history type-sort request should build"),
        )
        .await
        .expect("get history type-sort request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("history type-sort payload should expose content array");
    assert_eq!(
        content[0].get("id").and_then(Value::as_str),
        Some("event-book")
    );
    assert_eq!(
        content[1].get("id").and_then(Value::as_str),
        Some("event-series")
    );
}

#[tokio::test]
async fn router_get_history_maps_events_to_api_page_contract() {
    let ctx = TestFixture::new("router-get-history-api-page-contract").await;
    insert_history_event(
        ctx.paths(),
        "event-book",
        "BOOK_ADDED",
        Some("book-1"),
        None,
        "2024-01-01T00:00:00Z",
    )
    .await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/history")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get history api page request should build"),
        )
        .await
        .expect("get history api page request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "content": [{
                "id": "event-book",
                "type": "BOOK_ADDED",
                "bookId": "book-1",
                "seriesId": null,
                "timestamp": "2024-01-01T00:00:00",
                "properties": {},
            }],
            "pageable": {
                "pageNumber": 0,
                "pageSize": 20,
                "sort": {
                    "empty": false,
                    "sorted": true,
                    "unsorted": false,
                },
                "offset": 0,
                "paged": true,
                "unpaged": false,
            },
            "last": true,
            "totalElements": 1,
            "totalPages": 1,
            "first": true,
            "size": 20,
            "number": 0,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false,
            },
            "numberOfElements": 1,
            "empty": false,
        })
    );
}

#[tokio::test]
async fn router_get_history_marks_unknown_sort_as_unsorted_like_kotlin() {
    let ctx = TestFixture::new("router-get-history-unknown-sort-unsorted").await;
    insert_history_event(
        ctx.paths(),
        "event-1",
        "BOOK_ADDED",
        Some("book-1"),
        None,
        "2024-01-01T00:00:00Z",
    )
    .await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/history?sort=unknown,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get history unknown-sort request should build"),
        )
        .await
        .expect("get history unknown-sort request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("sort"),
        Some(&json!({
            "empty": true,
            "sorted": false,
            "unsorted": true,
        }))
    );
    assert_eq!(
        payload.get("pageable").and_then(|value| value.get("sort")),
        Some(&json!({
            "empty": true,
            "sorted": false,
            "unsorted": true,
        }))
    );
}

#[cfg(windows)]
fn expected_root_directories() -> Vec<Value> {
    ('A'..='Z')
        .map(|drive| format!("{drive}:\\"))
        .filter(|root| std::path::Path::new(root).exists())
        .map(|root| {
            json!({
                "type": "directory",
                "name": root,
                "path": root,
            })
        })
        .collect()
}

#[cfg(not(windows))]
fn expected_root_directories() -> Vec<Value> {
    vec![json!({
        "type": "directory",
        "name": "/",
        "path": "/",
    })]
}

#[tokio::test]
async fn router_post_filesystem_returns_unauthorized_for_anonymous_user_like_kotlin() {
    let ctx = TestFixture::new("router-post-filesystem-anonymous-unauthorized").await;

    let app = ctx.app().clone();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/filesystem")
                .body(Body::empty())
                .expect("post filesystem anonymous request should build"),
        )
        .await
        .expect("post filesystem anonymous request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn router_post_filesystem_returns_forbidden_for_regular_user_like_kotlin() {
    let ctx = TestFixture::new("router-post-filesystem-regular-user-forbidden").await;
    seed_router_library_restricted_user(
        ctx.paths(),
        "filesystem-user",
        "filesystem@example.org",
        "router-contract-filesystem-123",
        &["library-1"],
    )
    .await;

    let app = ctx.app().clone();
    let auth_token = ctx
        .login_with_credentials("filesystem@example.org", "router-contract-filesystem-123")
        .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/filesystem")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("post filesystem regular-user request should build"),
        )
        .await
        .expect("post filesystem regular-user request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn router_post_filesystem_empty_body_returns_root_directories_like_kotlin() {
    let ctx = TestFixture::new("router-post-filesystem-empty-body-root").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/filesystem")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("post filesystem empty-body request should build"),
        )
        .await
        .expect("post filesystem empty-body request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert!(payload.get("parent").is_none());
    assert!(payload.get("path").is_none());
    assert_eq!(payload.get("files"), Some(&json!([])));
    assert_eq!(
        payload.get("directories"),
        Some(&Value::Array(expected_root_directories()))
    );
}

#[tokio::test]
async fn router_post_filesystem_rejects_relative_path_even_when_it_exists_like_kotlin() {
    let ctx = TestFixture::new("router-post-filesystem-relative-path").await;
    std::fs::create_dir_all(ctx.paths().config_dir.join("relative-dir"))
        .expect("relative filesystem test directory should be created");

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/filesystem")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "path": "relative-dir" }).to_string()))
                .expect("post filesystem relative-path request should build"),
        )
        .await
        .expect("post filesystem relative-path request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn router_post_filesystem_absolute_directory_hides_hidden_entries_and_uses_parent_like_kotlin()
 {
    let ctx = TestFixture::new("router-post-filesystem-absolute-directory").await;

    let browse_dir = ctx.paths().config_dir.join("filesystem-browse");
    let visible_dir = browse_dir.join("VisibleDir");
    let hidden_dir = browse_dir.join(".hidden-dir");
    let visible_file = browse_dir.join("visible.txt");
    let hidden_file = browse_dir.join(".hidden.txt");
    std::fs::create_dir_all(&visible_dir).expect("visible browse directory should be created");
    std::fs::create_dir_all(&hidden_dir).expect("hidden browse directory should be created");
    std::fs::write(&visible_file, b"visible").expect("visible browse file should be written");
    std::fs::write(&hidden_file, b"hidden").expect("hidden browse file should be written");

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/filesystem")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "path": browse_dir.to_string_lossy().to_string(),
                        "showFiles": true,
                    })
                    .to_string(),
                ))
                .expect("post filesystem absolute-directory request should build"),
        )
        .await
        .expect("post filesystem absolute-directory request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("parent"),
        Some(&Value::String(
            ctx.paths().config_dir.to_string_lossy().to_string()
        ))
    );
    assert!(payload.get("path").is_none());
    assert_eq!(
        payload.get("directories"),
        Some(&json!([{
            "type": "directory",
            "name": "VisibleDir",
            "path": visible_dir.to_string_lossy().to_string(),
        }]))
    );
    assert_eq!(
        payload.get("files"),
        Some(&json!([{
            "type": "file",
            "name": "visible.txt",
            "path": visible_file.to_string_lossy().to_string(),
        }]))
    );
}

#[tokio::test]
async fn router_post_filesystem_returns_bad_request_for_nonexistent_absolute_path_like_kotlin() {
    let ctx = TestFixture::new("router-post-filesystem-nonexistent-path").await;
    let missing_parent = ctx.paths().config_dir.join("missing-parent");
    let missing_path = missing_parent.join("missing-child");

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/filesystem")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "path": missing_path.to_string_lossy().to_string() }).to_string(),
                ))
                .expect("post filesystem nonexistent-path request should build"),
        )
        .await
        .expect("post filesystem nonexistent-path request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}
