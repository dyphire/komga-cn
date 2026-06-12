#![allow(clippy::await_holding_lock)]

use super::*;

#[tokio::test]
async fn router_put_announcements_deduplicates_duplicate_ids() {
    let ctx = TestFixture::new("router-put-announcements-deduplicates-ids").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

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
        load_announcement_read_ids_for_user(ctx.paths(), "admin-user").await,
        vec!["announcement-1".to_string(), "announcement-2".to_string()]
    );
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

    let ctx = TestFixture::new("router-get-releases-upstream-failure").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

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

    let ctx = TestFixture::new("router-get-releases-non-array-payload").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

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

    restore_env_var("KOMGA_RUST_RELEASES_URL", previous);
    server
        .join
        .await
        .expect("releases non-array mock server should finish");
}

#[tokio::test]
async fn router_get_releases_maps_success_payload_to_api_contract() {
    let _guard = releases_env_lock()
        .lock()
        .expect("releases env lock should not be poisoned");
    let previous = std::env::var("KOMGA_RUST_RELEASES_URL").ok();

    let server = spawn_single_response_server(
        200,
        "application/json",
        r#"[
            {
                "html_url": "https://example.com/release/2",
                "tag_name": "v2.0.0",
                "published_at": "2024-02-03T04:05:06Z",
                "body": "Second",
                "prerelease": true
            },
            {
                "html_url": "https://example.com/release/1",
                "tag_name": "v1.0.0",
                "published_at": "2024-01-02T03:04:05Z",
                "body": "First",
                "prerelease": false
            }
        ]"#,
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_RELEASES_URL", server.url.clone());
    }

    let ctx = TestFixture::new("router-get-releases-success-contract").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/releases")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("get releases success request should build"),
        )
        .await
        .expect("get releases success request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!([
            {
                "version": "v2.0.0",
                "releaseDate": "2024-02-03T04:05:06Z",
                "url": "https://example.com/release/2",
                "latest": true,
                "preRelease": true,
                "description": "Second"
            },
            {
                "version": "v1.0.0",
                "releaseDate": "2024-01-02T03:04:05Z",
                "url": "https://example.com/release/1",
                "latest": false,
                "preRelease": false,
                "description": "First"
            }
        ])
    );

    restore_env_var("KOMGA_RUST_RELEASES_URL", previous);
    server
        .join
        .await
        .expect("releases success mock server should finish");
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

    let ctx = TestFixture::new("router-get-releases-non-success-valid-array").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

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

    let ctx = TestFixture::new("router-get-announcements-upstream-failure").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

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

    let ctx = TestFixture::new("router-get-announcements-known-dto-fields-only").await;
    seed_announcement_read_ids(ctx.paths(), "admin-user", &["announcement-1"]).await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

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

    let ctx = TestFixture::new("router-get-announcements-null-body").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

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

    let ctx = TestFixture::new("router-get-announcements-invalid-date-modified").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

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

    let ctx = TestFixture::new("router-get-announcements-non-success-status").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

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

    restore_env_var("KOMGA_RUST_ANNOUNCEMENTS_URL", previous);
    server
        .join
        .await
        .expect("announcement non-success mock server should finish");
}

#[tokio::test]
async fn router_client_settings_global_list_does_not_inject_missing_oauth_hide_login_default() {
    let ctx = TestFixture::new("router-client-settings-global-list-no-synthetic-default").await;
    seed_global_client_setting(ctx.paths(), "public.setting", "public-value", true).await;

    let app = ctx.app().clone();

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
}
