use super::*;

use crate::build_metadata::current_build_metadata;
use crate::runtime::background_workers::{SharedTaskQueue, TaskQueueWakeSignal};

macro_rules! query_service_op {
    ($adapter:expr, |$service:ident, $($arg:ident),*| $body:expr) => {{
        let adapter = $adapter.clone();
        Arc::new(move |$($arg),*| {
            let adapter = adapter.clone();
            Box::pin(async move {
                let $service = LibraryCatalogQueryService::new(adapter);
                $body
            })
        })
    }};
}

macro_rules! task_service_op {
    ($adapter:expr, |$service:ident, $($arg:ident),*| $body:expr) => {{
        let adapter = $adapter.clone();
        Arc::new(move |$($arg),*| {
            let adapter = adapter.clone();
            Box::pin(async move {
                let $service = LibraryTaskService::new(adapter);
                $body
            })
        })
    }};
}

fn with_task_queue<T>(
    task_queue: &SharedTaskQueue,
    operation: impl FnOnce(&mut komga_infrastructure::task_queue::TaskQueueScheduler) -> T,
) -> Result<T, String> {
    let mut queue = task_queue
        .lock()
        .map_err(|_| String::from("task queue lock poisoned"))?;
    Ok(operation(&mut queue))
}

pub(super) fn compose_operational_state(
    config: &RuntimeConfig,
    remember_me_runtime_key: String,
    task_queue: SharedTaskQueue,
    task_wakeup: TaskQueueWakeSignal,
    shutdown_trigger: Option<watch::Sender<bool>>,
) -> OperationalState {
    let runtime_for_apply = config.clone();
    let enqueue_task_queue = task_queue.clone();
    let clear_task_queue = task_queue.clone();
    let count_task_queue = task_queue.clone();
    let apply_task_queue = task_queue.clone();
    let build_metadata = current_build_metadata();
    let transient_books_state_file = http_state_runtime_config::transient_books_state_file(config);

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
        remember_me_runtime_key,
        build_metadata: OperationalBuildMetadata {
            version: build_metadata.version,
            build_time: build_metadata.build_time,
            git_branch: build_metadata.git_branch,
            git_commit_id: build_metadata.git_commit_id,
            git_commit_time: build_metadata.git_commit_time,
        },
        settings_store: Arc::new(
            http_state_operational_access::compose_server_settings_store(
                config.database_file.as_path(),
            ),
        ),
        oauth2_clients: Arc::new(oauth2_clients(config)),
        oauth2_account_creation: config.oauth2_account_creation,
        oidc_email_verification: config.oidc_email_verification,
        enqueue_task_records: Arc::new(move |task_records, urgent| {
            with_task_queue(&enqueue_task_queue, |queue| {
                for task_record in task_records {
                    queue.enqueue(task_record);
                }
            })?;
            if urgent {
                task_wakeup.notify_one();
            }
            Ok(())
        }),
        clear_unowned_tasks: Arc::new(move || {
            with_task_queue(&clear_task_queue, |queue| queue.clear_unowned())
                .expect("task queue state lock should not be poisoned")
        }),
        count_task_queue_by_type: Arc::new(move || {
            with_task_queue(&count_task_queue, |queue| queue.count_by_simple_type())
                .expect("task queue state lock should not be poisoned")
        }),
        apply_task_pool_size: Arc::new(move |value| {
            with_task_queue(&apply_task_queue, |queue| {
                queue.set_task_pool_size(value);
                queue.process_available(&runtime_for_apply)
            })?
            .map(|_| ())
            .map_err(|error: TaskProcessingError| error.to_string())
        }),
        library_catalog: compose_library_catalog_operations(&config.database_file),
        sse: Arc::new(Mutex::new(SseOperationalState {
            accepting_connections: true,
            book_import_events: Vec::<BookImportSseEvent>::new(),
            session_expired_events: Vec::new(),
            next_session_expired_event_id: 1,
        })),
        announcements_cache: Arc::new(Mutex::new(None::<RemoteCacheEntry>)),
        releases_cache: Arc::new(Mutex::new(None::<RemoteCacheEntry>)),
        load_transient_books_records: Arc::new({
            let state_file = transient_books_state_file.clone();
            move || http_state_runtime_config::load_transient_books_records(state_file.as_path())
        }),
        persist_transient_books_records: Arc::new({
            let state_file = transient_books_state_file.clone();
            move |records| {
                http_state_runtime_config::persist_transient_books_records(
                    state_file.as_path(),
                    records,
                )
            }
        }),
        transient_books: Arc::new(Mutex::new(TransientBooksStore::with_records(
            http_state_runtime_config::load_transient_books_records(
                transient_books_state_file.as_path(),
            )
            .unwrap_or_default(),
        ))),
        shutdown_trigger,
    }
}

