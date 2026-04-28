use super::*;
#[cfg(target_os = "linux")]
use std::fs;
use std::io::Write;

use time::OffsetDateTime;

const ACTUATOR_V3_JSON: &str = "application/vnd.spring-boot.actuator.v3+json";

fn measurement_value(payload: &Value, statistic: &str) -> f64 {
    payload
        .get("measurements")
        .and_then(Value::as_array)
        .and_then(|measurements| {
            measurements.iter().find_map(|measurement| {
                (measurement.get("statistic").and_then(Value::as_str) == Some(statistic))
                    .then(|| measurement.get("value").and_then(Value::as_f64))
                    .flatten()
            })
        })
        .unwrap_or_else(|| panic!("metric should expose {statistic} measurement: {payload:?}"))
}

#[cfg(target_os = "linux")]
fn expected_process_cpu_usage_fraction() -> Option<f64> {
    let schedstat = fs::read_to_string("/proc/self/schedstat").ok()?;
    let cpu_runtime_seconds =
        schedstat.split_whitespace().next()?.parse::<f64>().ok()? / 1_000_000_000.0;
    let process_uptime_seconds = expected_process_uptime_seconds()?;
    if process_uptime_seconds <= 0.0 {
        return None;
    }
    let cpu_count = std::thread::available_parallelism().ok()?.get() as f64;
    Some((cpu_runtime_seconds / process_uptime_seconds / cpu_count).clamp(0.0, 1.0))
}

#[cfg(target_os = "linux")]
fn expected_process_uptime_seconds() -> Option<f64> {
    let uptime = fs::read_to_string("/proc/uptime").ok()?;
    let system_uptime_seconds = uptime.split_whitespace().next()?.parse::<f64>().ok()?;
    let stat = fs::read_to_string("/proc/self/stat").ok()?;
    let after_paren = stat.split_once(") ")?.1;
    let fields = after_paren.split_whitespace().collect::<Vec<_>>();
    let ticks_since_boot = fields.get(19)?.parse::<f64>().ok()?;
    let ticks_per_second = std::process::Command::new("getconf")
        .arg("CLK_TCK")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|value| value.trim().parse::<f64>().ok())?;
    Some((system_uptime_seconds - (ticks_since_boot / ticks_per_second)).max(0.0))
}

fn assert_health_datasource_component(
    db_components: &serde_json::Map<String, Value>,
    component_name: &str,
    expected_status: &str,
    payload: &Value,
) {
    let datasource = db_components
        .get(component_name)
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("db contributor should expose {component_name}: {payload:?}"));
    assert_eq!(
        datasource.get("status").and_then(Value::as_str),
        Some(expected_status)
    );
    let details = datasource
        .get("details")
        .and_then(Value::as_object)
        .unwrap_or_else(|| panic!("{component_name} should expose details: {payload:?}"));
    assert_eq!(
        details.get("database").and_then(Value::as_str),
        Some("SQLite")
    );
    assert_eq!(
        details.get("validationQuery").and_then(Value::as_str),
        Some("isValid()")
    );
}

