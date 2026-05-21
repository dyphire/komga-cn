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
use crate::state::MediaAssetsState;

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

    enqueue_books_best_effort(payload, &app).await;

    let mut response = StatusCode::ACCEPTED.into_response();
    mark_runtime_owned(&mut response);
    response
}

async fn enqueue_books_best_effort(payload: BooksImportPayload, app: &MediaAssetsState) {
    for book in payload.books {
        let source_file = book.source_file.display().to_string();
        let series_id = book.series_id.clone();
        let task_id_suffix = kotlin_import_book_task_id_suffix(&book);
        match app
            .import
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
                    .queue
                    .enqueue_records(
                        vec![interface_task_record(task_record)],
                        komga_application::task_processing::SubmitUrgency::Immediate,
                    )
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
    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::state::{MediaAssetsState, TaskQueueAdmin, TaskQueueState};

    struct TestTaskQueue {
        persisted_ids: Arc<tokio::sync::Mutex<Vec<String>>>,
    }

    #[async_trait::async_trait]
    impl komga_application::task_processing::TaskQueue for TestTaskQueue {
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

        async fn enqueue_records(
            &self,
            records: Vec<komga_application::task_processing::TaskQueueRecord>,
            _urgency: komga_application::task_processing::SubmitUrgency,
        ) -> Result<(), String> {
            let task_record = records.into_iter().next().expect("task should exist");
            if task_record.id == "ImportBook_series-1_/tmp/book-a.cbz" {
                Err("first enqueue failed".to_string())
            } else {
                self.persisted_ids.lock().await.push(task_record.id);
                Ok(())
            }
        }

        async fn status(&self) -> komga_application::task_processing::QueueStatus {
            komga_application::task_processing::QueueStatus::default()
        }
    }

    #[async_trait::async_trait]
    impl TaskQueueAdmin for TestTaskQueue {
        async fn clear_unowned_tasks(&self) -> usize {
            0
        }

        async fn apply_pool_size(&self, _value: usize) -> Result<(), String> {
            Ok(())
        }

        fn wakeup(&self) {}
    }

    async fn test_media_state(task_queue: Arc<dyn TaskQueueAdmin>) -> MediaAssetsState {
        use komga_infrastructure::content_resolver::ContentResolver;
        use komga_infrastructure::database_handle::DatabaseHandle;
        use komga_infrastructure::discovery_detail_access::DiscoveryDetailAccess;
        use komga_infrastructure::filesystem::import::FilesystemImportPort;
        use komga_infrastructure::media_reader::MediaReader;
        use komga_infrastructure::progress_writer::ProgressWriter;
        use komga_infrastructure::thumbnail_writer::ThumbnailWriter;

        let pool = sqlx::SqlitePool::connect_lazy("sqlite::memory:").expect("test pool");
        let handle = DatabaseHandle::single_pool(PathBuf::from(":memory:"), pool.clone());

        let detail_access: Arc<
            komga_infrastructure::discovery_detail_access::DiscoveryDetailAccess,
        > = Arc::new(DiscoveryDetailAccess::new(handle, PathBuf::new()));

        MediaAssetsState {
            read_progress: crate::state::ReadProgressState::default(),
            identity: crate::state::tests::test_identity_state().await,
            task_queue: TaskQueueState {
                queue: task_queue.clone(),
            },
            book_detail: detail_access.clone(),
            series_detail: detail_access.clone(),
            collection: detail_access.clone(),
            readlist: detail_access,
            reader: MediaReader::new(pool.clone()),
            content: ContentResolver,
            thumbnails: ThumbnailWriter::new(pool.clone()),
            progress: ProgressWriter::new(pool.clone()),
            metadata: Arc::new(komga_application::media_assets::MetadataWriter::new(
                Box::new(crate::state::tests::NoopBookMetadataPort),
                Box::new(crate::state::tests::NoopSearchSyncPort),
                Box::new(
                    komga_infrastructure::task_enqueue_adapter::TaskEnqueueAdapter::new(task_queue),
                ),
                Box::new(komga_infrastructure::event_emitter_adapter::SseBookEventEmitter),
            )),
            import: Arc::new(komga_application::media_assets::MediaImportService::new(
                Arc::new(FilesystemImportPort::new("/tmp/test.db")),
            )),
        }
    }

    #[tokio::test]
    async fn enqueue_books_best_effort_keeps_later_books_when_earlier_enqueue_fails() {
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
        let media_state = test_media_state(Arc::new(TestTaskQueue {
            persisted_ids: persisted_ids.clone(),
        }))
        .await;

        enqueue_books_best_effort(payload, &media_state).await;

        assert_eq!(
            persisted_ids.lock().await.clone(),
            vec!["ImportBook_series-2_/tmp/book-b.cbz".to_string()]
        );
    }
}
