use serde_json::Map;

use super::TaskQueueRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskKind {
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskParseError {
    pub name: String,
}

impl std::fmt::Display for TaskParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown task kind: '{}'", self.name)
    }
}

impl std::error::Error for TaskParseError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskNameFormat {
    RuntimeSimple,
    PersistedSimple,
    PersistedClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskDefinition {
    pub kind: TaskKind,
    pub simple_type: &'static str,
    pub persisted_class_name: &'static str,
    pub aliases: &'static [&'static str],
    pub default_priority: i32,
}

pub trait TaskPayload: Clone {
    fn write_task_fields(&self, map: &mut Map<String, serde_json::Value>);

    fn primary_key(&self) -> Option<&str> {
        None
    }

    fn is_empty(&self) -> bool {
        false
    }
}

impl TaskPayload for () {
    fn write_task_fields(&self, _map: &mut Map<String, serde_json::Value>) {}

    fn is_empty(&self) -> bool {
        true
    }
}

#[derive(Debug, Clone)]
pub struct BookPayload {
    pub book_id: String,
}

impl BookPayload {
    pub fn new(book_id: impl Into<String>) -> Self {
        Self {
            book_id: book_id.into(),
        }
    }
}

impl TaskPayload for BookPayload {
    fn write_task_fields(&self, map: &mut Map<String, serde_json::Value>) {
        map.insert(
            "bookId".into(),
            serde_json::Value::String(self.book_id.clone()),
        );
    }

    fn primary_key(&self) -> Option<&str> {
        Some(&self.book_id)
    }
}

#[derive(Debug, Clone)]
pub struct SeriesPayload {
    pub series_id: String,
}

impl SeriesPayload {
    pub fn new(series_id: impl Into<String>) -> Self {
        Self {
            series_id: series_id.into(),
        }
    }
}

impl TaskPayload for SeriesPayload {
    fn write_task_fields(&self, map: &mut Map<String, serde_json::Value>) {
        map.insert(
            "seriesId".into(),
            serde_json::Value::String(self.series_id.clone()),
        );
    }

    fn primary_key(&self) -> Option<&str> {
        Some(&self.series_id)
    }
}

#[derive(Debug, Clone)]
pub struct LibraryPayload {
    pub library_id: String,
}

impl LibraryPayload {
    pub fn new(library_id: impl Into<String>) -> Self {
        Self {
            library_id: library_id.into(),
        }
    }
}

impl TaskPayload for LibraryPayload {
    fn write_task_fields(&self, map: &mut Map<String, serde_json::Value>) {
        map.insert(
            "libraryId".into(),
            serde_json::Value::String(self.library_id.clone()),
        );
    }

    fn primary_key(&self) -> Option<&str> {
        Some(&self.library_id)
    }
}

#[derive(Debug, Clone)]
pub struct RefreshBookMetadataPayload {
    pub book_id: String,
    pub capabilities: Option<Vec<String>>,
}

impl RefreshBookMetadataPayload {
    pub fn new(book_id: impl Into<String>) -> Self {
        Self {
            book_id: book_id.into(),
            capabilities: None,
        }
    }

    pub fn with_capabilities(mut self, capabilities: Vec<String>) -> Self {
        self.capabilities = Some(capabilities);
        self
    }
}

impl TaskPayload for RefreshBookMetadataPayload {
    fn write_task_fields(&self, map: &mut Map<String, serde_json::Value>) {
        map.insert(
            "bookId".into(),
            serde_json::Value::String(self.book_id.clone()),
        );
        if let Some(ref caps) = self.capabilities {
            map.insert(
                "capabilities".into(),
                serde_json::Value::Array(
                    caps.iter()
                        .map(|c| serde_json::Value::String(c.clone()))
                        .collect(),
                ),
            );
        }
    }

    fn primary_key(&self) -> Option<&str> {
        Some(&self.book_id)
    }
}

#[derive(Debug, Clone)]
pub struct ScanLibraryPayload {
    pub library_id: String,
    pub deep_scan: bool,
}

impl ScanLibraryPayload {
    pub fn new(library_id: impl Into<String>, deep_scan: bool) -> Self {
        Self {
            library_id: library_id.into(),
            deep_scan,
        }
    }
}

impl TaskPayload for ScanLibraryPayload {
    fn write_task_fields(&self, map: &mut Map<String, serde_json::Value>) {
        map.insert(
            "libraryId".into(),
            serde_json::Value::String(self.library_id.clone()),
        );
        map.insert("scanDeep".into(), serde_json::Value::Bool(self.deep_scan));
    }