fn assert_admin_actuator_health_payload(
    payload: &Value,
    expected_status: &str,
    expected_sqlite_status: &str,
    expected_tasks_status: &str,
) {
    assert_eq!(
        payload.get("status").and_then(Value::as_str),
        Some(expected_status)
    );

    let components = payload
        .get("components")
        .and_then(Value::as_object)
        .expect("admin actuator health should expose components");
    assert!(
        components.get("tasksDb").is_none(),
        "health should not expose tasksDb: {payload:?}"
    );

    let db = components
        .get("db")
        .and_then(Value::as_object)
        .expect("health should expose db component");
    assert_eq!(
        db.get("status").and_then(Value::as_str),
        Some(expected_status)
    );
    assert!(
        db.get("details").is_none(),
        "db should be a composite contributor: {payload:?}"
    );
    let db_components = db
        .get("components")
        .and_then(Value::as_object)
        .expect("db component should expose nested datasource contributors");

    for component_name in ["sqliteDataSourceRW", "sqliteDataSourceRO"] {
        assert_health_datasource_component(
            db_components,
            component_name,
            expected_sqlite_status,
            payload,
        );
    }
    for component_name in ["tasksDataSourceRW", "tasksDataSourceRO"] {
        assert_health_datasource_component(
            db_components,
            component_name,
            expected_tasks_status,
            payload,
        );
    }

    let disk_space = components
        .get("diskSpace")
        .and_then(Value::as_object)
        .expect("health should expose diskSpace component");
    assert_eq!(disk_space.get("status").and_then(Value::as_str), Some("UP"));
    let disk_space_details = disk_space
        .get("details")
        .and_then(Value::as_object)
        .expect("diskSpace component should expose details");
    assert_eq!(
        disk_space_details.get("threshold").and_then(Value::as_u64),
        Some(10 * 1024 * 1024)
    );
    assert!(
        disk_space_details
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "diskSpace path should be non-empty: {payload:?}"
    );
    assert!(
        disk_space_details
            .get("total")
            .and_then(Value::as_u64)
            .is_some(),
        "diskSpace total should be numeric: {payload:?}"
    );
    assert!(
        disk_space_details
            .get("free")
            .and_then(Value::as_u64)
            .is_some(),
        "diskSpace free should be numeric: {payload:?}"
    );

    let ping = components
        .get("ping")
        .and_then(Value::as_object)
        .expect("health should expose ping component");
    assert_eq!(ping.get("status").and_then(Value::as_str), Some("UP"));
}

