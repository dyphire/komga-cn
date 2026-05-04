use super::*;

use std::collections::{BTreeMap, HashMap};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use axum::body::{Bytes, to_bytes};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use komga_application::identity_access::AuthUser;
use komga_application::operational::PersistedServerSettings;
use komga_application::task_processing::{
    LibraryTaskBatch, QueueStatus, TaskEngine, TaskEnqueuer, TaskKind, TaskRequest,
};

use crate::state::OperationalState;
use crate::identity_access::auth::session_token_for_user_with_runtime_key;
use crate::state::default_test_identity_service;
use crate::state::{
    BookImportSseEvent, HttpAppState, HttpServerRequestsState, HttpServices, LibraryCatalogService,
    OAuth2ClientConfig, OperationalBuildMetadata, RemoteCacheEntry, RuntimeState,
    ServerSettingsService, SseOperationalState, StartupTimingState, TransientBooksStore,
    tests::{
        NoopDiscoveryAuthorService, NoopDiscoveryBookFeedService,
        NoopDiscoveryCollectionSearchService, NoopDiscoveryDetailService,
        NoopDiscoveryLibraryMappingService, NoopDiscoveryListService,
        NoopDiscoveryReadlistSearchService, NoopMediaAssetsService, NoopOpdsCatalogService,
        NoopOpdsPersistedService, NoopOperationalRuntimeService, NoopOperationalSettingsService,
    },
};