    fn primary_key(&self) -> Option<&str> {
        Some(&self.library_id)
    }
}

#[derive(Debug, Clone)]
pub struct TaskRequest<P: TaskPayload = ()> {
    pub kind: TaskKind,
    pub priority: i32,
    pub group: Option<String>,
    pub payload: P,
}

impl TaskRequest<()> {
    pub fn new(kind: TaskKind) -> Self {
        Self {
            kind,
            priority: kind.definition().default_priority,
            group: None,
            payload: (),
        }
    }
}

impl<P: TaskPayload> TaskRequest<P> {
    pub fn with_payload(kind: TaskKind, payload: P) -> Self {
        Self {
            kind,
            priority: kind.definition().default_priority,
            group: None,
            payload,
        }
    }

    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    pub fn group(mut self, group: impl Into<String>) -> Self {
        self.group = Some(group.into());
        self
    }

    fn build_payload_json(&self, task_id: &str) -> String {
        let mut map = Map::new();
        self.payload.write_task_fields(&mut map);
        map.insert(
            "priority".into(),
            serde_json::Value::Number(self.priority.into()),
        );
        map.insert(
            "groupId".into(),
            match &self.group {
                Some(g) => serde_json::Value::String(g.clone()),
                None => serde_json::Value::Null,
            },
        );
        map.insert(
            "uniqueId".into(),
            serde_json::Value::String(task_id.to_string()),
        );
        serde_json::Value::Object(map).to_string()
    }

    pub fn into_queue_record(self) -> TaskQueueRecord {
        let def = self.kind.definition();
        let simple_type = def.simple_type;
        let id = match self.payload.primary_key() {
            Some(pk) => format!("{simple_type}_{pk}"),
            None => simple_type.to_string(),
        };
        let payload_json = if self.payload.is_empty() {
            None
        } else {
            Some(self.build_payload_json(&id))
        };
        let mut record =
            TaskQueueRecord::new(id, self.priority, self.group).with_simple_type(simple_type);
        if let Some(p) = payload_json {
            record = record.with_payload(p);
        }
        record
    }

    pub fn into_queue_record_with_id(self, suffix: &str) -> TaskQueueRecord {
        let def = self.kind.definition();
        let simple_type = def.simple_type;
        let id = format!("{simple_type}_{suffix}");
        let payload_json = if self.payload.is_empty() {
            None
        } else {
            Some(self.build_payload_json(&id))
        };
        let mut record =
            TaskQueueRecord::new(id, self.priority, self.group).with_simple_type(simple_type);
        if let Some(p) = payload_json {
            record = record.with_payload(p);
        }
        record
    }
}

