use std::path::PathBuf;
use std::sync::Arc;

use crate::runtime_sse::RuntimeSseEventSink;
use crate::task_processing::{ImportBookPayload, SubmitUrgency, TaskQueue, TaskQueueRecord};

mod events;
mod payload;
mod task_records;
#[cfg(test)]
mod tests;

pub use events::{
    RuntimeBookImportEvent, RuntimeBookImportEventBatch, current_runtime_book_import_event_cursor,
    pending_runtime_book_import_events, register_runtime_book_import_event,
};
pub use payload::{BooksImportEntry, BooksImportPayload, ImportCopyMode};

use events::register_runtime_book_added_event;
use task_records::{
    build_import_task_records, import_follow_up_analyze_task, import_follow_up_local_artwork_task,
    import_follow_up_metadata_task, kotlin_import_book_task_id_suffix,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportBookOutcome {
    pub library_id: String,
    pub imported_book_id: String,
    pub sidecar_imported: bool,
    pub artwork_sidecar_imported: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookImportSubmissionFailureKind {
    CreateTask,
    EnqueueTask,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookImportSubmissionFailure {
    pub kind: BookImportSubmissionFailureKind,
    pub series_id: String,
    pub source_file: String,
    pub error: String,
}

#[async_trait::async_trait]
pub trait BookImportPort: Send + Sync {
    async fn import_book(
        &self,
        copy_mode: ImportCopyMode,
        book: BooksImportEntry,
    ) -> Result<Option<ImportBookOutcome>, String>;
}

pub struct BookImportService {
    port: Arc<dyn BookImportPort>,
    runtime_events: Arc<dyn RuntimeSseEventSink>,
}

impl BookImportService {
    pub fn new(
        port: Arc<dyn BookImportPort>,
        runtime_events: Arc<dyn RuntimeSseEventSink>,
    ) -> Self {
        Self {
            port,
            runtime_events,
        }
    }

    pub fn enqueue_books(
        &self,
        payload: BooksImportPayload,
        next_task_id: impl FnMut() -> String,
    ) -> Result<Vec<TaskQueueRecord>, String> {
        build_import_task_records(payload, next_task_id)
    }

    pub async fn submit_books_import(
        &self,
        payload: BooksImportPayload,
        task_queue: &dyn TaskQueue,
    ) -> Vec<BookImportSubmissionFailure> {
        let mut failures = Vec::new();

        for book in payload.books {
            let source_file = book.source_file.display().to_string();
            let series_id = book.series_id.clone();
            let task_id_suffix = kotlin_import_book_task_id_suffix(&book);
            let task_record = match self
                .enqueue_books(
                    BooksImportPayload {
                        copy_mode: payload.copy_mode,
                        books: vec![book],
                    },
                    &mut || task_id_suffix.clone(),
                )
                .and_then(|mut task_records| {
                    task_records
                        .pop()
                        .ok_or_else(|| "import task generation returned no task".to_string())
                }) {
                Ok(task_record) => task_record,
                Err(error) => {
                    failures.push(BookImportSubmissionFailure {
                        kind: BookImportSubmissionFailureKind::CreateTask,
                        series_id,
                        source_file,
                        error,
                    });
                    continue;
                }
            };

            if let Err(error) = task_queue
                .enqueue_records(vec![task_record], SubmitUrgency::Immediate)
                .await
            {
                failures.push(BookImportSubmissionFailure {
                    kind: BookImportSubmissionFailureKind::EnqueueTask,
                    series_id,
                    source_file,
                    error,
                });
            }
        }

        failures
    }

    pub async fn process_books_payload(
        &self,
        payload: BooksImportPayload,
        import_priority: i32,
    ) -> Result<Vec<TaskQueueRecord>, String> {
        let mut follow_up_tasks = Vec::new();

        for book in payload.books {
            let series_id = book.series_id.clone();
            let source_file = book.source_file.to_string_lossy().to_string();
            let outcome = match self.port.import_book(payload.copy_mode, book).await {
                Ok(Some(outcome)) => outcome,
                Ok(None) => continue,
                Err(error) => {
                    register_runtime_book_import_event(
                        self.runtime_events.as_ref(),
                        None,
                        source_file,
                        false,
                        Some(error.clone()),
                    );
                    return Err(error);
                }
            };
            register_runtime_book_import_event(
                self.runtime_events.as_ref(),
                Some(outcome.imported_book_id.clone()),
                source_file,
                true,
                None,
            );
            register_runtime_book_added_event(
                self.runtime_events.as_ref(),
                &outcome,
                series_id.as_str(),
            );
            let book_id = outcome.imported_book_id.as_str();

            follow_up_tasks.push(import_follow_up_analyze_task(
                book_id,
                import_priority,
                &series_id,
            ));

            if outcome.sidecar_imported {
                follow_up_tasks.push(import_follow_up_metadata_task(book_id, &series_id));
            }

            if outcome.artwork_sidecar_imported {
                follow_up_tasks.push(import_follow_up_local_artwork_task(book_id));
            }
        }

        Ok(follow_up_tasks)
    }

    pub async fn process_queued_book_payload(
        &self,
        payload: ImportBookPayload,
        import_priority: i32,
    ) -> Result<Vec<TaskQueueRecord>, String> {
        self.process_books_payload(
            BooksImportPayload {
                copy_mode: payload.copy_mode,
                books: vec![BooksImportEntry {
                    source_file: PathBuf::from(payload.source_file),
                    series_id: payload.series_id,
                    destination_name: payload.destination_name,
                    upgrade_book_id: payload.upgrade_book_id,
                }],
            },
            import_priority,
        )
        .await
    }
}
