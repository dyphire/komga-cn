use super::task_kind::{BookPayload, LibraryPayload, ScanLibraryPayload, TaskKind, TaskRequest};
use super::{LibraryScanInterval, TaskQueueRecord};

const MANUAL_SCAN_PRIORITY: i32 = 8;
const BACKGROUND_SCAN_PRIORITY: i32 = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskSchedule {
    Manual,
    Background,
    Startup,
    Interval(LibraryScanInterval),
}

impl TaskSchedule {
    pub fn scan_priority(self) -> i32 {
        match self {
            Self::Manual => MANUAL_SCAN_PRIORITY,
            Self::Background | Self::Startup | Self::Interval(_) => BACKGROUND_SCAN_PRIORITY,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookSeriesRef {
    pub book_id: String,
    pub series_id: String,
}

impl BookSeriesRef {
    pub fn new(book_id: impl Into<String>, series_id: impl Into<String>) -> Self {
        Self {
            book_id: book_id.into(),
            series_id: series_id.into(),
        }
    }
}

impl From<(String, String)> for BookSeriesRef {
    fn from((book_id, series_id): (String, String)) -> Self {
        Self { book_id, series_id }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryTaskCommand {
    ScanLibrary {
        library_id: String,
        deep_scan: bool,
        schedule: TaskSchedule,
    },
    AnalyzeBooks {
        books: Vec<BookSeriesRef>,
    },
    RefreshMetadata {
        series_ids: Vec<String>,
        books: Vec<BookSeriesRef>,
    },
    EmptyTrash {
        library_id: String,
    },
    HashBooks {
        book_ids: Vec<String>,
        priority: i32,
    },
    HashKoreaderBooks {
        book_ids: Vec<String>,
        priority: i32,
    },
    FindBooksWithMissingPageHash {
        library_id: String,
    },
    RepairExtensions {
        books: Vec<BookSeriesRef>,
        priority: i32,
    },
    FindBooksToConvert {
        library_id: String,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LibraryTaskBatch {
    pub records: Vec<TaskQueueRecord>,
}

impl LibraryTaskBatch {
    pub fn new(records: Vec<TaskQueueRecord>) -> Self {
        Self { records }
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn into_queue_records(self) -> Vec<TaskQueueRecord> {
        self.records
    }
}

pub fn emit_library_task_batch(command: LibraryTaskCommand) -> LibraryTaskBatch {
    match command {
        LibraryTaskCommand::ScanLibrary {
            library_id,
            deep_scan,
            schedule,
        } => {
            let priority = schedule.scan_priority();
            let deep_suffix = format!("{library_id}_DEEP_{deep_scan}");
            let record = TaskRequest::with_payload(
                TaskKind::ScanLibrary,
                ScanLibraryPayload::new(&library_id, deep_scan),
            )
            .priority(priority)
            .into_queue_record_with_id(&deep_suffix);
            LibraryTaskBatch::new(vec![record])
        }
        LibraryTaskCommand::AnalyzeBooks { books } => {
            let records = books
                .into_iter()
                .map(|book| {
                    TaskRequest::new(TaskKind::AnalyzeBook)
                        .group(book.series_id)
                        .into_queue_record_with_id(&book.book_id)
                })
                .collect();
            LibraryTaskBatch::new(records)
        }
        LibraryTaskCommand::RefreshMetadata { series_ids, books } => {
            let mut records = Vec::with_capacity((books.len() * 2) + series_ids.len());
            for book in books {
                records.push(
                    TaskRequest::new(TaskKind::RefreshBookMetadata)
                        .group(book.series_id)
                        .into_queue_record_with_id(&book.book_id),
                );
                records.push(
                    TaskRequest::new(TaskKind::RefreshBookLocalArtwork)
                        .into_queue_record_with_id(&book.book_id),
                );
            }
            for series_id in series_ids {
                records.push(
                    TaskRequest::new(TaskKind::RefreshSeriesLocalArtwork)
                        .into_queue_record_with_id(&series_id),
                );
            }
            LibraryTaskBatch::new(records)
        }
        LibraryTaskCommand::EmptyTrash { library_id } => {
            let record =
                TaskRequest::new(TaskKind::EmptyTrash).into_queue_record_with_id(&library_id);
            LibraryTaskBatch::new(vec![record])
        }
        LibraryTaskCommand::HashBooks { book_ids, priority } => {
            let records = book_ids
                .into_iter()
                .map(|book_id| {
                    TaskRequest::with_payload(TaskKind::HashBook, BookPayload::new(book_id))
                        .priority(priority)
                        .into_queue_record()
                })
                .collect();
            LibraryTaskBatch::new(records)
        }
        LibraryTaskCommand::HashKoreaderBooks { book_ids, priority } => {
            let records = book_ids
                .into_iter()
                .map(|book_id| {
                    TaskRequest::with_payload(TaskKind::HashBookKoreader, BookPayload::new(book_id))
                        .priority(priority)
                        .into_queue_record()
                })
                .collect();
            LibraryTaskBatch::new(records)
        }
        LibraryTaskCommand::FindBooksWithMissingPageHash { library_id } => {
            let record = TaskRequest::with_payload(
                TaskKind::FindBooksWithMissingPageHash,
                LibraryPayload::new(library_id),
            )
            .into_queue_record();
            LibraryTaskBatch::new(vec![record])
        }
        LibraryTaskCommand::RepairExtensions { books, priority } => {
            let records = books
                .into_iter()
                .map(|book| {
                    TaskRequest::with_payload(
                        TaskKind::RepairExtension,
                        BookPayload::new(book.book_id),
                    )
                    .priority(priority)
                    .group(book.series_id)
                    .into_queue_record()
                })
                .collect();
            LibraryTaskBatch::new(records)
        }
        LibraryTaskCommand::FindBooksToConvert { library_id } => {
            let record = TaskRequest::new(TaskKind::FindBooksToConvert)
                .priority(0)
                .into_queue_record_with_id(&library_id);
            LibraryTaskBatch::new(vec![record])
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedTaskRowShape {
    pub id: String,
    pub priority: i32,
    pub group: Option<String>,
    pub class_name: String,
    pub simple_type: String,
    pub payload: String,
    pub owner: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpaqueTask {
    pub runtime_simple_type: String,
    pub persisted_row: PersistedTaskRowShape,
}

impl OpaqueTask {
    pub fn into_queue_record(self) -> TaskQueueRecord {
        let PersistedTaskRowShape {
            id,
            priority,
            group,
            payload,
            owner,
            ..
        } = self.persisted_row;
        let mut record = TaskQueueRecord::new(id, priority, group)
            .with_simple_type(self.runtime_simple_type)
            .with_payload(payload);
        record.owner = owner;
        record
    }
}
