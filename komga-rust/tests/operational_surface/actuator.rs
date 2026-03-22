use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use tower::ServiceExt;

use super::basic_auth;

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
