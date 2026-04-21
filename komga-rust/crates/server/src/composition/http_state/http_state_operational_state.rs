use super::*;

use std::collections::BTreeMap;

use crate::build_metadata::current_build_metadata;
use crate::runtime::background_workers::{
    SharedTaskQueue, TaskQueueWakeSignal, WorkerRuntimeGuard,
};
use async_trait::async_trait;
use komga_application::library_catalog::{
    CreateLibraryResult, CreateLibraryService, DeleteLibraryService, LibraryCatalogMutationError,
    LibraryCatalogQueryService, LibraryChangeSet, LibraryRecord, LibraryTaskResult,
    LibraryTaskService, UpdateLibraryService,
};
use komga_application::task_processing::TaskQueueRecord;
use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};
use komga_infrastructure::library_catalog::SqliteLibraryCatalogAdapter;
use komga_infrastructure::sqlite::write_models::server_settings::ServerSettingsStore as InfrastructureServerSettingsStore;

#[derive(Clone)]
pub(super) struct SqliteLibraryCatalogService {
    adapter: SqliteLibraryCatalogAdapter,
}

impl SqliteLibraryCatalogService {
    pub(super) fn new(database_file: &Path) -> Self {
        Self {
            adapter: SqliteLibraryCatalogAdapter::new(database_file.to_path_buf()),
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

#[derive(Clone)]
pub(super) struct RuntimeTaskQueueService {
    task_queue: SharedTaskQueue,
    task_wakeup: TaskQueueWakeSignal,
    worker_runtime_guard: Option<WorkerRuntimeGuard>,
}

impl RuntimeTaskQueueService {
    pub(super) fn new(
        task_queue: SharedTaskQueue,
        task_wakeup: TaskQueueWakeSignal,
        worker_runtime_guard: Option<WorkerRuntimeGuard>,
    ) -> Self {
        Self {
            task_queue,
            task_wakeup,
            worker_runtime_guard,
        }
    }
}

#[async_trait]
impl TaskQueueService for RuntimeTaskQueueService {
    async fn enqueue_task_records(
        &self,
        task_records: Vec<TaskQueueRecord>,
        urgent: bool,
    ) -> Result<(), String> {
        let _worker_runtime_guard = self.worker_runtime_guard.clone();
        with_task_queue(&self.task_queue, |queue| {
            for task_record in task_records {
                queue.enqueue(task_record);
            }
        })?;
        if urgent {
            self.task_wakeup.notify_one();
        }
        Ok(())
    }

    async fn clear_unowned_tasks(&self) -> usize {
        with_task_queue(&self.task_queue, |queue| queue.clear_unowned())
            .expect("task queue state lock should not be poisoned")
    }

    async fn count_task_queue_by_type(&self) -> BTreeMap<String, usize> {
        with_task_queue(&self.task_queue, |queue| queue.count_by_simple_type())
            .expect("task queue state lock should not be poisoned")
    }

    async fn apply_task_pool_size(&self, value: usize) -> Result<(), String> {
        with_task_queue(&self.task_queue, |queue| queue.set_task_pool_size(value))?;
        self.task_wakeup.notify_one();
        Ok(())
    }
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

fn with_task_queue<T>(
    task_queue: &SharedTaskQueue,
    operation: impl FnOnce(
        &mut komga_infrastructure::task_queue::queue_scheduler::TaskQueueScheduler,
    ) -> T,
) -> Result<T, String> {
    let mut queue = task_queue
        .lock()
        .map_err(|_| String::from("task queue lock poisoned"))?;
    Ok(operation(&mut queue))
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
            database_file: config.database_file.clone(),
            tasks_db_file: config.tasks_db_file.clone(),
            lucene_data_directory: config.lucene_data_directory.clone(),
            fonts_data_directory: config.fonts_data_directory.clone(),
            log_file: config.log_file.clone(),
            config_dir: config.config_dir.clone(),
            bind_address: config.bind_address,
            server_context_path: config.server_context_path.clone(),
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
    use komga_infrastructure::task_queue::queue_scheduler::TaskQueueScheduler;
    use serde_json::json;
    use std::sync::Arc;
    use std::time::Duration;

    fn scan_library_task() -> TaskQueueRecord {
        TaskQueueRecord::new("SCAN_LIBRARY_library-1_DEEP_false", 8, None)
            .with_simple_type("SCAN_LIBRARY")
            .with_payload(
                json!({
                    "libraryId": "library-1",
                    "scanDeep": false,
                    "priority": 8,
                    "groupId": serde_json::Value::Null,
                    "uniqueId": "SCAN_LIBRARY_library-1_DEEP_false",
                })
                .to_string(),
            )
    }

    #[tokio::test]
    async fn enqueue_task_records_respects_urgent_wakeup_policy() {
        for (urgent, timeout_ms, should_notify) in [(true, 100_u64, true), (false, 25_u64, false)] {
            let config = RuntimeConfig::for_runtime_profile(
                komga_config::profile::RuntimeProfile::LiveLocaldb,
            );
            let task_queue = Arc::new(Mutex::new(TaskQueueScheduler::for_runtime(
                crate::config::task_runtime_context(&config),
                "rust-main",
            )));
            let task_wakeup = Arc::new(tokio::sync::Notify::new());
            let service =
                RuntimeTaskQueueService::new(task_queue.clone(), task_wakeup.clone(), None);

            service
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

            let queued_tasks = task_queue
                .lock()
                .expect("task queue lock should not be poisoned")
                .count_by_simple_type();
            assert_eq!(
                queued_tasks.get("SCAN_LIBRARY"),
                Some(&1),
                "urgent={urgent}"
            );
        }
    }
}
