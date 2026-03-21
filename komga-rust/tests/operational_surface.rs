use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use base64::Engine;
use serde_json::Value;
use tower::ServiceExt;

#[tokio::test]
async fn settings_and_actuator_routes_enforce_admin_access() {
    let app = komga_rust::app::build_router();

    for (method, uri) in [
        ("GET", "/api/v1/settings"),
        ("PATCH", "/api/v1/settings"),
        ("GET", "/actuator"),
        ("GET", "/actuator/info"),
        ("GET", "/actuator/metrics"),
        ("GET", "/actuator/metrics/komga.tasks.execution"),
        ("GET", "/actuator/logfile"),
        ("POST", "/actuator/shutdown"),
    ] {
        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            unauthorized.status(),
            StatusCode::UNAUTHORIZED,
            "{uri} should reject missing auth"
        );

        let forbidden = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header(header::AUTHORIZATION, basic_auth("user@example.org:user"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            forbidden.status(),
            StatusCode::FORBIDDEN,
            "{uri} should reject non-admin auth"
        );
    }
}

#[tokio::test]
async fn actuator_health_is_public() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/actuator/health")
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
    assert_eq!(json["status"], "UP");
}

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
async fn sse_events_route_requires_auth_and_accepts_session_cookie() {
    let app = komga_rust::app::build_router();

    let unauthorized = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/sse/v1/events")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/sse/v1/events")
                .header(header::COOKIE, "KOMGA-SESSION=komga-admin-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/event-stream"
    );
}

#[tokio::test]
async fn dev_frontend_origin_gets_cors_headers_on_normal_requests() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/claim")
                .header(header::ORIGIN, "http://127.0.0.1:8081")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "http://127.0.0.1:8081"
    );
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-credentials")
            .unwrap(),
        "true"
    );
    assert!(response.headers().get("vary").is_some());
}

#[tokio::test]
async fn dev_frontend_origin_gets_cors_headers_even_on_unauthorized_protected_requests() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(header::ORIGIN, "http://127.0.0.1:8081")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "http://127.0.0.1:8081"
    );
}

#[tokio::test]
async fn dev_frontend_preflight_request_succeeds_for_runtime_api() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/api/v2/users/me")
                .header(header::ORIGIN, "http://127.0.0.1:8081")
                .header("Access-Control-Request-Method", "GET")
                .header(
                    "Access-Control-Request-Headers",
                    "authorization,x-auth-token,content-type",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "http://127.0.0.1:8081"
    );
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-credentials")
            .unwrap(),
        "true"
    );
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-methods")
            .unwrap(),
        "GET,POST,PATCH,DELETE,OPTIONS"
    );
    let allow_headers = response
        .headers()
        .get("access-control-allow-headers")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(allow_headers.contains("authorization"));
    assert!(allow_headers.contains("x-auth-token"));
    assert!(allow_headers.contains("content-type"));
}