#[tokio::test]
async fn update_server_settings_does_not_apply_runtime_task_pool_before_persistence_succeeds() {
    let fixture_root = unique_fixture_root("server-settings-persistence-failure");
    std::fs::create_dir_all(&fixture_root).expect("fixture root should be created");
    let database_file = fixture_root.join("main.db");
    let persisted_settings = Arc::new(Mutex::new(HashMap::from([
        (
            "REMEMBER_ME_KEY".to_string(),
            Some("seeded-remember-me-key".to_string()),
        ),
        ("TASK_POOL_SIZE".to_string(), Some("1".to_string())),
    ])));
    let persist_attempts = Arc::new(AtomicUsize::new(0));
    let settings_store = fake_settings_store(persisted_settings.clone(), persist_attempts.clone());

    let apply_count = Arc::new(AtomicUsize::new(0));
    let state = test_operational_state(fixture_root.clone());
    let app = Arc::new(test_app_state(
        database_file.clone(),
        state,
        Box::new(FakeTaskEngine {
            apply: {
                let apply_count = apply_count.clone();
                move |_value| {
                    apply_count.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            },
        }),
        fake_settings_store(persisted_settings.clone(), persist_attempts.clone()),
    ));
    let headers = admin_headers(&fixture_root);

    let response = update_server_settings(
        State(app),
        headers,
        Bytes::from(serde_json::json!({ "taskPoolSize": 4_u64 }).to_string()),
    )
    .await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let response_body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("settings error response body should be readable");
    let response_body: Value = serde_json::from_slice(&response_body)
        .expect("settings error response should be valid JSON");
    assert!(response_body.get("message").is_some());
    assert_eq!(apply_count.load(Ordering::SeqCst), 0);
    assert_eq!(persist_attempts.load(Ordering::SeqCst), 1);

    let persisted = settings_store
        .load_map()
        .await
        .expect("settings should remain readable after failure");
    assert_eq!(
        persisted.get("TASK_POOL_SIZE"),
        Some(&Some("1".to_string()))
    );

    std::fs::remove_dir_all(&fixture_root).expect("fixture root should be removed");
}

#[tokio::test]
async fn update_server_settings_applies_runtime_task_pool_after_persistence_succeeds() {
    let fixture_root = unique_fixture_root("server-settings-task-pool-apply-success");
    std::fs::create_dir_all(&fixture_root).expect("fixture root should be created");
    let database_file = fixture_root.join("main.db");
    let persisted_settings = Arc::new(Mutex::new(HashMap::from([(
        "TASK_POOL_SIZE".to_string(),
        Some("1".to_string()),
    )])));
    let persist_attempts = Arc::new(AtomicUsize::new(0));
    let apply_count = Arc::new(AtomicUsize::new(0));
    let applied_value = Arc::new(AtomicUsize::new(0));

    let state = test_operational_state(fixture_root.clone());
    let app = Arc::new(test_app_state(
        database_file,
        state,
        Box::new(FakeTaskEngine {
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
        fake_settings_store(persisted_settings.clone(), persist_attempts.clone()),
    ));
    let headers = admin_headers(&fixture_root);

    let response = update_server_settings(
        State(app),
        headers,
        Bytes::from(serde_json::json!({ "taskPoolSize": 3_u64 }).to_string()),
    )
    .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(persist_attempts.load(Ordering::SeqCst), 1);
    assert_eq!(apply_count.load(Ordering::SeqCst), 1);
    assert_eq!(applied_value.load(Ordering::SeqCst), 3);
    assert_eq!(
        persisted_settings
            .lock()
            .expect("persisted settings should lock")
            .get("TASK_POOL_SIZE")
            .cloned(),
        Some(Some("3".to_string()))
    );

    std::fs::remove_dir_all(&fixture_root).expect("fixture root should be removed");
}

#[tokio::test]
async fn get_server_settings_does_not_apply_runtime_task_pool_size() {
    let fixture_root = unique_fixture_root("server-settings-read-side-effect-free");
    std::fs::create_dir_all(&fixture_root).expect("fixture root should be created");
    let database_file = fixture_root.join("main.db");
    let persisted_settings = Arc::new(Mutex::new(HashMap::from([(
        "TASK_POOL_SIZE".to_string(),
        Some("4".to_string()),
    )])));
    let apply_count = Arc::new(AtomicUsize::new(0));

    let state = test_operational_state(fixture_root.clone());
    let app = Arc::new(test_app_state(
        database_file,
        state,
        Box::new(FakeTaskEngine {
            apply: {
                let apply_count = apply_count.clone();
                move |_value| {
                    apply_count.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            },
        }),
        fake_settings_store(persisted_settings, Arc::new(AtomicUsize::new(0))),
    ));
    let headers = admin_headers(&fixture_root);

    let response = get_server_settings(State(app), headers).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(apply_count.load(Ordering::SeqCst), 0);

    std::fs::remove_dir_all(&fixture_root).expect("fixture root should be removed");
}

#[tokio::test]
async fn get_server_settings_returns_empty_string_placeholders_for_missing_string_sources() {
    let fixture_root = unique_fixture_root("server-settings-string-placeholders");
    std::fs::create_dir_all(&fixture_root).expect("fixture root should be created");
    let database_file = fixture_root.join("main.db");
    let persisted_settings = Arc::new(Mutex::new(HashMap::new()));
    let settings_store = fake_settings_store(persisted_settings, Arc::new(AtomicUsize::new(0)));

    let state = test_operational_state(fixture_root.clone());
    let app = Arc::new(test_app_state(
        database_file,
        state,
        Box::new(FakeTaskEngine { apply: |_| Ok(()) }),
        settings_store,
    ));
    let headers = admin_headers(&fixture_root);

    let response = get_server_settings(State(app), headers).await;

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

    std::fs::remove_dir_all(&fixture_root).expect("fixture root should be removed");
}

#[tokio::test]
async fn get_server_settings_returns_runtime_server_port_configuration_source() {
    let fixture_root = unique_fixture_root("server-settings-runtime-port-source");
    std::fs::create_dir_all(&fixture_root).expect("fixture root should be created");
    let database_file = fixture_root.join("main.db");
    let persisted_settings = Arc::new(Mutex::new(HashMap::from([(
        "SERVER_PORT".to_string(),
        Some("9090".to_string()),
    )])));
    let settings_store = fake_settings_store(persisted_settings, Arc::new(AtomicUsize::new(0)));

    let mut state = test_operational_state(fixture_root.clone());
    state.runtime.bind_address = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8081));
    state.runtime.configuration_bind_address =
        SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 8081));
    let app = Arc::new(test_app_state(
        database_file,
        state,
        Box::new(FakeTaskEngine { apply: |_| Ok(()) }),
        settings_store,
    ));
    let headers = admin_headers(&fixture_root);

    let response = get_server_settings(State(app), headers).await;

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

    std::fs::remove_dir_all(&fixture_root).expect("fixture root should be removed");
}

struct FakeSettingsStore {
    persisted: Arc<Mutex<HashMap<String, Option<String>>>>,
    persist_attempts: Arc<AtomicUsize>,
}

#[async_trait]
impl ServerSettingsService for FakeSettingsStore {
    async fn load_map(&self) -> Result<BTreeMap<String, Option<String>>, String> {
        Ok(self
            .persisted
            .lock()
            .expect("fake settings store should lock")
            .clone()
            .into_iter()
            .collect())
    }

