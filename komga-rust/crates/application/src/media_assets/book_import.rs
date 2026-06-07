use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::runtime_sse::{
    current_runtime_sse_event_cursor, pending_runtime_sse_events, register_runtime_sse_event,
};
use crate::task_processing::{SubmitUrgency, TaskKind, TaskQueue, TaskQueueRecord, TaskRequest};

static GENERATED_ID_COUNTER: AtomicU64 = AtomicU64::new(0);
const IMPORT_BOOK_PRIORITY: i32 = 100;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeBookImportEvent {
    pub id: u64,
    pub book_id: Option<String>,
    pub source_file: String,
    pub success: bool,
    pub message: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportCopyMode {
    Move,
    Copy,
    Hardlink,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BooksImportEntry {
    pub source_file: PathBuf,
    pub series_id: String,
    pub destination_name: Option<String>,
    pub upgrade_book_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BooksImportPayload {
    pub copy_mode: ImportCopyMode,
    pub books: Vec<BooksImportEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct QueuedBookImportPayload {
    pub copy_mode: ImportCopyMode,
    pub book: BooksImportEntry,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KotlinQueuedBookImportPayload {
    source_file: PathBuf,
    series_id: String,
    copy_mode: ImportCopyMode,
    destination_name: Option<String>,
    upgrade_book_id: Option<String>,
}

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

#[async_trait]
pub trait BookImportPort: Send + Sync {
    async fn import_book(
        &self,
        copy_mode: ImportCopyMode,
        book: BooksImportEntry,
    ) -> Result<Option<ImportBookOutcome>, String>;
}

pub struct BookImportService {
    port: Arc<dyn BookImportPort>,
}

impl BookImportService {
    pub fn new(port: Arc<dyn BookImportPort>) -> Self {
        Self { port }
    }

    pub fn enqueue_books(
        &self,
        payload: BooksImportPayload,
        mut next_task_id: impl FnMut() -> String,
    ) -> Result<Vec<TaskQueueRecord>, String> {
        payload
            .books
            .into_iter()
            .map(|book| {
                let group_id = book.series_id.clone();
                let task_id = next_task_id();
                let task_record = TaskRequest::new(TaskKind::ImportBook)
                    .priority(IMPORT_BOOK_PRIORITY)
                    .group(group_id)
                    .into_queue_record_with_id(&task_id);
                let task_payload = queued_book_import_task_payload(
                    payload.copy_mode,
                    &book,
                    task_record.priority,
                    task_record.group.as_deref(),
                    task_record.id.as_str(),
                );

                Ok(task_record.with_payload(task_payload))
            })
            .collect()
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
                        None,
                        source_file,
                        false,
                        Some(error.clone()),
                    );
                    return Err(error);
                }
            };
            register_runtime_book_import_event(
                Some(outcome.imported_book_id.clone()),
                source_file,
                true,
                None,
            );
            register_runtime_book_added_event(&outcome, series_id.as_str());
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
        task_payload: &str,
        import_priority: i32,
    ) -> Result<Vec<TaskQueueRecord>, String> {
        let payload = parse_queued_book_import_payload(task_payload)?;
        self.process_books_payload(
            BooksImportPayload {
                copy_mode: payload.copy_mode,
                books: vec![payload.book],
            },
            import_priority,
        )
        .await
    }
}

fn queued_book_import_task_payload(
    copy_mode: ImportCopyMode,
    book: &BooksImportEntry,
    priority: i32,
    group_id: Option<&str>,
    unique_id: &str,
) -> String {
    json!({
        "sourceFile": book.source_file.to_string_lossy().to_string(),
        "seriesId": book.series_id.as_str(),
        "copyMode": import_copy_mode_str(copy_mode),
        "destinationName": book.destination_name.as_deref(),
        "upgradeBookId": book.upgrade_book_id.as_deref(),
        "priority": priority,
        "groupId": group_id,
        "uniqueId": unique_id,
    })
    .to_string()
}

fn import_copy_mode_str(copy_mode: ImportCopyMode) -> &'static str {
    match copy_mode {
        ImportCopyMode::Move => "MOVE",
        ImportCopyMode::Copy => "COPY",
        ImportCopyMode::Hardlink => "HARDLINK",
    }
}

fn register_runtime_book_added_event(outcome: &ImportBookOutcome, series_id: &str) {
    register_runtime_sse_event(
        "BookAdded",
        json!({
            "bookId": outcome.imported_book_id,
            "seriesId": series_id,
            "libraryId": outcome.library_id,
        }),
        false,
        None,
    );
}

pub fn parse_books_import_payload(body: &Value) -> Result<BooksImportPayload, String> {
    let body = body
        .as_object()
        .ok_or_else(|| "books import payload must be a JSON object".to_string())?;

    let copy_mode = match body.get("copyMode").and_then(Value::as_str) {
        Some("MOVE") => ImportCopyMode::Move,
        Some("COPY") => ImportCopyMode::Copy,
        Some("HARDLINK") => ImportCopyMode::Hardlink,
        Some(_) => {
            return Err("copyMode must be one of MOVE, COPY, HARDLINK".to_string());
        }
        None => {
            return Err("copyMode is required".to_string());
        }
    };

    let books = match body.get("books") {
        Some(books) => books
            .as_array()
            .ok_or_else(|| "books must be an array".to_string())?
            .iter()
            .map(|entry| {
                let entry = entry
                    .as_object()
                    .ok_or_else(|| "books entries must be objects".to_string())?;

                let source_file = entry
                    .get("sourceFile")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "books[].sourceFile must be a string".to_string())?;
                let series_id = entry
                    .get("seriesId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| "books[].seriesId must be a string".to_string())?;
                if source_file.trim().is_empty() || series_id.trim().is_empty() {
                    return Err(
                        "books[].sourceFile and books[].seriesId must not be blank".to_string()
                    );
                }

                let destination_name = entry
                    .get("destinationName")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);

                let upgrade_book_id = entry
                    .get("upgradeBookId")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);

                Ok(BooksImportEntry {
                    source_file: PathBuf::from(source_file),
                    series_id: series_id.to_string(),
                    destination_name,
                    upgrade_book_id,
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };

    Ok(BooksImportPayload { copy_mode, books })
}

fn kotlin_import_book_task_id_suffix(book: &BooksImportEntry) -> String {
    format!("{}_{}", book.series_id, book.source_file.display())
}

fn import_follow_up_analyze_task(
    book_id: &str,
    import_priority: i32,
    series_id: &str,
) -> TaskQueueRecord {
    TaskRequest::with_payload(
        TaskKind::AnalyzeBook,
        crate::task_processing::BookPayload::new(book_id),
    )
    .priority(import_priority.saturating_add(1))
    .group(series_id)
    .into_queue_record()
}

fn import_follow_up_metadata_task(book_id: &str, series_id: &str) -> TaskQueueRecord {
    TaskRequest::with_payload(
        TaskKind::RefreshBookMetadata,
        crate::task_processing::BookPayload::new(book_id),
    )
    .priority(4)
    .group(series_id)
    .into_queue_record()
}

fn import_follow_up_local_artwork_task(book_id: &str) -> TaskQueueRecord {
    TaskRequest::with_payload(
        TaskKind::RefreshBookLocalArtwork,
        crate::task_processing::BookPayload::new(book_id),
    )
    .priority(4)
    .into_queue_record()
}

fn parse_queued_book_import_payload(task_payload: &str) -> Result<QueuedBookImportPayload, String> {
    serde_json::from_str::<QueuedBookImportPayload>(task_payload)
        .or_else(|_| {
            serde_json::from_str::<KotlinQueuedBookImportPayload>(task_payload).map(|payload| {
                QueuedBookImportPayload {
                    copy_mode: payload.copy_mode,
                    book: BooksImportEntry {
                        source_file: payload.source_file,
                        series_id: payload.series_id,
                        destination_name: payload.destination_name,
                        upgrade_book_id: payload.upgrade_book_id,
                    },
                }
            })
        })
        .map_err(|error| format!("parse queued import payload: {error}"))
}

pub fn generate_prefixed_id(prefix: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = GENERATED_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{timestamp:032x}{counter:016x}")
}

pub fn current_runtime_book_import_event_cursor() -> u64 {
    current_runtime_sse_event_cursor()
}

pub fn register_runtime_book_import_event(
    book_id: Option<String>,
    source_file: impl Into<String>,
    success: bool,
    message: Option<String>,
) {
    register_runtime_sse_event(
        "BookImported",
        json!({
            "bookId": book_id,
            "sourceFile": source_file.into(),
            "success": success,
            "message": message,
        }),
        true,
        None,
    );
}

pub fn pending_runtime_book_import_events(
    last_seen_event_id: u64,
) -> (u64, Vec<RuntimeBookImportEvent>) {
    let (current_cursor, events) = pending_runtime_sse_events(last_seen_event_id, "", true);
    let mapped = events
        .iter()
        .filter(|event| event.name == "BookImported")
        .filter_map(|event| {
            Some(RuntimeBookImportEvent {
                id: event.id,
                book_id: event.payload.get("bookId").and_then(|value| {
                    (!value.is_null())
                        .then(|| value.as_str().map(str::to_string))
                        .flatten()
                }),
                source_file: event
                    .payload
                    .get("sourceFile")
                    .and_then(|value| value.as_str())?
                    .to_string(),
                success: event
                    .payload
                    .get("success")
                    .and_then(|value| value.as_bool())?,
                message: event
                    .payload
                    .get("message")
                    .and_then(|value| (!value.is_null()).then(|| value.as_str()))
                    .flatten()
                    .map(str::to_string),
            })
        })
        .collect::<Vec<_>>();

    (current_cursor, mapped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    fn import_runtime_sse_lock() -> &'static std::sync::Mutex<()> {
        static LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    fn import_runtime_sse_guard() -> std::sync::MutexGuard<'static, ()> {
        import_runtime_sse_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

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

    #[derive(Clone)]
    struct StubImportPort {
        result: Result<Option<ImportBookOutcome>, String>,
    }

    #[async_trait]
    impl BookImportPort for StubImportPort {
        async fn import_book(
            &self,
            _copy_mode: ImportCopyMode,
            _book: BooksImportEntry,
        ) -> Result<Option<ImportBookOutcome>, String> {
            self.result.clone()
        }
    }

    struct RecordingTaskQueue {
        persisted_ids: std::sync::Mutex<Vec<String>>,
    }

    #[async_trait]
    impl crate::task_processing::TaskQueue for RecordingTaskQueue {
        async fn enqueue(&self, _kind: TaskKind, _target_id: &str) {}

        async fn enqueue_request(&self, _request: TaskRequest) {}

        async fn enqueue_batch(&self, _batch: crate::task_processing::LibraryTaskBatch) {}

        async fn enqueue_records(
            &self,
            records: Vec<TaskQueueRecord>,
            urgency: crate::task_processing::SubmitUrgency,
        ) -> Result<(), String> {
            assert_eq!(urgency, crate::task_processing::SubmitUrgency::Immediate);
            let task_record = records.into_iter().next().expect("task should exist");
            if task_record.id == "ImportBook_series-1_/tmp/book-a.cbz" {
                Err("first enqueue failed".to_string())
            } else {
                self.persisted_ids.lock().unwrap().push(task_record.id);
                Ok(())
            }
        }

        async fn status(&self) -> crate::task_processing::QueueStatus {
            crate::task_processing::QueueStatus::default()
        }
    }

    #[tokio::test]
    async fn submit_books_import_keeps_later_books_when_earlier_enqueue_fails() {
        let service = BookImportService::new(Arc::new(StubImportPort { result: Ok(None) }));
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
    fn parse_books_import_payload_accepts_http_camel_case_fields() {
        let payload = parse_books_import_payload(&serde_json::json!({
            "copyMode": "COPY",
            "books": [{
                "sourceFile": "/tmp/book-a.cbz",
                "seriesId": "series-1",
                "destinationName": "Book A",
                "upgradeBookId": "book-1"
            }]
        }))
        .expect("http import payload should parse");

        assert_eq!(
            payload,
            BooksImportPayload {
                copy_mode: ImportCopyMode::Copy,
                books: vec![BooksImportEntry {
                    source_file: PathBuf::from("/tmp/book-a.cbz"),
                    series_id: "series-1".to_string(),
                    destination_name: Some("Book A".to_string()),
                    upgrade_book_id: Some("book-1".to_string()),
                }]
            }
        );
    }

    #[test]
    fn enqueue_books_builds_flat_kotlin_task_payload_before_persistence() {
        let service = BookImportService::new(Arc::new(StubImportPort { result: Ok(None) }));
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
        let payload = serde_json::from_str::<Value>(
            record
                .payload
                .as_deref()
                .expect("task payload should exist"),
        )
        .expect("task payload should be valid JSON");

        assert_eq!(
            payload.get("sourceFile").and_then(Value::as_str),
            Some("/tmp/book-a.cbz")
        );
        assert_eq!(
            payload.get("seriesId").and_then(Value::as_str),
            Some("series-1")
        );
        assert_eq!(
            payload.get("copyMode").and_then(Value::as_str),
            Some("HARDLINK")
        );
        assert_eq!(
            payload.get("destinationName").and_then(Value::as_str),
            Some("dest-a"),
        );
        assert_eq!(
            payload.get("upgradeBookId").and_then(Value::as_str),
            Some("book-1"),
        );
        assert_eq!(payload.get("priority").and_then(Value::as_i64), Some(100));
        assert_eq!(
            payload.get("groupId").and_then(Value::as_str),
            Some("series-1")
        );
        assert_eq!(
            payload.get("uniqueId").and_then(Value::as_str),
            Some("ImportBook_series-1_/tmp/book-a.cbz"),
        );
        assert!(payload.get("book").is_none());
        assert!(payload.get("books").is_none());
    }

    #[test]
    fn process_books_payload_enqueues_local_artwork_refresh_for_imported_artwork_sidecars() {
        let service = BookImportService::new(Arc::new(StubImportPort {
            result: Ok(Some(ImportBookOutcome {
                library_id: "library-1".to_string(),
                imported_book_id: "book-1".to_string(),
                sidecar_imported: false,
                artwork_sidecar_imported: true,
            })),
        }));

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
    fn process_queued_book_payload_accepts_kotlin_style_import_payload() {
        let service = BookImportService::new(Arc::new(StubImportPort {
            result: Ok(Some(ImportBookOutcome {
                library_id: "library-1".to_string(),
                imported_book_id: "book-1".to_string(),
                sidecar_imported: false,
                artwork_sidecar_imported: false,
            })),
        }));

        let tasks = block_on_ready(
            service.process_queued_book_payload(
                &serde_json::json!({
                    "sourceFile": "/tmp/book.cbz",
                    "seriesId": "series-1",
                    "copyMode": "COPY",
                    "destinationName": "dest-a",
                    "upgradeBookId": "book-1",
                    "priority": 100,
                    "groupId": "series-1",
                    "uniqueId": "ImportBook:task-1"
                })
                .to_string(),
                100,
            ),
        )
        .expect("kotlin-style import payload should parse successfully");

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "AnalyzeBook_book-1");
        assert_eq!(tasks[0].simple_type, "AnalyzeBook");
        assert_eq!(tasks[0].priority, 101);
        assert_eq!(tasks[0].group.as_deref(), Some("series-1"));
    }

    #[test]
    fn process_books_payload_registers_import_runtime_events() {
        let _guard = import_runtime_sse_guard();
        let service = BookImportService::new(Arc::new(StubImportPort {
            result: Ok(Some(ImportBookOutcome {
                library_id: "library-1".to_string(),
                imported_book_id: "book-1".to_string(),
                sidecar_imported: false,
                artwork_sidecar_imported: false,
            })),
        }));
        let cursor = current_runtime_book_import_event_cursor();

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

        let (_, import_events) = pending_runtime_book_import_events(cursor);
        assert!(import_events.iter().any(|event| {
            event.book_id.as_deref() == Some("book-1")
                && event.source_file == "/tmp/book.cbz"
                && event.success
                && event.message.is_none()
        }));
        let (_, runtime_events) =
            crate::runtime_sse::pending_runtime_sse_events(cursor, "admin", true);
        assert!(runtime_events.iter().any(|event| {
            event.name == "BookAdded"
                && event.payload.get("bookId").and_then(Value::as_str) == Some("book-1")
                && event.payload.get("seriesId").and_then(Value::as_str) == Some("series-1")
                && event.payload.get("libraryId").and_then(Value::as_str) == Some("library-1")
        }));
    }

    #[test]
    fn process_books_payload_registers_failed_import_runtime_event() {
        let _guard = import_runtime_sse_guard();
        let service = BookImportService::new(Arc::new(StubImportPort {
            result: Err("source file does not exist".to_string()),
        }));
        let cursor = current_runtime_book_import_event_cursor();

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

        assert_eq!(error, "source file does not exist");
        let (_, import_events) = pending_runtime_book_import_events(cursor);
        assert!(import_events.iter().any(|event| {
            event.book_id.is_none()
                && event.source_file == "/tmp/missing.cbz"
                && !event.success
                && event.message.as_deref() == Some("source file does not exist")
        }));
    }
}
