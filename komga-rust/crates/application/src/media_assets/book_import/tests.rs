use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

use crate::runtime_sse::{RuntimeSseEvent, RuntimeSseEventLog, RuntimeSseEventStore};
use crate::task_processing::{ImportBookPayload, TaskKind, TaskQueueRecord, TaskRequest};

use super::*;

fn test_waker() -> Waker {
    fn noop(_: *const ()) {}
    fn clone(_: *const ()) -> RawWaker {
        RawWaker::new(std::ptr::null(), &VTABLE)
    }

    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
    let raw = RawWaker::new(std::ptr::null(), &VTABLE);
    unsafe { Waker::from_raw(raw) }
}

fn block_on_ready<F: Future>(future: F) -> F::Output {
    let waker = test_waker();
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("test future unexpectedly yielded pending"),
    }
}

struct StubImportPort {
    result: Result<Option<ImportBookOutcome>, String>,
}

#[async_trait::async_trait]
impl BookImportPort for StubImportPort {
    async fn import_book(
        &self,
        _copy_mode: ImportCopyMode,
        _book: BooksImportEntry,
    ) -> anyhow::Result<Option<ImportBookOutcome>> {
        self.result.clone().map_err(anyhow::Error::msg)
    }
}

fn import_service(
    result: Result<Option<ImportBookOutcome>, String>,
) -> (BookImportService, Arc<RuntimeSseEventStore>) {
    let runtime_events = Arc::new(RuntimeSseEventStore::default());
    (
        BookImportService::new(Arc::new(StubImportPort { result }), runtime_events.clone()),
        runtime_events,
    )
}

struct RecordingTaskQueue {
    persisted_ids: std::sync::Mutex<Vec<String>>,
}

#[async_trait::async_trait]
impl crate::task_processing::TaskQueue for RecordingTaskQueue {
    async fn enqueue(&self, _kind: TaskKind, _target_id: &str) {}

    async fn enqueue_request(&self, _request: TaskRequest) {}

    async fn enqueue_batch(&self, _batch: crate::task_processing::LibraryTaskBatch) {}

    async fn enqueue_records(
        &self,
        records: Vec<TaskQueueRecord>,
        urgency: crate::task_processing::SubmitUrgency,
    ) -> anyhow::Result<()> {
        assert_eq!(urgency, crate::task_processing::SubmitUrgency::Immediate);
        let task_record = records.into_iter().next().expect("task should exist");
        if task_record.id == "ImportBook_series-1_/tmp/book-a.cbz" {
            Err(anyhow::anyhow!("first enqueue failed"))
        } else {
            self.persisted_ids.lock().unwrap().push(task_record.id);
            Ok(())
        }
    }

    async fn status(&self) -> anyhow::Result<crate::task_processing::QueueStatus> {
        Ok(crate::task_processing::QueueStatus::default())
    }
}

#[tokio::test]
async fn submit_books_import_keeps_later_books_when_earlier_enqueue_fails() {
    let (service, _) = import_service(Ok(None));
    let queue = RecordingTaskQueue {
        persisted_ids: std::sync::Mutex::new(Vec::new()),
    };

    let failures = service
        .submit_books_import(
            BooksImportPayload {
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
            },
            &queue,
        )
        .await;

    assert_eq!(
        queue.persisted_ids.lock().unwrap().clone(),
        vec!["ImportBook_series-2_/tmp/book-b.cbz".to_string()]
    );
    assert_eq!(
        failures,
        vec![BookImportSubmissionFailure {
            kind: BookImportSubmissionFailureKind::EnqueueTask,
            series_id: "series-1".to_string(),
            source_file: "/tmp/book-a.cbz".to_string(),
            error: "first enqueue failed".to_string(),
        }]
    );
}

#[test]
fn enqueue_books_uses_application_import_task_payload_contract() {
    let (service, _) = import_service(Ok(None));
    let records = service
        .enqueue_books(
            BooksImportPayload {
                copy_mode: ImportCopyMode::Hardlink,
                books: vec![BooksImportEntry {
                    source_file: PathBuf::from("/tmp/book-a.cbz"),
                    series_id: "series-1".to_string(),
                    destination_name: Some("dest-a".to_string()),
                    upgrade_book_id: Some("book-1".to_string()),
                }],
            },
            || "series-1_/tmp/book-a.cbz".to_string(),
        )
        .expect("import task should be generated");

    let record = records.into_iter().next().expect("task should exist");
    assert_eq!(record.id, "ImportBook_series-1_/tmp/book-a.cbz");
    assert_eq!(record.simple_type, "ImportBook");
    assert_eq!(record.priority, 100);
    assert_eq!(record.group.as_deref(), Some("series-1"));

    let payload =
        ImportBookPayload::from_task_record(&record).expect("import task payload should parse");
    assert_eq!(payload.source_file, "/tmp/book-a.cbz");
    assert_eq!(payload.series_id, "series-1");
    assert_eq!(payload.copy_mode, ImportCopyMode::Hardlink);
    assert_eq!(payload.destination_name.as_deref(), Some("dest-a"));
    assert_eq!(payload.upgrade_book_id.as_deref(), Some("book-1"));
}