impl TaskKind {
    pub const fn definition(self) -> TaskDefinition {
        match self {
            Self::ScanLibrary => TaskDefinition {
                kind: Self::ScanLibrary,
                simple_type: "ScanLibrary",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$ScanLibrary",
                aliases: &[],
                default_priority: 4,
            },
            Self::AnalyzeBook => TaskDefinition {
                kind: Self::AnalyzeBook,
                simple_type: "AnalyzeBook",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$AnalyzeBook",
                aliases: &[],
                default_priority: 6,
            },
            Self::EmptyTrash => TaskDefinition {
                kind: Self::EmptyTrash,
                simple_type: "EmptyTrash",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$EmptyTrash",
                aliases: &[],
                default_priority: 6,
            },
            Self::ImportBook => TaskDefinition {
                kind: Self::ImportBook,
                simple_type: "ImportBook",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$ImportBook",
                aliases: &[],
                default_priority: 4,
            },
            Self::FindBooksWithMissingPageHash => TaskDefinition {
                kind: Self::FindBooksWithMissingPageHash,
                simple_type: "FindBooksWithMissingPageHash",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$FindBooksWithMissingPageHash",
                aliases: &[],
                default_priority: 0,
            },
            Self::FindDuplicatePagesToDelete => TaskDefinition {
                kind: Self::FindDuplicatePagesToDelete,
                simple_type: "FindDuplicatePagesToDelete",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$FindDuplicatePagesToDelete",
                aliases: &[],
                default_priority: 0,
            },
            Self::FindBookThumbnailsToRegenerate => TaskDefinition {
                kind: Self::FindBookThumbnailsToRegenerate,
                simple_type: "FindBookThumbnailsToRegenerate",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$FindBookThumbnailsToRegenerate",
                aliases: &[],
                default_priority: 0,
            },
            Self::RefreshBookMetadata => TaskDefinition {
                kind: Self::RefreshBookMetadata,
                simple_type: "RefreshBookMetadata",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$RefreshBookMetadata",
                aliases: &[],
                default_priority: 6,
            },
            Self::RefreshBookLocalArtwork => TaskDefinition {
                kind: Self::RefreshBookLocalArtwork,
                simple_type: "RefreshBookLocalArtwork",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$RefreshBookLocalArtwork",
                aliases: &[],
                default_priority: 6,
            },
            Self::RefreshSeriesLocalArtwork => TaskDefinition {
                kind: Self::RefreshSeriesLocalArtwork,
                simple_type: "RefreshSeriesLocalArtwork",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$RefreshSeriesLocalArtwork",
                aliases: &[],
                default_priority: 6,
            },
            Self::RefreshSeriesMetadata => TaskDefinition {
                kind: Self::RefreshSeriesMetadata,
                simple_type: "RefreshSeriesMetadata",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$RefreshSeriesMetadata",
                aliases: &[],
                default_priority: 6,
            },
            Self::AggregateSeriesMetadata => TaskDefinition {
                kind: Self::AggregateSeriesMetadata,
                simple_type: "AggregateSeriesMetadata",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$AggregateSeriesMetadata",
                aliases: &[],
                default_priority: 6,
            },
            Self::RepairExtension => TaskDefinition {
                kind: Self::RepairExtension,
                simple_type: "RepairExtension",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$RepairExtension",
                aliases: &[],
                default_priority: 4,
            },
            Self::GenerateBookThumbnail => TaskDefinition {
                kind: Self::GenerateBookThumbnail,
                simple_type: "GenerateBookThumbnail",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$GenerateBookThumbnail",
                aliases: &[],
                default_priority: 4,
            },
            Self::HashBook => TaskDefinition {
                kind: Self::HashBook,
                simple_type: "HashBook",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$HashBook",
                aliases: &[],
                default_priority: 4,
            },
            Self::HashBookKoreader => TaskDefinition {
                kind: Self::HashBookKoreader,
                simple_type: "HashBookKoreader",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$HashBookKoreader",
                aliases: &[],
                default_priority: 4,
            },
            Self::HashBookPages => TaskDefinition {
                kind: Self::HashBookPages,
                simple_type: "HashBookPages",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$HashBookPages",
                aliases: &[],
                default_priority: 4,
            },
            Self::RebuildIndex => TaskDefinition {
                kind: Self::RebuildIndex,
                simple_type: "RebuildIndex",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$RebuildIndex",
                aliases: &[],
                default_priority: 2,
            },
            Self::UpgradeIndex => TaskDefinition {
                kind: Self::UpgradeIndex,
                simple_type: "UpgradeIndex",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$UpgradeIndex",
                aliases: &[],
                default_priority: 2,
            },
            Self::RemoveHashedPages => TaskDefinition {
                kind: Self::RemoveHashedPages,
                simple_type: "RemoveHashedPages",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$RemoveHashedPages",
                aliases: &[],
                default_priority: 4,
            },
            Self::DeleteBook => TaskDefinition {
                kind: Self::DeleteBook,
                simple_type: "DeleteBook",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$DeleteBook",
                aliases: &[],
                default_priority: 4,
            },
            Self::DeleteSeries => TaskDefinition {
                kind: Self::DeleteSeries,
                simple_type: "DeleteSeries",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$DeleteSeries",
                aliases: &[],
                default_priority: 4,
            },
            Self::FindBooksToConvert => TaskDefinition {
                kind: Self::FindBooksToConvert,
                simple_type: "FindBooksToConvert",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$FindBooksToConvert",
                aliases: &[],
                default_priority: 0,
            },
            Self::ConvertBook => TaskDefinition {
                kind: Self::ConvertBook,
                simple_type: "ConvertBook",
                persisted_class_name: "org.gotson.komga.application.tasks.Task$ConvertBook",
                aliases: &[],
                default_priority: 4,
            },
        }
    }

