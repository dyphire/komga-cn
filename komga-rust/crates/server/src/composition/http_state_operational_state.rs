use super::*;

pub(super) fn compose_operational_state(
    config: &RuntimeConfig,
    task_queue: SharedTaskQueue,
    shutdown_trigger: Option<watch::Sender<bool>>,
) -> OperationalState {
    let runtime_for_apply = config.clone();
    let enqueue_task_queue = task_queue.clone();
    let clear_task_queue = task_queue.clone();
    let count_task_queue = task_queue.clone();
    let apply_task_queue = task_queue.clone();

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
        settings_store: Arc::new(
            http_state_operational_access::compose_server_settings_store(
                config.database_file.as_path(),
            ),
        ),
        oauth2_clients: Arc::new(oauth2_clients(config)),
        oauth2_account_creation: config.oauth2_account_creation,
        oidc_email_verification: config.oidc_email_verification,
        enqueue_task_records: Arc::new(move |task_records, _urgent| {
            let mut queue = enqueue_task_queue
                .lock()
                .map_err(|_| String::from("task queue lock poisoned"))?;
            for task_record in task_records {
                queue.enqueue(task_record);
            }
            Ok(())
        }),
        clear_unowned_tasks: Arc::new(move || {
            clear_task_queue
                .lock()
                .expect("task queue state lock should not be poisoned")
                .clear_unowned()
        }),
        count_task_queue_by_type: Arc::new(move || {
            count_task_queue
                .lock()
                .expect("task queue state lock should not be poisoned")
                .count_by_simple_type()
        }),
        apply_task_pool_size: Arc::new(move |value| {
            let mut queue = apply_task_queue
                .lock()
                .map_err(|_| String::from("task queue lock poisoned"))?;
            queue.set_task_pool_size(value);
            queue
                .process_available(&runtime_for_apply)
                .map(|_| ())
                .map_err(|error: TaskProcessingError| error.to_string())
        }),
        library_catalog: compose_library_catalog_operations(&config.database_file),
        sse: Arc::new(Mutex::new(SseOperationalState {
            accepting_connections: true,
            book_import_events: Vec::<BookImportSseEvent>::new(),
        })),
        announcements_cache: Arc::new(Mutex::new(None::<RemoteCacheEntry>)),
        releases_cache: Arc::new(Mutex::new(None::<RemoteCacheEntry>)),
        load_transient_books_records: Arc::new({
            let state_file = http_state_runtime_config::transient_books_state_file(config);
            move || http_state_runtime_config::load_transient_books_records(state_file.as_path())
        }),
        persist_transient_books_records: Arc::new({
            let state_file = http_state_runtime_config::transient_books_state_file(config);
            move |records| {
                http_state_runtime_config::persist_transient_books_records(
                    state_file.as_path(),
                    records,
                )
            }
        }),
        transient_books: Arc::new(Mutex::new(TransientBooksStore::with_records(
            http_state_runtime_config::load_transient_books_records(
                http_state_runtime_config::transient_books_state_file(config).as_path(),
            )
            .unwrap_or_default(),
        ))),
        shutdown_trigger,
    }
}

fn compose_library_catalog_operations(database_file: &Path) -> LibraryCatalogOperations {
    let adapter = SqliteLibraryCatalogAdapter::new(database_file.to_path_buf());

    LibraryCatalogOperations {
        list_libraries: Arc::new({
            let adapter = adapter.clone();
            move |context| {
                let adapter = adapter.clone();
                Box::pin(async move {
                    let service = LibraryCatalogQueryService::new(adapter);
                    service.list_libraries(&context).await
                })
            }
        }),
        get_library: Arc::new({
            let adapter = adapter.clone();
            move |context, library_id| {
                let adapter = adapter.clone();
                Box::pin(async move {
                    let service = LibraryCatalogQueryService::new(adapter);
                    service.get_library(&context, &library_id).await
                })
            }
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
        scan_library: Arc::new({
            let adapter = adapter.clone();
            move |library_id, deep_scan| {
                let adapter = adapter.clone();
                Box::pin(async move {
                    let service = LibraryTaskService::new(adapter);
                    service.scan_library(&library_id, deep_scan).await
                })
            }
        }),
        analyze_library: Arc::new({
            let adapter = adapter.clone();
            move |library_id| {
                let adapter = adapter.clone();
                Box::pin(async move {
                    let service = LibraryTaskService::new(adapter);
                    service.analyze_library(&library_id).await
                })
            }
        }),
        refresh_metadata: Arc::new({
            let adapter = adapter.clone();
            move |library_id| {
                let adapter = adapter.clone();
                Box::pin(async move {
                    let service = LibraryTaskService::new(adapter);
                    service.refresh_metadata(&library_id).await
                })
            }
        }),
        empty_trash: Arc::new(move |library_id| {
            let adapter = adapter.clone();
            Box::pin(async move {
                let service = LibraryTaskService::new(adapter);
                service.empty_trash(&library_id).await
            })
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