#[test]
fn process_books_payload_enqueues_local_artwork_refresh_for_imported_artwork_sidecars() {
    let (service, _) = import_service(Ok(Some(ImportBookOutcome {
        library_id: "library-1".to_string(),
        imported_book_id: "book-1".to_string(),
        sidecar_imported: false,
        artwork_sidecar_imported: true,
    })));

    let tasks = block_on_ready(service.process_books_payload(
        BooksImportPayload {
            copy_mode: ImportCopyMode::Copy,
            books: vec![BooksImportEntry {
                source_file: PathBuf::from("/tmp/book.cbz"),
                series_id: "series-1".to_string(),
                destination_name: None,
                upgrade_book_id: None,
            }],
        },
        100,
    ))
    .expect("artwork sidecar import should enqueue follow-up tasks");

    assert_eq!(
        tasks.len(),
        2,
        "analyze and artwork refresh should both be queued"
    );
    assert!(tasks.iter().any(|task| {
        task.id == "AnalyzeBook_book-1"
            && task.group == Some("series-1".to_string())
            && task.simple_type == "AnalyzeBook"
            && task.priority == 101
    }));
    assert!(tasks.iter().any(|task| {
        task.id == "RefreshBookLocalArtwork_book-1"
            && task.simple_type == "RefreshBookLocalArtwork"
            && task.group.is_none()
            && task.priority == 4
    }));
}

#[test]
fn process_queued_book_payload_imports_typed_task_payload() {
    let (service, _) = import_service(Ok(Some(ImportBookOutcome {
        library_id: "library-1".to_string(),
        imported_book_id: "book-1".to_string(),
        sidecar_imported: false,
        artwork_sidecar_imported: false,
    })));

    let tasks = block_on_ready(service.process_queued_book_payload(
        ImportBookPayload::new(
            "/tmp/book.cbz",
            "series-1",
            ImportCopyMode::Copy,
            Some("dest-a".to_string()),
            Some("book-1".to_string()),
        ),
        100,
    ))
    .expect("typed import payload should process successfully");

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, "AnalyzeBook_book-1");
    assert_eq!(tasks[0].simple_type, "AnalyzeBook");
    assert_eq!(tasks[0].priority, 101);
    assert_eq!(tasks[0].group.as_deref(), Some("series-1"));
}

#[test]
fn process_books_payload_registers_import_runtime_events() {
    let (service, runtime_events) = import_service(Ok(Some(ImportBookOutcome {
        library_id: "library-1".to_string(),
        imported_book_id: "book-1".to_string(),
        sidecar_imported: false,
        artwork_sidecar_imported: false,
    })));
    let cursor = current_runtime_book_import_event_cursor(runtime_events.as_ref());

    block_on_ready(service.process_books_payload(
        BooksImportPayload {
            copy_mode: ImportCopyMode::Copy,
            books: vec![BooksImportEntry {
                source_file: PathBuf::from("/tmp/book.cbz"),
                series_id: "series-1".to_string(),
                destination_name: None,
                upgrade_book_id: None,
            }],
        },
        100,
    ))
    .expect("successful import should produce follow-up tasks");

    let import_events = pending_runtime_book_import_events(runtime_events.as_ref(), cursor).events;
    assert!(import_events.iter().any(|event| {
        event.book_id.as_deref() == Some("book-1")
            && event.source_file == "/tmp/book.cbz"
            && event.success
            && event.message.is_none()
    }));
    let events = runtime_events.pending_events(cursor, "admin", true).events;
    assert!(events.iter().any(|event| matches!(
        &event.event,
        RuntimeSseEvent::BookAdded {
            book_id,
            series_id,
            library_id,
        } if book_id == "book-1" && series_id == "series-1" && library_id == "library-1"
    )));
}

#[test]
fn process_books_payload_registers_failed_import_runtime_event() {
    let (service, runtime_events) = import_service(Err("source file does not exist".to_string()));
    let cursor = current_runtime_book_import_event_cursor(runtime_events.as_ref());

    let error = block_on_ready(service.process_books_payload(
        BooksImportPayload {
            copy_mode: ImportCopyMode::Copy,
            books: vec![BooksImportEntry {
                source_file: PathBuf::from("/tmp/missing.cbz"),
                series_id: "series-1".to_string(),
                destination_name: None,
                upgrade_book_id: None,
            }],
        },
        100,
    ))
    .expect_err("failed import should return the port error");

    assert_eq!(error.to_string(), "source file does not exist");
    let import_events = pending_runtime_book_import_events(runtime_events.as_ref(), cursor).events;
    assert!(import_events.iter().any(|event| {
        event.book_id.is_none()
            && event.source_file == "/tmp/missing.cbz"
            && !event.success
            && event.message.as_deref() == Some("source file does not exist")
    }));
}
