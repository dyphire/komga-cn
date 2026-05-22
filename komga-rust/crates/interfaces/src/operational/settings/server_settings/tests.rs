use super::*;

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use axum::body::{Bytes, to_bytes};
use axum::http::StatusCode;
use komga_application::identity_access::AuthUser;
use komga_application::operational::ServerSettingsPort;
use komga_application::task_processing::{
    LibraryTaskBatch, QueueStatus, SubmitUrgency, TaskKind, TaskQueue, TaskQueueAdmin,
    TaskQueueRecord, TaskRequest,
};
use komga_infrastructure::sqlite::write_models::server_settings::ServerSettingsStore;

use crate::identity_access::auth::Admin;
use crate::state::OperationalState;
use crate::state::{
    BookImportSseEvent, HttpServerRequestsState, OAuth2ClientConfig, OperationalBuildMetadata,
    RemoteCacheEntry, RuntimeState, ServerSettingsState, SseOperationalState, StartupTimingState,
    TaskQueueState,
};

#[tokio::test]
async fn update_server_settings_applies_runtime_task_pool_after_persistence_succeeds() {
    let (fixture_root, store) = sqlite_fixture("task-pool-apply-success").await;
    store
        .apply_changes(&[("TASK_POOL_SIZE".to_string(), Some("1".to_string()))])
        .await
        .expect("seed task pool size should succeed");

    let apply_count = Arc::new(AtomicUsize::new(0));
    let applied_value = Arc::new(AtomicUsize::new(0));
    let state = test_operational_state(fixture_root.clone());
    let app = Arc::new(test_app_state(
        state,
        Arc::new(FakeTaskQueue {
            apply: {
                let apply_count = apply_count.clone();
                let applied_value = applied_value.clone();
                move |value| {
                    apply_count.fetch_add(1, Ordering::SeqCst);
                    applied_value.store(value, Ordering::SeqCst);
                    Ok(())
                }
            },
        }),
        store.clone(),
    ));
    let response = update_server_settings(
        State(test_server_settings_state(&app)),
        Admin(admin_user()),
        Bytes::from(serde_json::json!({ "taskPoolSize": 3_u64 }).to_string()),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(apply_count.load(Ordering::SeqCst), 1);
    assert_eq!(applied_value.load(Ordering::SeqCst), 3);
    let persisted = store
        .load_map()
        .await
        .expect("settings should be readable after update");
    assert_eq!(
        persisted.get("TASK_POOL_SIZE"),
        Some(&Some("3".to_string()))
    );

    cleanup_fixture(fixture_root).await;
}

#[tokio::test]
async fn update_server_settings_skips_task_pool_apply_when_payload_omits_change() {
    let (fixture_root, store) = sqlite_fixture("task-pool-not-changed").await;
    store
        .apply_changes(&[("TASK_POOL_SIZE".to_string(), Some("2".to_string()))])
        .await
        .expect("seed task pool size should succeed");

    let apply_count = Arc::new(AtomicUsize::new(0));
    let state = test_operational_state(fixture_root.clone());
    let app = Arc::new(test_app_state(
        state,
        Arc::new(FakeTaskQueue {
            apply: {
                let apply_count = apply_count.clone();
                move |_value| {
                    apply_count.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            },
        }),
        store.clone(),
    ));
    let response = update_server_settings(
        State(test_server_settings_state(&app)),
        Admin(admin_user()),
        Bytes::from(serde_json::json!({ "deleteEmptyCollections": true }).to_string()),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(apply_count.load(Ordering::SeqCst), 0);
    let persisted = store
        .load_map()
        .await
        .expect("settings should be readable after update");
    assert_eq!(
        persisted.get("TASK_POOL_SIZE"),
        Some(&Some("2".to_string()))
    );
    assert_eq!(
        persisted.get("DELETE_EMPTY_COLLECTIONS"),
        Some(&Some("true".to_string()))
    );

    cleanup_fixture(fixture_root).await;
}

#[tokio::test]
async fn get_server_settings_does_not_apply_runtime_task_pool_size() {
    let (fixture_root, store) = sqlite_fixture("read-side-effect-free").await;
    store
        .apply_changes(&[("TASK_POOL_SIZE".to_string(), Some("4".to_string()))])
        .await
        .expect("seed task pool size should succeed");

    let apply_count = Arc::new(AtomicUsize::new(0));
    let state = test_operational_state(fixture_root.clone());
    let app = Arc::new(test_app_state(
        state,
        Arc::new(FakeTaskQueue {
            apply: {
                let apply_count = apply_count.clone();
                move |_value| {
                    apply_count.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            },
        }),
        store,
    ));
    let response =
        get_server_settings(State(test_server_settings_state(&app)), Admin(admin_user())).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(apply_count.load(Ordering::SeqCst), 0);

    cleanup_fixture(fixture_root).await;
}

#[tokio::test]
async fn get_server_settings_returns_empty_string_placeholders_for_missing_string_sources() {
    let (fixture_root, store) = sqlite_fixture("string-placeholders").await;

    let state = test_operational_state(fixture_root.clone());
    let app = Arc::new(test_app_state(
        state,
        Arc::new(FakeTaskQueue { apply: |_| Ok(()) }),
        store,
    ));
    let response =
        get_server_settings(State(test_server_settings_state(&app)), Admin(admin_user())).await;

    assert_eq!(response.status(), StatusCode::OK);
    let response_body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("settings response body should be readable");
    let response_body: Value =
        serde_json::from_slice(&response_body).expect("settings response should be valid JSON");

    let placeholder = json!({
        "configurationSource": "",
        "databaseSource": "",
        "effectiveValue": "",
    });
    assert_eq!(response_body.get("serverContextPath"), Some(&placeholder));
    assert_eq!(response_body.get("kepubifyPath"), Some(&placeholder));

    cleanup_fixture(fixture_root).await;
}

#[tokio::test]
async fn get_server_settings_returns_runtime_server_port_configuration_source() {
    let (fixture_root, store) = sqlite_fixture("runtime-port-source").await;
    store
        .apply_changes(&[("SERVER_PORT".to_string(), Some("9090".to_string()))])
        .await
        .expect("seed server port should succeed");

    let mut state = test_operational_state(fixture_root.clone());
    state.runtime.bind_address = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8081));
    state.runtime.configuration_bind_address =
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8081));
    let app = Arc::new(test_app_state(
        state,
        Arc::new(FakeTaskQueue { apply: |_| Ok(()) }),
        store,
    ));
    let response =
        get_server_settings(State(test_server_settings_state(&app)), Admin(admin_user())).await;

    assert_eq!(response.status(), StatusCode::OK);
    let response_body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("settings response body should be readable");
    let response_body: Value =
        serde_json::from_slice(&response_body).expect("settings response should be valid JSON");

    assert_eq!(
        response_body.get("serverPort"),
        Some(&json!({
            "configurationSource": 8081,
            "databaseSource": 9090,
            "effectiveValue": 8081,
        }))
    );

    cleanup_fixture(fixture_root).await;
}

