use super::*;
use std::io::Write;

use time::OffsetDateTime;

#[tokio::test]
async fn router_actuator_root_exposed_by_default_without_beans_link() {
    let paths = new_router_fixture("router-actuator-root-omits-beans-link").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("actuator root request should build"),
        )
        .await
        .expect("actuator root request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let links = payload
        .get("_links")
        .and_then(Value::as_object)
        .expect("actuator root should include links object");
    assert!(links.get("beans").is_none());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_root_returns_unauthorized_for_anonymous() {
    let paths = new_router_fixture("router-actuator-root-anonymous-unauthorized").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator")
                .body(Body::empty())
                .expect("anonymous actuator root request should build"),
        )
        .await
        .expect("anonymous actuator root request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_root_returns_forbidden_for_authenticated_non_admin() {
    let paths = new_router_fixture("router-actuator-root-non-admin-forbidden").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "user-actuator-root-1",
        "actuator-root-user@example.org",
        "router-contract-user-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "actuator-root-user@example.org",
        "router-contract-user-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("non-admin actuator root request should build"),
        )
        .await
        .expect("non-admin actuator root request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_info_returns_build_and_os_metadata_for_admin() {
    let paths = new_router_fixture("router-actuator-info-build-and-os").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/info")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("actuator info request should build"),
        )
        .await
        .expect("actuator info request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;

    let build = payload
        .get("build")
        .and_then(Value::as_object)
        .expect("actuator info should include build object");
    assert!(
        build
            .get("artifact")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "actuator info build.artifact should be non-empty: {payload:?}"
    );
    assert!(
        build
            .get("name")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "actuator info build.name should be non-empty: {payload:?}"
    );
    assert!(
        build
            .get("group")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "actuator info build.group should be non-empty: {payload:?}"
    );
    assert!(
        build
            .get("version")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "actuator info build.version should be non-empty: {payload:?}"
    );
    assert!(
        build
            .get("time")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "actuator info build.time should be non-empty: {payload:?}"
    );

    if let Some(git) = payload.get("git").and_then(Value::as_object) {
        assert!(
            git.get("branch").is_none_or(|value| {
                value.is_null() || value.as_str().is_some_and(|value| !value.is_empty())
            }),
            "actuator info git.branch should be null or a non-empty string when present: {payload:?}"
        );
        let commit = git
            .get("commit")
            .and_then(Value::as_object)
            .expect("actuator info git object should include commit object");
        assert!(
            commit.get("id").is_none_or(|value| {
                value.is_null() || value.as_str().is_some_and(|value| !value.is_empty())
            }),
            "actuator info git.commit.id should be null or a non-empty string when present: {payload:?}"
        );
        assert!(
            commit.get("time").is_none_or(|value| {
                value.is_null() || value.as_str().is_some_and(|value| !value.is_empty())
            }),
            "actuator info git.commit.time should be null or a non-empty string when present: {payload:?}"
        );
    }

    let os = payload
        .get("os")
        .and_then(Value::as_object)
        .expect("actuator info should include os object");
    assert!(
        os.get("name")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "actuator info os.name should be non-empty: {payload:?}"
    );
    assert!(
        os.get("arch")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "actuator info os.arch should be non-empty: {payload:?}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_logfile_returns_unauthorized_for_anonymous() {
    let paths = new_router_fixture("router-actuator-logfile-anonymous-unauthorized").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/logfile")
                .body(Body::empty())
                .expect("anonymous actuator logfile request should build"),
        )
        .await
        .expect("anonymous actuator logfile request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_logfile_returns_forbidden_for_authenticated_non_admin() {
    let paths = new_router_fixture("router-actuator-logfile-non-admin-forbidden").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "user-actuator-logfile-1",
        "actuator-logfile-user@example.org",
        "router-contract-user-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "actuator-logfile-user@example.org",
        "router-contract-user-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/logfile")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("non-admin actuator logfile request should build"),
        )
        .await
        .expect("non-admin actuator logfile request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_logfile_returns_plaintext_body_for_admin() {
    let paths = new_router_fixture("router-actuator-logfile-admin-plaintext").await;
    seed_router_contract_data(&paths).await;

    let config = runtime_config_for_paths(&paths);
    std::fs::create_dir_all(
        config
            .log_file
            .parent()
            .expect("actuator logfile fixture should have parent directory"),
    )
    .expect("actuator logfile parent directory should be created");
    std::fs::write(&config.log_file, b"first line\nsecond line\n")
        .expect("actuator logfile fixture should be writable");

    let app = build_router_with_config(&config);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/logfile")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("admin actuator logfile request should build"),
        )
        .await
        .expect("admin actuator logfile request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("text/plain; charset=utf-8")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("actuator logfile response body should be readable");
    assert_eq!(String::from_utf8_lossy(&body), "first line\nsecond line\n");

    cleanup_router_fixture(paths);
}

#[test]
fn router_access_log_skips_actuator_and_sse_noise_routes() {
    let paths = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("noise access log test runtime should build")
        .block_on(async {
            let paths = new_router_fixture("router-access-log-skip-noise-routes").await;
            seed_router_contract_data(&paths).await;
            paths
        });
    let config = runtime_config_for_paths(&paths);
    std::fs::create_dir_all(
        config
            .log_file
            .parent()
            .expect("actuator logfile noise fixture should have parent directory"),
    )
    .expect("actuator logfile noise parent directory should be created");
    std::fs::write(&config.log_file, b"noise line\n")
        .expect("actuator logfile noise fixture should be writable");

    let auth_token = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("noise auth runtime should build")
        .block_on(async {
            seed_router_age_exclude_user_with_roles(
                &paths,
                "access-log-noise-admin",
                "access-log-noise-admin@example.org",
                "router-contract-access-log-noise-123",
                0,
                &["USER", "ADMIN", "FILE_DOWNLOAD", "PAGE_STREAMING"],
            )
            .await;
            let app = build_router_with_config(&config);
            login_with_basic_credentials_and_get_token(
                app,
                "access-log-noise-admin@example.org",
                "router-contract-access-log-noise-123",
            )
            .await
        });

    let (logs, statuses) = capture_router_logs_async_result(&config, {
        let config = config.clone();
        let auth_token = auth_token.clone();
        async move {
            let app = build_router_with_config(&config);

            let health = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/actuator/health")
                        .body(Body::empty())
                        .expect("actuator health noise request should build"),
                )
                .await
                .expect("actuator health noise request should complete")
                .status();
            let logfile = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/actuator/logfile")
                        .header("x-auth-token", &auth_token)
                        .body(Body::empty())
                        .expect("actuator logfile noise request should build"),
                )
                .await
                .expect("actuator logfile noise request should complete")
                .status();
            let sse = app
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/sse/v1/events")
                        .header("x-auth-token", &auth_token)
                        .body(Body::empty())
                        .expect("sse noise request should build"),
                )
                .await
                .expect("sse noise request should complete")
                .status();

            (health, logfile, sse)
        }
    });

    assert_eq!(statuses.0, StatusCode::OK);
    assert_eq!(statuses.1, StatusCode::OK);
    assert_eq!(statuses.2, StatusCode::OK);
    let events = parse_json_log_lines(&logs);
    let access_events = matching_event_fields(&events, "http_access");
    assert!(
        access_events.is_empty(),
        "actuator and SSE should be skipped by access logging noise policy: {logs}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_logfile_reads_current_active_file_after_rotation_compatible_writes() {
    let paths = new_router_fixture("router-actuator-logfile-admin-active-after-rotation").await;
    seed_router_contract_data(&paths).await;

    let config = runtime_config_for_paths(&paths);
    let initial_period = OffsetDateTime::parse(
        "2026-04-08T10:15:00Z",
        &time::format_description::well_known::Rfc3339,
    )
    .expect("initial test timestamp should parse");
    let rotated_period = OffsetDateTime::parse(
        "2026-04-08T10:16:00Z",
        &time::format_description::well_known::Rfc3339,
    )
    .expect("rotated test timestamp should parse");
    let clock = {
        let remaining = std::sync::Arc::new(std::sync::Mutex::new(
            vec![
                initial_period,
                initial_period,
                rotated_period,
                rotated_period,
            ]
            .into_iter(),
        ));
        move || {
            remaining
                .lock()
                .expect("test clock state should not be poisoned")
                .next()
                .expect("test clock should have another timestamp ready")
        }
    };
    let mut writer = komga_server::logging::StableFileAppender::new_with_clock(
        config.log_file.clone(),
        komga_server::logging::FileRotation::Minutely,
        clock,
    )
    .expect("stable rotating file appender should be created");
    writer
        .write_all(b"archived line\n")
        .expect("first period write should succeed");
    writer.flush().expect("first period flush should succeed");
    writer
        .write_all(b"active line\n")
        .expect("second period write should succeed");
    writer.flush().expect("second period flush should succeed");

    let app = build_router_with_config(&config);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/logfile")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("admin actuator logfile request should build"),
        )
        .await
        .expect("admin actuator logfile request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("actuator logfile response body should be readable");
    assert_eq!(String::from_utf8_lossy(&body), "active line\n");

    let archive_path = std::fs::read_dir(
        config
            .log_file
            .parent()
            .expect("configured logfile should have a parent directory"),
    )
    .expect("log archive directory should be readable")
    .filter_map(Result::ok)
    .map(|entry| entry.path())
    .find(|path| {
        path != &config.log_file
            && path
                .file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.starts_with("komga.log."))
    })
    .expect("rotation-compatible write should keep one sibling archive beside the active file");
    assert_eq!(
        std::fs::read_to_string(&archive_path).expect("archive logfile should be readable"),
        "archived line\n",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metrics_returns_unauthorized_for_anonymous() {
    let paths = new_router_fixture("router-actuator-metrics-anonymous-unauthorized").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics")
                .body(Body::empty())
                .expect("anonymous actuator metrics request should build"),
        )
        .await
        .expect("anonymous actuator metrics request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metrics_returns_forbidden_for_authenticated_non_admin() {
    let paths = new_router_fixture("router-actuator-metrics-non-admin-forbidden").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "user-actuator-metrics-1",
        "actuator-metrics-user@example.org",
        "router-contract-user-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "actuator-metrics-user@example.org",
        "router-contract-user-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("non-admin actuator metrics request should build"),
        )
        .await
        .expect("non-admin actuator metrics request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metrics_returns_metric_names_for_admin() {
    let paths = new_router_fixture("router-actuator-metrics-admin-names").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("admin actuator metrics request should build"),
        )
        .await
        .expect("admin actuator metrics request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let names = payload
        .get("names")
        .and_then(Value::as_array)
        .expect("actuator metrics should return names array");
    assert!(
        names
            .iter()
            .any(|value| value.as_str() == Some("komga.tasks.execution")),
        "actuator metrics names should include komga.tasks.execution: {payload:?}"
    );
    assert!(
        names
            .iter()
            .any(|value| value.as_str() == Some("komga.books")),
        "actuator metrics names should include komga.books: {payload:?}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metric_detail_includes_base_unit_for_books_filesize() {
    let paths = new_router_fixture("router-actuator-metric-detail-base-unit").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics/komga.books.filesize")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("admin actuator metric detail request should build"),
        )
        .await
        .expect("admin actuator metric detail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("name").and_then(Value::as_str),
        Some("komga.books.filesize")
    );
    assert_eq!(
        payload.get("baseUnit").and_then(Value::as_str),
        Some("bytes")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metric_detail_returns_unauthorized_for_anonymous() {
    let paths = new_router_fixture("router-actuator-metric-detail-anonymous-unauthorized").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics/komga.books.filesize")
                .body(Body::empty())
                .expect("anonymous actuator metric detail request should build"),
        )
        .await
        .expect("anonymous actuator metric detail request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metric_detail_returns_forbidden_for_authenticated_non_admin() {
    let paths = new_router_fixture("router-actuator-metric-detail-non-admin-forbidden").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "user-actuator-metric-detail-1",
        "actuator-metric-detail-user@example.org",
        "router-contract-user-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "actuator-metric-detail-user@example.org",
        "router-contract-user-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics/komga.books.filesize")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("non-admin actuator metric detail request should build"),
        )
        .await
        .expect("non-admin actuator metric detail request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_health_is_public_and_hides_details_for_anonymous() {
    let paths = new_router_fixture("router-actuator-health-public-status-only").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/health")
                .body(Body::empty())
                .expect("actuator health request should build"),
        )
        .await
        .expect("actuator health request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload.get("status").and_then(Value::as_str), Some("UP"));
    assert!(
        payload.get("components").is_none(),
        "anonymous actuator health should not expose component details: {payload:?}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_health_hides_details_for_authenticated_non_admin() {
    let paths = new_router_fixture("router-actuator-health-non-admin-status-only").await;
    seed_router_contract_data(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "user-health-1",
        "health-user@example.org",
        "router-contract-user-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "health-user@example.org",
        "router-contract-user-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/health")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("actuator health non-admin request should build"),
        )
        .await
        .expect("actuator health non-admin request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload.get("status").and_then(Value::as_str), Some("UP"));
    assert!(
        payload.get("components").is_none(),
        "non-admin actuator health should not expose component details: {payload:?}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_shutdown_requires_admin_authentication() {
    let paths = new_router_fixture("router-actuator-shutdown-auth").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/actuator/shutdown")
                .body(Body::empty())
                .expect("actuator shutdown request should build"),
        )
        .await
        .expect("actuator shutdown request should complete");

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("actuator shutdown response body should be readable");
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "unexpected actuator shutdown status={status}, body={}",
        String::from_utf8_lossy(&body),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_shutdown_returns_ok_message_for_admin() {
    let paths = new_router_fixture("router-actuator-shutdown-admin-success").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/actuator/shutdown")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("admin actuator shutdown request should build"),
        )
        .await
        .expect("admin actuator shutdown request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("message").and_then(Value::as_str),
        Some("Shutting down, bye...")
    );

    cleanup_router_fixture(paths);
}
