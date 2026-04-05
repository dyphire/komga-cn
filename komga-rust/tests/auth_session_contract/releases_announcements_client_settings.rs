use super::*;

#[tokio::test]
async fn router_get_font_file_downloads_embedded_font_without_auth_like_kotlin() {
    let paths = new_router_fixture("router-get-font-file-embedded-anonymous").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/fonts/resource/OpenDyslexic/OpenDyslexic-Bold.woff")
                .body(Body::empty())
                .expect("get embedded font file request should build"),
        )
        .await
        .expect("get embedded font file request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE),
        Some(&header::HeaderValue::from_static("font/woff"))
    );
    assert_eq!(
        response.headers().get(header::CONTENT_DISPOSITION),
        Some(&header::HeaderValue::from_static(
            "attachment; filename=\"OpenDyslexic-Bold.woff\"",
        ))
    );

    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("embedded font file response body should read");
    assert!(!bytes.is_empty());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_font_file_downloads_filesystem_font_without_auth_like_kotlin() {
    let paths = new_router_fixture("router-get-font-file-filesystem-anonymous").await;
    seed_router_contract_data(&paths).await;

    let family_dir = paths.config_dir.join("fonts").join("Custom Family");
    std::fs::create_dir_all(&family_dir).expect("custom font family dir should be created");
    std::fs::write(family_dir.join("Custom-Regular.ttf"), b"font-bytes")
        .expect("custom font file should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/fonts/resource/Custom%20Family/Custom-Regular.ttf")
                .body(Body::empty())
                .expect("get filesystem font file request should build"),
        )
        .await
        .expect("get filesystem font file request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE),
        Some(&header::HeaderValue::from_static("font/ttf"))
    );
    assert_eq!(
        response.headers().get(header::CONTENT_DISPOSITION),
        Some(&header::HeaderValue::from_static(
            "attachment; filename=\"Custom-Regular.ttf\"",
        ))
    );

    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("filesystem font file response body should read");
    assert_eq!(bytes.as_ref(), b"font-bytes");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_fonts_families_returns_unauthorized_for_anonymous_user_like_kotlin() {
    let paths = new_router_fixture("router-get-fonts-families-anonymous").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/fonts/families")
                .body(Body::empty())
                .expect("get fonts families anonymous request should build"),
        )
        .await
        .expect("get fonts families anonymous request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_fonts_families_returns_embedded_and_filesystem_families_like_kotlin() {
    let paths = new_router_fixture("router-get-fonts-families-embedded-and-filesystem").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "fonts-user",
        "fonts@example.org",
        "router-contract-fonts-123",
        &["library-1"],
    )
    .await;

    let family_dir = paths.config_dir.join("fonts").join("Custom Family");
    std::fs::create_dir_all(&family_dir).expect("custom font family dir should be created");
    std::fs::write(family_dir.join("Custom-Regular.ttf"), b"font-bytes")
        .expect("custom font file should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "fonts@example.org",
        "router-contract-fonts-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/fonts/families")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get fonts families authenticated request should build"),
        )
        .await
        .expect("get fonts families authenticated request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let families = payload
        .as_array()
        .expect("fonts families payload should be an array")
        .iter()
        .map(|value| {
            value
                .as_str()
                .expect("font family entry should be a string")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert!(families.contains(&"OpenDyslexic".to_string()));
    assert!(families.contains(&"Custom Family".to_string()));

    cleanup_router_fixture(paths);
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
    let paths = new_router_fixture("router-post-filesystem-anonymous-unauthorized").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_post_filesystem_returns_forbidden_for_regular_user_like_kotlin() {
    let paths = new_router_fixture("router-post-filesystem-regular-user-forbidden").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "filesystem-user",
        "filesystem@example.org",
        "router-contract-filesystem-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "filesystem@example.org",
        "router-contract-filesystem-123",
    )
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_post_filesystem_empty_body_returns_root_directories_like_kotlin() {
    let paths = new_router_fixture("router-post-filesystem-empty-body-root").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_post_filesystem_rejects_relative_path_even_when_it_exists_like_kotlin() {
    let paths = new_router_fixture("router-post-filesystem-relative-path").await;
    seed_router_contract_data(&paths).await;
    std::fs::create_dir_all(paths.config_dir.join("relative-dir"))
        .expect("relative filesystem test directory should be created");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_post_filesystem_absolute_directory_hides_hidden_entries_and_uses_parent_like_kotlin()
 {
    let paths = new_router_fixture("router-post-filesystem-absolute-directory").await;
    seed_router_contract_data(&paths).await;

    let browse_dir = paths.config_dir.join("filesystem-browse");
    let visible_dir = browse_dir.join("VisibleDir");
    let hidden_dir = browse_dir.join(".hidden-dir");
    let visible_file = browse_dir.join("visible.txt");
    let hidden_file = browse_dir.join(".hidden.txt");
    std::fs::create_dir_all(&visible_dir).expect("visible browse directory should be created");
    std::fs::create_dir_all(&hidden_dir).expect("hidden browse directory should be created");
    std::fs::write(&visible_file, b"visible").expect("visible browse file should be written");
    std::fs::write(&hidden_file, b"hidden").expect("hidden browse file should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

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
            paths.config_dir.to_string_lossy().to_string()
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_post_filesystem_returns_bad_request_for_nonexistent_absolute_path_like_kotlin() {
    let paths = new_router_fixture("router-post-filesystem-nonexistent-path").await;
    seed_router_contract_data(&paths).await;
    let missing_parent = paths.config_dir.join("missing-parent");
    let missing_path = missing_parent.join("missing-child");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_put_announcements_deduplicates_duplicate_ids() {
    let paths = new_router_fixture("router-put-announcements-deduplicates-ids").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/announcements")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"["announcement-1","announcement-1","announcement-2"]"#,
                ))
                .expect("put announcements duplicate ids request should build"),
        )
        .await
        .expect("put announcements duplicate ids request should complete");

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        load_announcement_read_ids_for_user(&paths, "admin-user").await,
        vec!["announcement-1".to_string(), "announcement-2".to_string()]
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_get_releases_returns_internal_error_when_upstream_fetch_fails() {
    let _guard = releases_env_lock()
        .lock()
        .expect("releases env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_RELEASES_URL").ok();
    unsafe {
        std::env::set_var("KOMGA_RUST_RELEASES_URL", "http://127.0.0.1:1/releases");
    }

    let paths = new_router_fixture("router-get-releases-upstream-failure").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/releases")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get releases request should build"),
        )
        .await
        .expect("get releases request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_RELEASES_URL", previous);
}

#[tokio::test]
async fn router_get_releases_returns_internal_error_for_non_array_payload() {
    let _guard = releases_env_lock()
        .lock()
        .expect("releases env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_RELEASES_URL").ok();

    let server =
        spawn_single_response_server(200, "application/json", r#"{"tag_name":"v1.0.0"}"#).await;
    unsafe {
        std::env::set_var("KOMGA_RUST_RELEASES_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-get-releases-non-array-payload").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/releases")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get releases non-array request should build"),
        )
        .await
        .expect("get releases non-array request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_RELEASES_URL", previous);
    server
        .join
        .await
        .expect("releases non-array mock server should finish");
}

#[tokio::test]
async fn router_get_releases_returns_internal_error_for_non_success_status_with_valid_array_body() {
    let _guard = releases_env_lock()
        .lock()
        .expect("releases env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_RELEASES_URL").ok();

    let server = spawn_single_response_server(
        503,
        "application/json",
        r#"[{"html_url":"https://example.com/release/1","tag_name":"v1.0.0","published_at":"2024-01-01T00:00:00Z","body":"desc","prerelease":false}]"#,
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_RELEASES_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-get-releases-non-success-valid-array").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/releases")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get releases non-success valid array request should build"),
        )
        .await
        .expect("get releases non-success valid array request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_RELEASES_URL", previous);
    server
        .join
        .await
        .expect("releases non-success valid-array mock server should finish");
}

#[tokio::test]
async fn router_get_announcements_returns_internal_error_when_upstream_fetch_fails() {
    let _guard = announcements_env_lock()
        .lock()
        .expect("announcements env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_ANNOUNCEMENTS_URL").ok();
    unsafe {
        std::env::set_var(
            "KOMGA_RUST_ANNOUNCEMENTS_URL",
            "http://127.0.0.1:1/feed.json",
        );
    }

    let paths = new_router_fixture("router-get-announcements-upstream-failure").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/announcements")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get announcements request should build"),
        )
        .await
        .expect("get announcements request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_ANNOUNCEMENTS_URL", previous);
}

#[tokio::test]
async fn router_get_announcements_does_not_passthrough_unknown_feed_fields() {
    let _guard = announcements_env_lock()
        .lock()
        .expect("announcements env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_ANNOUNCEMENTS_URL").ok();

    let server = spawn_single_response_server(
        200,
        "application/json",
        r#"{"version":"https://jsonfeed.org/version/1.1","title":"Komga News","home_page_url":"https://komga.org","description":"News","unexpected":"keep-me-out","items":[{"id":"announcement-1","url":"https://komga.org/post/1","title":"Hello","summary":"Summary","content_html":"<p>Hi</p>","date_modified":"2024-01-01T00:00:00Z","author":{"name":"Komga","url":"https://komga.org"},"tags":["news"],"unexpectedItemField":true}]}"#,
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_ANNOUNCEMENTS_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-get-announcements-known-dto-fields-only").await;
    seed_router_contract_data(&paths).await;
    seed_announcement_read_ids(&paths, "admin-user", &["announcement-1"]).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/announcements")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get announcements dto request should build"),
        )
        .await
        .expect("get announcements dto request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert!(payload.get("unexpected").is_none());
    let items = payload["items"]
        .as_array()
        .expect("announcements items should be an array");
    assert!(items[0].get("unexpectedItemField").is_none());
    assert_eq!(items[0]["date_modified"], "2024-01-01T00:00:00Z");
    assert_eq!(items[0]["_komga"]["read"], true);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_ANNOUNCEMENTS_URL", previous);
    server
        .join
        .await
        .expect("announcement mock server should finish");
}

#[tokio::test]
async fn router_get_announcements_returns_not_found_for_null_body_payload() {
    let _guard = announcements_env_lock()
        .lock()
        .expect("announcements env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_ANNOUNCEMENTS_URL").ok();

    let server = spawn_single_response_server(200, "application/json", "null").await;
    unsafe {
        std::env::set_var("KOMGA_RUST_ANNOUNCEMENTS_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-get-announcements-null-body").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/announcements")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get announcements null body request should build"),
        )
        .await
        .expect("get announcements null body request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_ANNOUNCEMENTS_URL", previous);
    server
        .join
        .await
        .expect("announcement null-body mock server should finish");
}

