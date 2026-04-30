use super::*;

use std::collections::BTreeMap;

use crate::build_metadata::current_build_metadata;
use crate::runtime::background_workers::{SharedTaskQueue, TaskQueueWakeSignal};
use async_trait::async_trait;
use komga_application::library_catalog::{
    CreateLibraryResult, CreateLibraryService, DeleteLibraryService, LibraryCatalogMutationError,
    LibraryCatalogQueryService, LibraryChangeSet, LibraryRecord, LibraryTaskResult,
    LibraryTaskService, UpdateLibraryService,
};
use sqlx::SqlitePool;

use komga_application::task_processing::TaskEngine;
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};
use komga_infrastructure::library_catalog::SqliteLibraryCatalogAdapter;
use komga_infrastructure::sqlite::write_models::server_settings::ServerSettingsStore as InfrastructureServerSettingsStore;
use komga_infrastructure::task_queue::{RuntimeTaskEngine, TaskExecutionPoolHandle};

#[derive(Clone)]
pub(super) struct SqliteLibraryCatalogService {
    adapter: SqliteLibraryCatalogAdapter,
}

impl SqliteLibraryCatalogService {
    pub(super) fn new(database_file: &Path, task_write_pool: SqlitePool) -> Self {
        Self {
            adapter: SqliteLibraryCatalogAdapter::new(database_file.to_path_buf(), task_write_pool),
        }
    }
}

#[async_trait]
impl LibraryCatalogService for SqliteLibraryCatalogService {
    async fn list_libraries(
        &self,
        context: DiscoveryQueryContext,
    ) -> Result<Vec<LibraryRecord>, DiscoveryError> {
        let service = LibraryCatalogQueryService::new(self.adapter.clone());
        service.list_libraries(&context).await
    }

    async fn get_library(
        &self,
        context: DiscoveryQueryContext,
        library_id: String,
    ) -> Result<Option<LibraryRecord>, DiscoveryError> {
        let service = LibraryCatalogQueryService::new(self.adapter.clone());
        service.get_library(&context, &library_id).await
    }

    async fn create_library(
        &self,
        changes: LibraryChangeSet,
    ) -> Result<CreateLibraryResult, LibraryCatalogMutationError> {
        let service = CreateLibraryService::new(self.adapter.clone());
        service.create_library(changes).await
    }

    async fn update_library(
        &self,
        library_id: String,
        changes: LibraryChangeSet,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        let service = UpdateLibraryService::new(self.adapter.clone());
        service.update_library(&library_id, changes).await
    }

    async fn delete_library(
        &self,
        library_id: String,
    ) -> Result<bool, LibraryCatalogMutationError> {
        let service = DeleteLibraryService::new(self.adapter.clone());
        service.delete_library(&library_id).await
    }

    async fn scan_library(
        &self,
        library_id: String,
        deep_scan: bool,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        let service = LibraryTaskService::new(self.adapter.clone());
        service.scan_library(&library_id, deep_scan).await
    }

    async fn analyze_library(
        &self,
        library_id: String,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        let service = LibraryTaskService::new(self.adapter.clone());
        service.analyze_library(&library_id).await
    }

    async fn refresh_metadata(
        &self,
        library_id: String,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        let service = LibraryTaskService::new(self.adapter.clone());
        service.refresh_metadata(&library_id).await
    }

    async fn empty_trash(
        &self,
        library_id: String,
    ) -> Result<LibraryTaskResult, LibraryCatalogMutationError> {
        let service = LibraryTaskService::new(self.adapter.clone());
        service.empty_trash(&library_id).await
    }
}

pub(super) fn create_task_engine(
    task_queue: SharedTaskQueue,
    task_wakeup: TaskQueueWakeSignal,
    task_execution_pool: TaskExecutionPoolHandle,
) -> Box<dyn TaskEngine> {
    Box::new(RuntimeTaskEngine::new(
        task_queue,
        task_execution_pool,
        task_wakeup,
    ))
}

