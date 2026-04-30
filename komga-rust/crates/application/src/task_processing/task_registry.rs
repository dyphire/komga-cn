use serde_json::Map;

use super::TaskQueueRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskTypeMetadata {
    pub simple_type: &'static str,
    pub persisted_class_name: &'static str,
    pub default_priority: i32,
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

macro_rules! define_task_registry {
    (
        $(
            $variant:ident => {
                simple_type: $simple_type:expr,
                persisted_class: $persisted_class:expr,
                default_priority: $priority:expr $(,)?
            }
        ),* $(,)?
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum TaskKind {
            $($variant),*
        }

        const TASK_REGISTRY: &[TaskTypeMetadata] = &[
            $(TaskTypeMetadata {
                simple_type: $simple_type,
                persisted_class_name: $persisted_class,
                default_priority: $priority,
            }),*
        ];

        impl TaskKind {
            pub const fn definition(self) -> &'static TaskTypeMetadata {
                &TASK_REGISTRY[self as usize]
            }

            pub fn parse(name: &str) -> Result<Self, TaskParseError> {
                for (i, metadata) in TASK_REGISTRY.iter().enumerate() {
                    if metadata.simple_type == name {
                        return Ok(unsafe { std::mem::transmute::<u8, TaskKind>(i as u8) });
                    }
                }
                for (i, metadata) in TASK_REGISTRY.iter().enumerate() {
                    if metadata.persisted_class_name == name {
                        return Ok(unsafe { std::mem::transmute::<u8, TaskKind>(i as u8) });
                    }
                }
                Err(TaskParseError { name: name.to_string() })
            }

            pub const fn simple_type(self) -> &'static str {
                self.definition().simple_type
            }

            pub fn all() -> &'static [TaskKind] {
                &[
                    $(Self::$variant),*
                ]
            }

            pub fn request_for(self, target_id: impl Into<String>) -> TaskQueueRecord {
                let target = target_id.into();
                match self {
                    Self::AnalyzeBook
                    | Self::HashBook
                    | Self::HashBookKoreader
                    | Self::HashBookPages
                    | Self::GenerateBookThumbnail
                    | Self::RepairExtension
                    | Self::RefreshBookMetadata
                    | Self::RefreshBookLocalArtwork
                    | Self::DeleteBook
                    | Self::ConvertBook => {
                        TaskRequest::with_payload(self, BookPayload::new(target)).into_queue_record()
                    }
                    Self::RefreshSeriesMetadata
                    | Self::AggregateSeriesMetadata
                    | Self::RefreshSeriesLocalArtwork
                    | Self::DeleteSeries => {
                        TaskRequest::with_payload(self, SeriesPayload::new(target)).into_queue_record()
                    }
                    Self::ScanLibrary => {
                        TaskRequest::with_payload(self, ScanLibraryPayload::new(target, false)).into_queue_record()
                    }
                    Self::EmptyTrash
                    | Self::FindBooksToConvert
                    | Self::FindDuplicatePagesToDelete
                    | Self::FindBooksWithMissingPageHash
                    | Self::FindBookThumbnailsToRegenerate
                    | Self::RebuildIndex
                    | Self::UpgradeIndex
                    | Self::RemoveHashedPages => {
                        TaskRequest::new(self).into_queue_record_with_id(&target)
                    }
                    Self::ImportBook => {
                        TaskRequest::new(self).into_queue_record()
                    }
                }
            }
        }

        impl PartialEq<str> for TaskKind {
            fn eq(&self, other: &str) -> bool {
                self.simple_type() == other
            }
        }

        impl PartialEq<&str> for TaskKind {
            fn eq(&self, other: &&str) -> bool {
                self.simple_type() == *other
            }
        }
    };
}

