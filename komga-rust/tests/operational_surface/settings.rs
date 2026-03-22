use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use tower::ServiceExt;

use super::basic_auth;

#[tokio::test]
async fn claim_status_is_public_and_reports_server_as_claimed() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/claim")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, serde_json::json!({"isClaimed": true}));
}

#[tokio::test]
async fn global_client_settings_list_is_public_and_shape_compatible_for_login_view() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/client-settings/global/list")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["webui.oauth2.hide_login"]["value"], "false");
    assert_eq!(json["webui.oauth2.hide_login"]["allowUnauthorized"], true);
    assert_eq!(json["webui.oauth2.auto_login"]["value"], "false");
    assert_eq!(json["webui.oauth2.auto_login"]["allowUnauthorized"], true);
}

#[tokio::test]
async fn oauth2_providers_list_is_public_and_returns_empty_array_for_startup_compat() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/oauth2/providers")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, serde_json::json!([]));
}

#[tokio::test]
async fn user_client_settings_list_requires_auth_and_returns_object_for_startup_flow() {
    let app = komga_rust::app::build_router();

    let unauthorized = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/client-settings/user/list")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/client-settings/user/list")
                .header(header::AUTHORIZATION, basic_auth("admin@example.org:admin"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, serde_json::json!({}));
}

#[tokio::test]
async fn delete_tasks_requires_admin_and_returns_zero_for_compat_runtime() {
    let app = komga_rust::app::build_router();

    let unauthorized = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/tasks")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let forbidden = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/tasks")
                .header(header::AUTHORIZATION, basic_auth("user@example.org:user"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/tasks")
                .header(header::AUTHORIZATION, basic_auth("admin@example.org:admin"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let deleted_count: u64 = serde_json::from_slice(&body).unwrap();
    assert_eq!(deleted_count, 0);
}

#[tokio::test]
async fn admin_can_read_settings_with_webui_operational_fields() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settings")
                .header(header::AUTHORIZATION, basic_auth("admin@example.org:admin"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["taskPoolSize"], 1);
    assert_eq!(json["serverPort"]["configurationSource"], Value::Null);
    assert_eq!(json["serverPort"]["databaseSource"], Value::Null);
    assert_eq!(json["serverPort"]["effectiveValue"], 25600);
    assert_eq!(json["serverContextPath"]["configurationSource"], "");
    assert_eq!(json["serverContextPath"]["databaseSource"], Value::Null);
    assert_eq!(json["serverContextPath"]["effectiveValue"], "");
    assert_eq!(json["kepubifyPath"]["effectiveValue"], Value::Null);
}

#[tokio::test]
async fn settings_surface_exposes_runtime_startup_contract_for_docker_profile() {
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "KOMGA_RUST_PLATFORM_PROFILE".to_string(),
        "docker".to_string(),
    );
    env.insert("KOMGA_CONFIG_DIR".to_string(), "/config".to_string());

    let config = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli::default(),
        &env,
    )
    .expect("runtime config should resolve");
    let app = komga_rust::app::build_router_with_config(&config);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settings")
                .header(header::AUTHORIZATION, basic_auth("admin@example.org:admin"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["serverPort"]["effectiveValue"], 25600);
    assert_eq!(json["serverContextPath"]["configurationSource"], "");
    assert_eq!(json["serverContextPath"]["effectiveValue"], "");
    assert_eq!(
        json["kepubifyPath"]["configurationSource"],
        "/usr/bin/kepubify"
    );
    assert_eq!(json["kepubifyPath"]["databaseSource"], Value::Null);
    assert_eq!(json["kepubifyPath"]["effectiveValue"], "/usr/bin/kepubify");
}

#[tokio::test]
async fn admin_can_update_key_settings_fields_and_read_them_back() {
    let app = komga_rust::app::build_router();

    let patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/settings")
                .header(header::AUTHORIZATION, basic_auth("admin@example.org:admin"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"taskPoolSize":4,"serverPort":4567,"serverContextPath":"/komga-rust"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(patch.status(), StatusCode::NO_CONTENT);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/settings")
                .header(header::AUTHORIZATION, basic_auth("admin@example.org:admin"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["taskPoolSize"], 4);
    assert_eq!(json["serverPort"]["databaseSource"], 4567);
    assert_eq!(json["serverPort"]["effectiveValue"], 4567);
    assert_eq!(json["serverContextPath"]["databaseSource"], "/komga-rust");
    assert_eq!(json["serverContextPath"]["effectiveValue"], "/komga-rust");
}

#[tokio::test]
async fn settings_patch_rejects_invalid_operational_values() {
    let app = komga_rust::app::build_router();

    for payload in [
        r#"{"taskPoolSize":0}"#,
        r#"{"serverPort":0}"#,
        r#"{"serverPort":65536}"#,
        r#"{"serverContextPath":"noslash"}"#,
        r#"{"serverContextPath":"/slash-end/"}"#,
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/settings")
                    .header(header::AUTHORIZATION, basic_auth("admin@example.org:admin"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(payload))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "payload {payload} should be rejected"
        );
    }
}
