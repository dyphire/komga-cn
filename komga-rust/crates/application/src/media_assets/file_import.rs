use std::collections::VecDeque;
use std::future::Future;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::task_processing::TaskQueueRecord;

static GENERATED_ID_COUNTER: AtomicU64 = AtomicU64::new(0);
const MAX_RUNTIME_BOOK_IMPORT_EVENTS: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeBookImportEvent {
    pub id: u64,
    pub book_id: Option<String>,
    pub source_file: String,
    pub success: bool,
    pub message: Option<String>,
}

#[derive(Default)]
struct RuntimeBookImportEventState {
    last_event_id: u64,
    events: VecDeque<RuntimeBookImportEvent>,
}

static RUNTIME_BOOK_IMPORT_EVENTS: OnceLock<Mutex<RuntimeBookImportEventState>> = OnceLock::new();

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

pub trait MediaImportPort {
    fn import_book(
        &self,
        copy_mode: ImportCopyMode,
        book: BooksImportEntry,
    ) -> impl Future<Output = Result<Option<ImportBookOutcome>, String>>;
}

pub struct MediaImportService<P> {
    port: P,
}

impl<P> MediaImportService<P>
where
    P: MediaImportPort,
{
    pub fn new(port: P) -> Self {
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
                let task_payload = serde_json::to_string(&QueuedBookImportPayload {
                    copy_mode: payload.copy_mode,
                    book,
                })
                .map_err(|error| format!("serialize books import payload: {error}"))?;

                Ok(TaskQueueRecord::new(next_task_id(), 100, Some(group_id))
                    .with_payload(task_payload))
            })
            .collect()
    }

    pub async fn process_books_payload(
        &self,
        payload: BooksImportPayload,
        import_priority: i32,
    ) -> Result<Vec<TaskQueueRecord>, String> {
        let mut follow_up_tasks = Vec::new();

        for book in payload.books {
            let series_id = book.series_id.clone();
            let Some(outcome) = self.port.import_book(payload.copy_mode, book).await? else {
                continue;
            };
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

    pub async fn process_queued_books_payload(
        &self,
        task_payload: &str,
        import_priority: i32,
    ) -> Result<Vec<TaskQueueRecord>, String> {
        let payload = serde_json::from_str::<BooksImportPayload>(task_payload)
            .map_err(|error| format!("parse queued import payload: {error}"))?;
        self.process_books_payload(payload, import_priority).await
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

fn import_follow_up_analyze_task(
    book_id: &str,
    import_priority: i32,
    series_id: &str,
) -> TaskQueueRecord {
    TaskQueueRecord::new(
        format!("ANALYZE_BOOK:{book_id}"),
        import_priority.saturating_add(1),
        Some(series_id.to_string()),
    )
}

fn import_follow_up_metadata_task(book_id: &str, series_id: &str) -> TaskQueueRecord {
    import_follow_up_refresh_task(
        format!("REFRESH_BOOK_METADATA_{book_id}"),
        Some(series_id.to_string()),
    )
    .with_simple_type("REFRESH_BOOK_METADATA")
}

fn import_follow_up_local_artwork_task(book_id: &str) -> TaskQueueRecord {
    import_follow_up_refresh_task(format!("REFRESH_BOOK_LOCAL_ARTWORK_{book_id}"), None)
        .with_simple_type("REFRESH_BOOK_LOCAL_ARTWORK")
}

fn import_follow_up_refresh_task(task_id: String, group_id: Option<String>) -> TaskQueueRecord {
    TaskQueueRecord::new(task_id, 4, group_id)
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

fn runtime_book_import_events() -> &'static Mutex<RuntimeBookImportEventState> {
    RUNTIME_BOOK_IMPORT_EVENTS.get_or_init(|| Mutex::new(RuntimeBookImportEventState::default()))
}

pub fn current_runtime_book_import_event_cursor() -> u64 {
    runtime_book_import_events()
        .lock()
        .expect("runtime book import event state lock should not be poisoned")
        .last_event_id
}

pub fn register_runtime_book_import_event(
    book_id: Option<String>,
    source_file: impl Into<String>,
    success: bool,
    message: Option<String>,
) {
    let mut state = runtime_book_import_events()
        .lock()
        .expect("runtime book import event state lock should not be poisoned");
    state.last_event_id += 1;
    let event_id = state.last_event_id;
    state.events.push_back(RuntimeBookImportEvent {
        id: event_id,
        book_id,
        source_file: source_file.into(),
        success,
        message,
    });

    while state.events.len() > MAX_RUNTIME_BOOK_IMPORT_EVENTS {
        state.events.pop_front();
    }
}

pub fn pending_runtime_book_import_events(
    last_seen_event_id: u64,
) -> (u64, Vec<RuntimeBookImportEvent>) {
    let state = runtime_book_import_events()
        .lock()
        .expect("runtime book import event state lock should not be poisoned");
    let current_cursor = state.last_event_id;
    let events = state
        .events
        .iter()
        .filter(|event| event.id > last_seen_event_id)
        .cloned()
        .collect::<Vec<_>>();

    (current_cursor, events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

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
        outcome: Option<ImportBookOutcome>,
    }

    impl MediaImportPort for StubImportPort {
        async fn import_book(
            &self,
            _copy_mode: ImportCopyMode,
            _book: BooksImportEntry,
        ) -> Result<Option<ImportBookOutcome>, String> {
            Ok(self.outcome.clone())
        }
    }

    #[test]
    fn process_books_payload_enqueues_local_artwork_refresh_for_imported_artwork_sidecars() {
        let service = MediaImportService::new(StubImportPort {
            outcome: Some(ImportBookOutcome {
                library_id: "library-1".to_string(),
                imported_book_id: "book-1".to_string(),
                sidecar_imported: false,
                artwork_sidecar_imported: true,
            }),
        });

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
            task.id == "ANALYZE_BOOK:book-1"
                && task.group == Some("series-1".to_string())
                && task.simple_type == "ANALYZE_BOOK"
                && task.priority == 101
        }));
        assert!(tasks.iter().any(|task| {
            task.id == "REFRESH_BOOK_LOCAL_ARTWORK_book-1"
                && task.simple_type == "REFRESH_BOOK_LOCAL_ARTWORK"
                && task.group.is_none()
                && task.priority == 4
        }));
    }

    #[test]
    fn process_queued_book_payload_accepts_kotlin_style_import_payload() {
        let service = MediaImportService::new(StubImportPort {
            outcome: Some(ImportBookOutcome {
                library_id: "library-1".to_string(),
                imported_book_id: "book-1".to_string(),
                sidecar_imported: false,
                artwork_sidecar_imported: false,
            }),
        });

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
                    "uniqueId": "IMPORT_BOOK:task-1"
                })
                .to_string(),
                100,
            ),
        )
        .expect("kotlin-style import payload should parse successfully");

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "ANALYZE_BOOK:book-1");
        assert_eq!(tasks[0].simple_type, "ANALYZE_BOOK");
        assert_eq!(tasks[0].priority, 101);
        assert_eq!(tasks[0].group.as_deref(), Some("series-1"));
    }
}
