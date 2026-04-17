use super::{LibraryScanInterval, TaskQueueRecord};
use serde_json::json;

const MANUAL_SCAN_PRIORITY: i32 = 8;
const BACKGROUND_SCAN_PRIORITY: i32 = 4;
const ANALYZE_LIBRARY_PRIORITY: i32 = 6;
const METADATA_REFRESH_PRIORITY: i32 = 6;
const EMPTY_TRASH_PRIORITY: i32 = 6;
const LOWEST_PRIORITY: i32 = 0;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlannedTaskKind {
    ScanLibrary,
    AnalyzeBook,
    EmptyTrash,
    ImportBook,
    FindBooksWithMissingPageHash,
    FindDuplicatePagesToDelete,
    FindBookThumbnailsToRegenerate,
    RefreshBookMetadata,
    RefreshBookLocalArtwork,
    RefreshSeriesLocalArtwork,
    RefreshSeriesMetadata,
    AggregateSeriesMetadata,
    RepairExtension,
    GenerateBookThumbnail,
    HashBook,
    HashBookKoreader,
    HashBookPages,
    RebuildIndex,
    UpgradeIndex,
    RemoveHashedPages,
    DeleteBook,
    DeleteSeries,
    FindBooksToConvert,
    ConvertBook,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskDescriptor {
    pub runtime_simple_type: &'static str,
    pub persisted_simple_type: &'static str,
    pub persisted_class_name: &'static str,
}

impl PlannedTaskKind {
    pub const fn descriptor(self) -> TaskDescriptor {
        match self {
            Self::ScanLibrary => TaskDescriptor {
                runtime_simple_type: "SCAN_LIBRARY",
                persisted_simple_type: "ScanLibrary",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$ScanLibrary",
            },
            Self::AnalyzeBook => TaskDescriptor {
                runtime_simple_type: "ANALYZE_BOOK",
                persisted_simple_type: "AnalyzeBook",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$AnalyzeBook",
            },
            Self::EmptyTrash => TaskDescriptor {
                runtime_simple_type: "EMPTY_TRASH",
                persisted_simple_type: "EmptyTrash",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$EmptyTrash",
            },
            Self::ImportBook => TaskDescriptor {
                runtime_simple_type: "IMPORT_BOOK",
                persisted_simple_type: "ImportBook",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$ImportBook",
            },
            Self::FindBooksWithMissingPageHash => TaskDescriptor {
                runtime_simple_type: "FIND_BOOKS_WITH_MISSING_PAGE_HASH",
                persisted_simple_type: "FindBooksWithMissingPageHash",
                persisted_class_name:
                    "org.gotson.komga.application.tasks.Task$FindBooksWithMissingPageHash",
            },
            Self::FindDuplicatePagesToDelete => TaskDescriptor {
                runtime_simple_type: "FIND_DUPLICATE_PAGES_TO_DELETE",
                persisted_simple_type: "FindDuplicatePagesToDelete",
                persisted_class_name:
                    "org.gotson.komga.application.tasks.Task$FindDuplicatePagesToDelete",
            },
            Self::FindBookThumbnailsToRegenerate => TaskDescriptor {
                runtime_simple_type: "FIND_BOOK_THUMBNAILS_TO_REGENERATE",
                persisted_simple_type: "FindBookThumbnailsToRegenerate",
                persisted_class_name:
                    "org.gotson.komga.application.tasks.Task$FindBookThumbnailsToRegenerate",
            },
            Self::RefreshBookMetadata => TaskDescriptor {
                runtime_simple_type: "REFRESH_BOOK_METADATA",
                persisted_simple_type: "RefreshBookMetadata",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$RefreshBookMetadata",
            },
            Self::RefreshBookLocalArtwork => TaskDescriptor {
                runtime_simple_type: "REFRESH_BOOK_LOCAL_ARTWORK",
                persisted_simple_type: "RefreshBookLocalArtwork",
                persisted_class_name:
                    "org.gotson.komga.application.tasks.Task$RefreshBookLocalArtwork",
            },
            Self::RefreshSeriesLocalArtwork => TaskDescriptor {
                runtime_simple_type: "REFRESH_SERIES_LOCAL_ARTWORK",
                persisted_simple_type: "RefreshSeriesLocalArtwork",
                persisted_class_name:
                    "org.gotson.komga.application.tasks.Task$RefreshSeriesLocalArtwork",
            },
            Self::RefreshSeriesMetadata => TaskDescriptor {
                runtime_simple_type: "REFRESH_SERIES_METADATA",
                persisted_simple_type: "RefreshSeriesMetadata",
                persisted_class_name:
                    "org.gotson.komga.application.tasks.Task$RefreshSeriesMetadata",
            },
            Self::AggregateSeriesMetadata => TaskDescriptor {
                runtime_simple_type: "AGGREGATE_SERIES_METADATA",
                persisted_simple_type: "AggregateSeriesMetadata",
                persisted_class_name:
                    "org.gotson.komga.application.tasks.Task$AggregateSeriesMetadata",
            },
            Self::RepairExtension => TaskDescriptor {
                runtime_simple_type: "REPAIR_EXTENSION",
                persisted_simple_type: "RepairExtension",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$RepairExtension",
            },
            Self::GenerateBookThumbnail => TaskDescriptor {
                runtime_simple_type: "GENERATE_BOOK_THUMBNAIL",
                persisted_simple_type: "GenerateBookThumbnail",
                persisted_class_name:
                    "org.gotson.komga.application.tasks.Task$GenerateBookThumbnail",
            },
            Self::HashBook => TaskDescriptor {
                runtime_simple_type: "HASH_BOOK",
                persisted_simple_type: "HashBook",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$HashBook",
            },
            Self::HashBookKoreader => TaskDescriptor {
                runtime_simple_type: "HASH_BOOK_KOREADER",
                persisted_simple_type: "HashBookKoreader",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$HashBookKoreader",
            },
            Self::HashBookPages => TaskDescriptor {
                runtime_simple_type: "HASH_BOOK_PAGES",
                persisted_simple_type: "HashBookPages",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$HashBookPages",
            },
            Self::RebuildIndex => TaskDescriptor {
                runtime_simple_type: "REBUILD_INDEX",
                persisted_simple_type: "RebuildIndex",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$RebuildIndex",
            },
            Self::UpgradeIndex => TaskDescriptor {
                runtime_simple_type: "UPGRADE_INDEX",
                persisted_simple_type: "UpgradeIndex",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$UpgradeIndex",
            },
            Self::RemoveHashedPages => TaskDescriptor {
                runtime_simple_type: "REMOVE_HASHED_PAGES",
                persisted_simple_type: "RemoveHashedPages",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$RemoveHashedPages",
            },
            Self::DeleteBook => TaskDescriptor {
                runtime_simple_type: "DELETE_BOOK",
                persisted_simple_type: "DeleteBook",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$DeleteBook",
            },
            Self::DeleteSeries => TaskDescriptor {
                runtime_simple_type: "DELETE_SERIES",
                persisted_simple_type: "DeleteSeries",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$DeleteSeries",
            },
            Self::FindBooksToConvert => TaskDescriptor {
                runtime_simple_type: "FIND_BOOKS_TO_CONVERT",
                persisted_simple_type: "FindBooksToConvert",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$FindBooksToConvert",
            },
            Self::ConvertBook => TaskDescriptor {
                runtime_simple_type: "CONVERT_BOOK",
                persisted_simple_type: "ConvertBook",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$ConvertBook",
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlannedTask {
    pub kind: PlannedTaskKind,
    pub schedule: TaskSchedule,
    pub descriptor: TaskDescriptor,
    pub id: String,
    pub priority: i32,
    pub group: Option<String>,
    pub payload: Option<String>,
}

impl PlannedTask {
    pub fn into_queue_record(self) -> TaskQueueRecord {
        let mut record = TaskQueueRecord::new(self.id, self.priority, self.group)
            .with_simple_type(self.descriptor.runtime_simple_type);
        if let Some(payload) = self.payload {
            record = record.with_payload(payload);
        }
        record
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

pub trait TaskProtocolCatalog {
    fn descriptor(&self, kind: PlannedTaskKind) -> TaskDescriptor;

    fn known_kind_from_runtime_simple_type(&self, simple_type: &str) -> Option<PlannedTaskKind>;

    fn known_kind_from_persisted_simple_type(&self, simple_type: &str) -> Option<PlannedTaskKind>;

    fn plan_task(
        &self,
        kind: PlannedTaskKind,
        schedule: TaskSchedule,
        id: String,
        priority: i32,
        group: Option<String>,
        payload: Option<String>,
    ) -> PlannedTask {
        PlannedTask {
            kind,
            schedule,
            descriptor: self.descriptor(kind),
            id,
            priority,
            group,
            payload,
        }
    }

    fn opaque_task(
        &self,
        runtime_simple_type: String,
        persisted_row: PersistedTaskRowShape,
    ) -> OpaqueTask {
        OpaqueTask {
            runtime_simple_type,
            persisted_row,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultTaskProtocolCatalog;

impl TaskProtocolCatalog for DefaultTaskProtocolCatalog {
    fn descriptor(&self, kind: PlannedTaskKind) -> TaskDescriptor {
        kind.descriptor()
    }

    fn known_kind_from_runtime_simple_type(&self, simple_type: &str) -> Option<PlannedTaskKind> {
        match simple_type {
            "SCAN_LIBRARY" => Some(PlannedTaskKind::ScanLibrary),
            "ANALYZE_BOOK" => Some(PlannedTaskKind::AnalyzeBook),
            "EMPTY_TRASH" => Some(PlannedTaskKind::EmptyTrash),
            "IMPORT_BOOK" => Some(PlannedTaskKind::ImportBook),
            "FIND_BOOKS_WITH_MISSING_PAGE_HASH" => {
                Some(PlannedTaskKind::FindBooksWithMissingPageHash)
            }
            "FIND_DUPLICATE_PAGES_TO_DELETE" => Some(PlannedTaskKind::FindDuplicatePagesToDelete),
            "FIND_BOOK_THUMBNAILS_TO_REGENERATE" => {
                Some(PlannedTaskKind::FindBookThumbnailsToRegenerate)
            }
            "REFRESH_BOOK_METADATA" => Some(PlannedTaskKind::RefreshBookMetadata),
            "REFRESH_BOOK_LOCAL_ARTWORK" => Some(PlannedTaskKind::RefreshBookLocalArtwork),
            "REFRESH_SERIES_LOCAL_ARTWORK" => Some(PlannedTaskKind::RefreshSeriesLocalArtwork),
            "RefreshSeriesMetadata" | "REFRESH_SERIES_METADATA" => {
                Some(PlannedTaskKind::RefreshSeriesMetadata)
            }
            "AggregateSeriesMetadata" | "AGGREGATE_SERIES_METADATA" => {
                Some(PlannedTaskKind::AggregateSeriesMetadata)
            }
            "REPAIR_EXTENSION" => Some(PlannedTaskKind::RepairExtension),
            "GENERATE_BOOK_THUMBNAIL" => Some(PlannedTaskKind::GenerateBookThumbnail),
            "HASH_BOOK" => Some(PlannedTaskKind::HashBook),
            "HASH_BOOK_KOREADER" => Some(PlannedTaskKind::HashBookKoreader),
            "HASH_BOOK_PAGES" => Some(PlannedTaskKind::HashBookPages),
            "REBUILD_INDEX" => Some(PlannedTaskKind::RebuildIndex),
            "UPGRADE_INDEX" => Some(PlannedTaskKind::UpgradeIndex),
            "REMOVE_HASHED_PAGES" => Some(PlannedTaskKind::RemoveHashedPages),
            "DELETE_BOOK" => Some(PlannedTaskKind::DeleteBook),
            "DELETE_SERIES" => Some(PlannedTaskKind::DeleteSeries),
            "FindBooksToConvert" | "FIND_BOOKS_TO_CONVERT" => {
                Some(PlannedTaskKind::FindBooksToConvert)
            }
            "ConvertBook" | "CONVERT_BOOK" => Some(PlannedTaskKind::ConvertBook),
            _ => None,
        }
    }

    fn known_kind_from_persisted_simple_type(&self, simple_type: &str) -> Option<PlannedTaskKind> {
        match simple_type {
            "ScanLibrary" => Some(PlannedTaskKind::ScanLibrary),
            "AnalyzeBook" => Some(PlannedTaskKind::AnalyzeBook),
            "EmptyTrash" => Some(PlannedTaskKind::EmptyTrash),
            "ImportBook" => Some(PlannedTaskKind::ImportBook),
            "FindBooksWithMissingPageHash" => Some(PlannedTaskKind::FindBooksWithMissingPageHash),
            "FindDuplicatePagesToDelete" => Some(PlannedTaskKind::FindDuplicatePagesToDelete),
            "FindBookThumbnailsToRegenerate" => {
                Some(PlannedTaskKind::FindBookThumbnailsToRegenerate)
            }
            "RefreshBookMetadata" => Some(PlannedTaskKind::RefreshBookMetadata),
            "RefreshBookLocalArtwork" => Some(PlannedTaskKind::RefreshBookLocalArtwork),
            "RefreshSeriesLocalArtwork" => Some(PlannedTaskKind::RefreshSeriesLocalArtwork),
            "RefreshSeriesMetadata" | "REFRESH_SERIES_METADATA" => {
                Some(PlannedTaskKind::RefreshSeriesMetadata)
            }
            "AggregateSeriesMetadata" | "AGGREGATE_SERIES_METADATA" => {
                Some(PlannedTaskKind::AggregateSeriesMetadata)
            }
            "RepairExtension" => Some(PlannedTaskKind::RepairExtension),
            "GenerateBookThumbnail" => Some(PlannedTaskKind::GenerateBookThumbnail),
            "HashBook" => Some(PlannedTaskKind::HashBook),
            "HashBookKoreader" => Some(PlannedTaskKind::HashBookKoreader),
            "HashBookPages" => Some(PlannedTaskKind::HashBookPages),
            "RebuildIndex" => Some(PlannedTaskKind::RebuildIndex),
            "UpgradeIndex" => Some(PlannedTaskKind::UpgradeIndex),
            "RemoveHashedPages" => Some(PlannedTaskKind::RemoveHashedPages),
            "DeleteBook" => Some(PlannedTaskKind::DeleteBook),
            "DeleteSeries" => Some(PlannedTaskKind::DeleteSeries),
            "FindBooksToConvert" | "FIND_BOOKS_TO_CONVERT" => {
                Some(PlannedTaskKind::FindBooksToConvert)
            }
            "ConvertBook" | "CONVERT_BOOK" => Some(PlannedTaskKind::ConvertBook),
            _ => None,
        }
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
    pub tasks: Vec<PlannedTask>,
}

impl LibraryTaskBatch {
    pub fn new(tasks: Vec<PlannedTask>) -> Self {
        Self { tasks }
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn into_queue_records(self) -> Vec<TaskQueueRecord> {
        self.tasks
            .into_iter()
            .map(PlannedTask::into_queue_record)
            .collect()
    }
}

pub trait LibraryTaskEmitter {
    fn emit(&self, command: LibraryTaskCommand) -> LibraryTaskBatch;
}

#[derive(Clone, Debug)]
pub struct DefaultLibraryTaskEmitter<C> {
    catalog: C,
}

impl<C> DefaultLibraryTaskEmitter<C> {
    pub fn new(catalog: C) -> Self {
        Self { catalog }
    }
}

impl Default for DefaultLibraryTaskEmitter<DefaultTaskProtocolCatalog> {
    fn default() -> Self {
        Self::new(DefaultTaskProtocolCatalog)
    }
}

impl<C> LibraryTaskEmitter for DefaultLibraryTaskEmitter<C>
where
    C: TaskProtocolCatalog,
{
    fn emit(&self, command: LibraryTaskCommand) -> LibraryTaskBatch {
        match command {
            LibraryTaskCommand::ScanLibrary {
                library_id,
                deep_scan,
                schedule,
            } => {
                let priority = schedule.scan_priority();
                let task_id = format!("SCAN_LIBRARY_{library_id}_DEEP_{deep_scan}");
                LibraryTaskBatch::new(vec![self.catalog.plan_task(
                    PlannedTaskKind::ScanLibrary,
                    schedule,
                    task_id.clone(),
                    priority,
                    None,
                    Some(scan_library_payload(
                        &library_id,
                        deep_scan,
                        priority,
                        &task_id,
                    )),
                )])
            }
            LibraryTaskCommand::AnalyzeBooks { books } => LibraryTaskBatch::new(
                books
                    .into_iter()
                    .map(|book| {
                        self.catalog.plan_task(
                            PlannedTaskKind::AnalyzeBook,
                            TaskSchedule::Manual,
                            format!("ANALYZE_BOOK_{}", book.book_id),
                            ANALYZE_LIBRARY_PRIORITY,
                            Some(book.series_id),
                            None,
                        )
                    })
                    .collect(),
            ),
            LibraryTaskCommand::RefreshMetadata { series_ids, books } => {
                let mut tasks = Vec::with_capacity((books.len() * 2) + series_ids.len());
                for book in books {
                    let metadata_id = format!("REFRESH_BOOK_METADATA_{}", book.book_id);
                    tasks.push(self.catalog.plan_task(
                        PlannedTaskKind::RefreshBookMetadata,
                        TaskSchedule::Manual,
                        metadata_id,
                        METADATA_REFRESH_PRIORITY,
                        Some(book.series_id),
                        None,
                    ));
                    let artwork_id = format!("REFRESH_BOOK_LOCAL_ARTWORK_{}", book.book_id);
                    tasks.push(self.catalog.plan_task(
                        PlannedTaskKind::RefreshBookLocalArtwork,
                        TaskSchedule::Manual,
                        artwork_id,
                        METADATA_REFRESH_PRIORITY,
                        None,
                        None,
                    ));
                }
                for series_id in series_ids {
                    tasks.push(self.catalog.plan_task(
                        PlannedTaskKind::RefreshSeriesLocalArtwork,
                        TaskSchedule::Manual,
                        format!("REFRESH_SERIES_LOCAL_ARTWORK_{series_id}"),
                        METADATA_REFRESH_PRIORITY,
                        None,
                        None,
                    ));
                }
                LibraryTaskBatch::new(tasks)
            }
            LibraryTaskCommand::EmptyTrash { library_id } => {
                LibraryTaskBatch::new(vec![self.catalog.plan_task(
                    PlannedTaskKind::EmptyTrash,
                    TaskSchedule::Manual,
                    format!("EMPTY_TRASH_{library_id}"),
                    EMPTY_TRASH_PRIORITY,
                    None,
                    None,
                )])
            }
            LibraryTaskCommand::HashBooks { book_ids, priority } => LibraryTaskBatch::new(
                book_ids
                    .into_iter()
                    .map(|book_id| {
                        let task_id = format!("HASH_BOOK_{book_id}");
                        self.catalog.plan_task(
                            PlannedTaskKind::HashBook,
                            TaskSchedule::Background,
                            task_id.clone(),
                            priority,
                            None,
                            Some(book_task_payload(
                                "bookId", &book_id, priority, None, &task_id,
                            )),
                        )
                    })
                    .collect(),
            ),
            LibraryTaskCommand::HashKoreaderBooks { book_ids, priority } => LibraryTaskBatch::new(
                book_ids
                    .into_iter()
                    .map(|book_id| {
                        let task_id = format!("HASH_BOOK_KOREADER_{book_id}");
                        self.catalog.plan_task(
                            PlannedTaskKind::HashBookKoreader,
                            TaskSchedule::Background,
                            task_id.clone(),
                            priority,
                            None,
                            Some(book_task_payload(
                                "bookId", &book_id, priority, None, &task_id,
                            )),
                        )
                    })
                    .collect(),
            ),
            LibraryTaskCommand::FindBooksWithMissingPageHash { library_id } => {
                let task_id = format!("FIND_BOOKS_WITH_MISSING_PAGE_HASH_{library_id}");
                LibraryTaskBatch::new(vec![self.catalog.plan_task(
                    PlannedTaskKind::FindBooksWithMissingPageHash,
                    TaskSchedule::Background,
                    task_id.clone(),
                    LOWEST_PRIORITY,
                    None,
                    Some(book_task_payload(
                        "libraryId",
                        &library_id,
                        LOWEST_PRIORITY,
                        None,
                        &task_id,
                    )),
                )])
            }
            LibraryTaskCommand::RepairExtensions { books, priority } => LibraryTaskBatch::new(
                books
                    .into_iter()
                    .map(|book| {
                        let task_id = format!("REPAIR_EXTENSION_{}", book.book_id);
                        self.catalog.plan_task(
                            PlannedTaskKind::RepairExtension,
                            TaskSchedule::Background,
                            task_id.clone(),
                            priority,
                            Some(book.series_id.clone()),
                            Some(book_task_payload(
                                "bookId",
                                &book.book_id,
                                priority,
                                Some(book.series_id.as_str()),
                                &task_id,
                            )),
                        )
                    })
                    .collect(),
            ),
            LibraryTaskCommand::FindBooksToConvert { library_id } => {
                let task_id = format!("FIND_BOOKS_TO_CONVERT_{library_id}");
                LibraryTaskBatch::new(vec![self.catalog.plan_task(
                    PlannedTaskKind::FindBooksToConvert,
                    TaskSchedule::Background,
                    task_id.clone(),
                    LOWEST_PRIORITY,
                    None,
                    Some(book_task_payload(
                        "libraryId",
                        &library_id,
                        LOWEST_PRIORITY,
                        None,
                        &task_id,
                    )),
                )])
            }
        }
    }
}

fn scan_library_payload(library_id: &str, deep_scan: bool, priority: i32, task_id: &str) -> String {
    json!({
        "libraryId": library_id,
        "scanDeep": deep_scan,
        "priority": priority,
        "groupId": serde_json::Value::Null,
        "uniqueId": task_id,
    })
    .to_string()
}

fn book_task_payload(
    target_key: &str,
    target_value: &str,
    priority: i32,
    group_id: Option<&str>,
    task_id: &str,
) -> String {
    json!({
        target_key: target_value,
        "priority": priority,
        "groupId": group_id,
        "uniqueId": task_id,
    })
    .to_string()
}
