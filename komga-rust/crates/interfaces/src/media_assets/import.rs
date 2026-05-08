use super::*;
use axum::extract::State;
use komga_application::media_assets::{
    BooksImportEntry as ApplicationBooksImportEntry,
    BooksImportPayload as ApplicationBooksImportPayload,
    ImportCopyMode as ApplicationImportCopyMode,
};
use komga_application::task_processing::TaskQueueRecord as ApplicationTaskQueueRecord;
use tracing::error;

use crate::identity_access::auth::Admin;
use crate::state::{MediaAssetsState, RuntimeMediaImportService};

pub async fn books_import(
    State(app): State<MediaAssetsState>,
    _admin: Admin,
    Json(body): Json<Value>,
) -> Response {
    let payload = match parse_books_import_payload(&body) {
        Ok(payload) => payload,
        Err(error) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))).into_response();
        }
    };

    let service = app.media_assets.media_import_service();
    enqueue_books_best_effort(service.as_ref(), payload, &app).await;

    let mut response = StatusCode::ACCEPTED.into_response();
    mark_runtime_owned(&mut response);
    response
}

async fn enqueue_books_best_effort(
    service: &dyn RuntimeMediaImportService,
    payload: BooksImportPayload,
    app: &MediaAssetsState,
) {
    for book in payload.books {
        let source_file = book.source_file.display().to_string();
        let series_id = book.series_id.clone();
        let task_id_suffix = kotlin_import_book_task_id_suffix(&book);
        match service
            .enqueue_books(
                application_import_payload(BooksImportPayload {
                    copy_mode: payload.copy_mode,
                    books: vec![book],
                }),
                &mut || task_id_suffix.clone(),
            )
            .and_then(|mut task_records| {
                task_records
                    .pop()
                    .ok_or_else(|| "import task generation returned no task".to_string())
            }) {
            Ok(task_record) => {
                if let Err(err) = app
                    .task_queue
                    .engine
                    .enqueue_task_records(vec![interface_task_record(task_record)], true)
                    .await
                {
                    error!(
                        %series_id,
                        source_file = %source_file,
                        error = %err,
                        "Failed to enqueue import task"
                    );
                }
            }
            Err(err) => {
                error!(
                    %series_id,
                    source_file = %source_file,
                    error = %err,
                    "Failed to create import task"
                );
            }
        }
    }
}

fn kotlin_import_book_task_id_suffix(book: &BooksImportEntry) -> String {
    format!("{}_{}", book.series_id, book.source_file.display())
}

fn application_import_payload(payload: BooksImportPayload) -> ApplicationBooksImportPayload {
    ApplicationBooksImportPayload {
        copy_mode: match payload.copy_mode {
            ImportCopyMode::Move => ApplicationImportCopyMode::Move,
            ImportCopyMode::Copy => ApplicationImportCopyMode::Copy,
            ImportCopyMode::Hardlink => ApplicationImportCopyMode::Hardlink,
        },
        books: payload
            .books
            .into_iter()
            .map(|book| ApplicationBooksImportEntry {
                source_file: book.source_file,
                series_id: book.series_id,
                destination_name: book.destination_name,
                upgrade_book_id: book.upgrade_book_id,
            })
            .collect(),
    }
}