#[derive(Clone)]
pub(super) struct RuntimeServerSettingsService {
    store: InfrastructureServerSettingsStore,
}

impl RuntimeServerSettingsService {
    pub(super) fn new(database_file: &Path) -> Self {
        Self {
            store: InfrastructureServerSettingsStore::new(database_file.to_path_buf()),
        }
    }
}

#[async_trait]
impl ServerSettingsService for RuntimeServerSettingsService {
    async fn load_map(&self) -> Result<BTreeMap<String, Option<String>>, String> {
        self.store
            .load_map()
            .await
            .map_err(|error| error.to_string())
    }

    async fn load_settings(&self) -> Result<InterfacesPersistedServerSettings, String> {
        infrastructure_operational_settings::load_server_settings(&self.store)
            .await
            .map(|settings| InterfacesPersistedServerSettings {
                delete_empty_collections: settings.delete_empty_collections,
                delete_empty_read_lists: settings.delete_empty_read_lists,
                remember_me_key: settings.remember_me_key,
                remember_me_duration_days: settings.remember_me_duration_days,
                thumbnail_size: settings.thumbnail_size,
                task_pool_size: settings.task_pool_size,
                server_port: settings.server_port,
                server_context_path: settings.server_context_path,
                kobo_proxy: settings.kobo_proxy,
                kobo_port: settings.kobo_port,
            })
            .map_err(|error| error.to_string())
    }

    async fn apply_changes(&self, changes: &[(String, Option<String>)]) -> Result<(), String> {
        self.store
            .apply_changes(changes)
            .await
            .map_err(|error| error.to_string())
    }
}

pub(super) fn compose_operational_state(
    config: &RuntimeConfig,
    startup_timing: StartupTimingState,
    remember_me_runtime_key: String,
    shutdown_trigger: Option<watch::Sender<bool>>,
) -> OperationalState {
    let build_metadata = current_build_metadata();
    OperationalState {
        runtime: RuntimeState {
            tasks_db_file: config.tasks_db_file.clone(),
            lucene_data_directory: config.lucene_data_directory.clone(),
            fonts_data_directory: config.fonts_data_directory.clone(),
            log_file: config.log_file.clone(),
            config_dir: config.config_dir.clone(),
            bind_address: config.bind_address,
            configuration_bind_address: config.configuration_bind_address,
            server_context_path: config.server_context_path.clone(),
            configuration_server_context_path: config.configuration_server_context_path.clone(),
        },
        startup_timing,
        http_server_requests: HttpServerRequestsState::default(),
        remember_me_runtime_key,
        build_metadata: OperationalBuildMetadata {
            version: build_metadata.version,
            build_time: build_metadata.build_time,
            git_branch: build_metadata.git_branch,
            git_commit_id: build_metadata.git_commit_id,
            git_commit_time: build_metadata.git_commit_time,
        },
        oauth2_clients: oauth2_clients(config),
        oauth2_account_creation: config.oauth2_account_creation,
        oidc_email_verification: config.oidc_email_verification,
        sse: Mutex::new(SseOperationalState {
            accepting_connections: true,
            book_import_events: Vec::<BookImportSseEvent>::new(),
            session_expired_events: Vec::new(),
            next_session_expired_event_id: 1,
        }),
        announcements_cache: Mutex::new(None::<RemoteCacheEntry>),
        releases_cache: Mutex::new(None::<RemoteCacheEntry>),
        transient_books: Mutex::new(TransientBooksStore::default()),
        shutdown_trigger,
    }
}