#[tokio::test]
async fn router_get_announcements_returns_internal_error_for_invalid_date_modified() {
    let _guard = announcements_env_lock()
        .lock()
        .expect("announcements env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_ANNOUNCEMENTS_URL").ok();

    let server = spawn_single_response_server(
        200,
        "application/json",
        r#"{"version":"https://jsonfeed.org/version/1.1","title":"Komga News","items":[{"id":"announcement-1","date_modified":"not-a-date"}]}"#,
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_ANNOUNCEMENTS_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-get-announcements-invalid-date-modified").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/announcements")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get announcements invalid date request should build"),
        )
        .await
        .expect("get announcements invalid date request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_ANNOUNCEMENTS_URL", previous);
    server
        .join
        .await
        .expect("announcement invalid-date mock server should finish");
}

#[tokio::test]
async fn router_get_announcements_returns_internal_error_for_non_success_upstream_status() {
    let _guard = announcements_env_lock()
        .lock()
        .expect("announcements env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_ANNOUNCEMENTS_URL").ok();

    let server = spawn_single_response_server(
        503,
        "application/json",
        r#"{"version":"https://jsonfeed.org/version/1.1","title":"Komga News","items":[]}"#,
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_ANNOUNCEMENTS_URL", server.url.clone());
    }

    let paths = new_router_fixture("router-get-announcements-non-success-status").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/announcements")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get announcements non-success request should build"),
        )
        .await
        .expect("get announcements non-success request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_ANNOUNCEMENTS_URL", previous);
    server
        .join
        .await
        .expect("announcement non-success mock server should finish");
}

#[tokio::test]
async fn router_client_settings_global_list_does_not_inject_missing_oauth_hide_login_default() {
    let paths = new_router_fixture("router-client-settings-global-list-no-synthetic-default").await;
    seed_router_contract_data(&paths).await;
    seed_global_client_setting(&paths, "public.setting", "public-value", true).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/client-settings/global/list")
                .body(Body::empty())
                .expect("client settings global list request should build"),
        )
        .await
        .expect("client settings global list request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let settings = payload
        .as_object()
        .expect("global client settings response should be an object");
    assert_eq!(settings["public.setting"]["value"], "public-value");
    assert!(settings.get("webui.oauth2.hide_login").is_none());

    cleanup_router_fixture(paths);
}