    pub fn parse(name: &str) -> Result<Self, TaskParseError> {
        match name {
            "ScanLibrary" | "org.gotson.komga.application.tasks.Task$ScanLibrary" => {
                Ok(Self::ScanLibrary)
            }
            "AnalyzeBook" | "org.gotson.komga.application.tasks.Task$AnalyzeBook" => {
                Ok(Self::AnalyzeBook)
            }
            "EmptyTrash" | "org.gotson.komga.application.tasks.Task$EmptyTrash" => {
                Ok(Self::EmptyTrash)
            }
            "ImportBook" | "org.gotson.komga.application.tasks.Task$ImportBook" => {
                Ok(Self::ImportBook)
            }
            "FindBooksWithMissingPageHash"
            | "org.gotson.komga.application.tasks.Task$FindBooksWithMissingPageHash" => {
                Ok(Self::FindBooksWithMissingPageHash)
            }
            "FindDuplicatePagesToDelete"
            | "org.gotson.komga.application.tasks.Task$FindDuplicatePagesToDelete" => {
                Ok(Self::FindDuplicatePagesToDelete)
            }
            "FindBookThumbnailsToRegenerate"
            | "org.gotson.komga.application.tasks.Task$FindBookThumbnailsToRegenerate" => {
                Ok(Self::FindBookThumbnailsToRegenerate)
            }
            "RefreshBookMetadata"
            | "org.gotson.komga.application.tasks.Task$RefreshBookMetadata" => {
                Ok(Self::RefreshBookMetadata)
            }
            "RefreshBookLocalArtwork"
            | "org.gotson.komga.application.tasks.Task$RefreshBookLocalArtwork" => {
                Ok(Self::RefreshBookLocalArtwork)
            }
            "RefreshSeriesLocalArtwork"
            | "org.gotson.komga.application.tasks.Task$RefreshSeriesLocalArtwork" => {
                Ok(Self::RefreshSeriesLocalArtwork)
            }
            "RefreshSeriesMetadata"
            | "org.gotson.komga.application.tasks.Task$RefreshSeriesMetadata" => {
                Ok(Self::RefreshSeriesMetadata)
            }
            "AggregateSeriesMetadata"
            | "org.gotson.komga.application.tasks.Task$AggregateSeriesMetadata" => {
                Ok(Self::AggregateSeriesMetadata)
            }
            "RepairExtension" | "org.gotson.komga.application.tasks.Task$RepairExtension" => {
                Ok(Self::RepairExtension)
            }
            "GenerateBookThumbnail"
            | "org.gotson.komga.application.tasks.Task$GenerateBookThumbnail" => {
                Ok(Self::GenerateBookThumbnail)
            }
            "HashBook" | "org.gotson.komga.application.tasks.Task$HashBook" => Ok(Self::HashBook),
            "HashBookKoreader" | "org.gotson.komga.application.tasks.Task$HashBookKoreader" => {
                Ok(Self::HashBookKoreader)
            }
            "HashBookPages" | "org.gotson.komga.application.tasks.Task$HashBookPages" => {
                Ok(Self::HashBookPages)
            }
            "RebuildIndex" | "org.gotson.komga.application.tasks.Task$RebuildIndex" => {
                Ok(Self::RebuildIndex)
            }
            "UpgradeIndex" | "org.gotson.komga.application.tasks.Task$UpgradeIndex" => {
                Ok(Self::UpgradeIndex)
            }
            "RemoveHashedPages" | "org.gotson.komga.application.tasks.Task$RemoveHashedPages" => {
                Ok(Self::RemoveHashedPages)
            }
            "DeleteBook" | "org.gotson.komga.application.tasks.Task$DeleteBook" => {
                Ok(Self::DeleteBook)
            }
            "DeleteSeries" | "org.gotson.komga.application.tasks.Task$DeleteSeries" => {
                Ok(Self::DeleteSeries)
            }
            "FindBooksToConvert" | "org.gotson.komga.application.tasks.Task$FindBooksToConvert" => {
                Ok(Self::FindBooksToConvert)
            }
            "ConvertBook" | "org.gotson.komga.application.tasks.Task$ConvertBook" => {
                Ok(Self::ConvertBook)
            }
            _ => Err(TaskParseError {
                name: name.to_string(),
            }),
        }
    }

    pub const fn simple_type(self) -> &'static str {
        self.definition().simple_type
    }

    pub fn all() -> &'static [TaskKind] {
        &[
            Self::ScanLibrary,
            Self::AnalyzeBook,
            Self::EmptyTrash,
            Self::ImportBook,
            Self::FindBooksWithMissingPageHash,
            Self::FindDuplicatePagesToDelete,
            Self::FindBookThumbnailsToRegenerate,
            Self::RefreshBookMetadata,
            Self::RefreshBookLocalArtwork,
            Self::RefreshSeriesLocalArtwork,
            Self::RefreshSeriesMetadata,
            Self::AggregateSeriesMetadata,
            Self::RepairExtension,
            Self::GenerateBookThumbnail,
            Self::HashBook,
            Self::HashBookKoreader,
            Self::HashBookPages,
            Self::RebuildIndex,
            Self::UpgradeIndex,
            Self::RemoveHashedPages,
            Self::DeleteBook,
            Self::DeleteSeries,
            Self::FindBooksToConvert,
            Self::ConvertBook,
        ]
    }
}