fn oauth2_clients(config: &RuntimeConfig) -> Vec<OAuth2ClientConfig> {
    config
        .oauth2_clients
        .iter()
        .map(|client| OAuth2ClientConfig {
            registration_id: client.registration_id.clone(),
            client_name: client.client_name.clone(),
            client_id: client.client_id.clone(),
            client_secret: client.client_secret.clone(),
            authorization_uri: client.authorization_uri.clone(),
            token_uri: client.token_uri.clone(),
            user_info_uri: client.user_info_uri.clone(),
            scopes: client.scopes.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use komga_application::task_processing::TaskQueueRecord;
    use komga_infrastructure::database_handle::DatabaseHandle;
    use komga_infrastructure::sqlite::{
        connect_task_pool, connect_task_write_pool, default_read_max_connections,
    };
    use komga_infrastructure::task_queue::TaskRuntimeContext;
    use komga_infrastructure::task_queue::queue_scheduler::TaskQueueScheduler;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::sync::Mutex;

    async fn test_task_runtime_context() -> TaskRuntimeContext {
        let root = std::env::temp_dir().join(format!(
            "komga-operational-state-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).expect("test temp dir should be created");
        let main_db = DatabaseHandle::file_backed(root.join("database.sqlite"))
            .await
            .expect("test db should open");
        let task_write_pool = connect_task_write_pool(main_db.database_file())
            .await
            .expect("test task write pool should open");
        let task_read_pool =
            connect_task_pool(main_db.database_file(), default_read_max_connections())
                .await
                .expect("test task read pool should open");
        TaskRuntimeContext {
            main_db,
            tasks_db_file: root.join("tasks.sqlite"),
            lucene_data_directory: root.join("lucene"),
            consumes_queue: true,
            owns_main_database: true,
            owns_filesystem_scan_output: true,
            owns_sidecar_output: true,
            owns_search_index: true,
            task_pool_size: 1,
            task_write_pool,
            task_read_pool,
        }
    }

    fn scan_library_task() -> TaskQueueRecord {
        use komga_application::task_processing::{ScanLibraryPayload, TaskKind, TaskRequest};
        TaskRequest::with_payload(
            TaskKind::ScanLibrary,
            ScanLibraryPayload::new("library-1", false),
        )
        .priority(8)
        .into_queue_record_with_id("library-1_DEEP_false")
    }

    #[tokio::test]
    async fn enqueue_task_records_respects_urgent_wakeup_policy() {
        for (urgent, timeout_ms, should_notify) in [(true, 100_u64, true), (false, 25_u64, false)] {
            let runtime = test_task_runtime_context().await;
            let task_execution_pool = TaskExecutionPoolHandle::new(runtime.task_pool_size);
            let task_queue = Arc::new(Mutex::new(
                TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await,
            ));
            let task_wakeup = Arc::new(tokio::sync::Notify::new());
            let engine =
                create_task_engine(task_queue.clone(), task_wakeup.clone(), task_execution_pool);

            engine
                .enqueue_task_records(vec![scan_library_task()], urgent)
                .await
                .expect("task enqueue should succeed");

            let notified =
                tokio::time::timeout(Duration::from_millis(timeout_ms), task_wakeup.notified())
                    .await
                    .is_ok();
            assert_eq!(
                notified, should_notify,
                "urgent={urgent} should control background worker wakeup"
            );

            let queued_tasks = task_queue.lock().await.count_by_simple_type().await;
            assert_eq!(queued_tasks.get("ScanLibrary"), Some(&1), "urgent={urgent}");
        }
    }

    #[tokio::test]
    async fn apply_task_pool_size_resizes_execution_pool_and_wakes_scheduler() {
        let runtime = test_task_runtime_context().await;
        let task_execution_pool = TaskExecutionPoolHandle::new(runtime.task_pool_size);
        let task_queue = Arc::new(Mutex::new(
            TaskQueueScheduler::for_runtime(runtime.clone(), "rust-main").await,
        ));
        let task_wakeup = Arc::new(tokio::sync::Notify::new());
        let engine =
            create_task_engine(task_queue, task_wakeup.clone(), task_execution_pool.clone());

        engine
            .apply_task_pool_size(3)
            .await
            .expect("task pool resize should succeed");

        tokio::time::timeout(Duration::from_millis(100), task_wakeup.notified())
            .await
            .expect("task pool resize should wake the background scheduler");
        assert_eq!(task_execution_pool.desired_size(), 3);
    }
}