fn compose_library_catalog_operations(database_file: &Path) -> LibraryCatalogOperations {
    let adapter = SqliteLibraryCatalogAdapter::new(database_file.to_path_buf());

    LibraryCatalogOperations {
        list_libraries: query_service_op!(adapter, |service, context| {
            service.list_libraries(&context).await
        }),
        get_library: query_service_op!(adapter, |service, context, library_id| {
            service.get_library(&context, &library_id).await
        }),
        create_library: Arc::new({
            let adapter = adapter.clone();
            move |changes| {
                let adapter = adapter.clone();
                Box::pin(async move {
                    let service = CreateLibraryService::new(adapter);
                    service.create_library(changes).await
                })
            }
        }),
        update_library: Arc::new({
            let adapter = adapter.clone();
            move |library_id, changes| {
                let adapter = adapter.clone();
                Box::pin(async move {
                    let service = UpdateLibraryService::new(adapter);
                    service.update_library(&library_id, changes).await
                })
            }
        }),
        delete_library: Arc::new({
            let adapter = adapter.clone();
            move |library_id| {
                let adapter = adapter.clone();
                Box::pin(async move {
                    let service = DeleteLibraryService::new(adapter);
                    service.delete_library(&library_id).await
                })
            }
        }),
        scan_library: task_service_op!(adapter, |service, library_id, deep_scan| {
            service.scan_library(&library_id, deep_scan).await
        }),
        analyze_library: task_service_op!(adapter, |service, library_id| {
            service.analyze_library(&library_id).await
        }),
        refresh_metadata: task_service_op!(adapter, |service, library_id| {
            service.refresh_metadata(&library_id).await
        }),
        empty_trash: task_service_op!(adapter, |service, library_id| {
            service.empty_trash(&library_id).await
        }),
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
            scopes: client.scopes.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use komga_application::task_processing::TaskQueueRecord;
    use komga_application::task_processing::TaskRuntimeConfig;
    use komga_infrastructure::task_queue::TaskQueueScheduler;
    use serde_json::json;
    use std::time::Duration;

    fn scan_library_task() -> TaskQueueRecord {
        TaskQueueRecord::new("SCAN_LIBRARY:library-1:DEEP:false", 100, None)
            .with_simple_type("SCAN_LIBRARY")
            .with_payload(
                json!({
                    "libraryId": "library-1",
                    "scanDeep": false,
                    "priority": 100,
                    "groupId": serde_json::Value::Null,
                    "uniqueId": "SCAN_LIBRARY:library-1:DEEP:false",
                })
                .to_string(),
            )
    }

    #[tokio::test]
    async fn enqueue_task_records_respects_urgent_wakeup_policy() {
        for (urgent, timeout_ms, should_notify) in [(true, 100_u64, true), (false, 25_u64, false)] {
            let config =
                RuntimeConfig::for_runtime_profile(crate::config::RuntimeProfile::LiveLocaldb);
            let task_queue = Arc::new(Mutex::new(TaskQueueScheduler::for_runtime(
                config.task_runtime_context(),
                "rust-main",
            )));
            let task_wakeup = Arc::new(tokio::sync::Notify::new());
            let state = compose_operational_state(
                &config,
                "test-runtime".to_string(),
                task_queue.clone(),
                task_wakeup.clone(),
                None,
            );

            (state.enqueue_task_records)(vec![scan_library_task()], urgent)
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