struct FakeTaskQueue<F> {
    apply: F,
}

#[async_trait]
impl<F> TaskQueue for FakeTaskQueue<F>
where
    F: Fn(usize) -> Result<(), String> + Send + Sync,
{
    async fn enqueue(&self, _kind: TaskKind, _target_id: &str) {}

    async fn enqueue_request(&self, _request: TaskRequest) {}

    async fn enqueue_batch(&self, _batch: LibraryTaskBatch) {}

    async fn enqueue_records(
        &self,
        _records: Vec<TaskQueueRecord>,
        _urgency: SubmitUrgency,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn status(&self) -> QueueStatus {
        QueueStatus::default()
    }
}

#[async_trait]
impl<F> TaskQueueAdmin for FakeTaskQueue<F>
where
    F: Fn(usize) -> Result<(), String> + Send + Sync,
{
    async fn clear_unowned_tasks(&self) -> usize {
        0
    }

    async fn apply_pool_size(&self, value: usize) -> Result<(), String> {
        (self.apply)(value)
    }

    fn wakeup(&self) {}
}

async fn sqlite_fixture(case: &str) -> (PathBuf, Arc<ServerSettingsStore>) {
    let root = unique_fixture_root(case);
    std::fs::create_dir_all(&root).expect("fixture root should be created");
    let store = Arc::new(ServerSettingsStore::new(root.join("main.db")));
    // First access bootstraps the main schema, including SERVER_SETTINGS.
    store
        .load_map()
        .await
        .expect("schema bootstrap should succeed");
    (root, store)
}

async fn cleanup_fixture(root: PathBuf) {
    let db_path = root.join("main.db");
    let evicted = komga_infrastructure::sqlite::evict_shared_pools_for_paths(&[db_path]);
    for pool in evicted {
        pool.close().await;
    }
    std::fs::remove_dir_all(&root).expect("fixture root should be removed");
}

fn test_app_state(
    operational: OperationalState,
    task_queue: Arc<dyn TaskQueueAdmin>,
    server_settings: Arc<ServerSettingsStore>,
) -> ServerSettingsState {
    ServerSettingsState {
        runtime: operational.runtime,
        server_settings,
        task_queue: TaskQueueState { queue: task_queue },
    }
}

fn test_operational_state(fixture_root: PathBuf) -> OperationalState {
    OperationalState {
        runtime: RuntimeState {
            tasks_db_file: fixture_root.join("tasks.db"),
            lucene_data_directory: fixture_root.join("lucene"),
            fonts_data_directory: fixture_root.join("fonts"),
            log_file: fixture_root.join("komga.log"),
            config_dir: Some(fixture_root.clone()),
            bind_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)),
            configuration_bind_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)),
            server_context_path: None,
            configuration_server_context_path: None,
        },
        startup_timing: StartupTimingState::default(),
        http_server_requests: HttpServerRequestsState::default(),
        remember_me_runtime_key: "settings-test-runtime".to_string(),
        build_metadata: OperationalBuildMetadata {
            version: "0.1.0".to_string(),
            build_time: "2026-04-09T00:00:00Z".to_string(),
            git_branch: Some("main".to_string()),
            git_commit_id: Some("deadbeef".to_string()),
            git_commit_time: Some("2026-04-09T00:00:00Z".to_string()),
        },
        oauth2_clients: Vec::<OAuth2ClientConfig>::new(),
        oauth2_account_creation: false,
        oidc_email_verification: true,
        sse: Arc::new(Mutex::new(SseOperationalState {
            accepting_connections: true,
            book_import_events: Vec::<BookImportSseEvent>::new(),
            session_expired_events: Vec::new(),
            next_session_expired_event_id: 1,
        })),
        announcements_cache: Arc::new(Mutex::new(None::<RemoteCacheEntry>)),
        releases_cache: Arc::new(Mutex::new(None::<RemoteCacheEntry>)),
        shutdown_trigger: None,
    }
}

fn admin_user() -> AuthUser {
    AuthUser {
        id: "admin-user".to_string(),
        email: "admin@example.org".to_string(),
        password: String::new(),
        roles: vec!["ADMIN".to_string()],
        shared_all_libraries: true,
        shared_library_ids: Vec::new(),
        labels_allow: Vec::new(),
        labels_exclude: Vec::new(),
        age_restriction: None,
    }
}

fn test_server_settings_state(app: &Arc<ServerSettingsState>) -> ServerSettingsState {
    app.as_ref().clone()
}

fn unique_fixture_root(case_name: &str) -> PathBuf {
    let unique_suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "komga-rust-server-settings-{case_name}-{}-{unique_suffix}",
        std::process::id()
    ))
}