    async fn load_settings(&self) -> Result<PersistedServerSettings, String> {
        let persisted = self.load_map().await?;
        Ok(PersistedServerSettings {
            delete_empty_collections: persisted
                .get("DELETE_EMPTY_COLLECTIONS")
                .and_then(|v| v.as_deref())
                .is_some_and(|v| v == "true"),
            delete_empty_read_lists: persisted
                .get("DELETE_EMPTY_READLISTS")
                .and_then(|v| v.as_deref())
                .is_some_and(|v| v == "true"),
            remember_me_key: persisted
                .get("REMEMBER_ME_KEY")
                .and_then(|v| v.clone())
                .unwrap_or_default(),
            remember_me_duration_days: persisted
                .get("REMEMBER_ME_DURATION")
                .and_then(|v| v.as_deref())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(365),
            thumbnail_size: "DEFAULT",
            task_pool_size: persisted
                .get("TASK_POOL_SIZE")
                .and_then(|v| v.as_deref())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(1),
            server_port: persisted
                .get("SERVER_PORT")
                .and_then(|v| v.as_deref())
                .and_then(|v| v.parse::<u16>().ok()),
            server_context_path: persisted.get("SERVER_CONTEXT_PATH").and_then(|v| v.clone()),
            kobo_proxy: false,
            kobo_port: None,
        })
    }

    async fn apply_changes(&self, changes: &[(String, Option<String>)]) -> Result<(), String> {
        self.persist_attempts.fetch_add(1, Ordering::SeqCst);
        if changes
            .iter()
            .any(|(key, value)| key == "TASK_POOL_SIZE" && value.as_deref() == Some("4"))
        {
            return Err("reject task pool size update".to_string());
        }

        let mut persisted = self
            .persisted
            .lock()
            .expect("fake settings store should lock");
        for (key, value) in changes {
            if let Some(value) = value {
                persisted.insert(key.clone(), Some(value.clone()));
            } else {
                persisted.remove(key);
            }
        }
        Ok(())
    }
}

struct FakeTaskEngine<F> {
    apply: F,
}

#[async_trait]
impl<F> TaskEnqueuer for FakeTaskEngine<F>
where
    F: Fn(usize) -> Result<(), String> + Send + Sync,
{
    async fn enqueue(&self, _kind: TaskKind, _target_id: &str) {}

    async fn enqueue_request(&self, _request: TaskRequest) {}

    async fn enqueue_batch(&self, _batch: LibraryTaskBatch) {}
}

#[async_trait]
impl<F> TaskEngine for FakeTaskEngine<F>
where
    F: Fn(usize) -> Result<(), String> + Send + Sync,
{
    async fn status(&self) -> QueueStatus {
        QueueStatus::default()
    }

    async fn clear_unowned_tasks(&self) -> usize {
        0
    }

    async fn apply_task_pool_size(&self, value: usize) -> Result<(), String> {
        (self.apply)(value)
    }

    async fn enqueue_task_records(
        &self,
        _task_records: Vec<komga_application::task_processing::TaskQueueRecord>,
        _urgent: bool,
    ) -> Result<(), String> {
        Ok(())
    }

    fn wakeup(&self) {}
}

struct NoopLibraryCatalogService;

#[async_trait]
impl LibraryCatalogService for NoopLibraryCatalogService {
    async fn list_libraries(
        &self,
        _context: komga_domain::discovery::DiscoveryQueryContext,
    ) -> Result<
        Vec<komga_application::library_catalog::LibraryRecord>,
        komga_domain::discovery::DiscoveryError,
    > {
        panic!("library catalog should not be used in server settings tests")
    }

    async fn get_library(
        &self,
        _context: komga_domain::discovery::DiscoveryQueryContext,
        _library_id: String,
    ) -> Result<
        Option<komga_application::library_catalog::LibraryRecord>,
        komga_domain::discovery::DiscoveryError,
    > {
        panic!("library catalog should not be used in server settings tests")
    }

    async fn create_library(
        &self,
        _changes: komga_application::library_catalog::LibraryChangeSet,
    ) -> Result<
        komga_application::library_catalog::CreateLibraryResult,
        komga_application::library_catalog::LibraryCatalogMutationError,
    > {
        panic!("library catalog should not be used in server settings tests")
    }

    async fn update_library(
        &self,
        _library_id: String,
        _changes: komga_application::library_catalog::LibraryChangeSet,
    ) -> Result<
        komga_application::library_catalog::LibraryTaskResult,
        komga_application::library_catalog::LibraryCatalogMutationError,
    > {
        panic!("library catalog should not be used in server settings tests")
    }

    async fn delete_library(
        &self,
        _library_id: String,
    ) -> Result<bool, komga_application::library_catalog::LibraryCatalogMutationError> {
        panic!("library catalog should not be used in server settings tests")
    }

    async fn scan_library(
        &self,
        _library_id: String,
        _deep_scan: bool,
    ) -> Result<
        komga_application::library_catalog::LibraryTaskResult,
        komga_application::library_catalog::LibraryCatalogMutationError,
    > {
        panic!("library catalog should not be used in server settings tests")
    }

