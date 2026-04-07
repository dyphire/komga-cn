use std::collections::BTreeSet;
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportBookOutcome {
    pub library_id: String,
    pub imported_book_id: String,
    pub sidecar_imported: bool,
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
    ) -> Result<Vec<TaskQueueRecord>, String> {
        let mut library_ids = BTreeSet::new();
        let mut deferred_tasks = Vec::new();

        for book in payload.books {
            let Some(outcome) = self.port.import_book(payload.copy_mode, book).await? else {
                continue;
            };

            if outcome.sidecar_imported {
                deferred_tasks.push(TaskQueueRecord::new(
                    format!("REFRESH_BOOK_METADATA:{}", outcome.imported_book_id),
                    80,
                    Some(outcome.imported_book_id.clone()),
                ));
            }

            library_ids.insert(outcome.library_id);
        }

        let mut follow_up_tasks = library_ids
            .into_iter()
            .map(|library_id| {
                TaskQueueRecord::new(format!("SCAN_LIBRARY:{library_id}"), 100, Some(library_id))
            })
            .collect::<Vec<_>>();
        follow_up_tasks.extend(deferred_tasks);

        Ok(follow_up_tasks)
    }

    pub async fn process_queued_books_payload(
        &self,
        task_payload: &str,
    ) -> Result<Vec<TaskQueueRecord>, String> {
        let payload = serde_json::from_str::<BooksImportPayload>(task_payload)
            .map_err(|error| format!("parse queued import payload: {error}"))?;
        self.process_books_payload(payload).await
    }

    pub async fn process_queued_book_payload(
        &self,
        task_payload: &str,
    ) -> Result<Vec<TaskQueueRecord>, String> {
        let payload = serde_json::from_str::<QueuedBookImportPayload>(task_payload)
            .map_err(|error| format!("parse queued import payload: {error}"))?;
        self.process_books_payload(BooksImportPayload {
            copy_mode: payload.copy_mode,
            books: vec![payload.book],
        })
        .await
    }
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