#[tokio::test]
async fn non_dev_origin_does_not_get_dev_cors_headers() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/claim")
                .header(header::ORIGIN, "http://malicious.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );
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

#[tokio::test]
async fn admin_can_read_actuator_info_and_download_logfile() {
    let app = komga_rust::app::build_router();

    let info = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/actuator/info")
                .header(header::AUTHORIZATION, basic_auth("admin@example.org:admin"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(info.status(), StatusCode::OK);
    let info_body = axum::body::to_bytes(info.into_body(), usize::MAX)
        .await
        .unwrap();
    let info_json: Value = serde_json::from_slice(&info_body).unwrap();
    assert_eq!(info_json["build"]["name"], "Komga");
    assert_eq!(info_json["build"]["artifact"], "komga");
    assert!(info_json["build"]["version"].is_string());
    assert!(info_json["git"]["branch"].is_string());
    assert!(info_json["git"]["commit"]["id"].is_string());

    let logfile = app
        .oneshot(
            Request::builder()
                .uri("/actuator/logfile")
                .header(header::AUTHORIZATION, basic_auth("admin@example.org:admin"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(logfile.status(), StatusCode::OK);
    assert_eq!(
        logfile.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/plain; charset=utf-8"
    );
    let logfile_body = axum::body::to_bytes(logfile.into_body(), usize::MAX)
        .await
        .unwrap();
    let logfile_text = String::from_utf8(logfile_body.to_vec()).unwrap();
    assert!(logfile_text.contains("komga-rust operational logfile"));
}

#[tokio::test]
async fn admin_can_trigger_shutdown_and_query_metrics_with_expected_names_and_tags() {
    let app = komga_rust::app::build_router();

    let metrics = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/actuator/metrics")
                .header(header::AUTHORIZATION, basic_auth("admin@example.org:admin"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(metrics.status(), StatusCode::OK);
    let metrics_body = axum::body::to_bytes(metrics.into_body(), usize::MAX)
        .await
        .unwrap();
    let metrics_json: Value = serde_json::from_slice(&metrics_body).unwrap();
    let names = metrics_json["names"].as_array().unwrap();
    for metric in [
        "komga.tasks.execution",
        "komga.tasks.failure",
        "komga.series",
        "komga.books",
        "komga.books.filesize",
        "komga.sidecars",
        "komga.collections",
        "komga.readlists",
    ] {
        assert!(
            names.iter().any(|name| name == metric),
            "metrics index should include {metric}"
        );
    }

    let tasks = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/actuator/metrics/komga.tasks.execution")
                .header(header::AUTHORIZATION, basic_auth("admin@example.org:admin"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(tasks.status(), StatusCode::OK);
    let tasks_body = axum::body::to_bytes(tasks.into_body(), usize::MAX)
        .await
        .unwrap();
    let tasks_json: Value = serde_json::from_slice(&tasks_body).unwrap();
    assert_eq!(tasks_json["name"], "komga.tasks.execution");
    assert_eq!(tasks_json["availableTags"][0]["tag"], "type");
    assert!(
        tasks_json["availableTags"][0]["values"]
            .as_array()
            .unwrap()
            .iter()
            .any(|value| value == "SCAN_LIBRARY")
    );

    let task_failures = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/actuator/metrics/komga.tasks.failure")
                .header(header::AUTHORIZATION, basic_auth("admin@example.org:admin"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(task_failures.status(), StatusCode::OK);
    let task_failures_body = axum::body::to_bytes(task_failures.into_body(), usize::MAX)
        .await
        .unwrap();
    let task_failures_json: Value = serde_json::from_slice(&task_failures_body).unwrap();
    assert_eq!(task_failures_json["name"], "komga.tasks.failure");
    assert_eq!(task_failures_json["measurements"][0]["statistic"], "COUNT");
    assert!(task_failures_json["measurements"][0]["value"].is_number());
    assert_eq!(task_failures_json["availableTags"], serde_json::json!([]));

    let series = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/actuator/metrics/komga.series?tag=library:1")
                .header(header::AUTHORIZATION, basic_auth("admin@example.org:admin"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(series.status(), StatusCode::OK);
    let series_body = axum::body::to_bytes(series.into_body(), usize::MAX)
        .await
        .unwrap();
    let series_json: Value = serde_json::from_slice(&series_body).unwrap();
    assert_eq!(series_json["name"], "komga.series");
    assert_eq!(series_json["measurements"][0]["statistic"], "VALUE");
    assert!(series_json["measurements"][0]["value"].is_number());

    let shutdown = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/actuator/shutdown")
                .header(header::AUTHORIZATION, basic_auth("admin@example.org:admin"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(shutdown.status(), StatusCode::OK);
    let shutdown_body = axum::body::to_bytes(shutdown.into_body(), usize::MAX)
        .await
        .unwrap();
    let shutdown_json: Value = serde_json::from_slice(&shutdown_body).unwrap();
    assert_eq!(shutdown_json["message"], "Shutting down, bye...");
}

#[tokio::test]
async fn admin_bootstrap_token_and_cookie_can_read_tasks_failure_metric() {
    let app = komga_rust::app::build_router();

    let bootstrap = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me?remember-me=false")
                .header(header::AUTHORIZATION, basic_auth("admin@example.org:admin"))
                .header("X-Auth-Token", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(bootstrap.status(), StatusCode::OK);
    let auth_token = bootstrap
        .headers()
        .get("x-auth-token")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let session_cookie = bootstrap
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    let metric_with_token = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/actuator/metrics/komga.tasks.failure")
                .header("X-Auth-Token", auth_token.as_str())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(metric_with_token.status(), StatusCode::OK);
    let token_body = axum::body::to_bytes(metric_with_token.into_body(), usize::MAX)
        .await
        .unwrap();
    let token_json: Value = serde_json::from_slice(&token_body).unwrap();
    assert_eq!(token_json["name"], "komga.tasks.failure");
    assert_eq!(token_json["measurements"][0]["statistic"], "COUNT");
    assert!(token_json["measurements"][0]["value"].is_number());
    assert_eq!(token_json["availableTags"], serde_json::json!([]));

    let metric_with_cookie = app
        .oneshot(
            Request::builder()
                .uri("/actuator/metrics/komga.tasks.failure")
                .header(header::COOKIE, session_cookie.as_str())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(metric_with_cookie.status(), StatusCode::OK);
}

fn basic_auth(credentials: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(credentials)
    )
}