    async fn analyze_library(
        &self,
        _library_id: String,
    ) -> Result<
        komga_application::library_catalog::LibraryTaskResult,
        komga_application::library_catalog::LibraryCatalogMutationError,
    > {
        panic!("library catalog should not be used in server settings tests")
    }

    async fn refresh_metadata(
        &self,
        _library_id: String,
    ) -> Result<
        komga_application::library_catalog::LibraryTaskResult,
        komga_application::library_catalog::LibraryCatalogMutationError,
    > {
        panic!("library catalog should not be used in server settings tests")
    }

    async fn empty_trash(
        &self,
        _library_id: String,
    ) -> Result<
        komga_application::library_catalog::LibraryTaskResult,
        komga_application::library_catalog::LibraryCatalogMutationError,
    > {
        panic!("library catalog should not be used in server settings tests")
    }
}

fn test_app_state(
    database_file: PathBuf,
    operational: OperationalState,
    task_queue: Box<dyn TaskEngine>,
    server_settings: Box<dyn ServerSettingsService>,
) -> HttpAppState {
    HttpAppState {
        profile: crate::state::RuntimeProfile::LiveLocaldb,
        read_progress: crate::state::ReadProgressState::default(),
        discovery_auth: crate::discovery_auth::state::DiscoveryAuthState::default(),
        auth_db: crate::state::AuthDatabaseState {
            db: komga_infrastructure::database_handle::DatabaseHandle::single_pool(
                database_file,
                sqlx::sqlite::SqlitePoolOptions::new()
                    .connect_lazy("sqlite::memory:")
                    .expect("lazy in-memory pool should open"),
            ),
            demo_mode: false,
            session_runtime_key: operational.remember_me_runtime_key.clone(),
            remember_me_runtime_key: operational.remember_me_runtime_key.clone(),
        },
        services: HttpServices {
            library_catalog: Box::new(NoopLibraryCatalogService),
            task_queue,
            server_settings,
            runtime_identity: crate::state::default_test_identity_service(),
            operational_runtime: Box::new(NoopOperationalRuntimeService),
            operational_settings: Box::new(NoopOperationalSettingsService),
            media_assets: Box::new(NoopMediaAssetsService),
            opds_catalog: Box::new(NoopOpdsCatalogService),
            opds_persisted: Box::new(NoopOpdsPersistedService),
            discovery_authors: Box::new(NoopDiscoveryAuthorService),
            discovery_library_mapping: Box::new(NoopDiscoveryLibraryMappingService),
            discovery_collection_search: Box::new(NoopDiscoveryCollectionSearchService),
            discovery_readlist_search: Box::new(NoopDiscoveryReadlistSearchService),
            discovery_book_feeds: Box::new(NoopDiscoveryBookFeedService),
            discovery_detail: Box::new(NoopDiscoveryDetailService),
            discovery_list: Box::new(NoopDiscoveryListService),
        },
        operational,
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
        sse: Mutex::new(SseOperationalState {
            accepting_connections: true,
            book_import_events: Vec::<BookImportSseEvent>::new(),
            session_expired_events: Vec::new(),
            next_session_expired_event_id: 1,
        }),
        announcements_cache: Mutex::new(None::<RemoteCacheEntry>),
        releases_cache: Mutex::new(None::<RemoteCacheEntry>),
        transient_books: Mutex::new(TransientBooksStore::with_records(
            std::collections::HashMap::new(),
        )),
        shutdown_trigger: None,
    }
}

fn fake_settings_store(
    persisted: Arc<Mutex<HashMap<String, Option<String>>>>,
    persist_attempts: Arc<AtomicUsize>,
) -> Box<dyn ServerSettingsService> {
    Box::new(FakeSettingsStore {
        persisted,
        persist_attempts,
    })
}

fn admin_headers(fixture_root: &Path) -> HeaderMap {
    let user = AuthUser {
        id: "admin-user".to_string(),
        email: "admin@example.org".to_string(),
        password: String::new(),
        roles: vec!["ADMIN".to_string()],
        shared_all_libraries: true,
        shared_library_ids: Vec::new(),
        labels_allow: Vec::new(),
        labels_exclude: Vec::new(),
        age_restriction: None,
    };
    let identity = default_test_identity_service();
    let token = session_token_for_user_with_runtime_key(
        &*identity,
        &user,
        fixture_root.to_string_lossy().as_ref(),
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        header::HeaderName::from_static("x-auth-token"),
        HeaderValue::from_str(&token).expect("auth token header should be valid"),
    );
    headers
}

fn unique_fixture_root(case_name: &str) -> PathBuf {
    let unique_suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("komga-rust-{case_name}-{unique_suffix}"))
}