define_task_registry! {
    ScanLibrary => {
        simple_type: "ScanLibrary",
        persisted_class: "org.gotson.komga.application.tasks.Task$ScanLibrary",
        default_priority: 4,
    },
    AnalyzeBook => {
        simple_type: "AnalyzeBook",
        persisted_class: "org.gotson.komga.application.tasks.Task$AnalyzeBook",
        default_priority: 6,
    },
    EmptyTrash => {
        simple_type: "EmptyTrash",
        persisted_class: "org.gotson.komga.application.tasks.Task$EmptyTrash",
        default_priority: 6,
    },
    ImportBook => {
        simple_type: "ImportBook",
        persisted_class: "org.gotson.komga.application.tasks.Task$ImportBook",
        default_priority: 4,
    },
    FindBooksWithMissingPageHash => {
        simple_type: "FindBooksWithMissingPageHash",
        persisted_class: "org.gotson.komga.application.tasks.Task$FindBooksWithMissingPageHash",
        default_priority: 0,
    },
    FindDuplicatePagesToDelete => {
        simple_type: "FindDuplicatePagesToDelete",
        persisted_class: "org.gotson.komga.application.tasks.Task$FindDuplicatePagesToDelete",
        default_priority: 0,
    },
    FindBookThumbnailsToRegenerate => {
        simple_type: "FindBookThumbnailsToRegenerate",
        persisted_class: "org.gotson.komga.application.tasks.Task$FindBookThumbnailsToRegenerate",
        default_priority: 0,
    },
    RefreshBookMetadata => {
        simple_type: "RefreshBookMetadata",
        persisted_class: "org.gotson.komga.application.tasks.Task$RefreshBookMetadata",
        default_priority: 6,
    },
    RefreshBookLocalArtwork => {
        simple_type: "RefreshBookLocalArtwork",
        persisted_class: "org.gotson.komga.application.tasks.Task$RefreshBookLocalArtwork",
        default_priority: 6,
    },
    RefreshSeriesLocalArtwork => {
        simple_type: "RefreshSeriesLocalArtwork",
        persisted_class: "org.gotson.komga.application.tasks.Task$RefreshSeriesLocalArtwork",
        default_priority: 6,
    },
    RefreshSeriesMetadata => {
        simple_type: "RefreshSeriesMetadata",
        persisted_class: "org.gotson.komga.application.tasks.Task$RefreshSeriesMetadata",
        default_priority: 6,
    },
    AggregateSeriesMetadata => {
        simple_type: "AggregateSeriesMetadata",
        persisted_class: "org.gotson.komga.application.tasks.Task$AggregateSeriesMetadata",
        default_priority: 6,
    },
    RepairExtension => {
        simple_type: "RepairExtension",
        persisted_class: "org.gotson.komga.application.tasks.Task$RepairExtension",
        default_priority: 4,
    },
    GenerateBookThumbnail => {
        simple_type: "GenerateBookThumbnail",
        persisted_class: "org.gotson.komga.application.tasks.Task$GenerateBookThumbnail",
        default_priority: 4,
    },
    HashBook => {
        simple_type: "HashBook",
        persisted_class: "org.gotson.komga.application.tasks.Task$HashBook",
        default_priority: 4,
    },
    HashBookKoreader => {
        simple_type: "HashBookKoreader",
        persisted_class: "org.gotson.komga.application.tasks.Task$HashBookKoreader",
        default_priority: 4,
    },
    HashBookPages => {
        simple_type: "HashBookPages",
        persisted_class: "org.gotson.komga.application.tasks.Task$HashBookPages",
        default_priority: 4,
    },
    RebuildIndex => {
        simple_type: "RebuildIndex",
        persisted_class: "org.gotson.komga.application.tasks.Task$RebuildIndex",
        default_priority: 2,
    },
    UpgradeIndex => {
        simple_type: "UpgradeIndex",
        persisted_class: "org.gotson.komga.application.tasks.Task$UpgradeIndex",
        default_priority: 2,
    },
    RemoveHashedPages => {
        simple_type: "RemoveHashedPages",
        persisted_class: "org.gotson.komga.application.tasks.Task$RemoveHashedPages",
        default_priority: 4,
    },
    DeleteBook => {
        simple_type: "DeleteBook",
        persisted_class: "org.gotson.komga.application.tasks.Task$DeleteBook",
        default_priority: 4,
    },
    DeleteSeries => {
        simple_type: "DeleteSeries",
        persisted_class: "org.gotson.komga.application.tasks.Task$DeleteSeries",
        default_priority: 4,
    },
    FindBooksToConvert => {
        simple_type: "FindBooksToConvert",
        persisted_class: "org.gotson.komga.application.tasks.Task$FindBooksToConvert",
        default_priority: 0,
    },
    ConvertBook => {
        simple_type: "ConvertBook",
        persisted_class: "org.gotson.komga.application.tasks.Task$ConvertBook",
        default_priority: 4,
    },
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_kind_parse_simple_type() {
        assert_eq!(
            TaskKind::parse("ScanLibrary").unwrap(),
            TaskKind::ScanLibrary
        );
        assert_eq!(
            TaskKind::parse("AnalyzeBook").unwrap(),
            TaskKind::AnalyzeBook
        );
        assert_eq!(
            TaskKind::parse("ConvertBook").unwrap(),
            TaskKind::ConvertBook
        );
    }

    #[test]
    fn task_kind_parse_kotlin_class() {
        assert_eq!(
            TaskKind::parse("org.gotson.komga.application.tasks.Task$ScanLibrary").unwrap(),
            TaskKind::ScanLibrary
        );
        assert_eq!(
            TaskKind::parse("org.gotson.komga.application.tasks.Task$AnalyzeBook").unwrap(),
            TaskKind::AnalyzeBook
        );
    }

    #[test]
    fn task_kind_parse_unknown() {
        assert!(TaskKind::parse("UnknownTask").is_err());
    }

    #[test]
    fn task_kind_all_count() {
        assert_eq!(TaskKind::all().len(), 24);
    }

    #[test]
    fn task_kind_definition_simple_type() {
        assert_eq!(TaskKind::ScanLibrary.simple_type(), "ScanLibrary");
        assert_eq!(TaskKind::AnalyzeBook.simple_type(), "AnalyzeBook");
        assert_eq!(TaskKind::ConvertBook.simple_type(), "ConvertBook");
    }

    #[test]
    fn task_kind_definition_priority() {
        assert_eq!(TaskKind::ScanLibrary.definition().default_priority, 4);
        assert_eq!(TaskKind::AnalyzeBook.definition().default_priority, 6);
        assert_eq!(TaskKind::RebuildIndex.definition().default_priority, 2);
    }

    #[test]
    fn task_request_new_default_priority() {
        let request = TaskRequest::new(TaskKind::ScanLibrary);
        assert_eq!(request.priority, 4);
        assert!(request.group.is_none());
    }

    #[test]
    fn task_request_with_payload() {
        let request = TaskRequest::with_payload(TaskKind::AnalyzeBook, BookPayload::new("book-1"));
        assert_eq!(request.priority, 6);
        assert_eq!(request.payload.book_id, "book-1");
    }

    #[test]
    fn task_request_into_queue_record() {
        let request = TaskRequest::with_payload(TaskKind::AnalyzeBook, BookPayload::new("book-1"));
        let record = request.into_queue_record();
        assert_eq!(record.id, "AnalyzeBook_book-1");
        assert_eq!(record.simple_type, "AnalyzeBook");
        assert!(record.payload.is_some());
    }

    #[test]
    fn task_request_into_queue_record_with_id() {
        let request = TaskRequest::new(TaskKind::EmptyTrash);
        let record = request.into_queue_record_with_id("library-1");
        assert_eq!(record.id, "EmptyTrash_library-1");
        assert_eq!(record.simple_type, "EmptyTrash");
    }
}
