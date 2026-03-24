use komga_compat_testkit::contract_matrix::assert_required_target_declared;
use reqwest::StatusCode;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[path = "compat/auth_env.rs"]
mod compat_auth_env;

#[path = "support/persistence_seed_fixture.rs"]
mod persistence_seed_fixture;

#[test]
fn release_cutover_contract_target_is_registered() {
    assert_required_target_declared("release cutover", "release_cutover_contract");
}

#[tokio::test]
async fn release_form_operational_surface_exposes_health_and_metrics() {
    let (base_url, _handle) = spawn_release_runtime().await;
    let client = reqwest::Client::new();

    let live = client
        .get(format!("{base_url}/health/live"))
        .send()
        .await
        .expect("live endpoint request should send");
    assert_eq!(live.status(), StatusCode::OK);
    let live_json: serde_json::Value = live
        .json()
        .await
        .expect("live endpoint should return json body");
    assert_eq!(live_json["status"], "UP");

    let ready = client
        .get(format!("{base_url}/health/ready"))
        .send()
        .await
        .expect("ready endpoint request should send");
    assert_eq!(ready.status(), StatusCode::OK);
    let ready_json: serde_json::Value = ready
        .json()
        .await
        .expect("ready endpoint should return json body");
    assert_eq!(ready_json["status"], "UP");

    let metrics = client
        .get(format!("{base_url}/metrics"))
        .send()
        .await
        .expect("metrics endpoint request should send");
    assert_eq!(metrics.status(), StatusCode::OK);
    let metrics_body = metrics
        .text()
        .await
        .expect("metrics endpoint should return text body");
    assert!(
        metrics_body.contains("komga_runtime_up 1"),
        "metrics payload should contain runtime health gauge"
    );
}

#[tokio::test]
async fn release_form_default_contract_does_not_require_actuator_surface() {
    let (base_url, _handle) = spawn_release_runtime().await;
    let client = reqwest::Client::new();

    let actuator = client
        .get(format!("{base_url}/actuator/info"))
        .send()
        .await
        .expect("actuator info request should send");
    assert_eq!(
        actuator.status(),
        StatusCode::NOT_FOUND,
        "default release-form runtime should not expose /actuator/**",
    );
}

#[tokio::test]
async fn rollback_rehearsal_proves_kotlin_compatible_auth_activity_after_rust_login_write() {
    let (base_url, runtime_handle) = spawn_release_runtime().await;
    let client = reqwest::Client::new();

    let login = client
        .get(format!("{base_url}/api/v2/users/me?remember-me=false"))
        .header(
            "Authorization",
            format!("Basic {}", compat_auth_env::COMPAT_ADMIN_BASIC_AUTH_BASE64),
        )
        .header("X-Auth-Token", "")
        .send()
        .await
        .expect("release rehearsal login request should send");
    assert_eq!(
        login.status(),
        StatusCode::OK,
        "release rehearsal login should succeed against rust default runtime"
    );

    let db_path = runtime_handle.config.database_file.clone();
    let pool = komga_rust::persistence::sqlite::connect_pool(&db_path, 1)
        .await
        .expect("rollback rehearsal should open rust-written database");

    let latest_source: Option<String> = sqlx::query_scalar(
        "SELECT SOURCE FROM AUTHENTICATION_ACTIVITY WHERE EMAIL = ? ORDER BY DATE_TIME DESC LIMIT 1",
    )
    .bind("admin@example.org")
    .fetch_one(&pool)
    .await
    .expect("rollback rehearsal should query latest auth source from kotlin-compatible table");
    assert_eq!(latest_source.as_deref(), Some("BASIC"));

    let latest_success: bool = sqlx::query_scalar(
        "SELECT SUCCESS FROM AUTHENTICATION_ACTIVITY WHERE EMAIL = ? ORDER BY DATE_TIME DESC LIMIT 1",
    )
    .bind("admin@example.org")
    .fetch_one(&pool)
    .await
    .expect("rollback rehearsal should query success flag from kotlin-compatible table");
    assert!(latest_success, "latest auth activity should mark successful login");

    pool.close().await;
}

struct RuntimeHandle {
    _server: tokio::task::JoinHandle<()>,
    config: komga_rust::config::RuntimeConfig,
}

async fn spawn_release_runtime() -> (String, RuntimeHandle) {
    compat_auth_env::ensure_compat_auth_env();

    let config = release_runtime_config();
    prepare_release_runtime_storage(&config)
        .await
        .expect("release runtime storage should be prepared");

    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("release runtime listener should bind");
    let addr = listener
        .local_addr()
        .expect("release runtime listener should expose address");

    let config_for_server = config.clone();
    let server = tokio::spawn(async move {
        komga_rust::app::serve_with_config(listener, config_for_server)
            .await
            .expect("release runtime should serve");
    });

    (
        format!("http://{addr}"),
        RuntimeHandle {
            _server: server,
            config,
        },
    )
}

fn release_runtime_config() -> komga_rust::config::RuntimeConfig {
    let mut config =
        komga_rust::config::RuntimeConfig::for_compat_profile(komga_rust::config::CompatProfile::SnapshotAligned);
    let root = release_runtime_root();
    config.config_dir = Some(root.clone());
    config.database_file = root.join("database.sqlite");
    config.tasks_db_file = root.join("tasks.sqlite");
    config.lucene_data_directory = root.join("lucene");
    config.fonts_data_directory = root.join("fonts");
    config.log_file = root.join("komga.log");
    config
}

fn release_runtime_root() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after epoch")
        .as_nanos();
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("release-cutover-runtime-{nanos}"))
}

async fn prepare_release_runtime_storage(config: &komga_rust::config::RuntimeConfig) -> anyhow::Result<()> {
    let config_dir = config
        .config_dir
        .as_ref()
        .expect("release runtime should have config dir");
    fs::create_dir_all(config_dir)?;
    fs::create_dir_all(&config.lucene_data_directory)?;
    fs::create_dir_all(&config.fonts_data_directory)?;

    persistence_seed_fixture::seed_main_db_from_flyway(&config.database_file).await?;
    persistence_seed_fixture::seed_tasks_db_from_flyway(&config.tasks_db_file).await?;

    Ok(())
}