fn interface_task_record(task: ApplicationTaskQueueRecord) -> TaskQueueRecord {
    let mut record = TaskQueueRecord::new(task.id, task.priority, task.group);
    record.simple_type = task.simple_type;
    record.payload = task.payload;
    record.owner = task.owner;
    record.order = task.order;
    record
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use komga_application::task_processing::{TaskKind, TaskRequest};

    use crate::state::{
        HttpServerRequestsState, IdentityState, MediaAssetsState, OAuth2ClientConfig,
        OperationalBuildMetadata, OperationalState, RemoteCacheEntry, RuntimeState,
        SseOperationalState, StartupTimingState, TaskEngine, TaskQueueState, TransientBooksStore,
        tests::{NoopDiscoveryDetailService, NoopMediaAssetsService},
    };

    struct StubImportService;

    impl RuntimeMediaImportService for StubImportService {
        fn enqueue_books(
            &self,
            payload: ApplicationBooksImportPayload,
            next_task_id: &mut dyn FnMut() -> String,
        ) -> Result<Vec<ApplicationTaskQueueRecord>, String> {
            let book = payload
                .books
                .into_iter()
                .next()
                .expect("test payload should contain one book");
            Ok(vec![
                TaskRequest::new(TaskKind::ImportBook)
                    .priority(100)
                    .group(book.series_id)
                    .into_queue_record_with_id(&next_task_id()),
            ])
        }

        fn process_queued_book_payload<'a>(
            &'a self,
            _task_payload: &'a str,
            _import_priority: i32,
        ) -> futures_util::future::BoxFuture<'a, Result<Vec<ApplicationTaskQueueRecord>, String>>
        {
            Box::pin(async { panic!("process_queued_book_payload should not be called in tests") })
        }
    }

    struct TestTaskEngine {
        persisted_ids: Arc<tokio::sync::Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl komga_application::task_processing::TaskEnqueuer for TestTaskEngine {
        async fn enqueue(
            &self,
            _kind: komga_application::task_processing::TaskKind,
            _target_id: &str,
        ) {
        }

        async fn enqueue_request(&self, _request: komga_application::task_processing::TaskRequest) {
        }

        async fn enqueue_batch(
            &self,
            _batch: komga_application::task_processing::LibraryTaskBatch,
        ) {
        }
    }

    #[async_trait::async_trait]
    impl TaskEngine for TestTaskEngine {
        async fn status(&self) -> komga_application::task_processing::QueueStatus {
            komga_application::task_processing::QueueStatus::default()
        }

        async fn clear_unowned_tasks(&self) -> usize {
            0
        }

        async fn apply_task_pool_size(&self, _value: usize) -> Result<(), String> {
            Ok(())
        }

        async fn enqueue_task_records(
            &self,
            task_records: Vec<komga_application::task_processing::TaskQueueRecord>,
            _urgent: bool,
        ) -> Result<(), String> {
            let task_record = task_records.into_iter().next().expect("task should exist");
            if task_record.id == "ImportBook_series-1_/tmp/book-a.cbz" {
                Err("first enqueue failed".to_string())
            } else {
                self.persisted_ids.lock().await.push(task_record.id);
                Ok(())
            }
        }

        fn wakeup(&self) {}
    }

    fn test_media_state(task_queue: Arc<dyn TaskEngine>) -> MediaAssetsState {
        let operational = OperationalState {
            runtime: RuntimeState {
                tasks_db_file: PathBuf::from("/tmp/tasks.db"),
                lucene_data_directory: PathBuf::from("/tmp/lucene"),
                fonts_data_directory: PathBuf::from("/tmp/fonts"),
                log_file: PathBuf::from("/tmp/komga.log"),
                config_dir: None,
                bind_address: "127.0.0.1:0".parse().expect("bind address"),
                configuration_bind_address: "127.0.0.1:0"
                    .parse()
                    .expect("configuration bind address"),
                server_context_path: None,
                configuration_server_context_path: None,
            },
            startup_timing: StartupTimingState::default(),
            http_server_requests: HttpServerRequestsState::default(),
            remember_me_runtime_key: "test".to_string(),
            build_metadata: OperationalBuildMetadata {
                version: "0.1.0".to_string(),
                build_time: "2026-04-09T00:00:00Z".to_string(),
                git_branch: None,
                git_commit_id: None,
                git_commit_time: None,
            },
            oauth2_clients: Vec::<OAuth2ClientConfig>::new(),
            oauth2_account_creation: false,
            oidc_email_verification: false,
            sse: Arc::new(Mutex::new(SseOperationalState::default())),
            announcements_cache: Arc::new(Mutex::new(None::<RemoteCacheEntry>)),
            releases_cache: Arc::new(Mutex::new(None::<RemoteCacheEntry>)),
            transient_books: Arc::new(Mutex::new(
                TransientBooksStore::with_records(HashMap::new()),
            )),
            shutdown_trigger: None,
        };

        MediaAssetsState {
            read_progress: crate::state::ReadProgressState::default(),
            operational,
            identity: IdentityState {
                service: crate::state::default_test_identity_service(),
            },
            media_assets: Arc::new(NoopMediaAssetsService),
            task_queue: TaskQueueState { engine: task_queue },
            discovery_detail: Arc::new(NoopDiscoveryDetailService),
        }
    }

    #[tokio::test]
    async fn enqueue_books_best_effort_keeps_later_books_when_earlier_enqueue_fails() {
        let service = StubImportService;
        let payload = BooksImportPayload {
            copy_mode: ImportCopyMode::Copy,
            books: vec![
                BooksImportEntry {
                    source_file: PathBuf::from("/tmp/book-a.cbz"),
                    series_id: "series-1".to_string(),
                    destination_name: None,
                    upgrade_book_id: None,
                },
                BooksImportEntry {
                    source_file: PathBuf::from("/tmp/book-b.cbz"),
                    series_id: "series-2".to_string(),
                    destination_name: None,
                    upgrade_book_id: None,
                },
            ],
        };
        let persisted_ids = Arc::new(tokio::sync::Mutex::new(Vec::new()));
        let media_state = test_media_state(Arc::new(TestTaskEngine {
            persisted_ids: persisted_ids.clone(),
        }));

        enqueue_books_best_effort(&service, payload, &media_state).await;

        assert_eq!(
            persisted_ids.lock().await.clone(),
            vec!["ImportBook_series-2_/tmp/book-b.cbz".to_string()]
        );
    }
}