#[tokio::test]
async fn router_actuator_root_exposes_spring_boot_style_discovery_links() {
    let paths = new_router_fixture("router-actuator-root-spring-boot-discovery-links").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some(ACTUATOR_V3_JSON)
    );
    let payload = response_json(response).await;
    let links = payload
        .get("_links")
        .and_then(Value::as_object)
        .expect("actuator root should include links object");
    for link_name in ["self", "health", "info", "metrics", "shutdown"] {
        assert!(
            links
                .get(link_name)
                .and_then(Value::as_object)
                .and_then(|link| link.get("href"))
                .and_then(Value::as_str)
                .is_some_and(|value| !value.is_empty()),
            "actuator root should expose {link_name} href: {payload:?}"
        );
    }
    assert_eq!(
        links
            .get("metrics-requiredMetricName")
            .and_then(Value::as_object)
            .and_then(|link| link.get("templated"))
            .and_then(Value::as_bool),
        Some(true)
    );
    assert_eq!(
        links
            .get("health-path")
            .and_then(Value::as_object)
            .and_then(|link| link.get("templated"))
            .and_then(Value::as_bool),
        Some(true)
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_root_returns_unauthorized_for_anonymous() {
    let paths = new_router_fixture("router-actuator-root-anonymous-unauthorized").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;

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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "actuator-root-user@example.org",
        "router-contract-user-123",
    )
    .await;

    let response = app
        .clone()
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some(ACTUATOR_V3_JSON)
    );
    let payload = response_json(response).await;

    let build = payload
        .get("build")
        .and_then(Value::as_object)
        .expect("actuator info should include build object");
    assert!(
        build
            .get("version")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "actuator info build.version should be non-empty: {payload:?}"
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
    assert!(
        os.get("version")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty()),
        "actuator info os.version should be non-empty: {payload:?}"
    );

    let process = payload
        .get("process")
        .and_then(Value::as_object)
        .expect("actuator info should include process object");
    assert!(
        process
            .get("pid")
            .and_then(Value::as_u64)
            .is_some_and(|value| value > 0),
        "actuator info process.pid should be positive: {payload:?}"
    );
    assert!(process.get("memory").and_then(Value::as_object).is_some());

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_logfile_returns_unauthorized_for_anonymous() {
    let paths = new_router_fixture("router-actuator-logfile-anonymous-unauthorized").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;

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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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

    let app = build_router_with_config(&config).await;
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
            let app = build_router_with_config(&config).await;
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
            let app = build_router_with_config(&config).await;

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

    let app = build_router_with_config(&config).await;
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;

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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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
    for metric_name in [
        "komga.tasks.execution",
        "komga.books",
        "http.server.requests",
    ] {
        assert!(
            names
                .iter()
                .any(|value| value.as_str() == Some(metric_name)),
            "actuator metrics names should include {metric_name}: {payload:?}"
        );
    }
    assert!(
        !names
            .iter()
            .any(|value| value.as_str() == Some("logback.events")),
        "actuator metrics names should not include removed logback.events metric: {payload:?}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metric_detail_includes_base_unit_for_books_filesize() {
    let paths = new_router_fixture("router-actuator-metric-detail-base-unit").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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
    assert!(
        payload
            .get("measurements")
            .and_then(Value::as_array)
            .is_some_and(|measurements| !measurements.is_empty()),
        "actuator metric detail should expose measurements: {payload:?}"
    );
    let available_tags = payload
        .get("availableTags")
        .and_then(Value::as_array)
        .expect("actuator metric detail should expose availableTags array");
    assert!(
        available_tags.iter().any(|tag| {
            tag.get("tag").and_then(Value::as_str) == Some("library")
                && tag
                    .get("values")
                    .and_then(Value::as_array)
                    .is_some_and(|values| !values.is_empty())
        }),
        "actuator metric detail should expose library tag values: {payload:?}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metric_detail_uses_runtime_startup_timings_for_admin() {
    let paths = new_router_fixture("router-actuator-metric-detail-runtime-startup-timings").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let started_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics/application.started.time")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("application.started.time request should build"),
        )
        .await
        .expect("application.started.time request should complete");
    let ready_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics/application.ready.time")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("application.ready.time request should build"),
        )
        .await
        .expect("application.ready.time request should complete");

    assert_eq!(started_response.status(), StatusCode::OK);
    assert_eq!(ready_response.status(), StatusCode::OK);
    let started_payload = response_json(started_response).await;
    let ready_payload = response_json(ready_response).await;
    let started_seconds = measurement_value(&started_payload, "TOTAL_TIME");
    let ready_seconds = measurement_value(&ready_payload, "TOTAL_TIME");

    assert!(
        started_seconds > 0.0,
        "application.started.time should be positive: {started_payload:?}"
    );
    assert!(
        ready_seconds >= started_seconds,
        "application.ready.time should be >= application.started.time: started={started_seconds}, ready={ready_seconds}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metric_detail_uses_runtime_process_cpu_usage_for_admin() {
    let paths = new_router_fixture("router-actuator-metric-detail-runtime-process-cpu").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics/process.cpu.usage")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("process.cpu.usage request should build"),
        )
        .await
        .expect("process.cpu.usage request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let cpu_usage = measurement_value(&payload, "VALUE");

    assert!(
        (0.0..=1.0).contains(&cpu_usage),
        "process.cpu.usage should be a fraction: {payload:?}"
    );

    #[cfg(target_os = "linux")]
    {
        let expected = expected_process_cpu_usage_fraction()
            .expect("linux test host should expose process cpu usage via /proc");
        assert!(
            (cpu_usage - expected).abs() <= 0.05,
            "process.cpu.usage should track /proc-derived process cpu usage: actual={cpu_usage}, expected={expected}, payload={payload:?}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metric_detail_exposes_datasource_tags_for_admin() {
    let paths = new_router_fixture("router-actuator-metric-detail-datasource-tags").await;
    seed_router_contract_data(&paths).await;

    let _main_pool_one = komga_infrastructure::sqlite::connect_shared_pool(&paths.main_db, 1)
        .await
        .expect("main shared sqlx pool with max=1 should open");
    let _main_pool_two = komga_infrastructure::sqlite::connect_shared_pool(&paths.main_db, 2)
        .await
        .expect("main shared sqlx pool with max=2 should open");
    let _tasks_pool_one = komga_infrastructure::sqlite::connect_shared_pool(&paths.tasks_db, 1)
        .await
        .expect("tasks shared sqlx pool with max=1 should open");

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics/jdbc.connections.active")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("admin actuator datasource metric detail request should build"),
        )
        .await
        .expect("admin actuator datasource metric detail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("name").and_then(Value::as_str),
        Some("jdbc.connections.active")
    );
    let available_tags = payload
        .get("availableTags")
        .and_then(Value::as_array)
        .expect("datasource metric should expose availableTags array");
    assert!(
        available_tags.iter().any(|tag| {
            tag.get("tag").and_then(Value::as_str) == Some("name")
                && tag
                    .get("values")
                    .and_then(Value::as_array)
                    .is_some_and(|values| {
                        values
                            .iter()
                            .any(|value| value.as_str() == Some("main-pool-max-2"))
                    })
        }),
        "datasource metric should expose live sqlx pool entry names: {payload:?}"
    );

    let filtered_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics/jdbc.connections.max?tag=name:main-pool-max-2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("filtered datasource metric detail request should build"),
        )
        .await
        .expect("filtered datasource metric detail request should complete");

    assert_eq!(filtered_response.status(), StatusCode::OK);
    let filtered_payload = response_json(filtered_response).await;
    assert!(
        filtered_payload
            .get("measurements")
            .and_then(Value::as_array)
            .is_some_and(|measurements| measurements.iter().any(|measurement| {
                measurement.get("statistic").and_then(Value::as_str) == Some("VALUE")
                    && measurement.get("value").and_then(Value::as_f64) == Some(2.0)
            })),
        "filtered datasource metric detail should reflect the real sqlx pool max_connections: {filtered_payload:?}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metric_detail_exposes_task_timer_shape_for_admin() {
    let paths = new_router_fixture("router-actuator-metric-detail-task-timer-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics/komga.tasks.execution")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("admin actuator task execution detail request should build"),
        )
        .await
        .expect("admin actuator task execution detail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let measurements = payload
        .get("measurements")
        .and_then(Value::as_array)
        .expect("task execution metric should expose measurements");
    for statistic in ["COUNT", "TOTAL_TIME", "MAX"] {
        assert!(
            measurements.iter().any(|measurement| {
                measurement.get("statistic").and_then(Value::as_str) == Some(statistic)
            }),
            "task execution metric should expose {statistic}: {payload:?}"
        );
    }
    let available_tags = payload
        .get("availableTags")
        .and_then(Value::as_array)
        .expect("task execution metric should expose availableTags array");
    assert!(
        available_tags
            .iter()
            .any(|tag| tag.get("tag").and_then(Value::as_str) == Some("type")),
        "task execution metric should expose type tag: {payload:?}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metric_detail_exposes_task_failure_tags_for_admin() {
    let paths = new_router_fixture("router-actuator-metric-detail-task-failure-tags").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics/komga.tasks.failure")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("admin actuator task failure detail request should build"),
        )
        .await
        .expect("admin actuator task failure detail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let available_tags = payload
        .get("availableTags")
        .and_then(Value::as_array)
        .expect("task failure metric should expose availableTags array");
    assert!(
        available_tags.iter().any(|tag| {
            tag.get("tag").and_then(Value::as_str) == Some("type")
                && tag
                    .get("values")
                    .and_then(Value::as_array)
                    .is_some_and(|values| !values.is_empty())
        }),
        "task failure metric should expose type tag values: {payload:?}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metric_detail_reflects_real_http_requests_for_admin() {
    let paths = new_router_fixture("router-actuator-metric-detail-real-http-requests").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let info_response = app
        .clone()
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
    assert_eq!(info_response.status(), StatusCode::OK);

    let unauthorized_root_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator")
                .body(Body::empty())
                .expect("anonymous actuator root request should build"),
        )
        .await
        .expect("anonymous actuator root request should complete");
    assert_eq!(
        unauthorized_root_response.status(),
        StatusCode::UNAUTHORIZED
    );

    let info_metric_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics/http.server.requests?tag=exception:None&tag=method:GET&tag=outcome:SUCCESS&tag=status:200&tag=uri:/actuator/info")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("filtered http.server.requests success detail request should build"),
        )
        .await
        .expect("filtered http.server.requests success detail request should complete");
    assert_eq!(info_metric_response.status(), StatusCode::OK);
    let info_metric_payload = response_json(info_metric_response).await;
    assert_eq!(
        measurement_value(&info_metric_payload, "COUNT"),
        1.0,
        "http.server.requests should count the real /actuator/info request: {info_metric_payload:?}"
    );
    assert!(
        measurement_value(&info_metric_payload, "TOTAL_TIME") > 0.0,
        "http.server.requests should accumulate real latency for /actuator/info: {info_metric_payload:?}"
    );
    assert!(
        measurement_value(&info_metric_payload, "MAX") > 0.0,
        "http.server.requests should expose a real max latency for /actuator/info: {info_metric_payload:?}"
    );

    let unauthorized_metric_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/metrics/http.server.requests?tag=exception:None&tag=method:GET&tag=outcome:CLIENT_ERROR&tag=status:401&tag=uri:/actuator")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("filtered http.server.requests unauthorized detail request should build"),
        )
        .await
        .expect("filtered http.server.requests unauthorized detail request should complete");
    assert_eq!(unauthorized_metric_response.status(), StatusCode::OK);
    let unauthorized_metric_payload = response_json(unauthorized_metric_response).await;
    assert_eq!(
        measurement_value(&unauthorized_metric_payload, "COUNT"),
        1.0,
        "http.server.requests should count the real anonymous /actuator request: {unauthorized_metric_payload:?}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_metric_detail_returns_unauthorized_for_anonymous() {
    let paths = new_router_fixture("router-actuator-metric-detail-anonymous-unauthorized").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;

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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;

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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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
async fn router_actuator_health_exposes_spring_style_components_for_admin() {
    let paths = new_router_fixture("router-actuator-health-admin-components").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/health")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("admin actuator health request should build"),
        )
        .await
        .expect("admin actuator health request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_admin_actuator_health_payload(&payload, "UP", "UP", "UP");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_health_exposes_details_for_admin_basic_auth_like_kotlin() {
    let paths = new_router_fixture("router-actuator-health-admin-basic-auth").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let authorization =
        basic_authorization_header_value("admin@example.org", "router-contract-admin-123");

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/health")
                .header(header::AUTHORIZATION, authorization.as_str())
                .body(Body::empty())
                .expect("basic-auth actuator health request should build"),
        )
        .await
        .expect("basic-auth actuator health request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_admin_actuator_health_payload(&payload, "UP", "UP", "UP");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_health_exposes_details_for_admin_api_key_like_kotlin() {
    let paths = new_router_fixture("router-actuator-health-admin-api-key").await;
    seed_router_contract_data(&paths).await;
    seed_kobo_sync_api_key(&paths, "actuator-health-admin-api-key", "admin-user").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/health")
                .header("x-api-key", "actuator-health-admin-api-key")
                .body(Body::empty())
                .expect("api-key actuator health request should build"),
        )
        .await
        .expect("api-key actuator health request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_admin_actuator_health_payload(&payload, "UP", "UP", "UP");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_health_aggregates_down_when_database_file_is_missing() {
    let paths = new_router_fixture("router-actuator-health-down-when-db-missing").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    close_router_fixture_shared_pools(&paths).await;
    std::fs::remove_file(&paths.main_db).expect("main db should be removable for health down test");

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator/health")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("down actuator health request should build"),
        )
        .await
        .expect("down actuator health request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_admin_actuator_health_payload(&payload, "DOWN", "DOWN", "UP");

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_actuator_shutdown_requires_admin_authentication() {
    let paths = new_router_fixture("router-actuator-shutdown-auth").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;

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

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
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
