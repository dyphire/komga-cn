use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{TaskProcessingError, TaskQueueRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskTypeMetadata {
    pub simple_type: &'static str,
    pub persisted_class_name: &'static str,
    pub default_priority: i32,
    pub compat_target_key: Option<&'static str>,
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
                default_priority: $priority:expr,
                target: $target:tt,
                compat_key: $compat_key:expr $(,)?
            }
        ),* $(,)?
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum TaskKind {
            $($variant),*
        }

        impl TaskKind {
            pub const fn definition(self) -> TaskTypeMetadata {
                match self {
                    $(Self::$variant => TaskTypeMetadata {
                        simple_type: $simple_type,
                        persisted_class_name: $persisted_class,
                        default_priority: $priority,
                        compat_target_key: $compat_key,
                    }),*
                }
            }

            pub fn parse(name: &str) -> Result<Self, TaskParseError> {
                match name {
                    $($simple_type | $persisted_class => Ok(Self::$variant),)*
                    _ => Err(TaskParseError { name: name.to_string() }),
                }
            }

            pub const fn simple_type(self) -> &'static str {
                self.definition().simple_type
            }

            pub const fn compat_target_key(self) -> Option<&'static str> {
                self.definition().compat_target_key
            }

            pub fn all() -> &'static [TaskKind] {
                &[
                    $(Self::$variant),*
                ]
            }

            pub fn request_for(self, target_id: impl Into<String>) -> TaskQueueRecord {
                let target = target_id.into();
                define_task_registry!(@request_for_body self target; $($variant => $target),*)
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

    // Generate the entire request_for match body
    (@request_for_body $self:ident $target:ident; $($variant:ident => $ttype:tt),*) => {
        match $self {
            $(TaskKind::$variant => define_task_registry!(@request_for_expr $self $target $ttype)),*
        }
    };

    // request_for expression generators
    (@request_for_expr $self:ident $target:ident Book) => {
        TaskRequest::with_payload($self, BookPayload::new($target)).into_queue_record()
    };
    (@request_for_expr $self:ident $target:ident Series) => {
        TaskRequest::with_payload($self, SeriesPayload::new($target)).into_queue_record()
    };
    (@request_for_expr $self:ident $target:ident Library) => {
        TaskRequest::with_payload($self, LibraryPayload::new($target)).into_queue_record()
    };
    (@request_for_expr $self:ident $target:ident TargetId) => {
        TaskRequest::new($self).into_queue_record_with_id(&$target)
    };
    (@request_for_expr $self:ident $target:ident Custom) => {
        request_for_custom($self, $target)
    };
}

fn request_for_custom(kind: TaskKind, target: String) -> TaskQueueRecord {
    match kind {
        TaskKind::ScanLibrary => {
            TaskRequest::with_payload(kind, ScanLibraryPayload::new(target, false))
                .into_queue_record()
        }
        TaskKind::ImportBook => TaskRequest::new(kind).into_queue_record(),
        _ => unreachable!(),
    }
}

define_task_registry! {
    ScanLibrary => {
        simple_type: "ScanLibrary",
        persisted_class: "org.gotson.komga.application.tasks.Task$ScanLibrary",
        default_priority: 4,
        target: Custom,
        compat_key: None,
    },
    AnalyzeBook => {
        simple_type: "AnalyzeBook",
        persisted_class: "org.gotson.komga.application.tasks.Task$AnalyzeBook",
        default_priority: 6,
        target: Book,
        compat_key: Some("bookId"),
    },
    EmptyTrash => {
        simple_type: "EmptyTrash",
        persisted_class: "org.gotson.komga.application.tasks.Task$EmptyTrash",
        default_priority: 6,
        target: TargetId,
        compat_key: Some("libraryId"),
    },
    ImportBook => {
        simple_type: "ImportBook",
        persisted_class: "org.gotson.komga.application.tasks.Task$ImportBook",
        default_priority: 4,
        target: Custom,
        compat_key: None,
    },
    FindBooksWithMissingPageHash => {
        simple_type: "FindBooksWithMissingPageHash",
        persisted_class: "org.gotson.komga.application.tasks.Task$FindBooksWithMissingPageHash",
        default_priority: 0,
        target: TargetId,
        compat_key: Some("libraryId"),
    },
    FindDuplicatePagesToDelete => {
        simple_type: "FindDuplicatePagesToDelete",
        persisted_class: "org.gotson.komga.application.tasks.Task$FindDuplicatePagesToDelete",
        default_priority: 0,
        target: TargetId,
        compat_key: Some("libraryId"),
    },
    FindBookThumbnailsToRegenerate => {
        simple_type: "FindBookThumbnailsToRegenerate",
        persisted_class: "org.gotson.komga.application.tasks.Task$FindBookThumbnailsToRegenerate",
        default_priority: 0,
        target: TargetId,
        compat_key: None,
    },
    RefreshBookMetadata => {
        simple_type: "RefreshBookMetadata",
        persisted_class: "org.gotson.komga.application.tasks.Task$RefreshBookMetadata",
        default_priority: 6,
        target: Book,
        compat_key: None,
    },
    RefreshBookLocalArtwork => {
        simple_type: "RefreshBookLocalArtwork",
        persisted_class: "org.gotson.komga.application.tasks.Task$RefreshBookLocalArtwork",
        default_priority: 6,
        target: Book,
        compat_key: Some("bookId"),
    },
    RefreshSeriesLocalArtwork => {
        simple_type: "RefreshSeriesLocalArtwork",
        persisted_class: "org.gotson.komga.application.tasks.Task$RefreshSeriesLocalArtwork",
        default_priority: 6,
        target: Series,
        compat_key: Some("seriesId"),
    },
    RefreshSeriesMetadata => {
        simple_type: "RefreshSeriesMetadata",
        persisted_class: "org.gotson.komga.application.tasks.Task$RefreshSeriesMetadata",
        default_priority: 6,
        target: Series,
        compat_key: Some("seriesId"),
    },
    AggregateSeriesMetadata => {
        simple_type: "AggregateSeriesMetadata",
        persisted_class: "org.gotson.komga.application.tasks.Task$AggregateSeriesMetadata",
        default_priority: 6,
        target: Series,
        compat_key: Some("seriesId"),
    },
    RepairExtension => {
        simple_type: "RepairExtension",
        persisted_class: "org.gotson.komga.application.tasks.Task$RepairExtension",
        default_priority: 4,
        target: Book,
        compat_key: Some("bookId"),
    },
    GenerateBookThumbnail => {
        simple_type: "GenerateBookThumbnail",
        persisted_class: "org.gotson.komga.application.tasks.Task$GenerateBookThumbnail",
        default_priority: 4,
        target: Book,
        compat_key: Some("bookId"),
    },
    HashBook => {
        simple_type: "HashBook",
        persisted_class: "org.gotson.komga.application.tasks.Task$HashBook",
        default_priority: 4,
        target: Book,
        compat_key: Some("bookId"),
    },
    HashBookKoreader => {
        simple_type: "HashBookKoreader",
        persisted_class: "org.gotson.komga.application.tasks.Task$HashBookKoreader",
        default_priority: 4,
        target: Book,
        compat_key: Some("bookId"),
    },
    HashBookPages => {
        simple_type: "HashBookPages",
        persisted_class: "org.gotson.komga.application.tasks.Task$HashBookPages",
        default_priority: 4,
        target: Book,
        compat_key: Some("bookId"),
    },
    RebuildIndex => {
        simple_type: "RebuildIndex",
        persisted_class: "org.gotson.komga.application.tasks.Task$RebuildIndex",
        default_priority: 2,
        target: TargetId,
        compat_key: None,
    },
    UpgradeIndex => {
        simple_type: "UpgradeIndex",
        persisted_class: "org.gotson.komga.application.tasks.Task$UpgradeIndex",
        default_priority: 2,
        target: TargetId,
        compat_key: None,
    },
    RemoveHashedPages => {
        simple_type: "RemoveHashedPages",
        persisted_class: "org.gotson.komga.application.tasks.Task$RemoveHashedPages",
        default_priority: 4,
        target: TargetId,
        compat_key: None,
    },
    DeleteBook => {
        simple_type: "DeleteBook",
        persisted_class: "org.gotson.komga.application.tasks.Task$DeleteBook",
        default_priority: 4,
        target: Book,
        compat_key: Some("bookId"),
    },
    DeleteSeries => {
        simple_type: "DeleteSeries",
        persisted_class: "org.gotson.komga.application.tasks.Task$DeleteSeries",
        default_priority: 4,
        target: Series,
        compat_key: Some("seriesId"),
    },
    FindBooksToConvert => {
        simple_type: "FindBooksToConvert",
        persisted_class: "org.gotson.komga.application.tasks.Task$FindBooksToConvert",
        default_priority: 0,
        target: TargetId,
        compat_key: Some("libraryId"),
    },
    ConvertBook => {
        simple_type: "ConvertBook",
        persisted_class: "org.gotson.komga.application.tasks.Task$ConvertBook",
        default_priority: 4,
        target: Book,
        compat_key: Some("bookId"),
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

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImportBookCopyMode {
    Move,
    Copy,
    Hardlink,
}

impl ImportBookCopyMode {
    fn persisted_name(self) -> &'static str {
        match self {
            Self::Move => "MOVE",
            Self::Copy => "COPY",
            Self::Hardlink => "HARDLINK",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ImportBookPayload {
    pub source_file: String,
    pub series_id: String,
    pub copy_mode: ImportBookCopyMode,
    pub destination_name: Option<String>,
    pub upgrade_book_id: Option<String>,
}

impl ImportBookPayload {
    pub fn new(
        source_file: impl Into<String>,
        series_id: impl Into<String>,
        copy_mode: ImportBookCopyMode,
        destination_name: Option<String>,
        upgrade_book_id: Option<String>,
    ) -> Self {
        Self {
            source_file: source_file.into(),
            series_id: series_id.into(),
            copy_mode,
            destination_name,
            upgrade_book_id,
        }
    }

    pub fn from_task_record(task: &TaskQueueRecord) -> Result<Self, TaskProcessingError> {
        let Some(payload) = task.payload.as_deref() else {
            return Err(TaskProcessingError::invalid_task(
                "ImportBook task requires serialized payload",
            ));
        };
        let payload = serde_json::from_str::<Value>(payload).map_err(|error| {
            TaskProcessingError::runtime(format!("failed to parse ImportBook payload: {error}"))
        })?;

        parse_import_book_payload(&payload)
    }
}

impl TaskPayload for ImportBookPayload {
    fn write_task_fields(&self, map: &mut Map<String, serde_json::Value>) {
        map.insert(
            "sourceFile".into(),
            serde_json::Value::String(self.source_file.clone()),
        );
        map.insert(
            "seriesId".into(),
            serde_json::Value::String(self.series_id.clone()),
        );
        map.insert(
            "copyMode".into(),
            serde_json::Value::String(self.copy_mode.persisted_name().to_string()),
        );
        map.insert(
            "destinationName".into(),
            optional_payload_string(self.destination_name.as_deref()),
        );
        map.insert(
            "upgradeBookId".into(),
            optional_payload_string(self.upgrade_book_id.as_deref()),
        );
    }
}

fn optional_payload_string(value: Option<&str>) -> serde_json::Value {
    value
        .map(|value| serde_json::Value::String(value.to_string()))
        .unwrap_or(serde_json::Value::Null)
}

fn parse_import_book_payload(payload: &Value) -> Result<ImportBookPayload, TaskProcessingError> {
    if import_book_payload_has_flat_fields(payload) {
        parse_flat_import_book_payload(payload)
    } else {
        parse_nested_import_book_payload(payload)
    }
}

fn import_book_payload_has_flat_fields(payload: &Value) -> bool {
    payload.get("sourceFile").is_some()
        || payload.get("seriesId").is_some()
        || payload.get("copyMode").is_some()
        || payload.get("destinationName").is_some()
        || payload.get("upgradeBookId").is_some()
}

fn parse_flat_import_book_payload(
    payload: &Value,
) -> Result<ImportBookPayload, TaskProcessingError> {
    Ok(ImportBookPayload::new(
        required_import_book_string(payload, "sourceFile")?,
        required_import_book_string(payload, "seriesId")?,
        parse_import_book_copy_mode(
            "copyMode",
            required_import_book_string(payload, "copyMode")?,
        )?,
        optional_import_book_string(payload.get("destinationName"), "destinationName")?,
        optional_import_book_string(payload.get("upgradeBookId"), "upgradeBookId")?,
    ))
}

fn parse_nested_import_book_payload(
    payload: &Value,
) -> Result<ImportBookPayload, TaskProcessingError> {
    let book = payload.get("book").ok_or_else(|| {
        TaskProcessingError::invalid_task(
            "ImportBook payload must include sourceFile, seriesId, and copyMode",
        )
    })?;
    Ok(ImportBookPayload::new(
        required_nested_import_book_string(book, "source_file")?,
        required_nested_import_book_string(book, "series_id")?,
        parse_import_book_copy_mode(
            "copy_mode",
            required_nested_import_book_string(payload, "copy_mode")?,
        )?,
        optional_import_book_string(book.get("destination_name"), "destination_name")?,
        optional_import_book_string(book.get("upgrade_book_id"), "upgrade_book_id")?,
    ))
}

fn required_import_book_string<'a>(
    payload: &'a Value,
    key: &str,
) -> Result<&'a str, TaskProcessingError> {
    let Some(value) = payload.get(key) else {
        return Err(TaskProcessingError::invalid_task(
            "ImportBook payload must include sourceFile, seriesId, and copyMode",
        ));
    };
    value.as_str().ok_or_else(|| {
        TaskProcessingError::runtime(format!("ImportBook payload field '{key}' must be a string"))
    })
}

fn required_nested_import_book_string<'a>(
    payload: &'a Value,
    key: &str,
) -> Result<&'a str, TaskProcessingError> {
    let Some(value) = payload.get(key) else {
        return Err(TaskProcessingError::invalid_task(
            "ImportBook payload must include source_file, series_id, and copy_mode",
        ));
    };
    value.as_str().ok_or_else(|| {
        TaskProcessingError::runtime(format!("ImportBook payload field '{key}' must be a string"))
    })
}

fn optional_import_book_string(
    value: Option<&Value>,
    key: &str,
) -> Result<Option<String>, TaskProcessingError> {
    match value {
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(Value::Null) | None => Ok(None),
        _ => Err(TaskProcessingError::runtime(format!(
            "ImportBook payload field '{key}' must be a string or null"
        ))),
    }
}

fn parse_import_book_copy_mode(
    key: &str,
    value: &str,
) -> Result<ImportBookCopyMode, TaskProcessingError> {
    match value {
        "MOVE" => Ok(ImportBookCopyMode::Move),
        "COPY" => Ok(ImportBookCopyMode::Copy),
        "HARDLINK" => Ok(ImportBookCopyMode::Hardlink),
        _ => Err(TaskProcessingError::runtime(format!(
            "ImportBook payload field '{key}' must be one of MOVE, COPY, HARDLINK: {value}"
        ))),
    }
}

#[derive(Debug, Clone)]
pub struct RefreshBookMetadataPayload {
    pub book_id: String,
    pub capabilities: Option<Vec<String>>,
}

impl RefreshBookMetadataPayload {
    const DEFAULT_CAPABILITIES: &'static [&'static str] = &[
        "TITLE",
        "SUMMARY",
        "NUMBER",
        "NUMBER_SORT",
        "RELEASE_DATE",
        "AUTHORS",
        "TAGS",
        "ISBN",
        "READ_LISTS",
        "THUMBNAILS",
        "LINKS",
    ];

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

    pub fn with_default_capabilities(mut self) -> Self {
        if self
            .capabilities
            .as_ref()
            .map(Vec::is_empty)
            .unwrap_or(true)
        {
            self.capabilities = Some(Self::default_capabilities());
        }
        self
    }

    pub fn from_task_record(
        task: &TaskQueueRecord,
        book_id: &str,
    ) -> Result<Self, TaskProcessingError> {
        let mut payload = Self::new(book_id);
        if let Some(capabilities) =
            parse_refresh_book_metadata_payload(task.payload.as_deref(), book_id)?
        {
            payload = payload.with_capabilities(capabilities);
        }
        Ok(payload)
    }

    pub fn default_capabilities() -> Vec<String> {
        Self::DEFAULT_CAPABILITIES
            .iter()
            .map(|capability| capability.to_string())
            .collect()
    }

    pub fn capabilities_for_execution(&self) -> BTreeSet<String> {
        self.capabilities
            .clone()
            .filter(|capabilities| !capabilities.is_empty())
            .unwrap_or_else(Self::default_capabilities)
            .into_iter()
            .collect()
    }
}

fn parse_refresh_book_metadata_payload(
    payload: Option<&str>,
    book_id: &str,
) -> Result<Option<Vec<String>>, TaskProcessingError> {
    let Some(payload) = payload else {
        return Ok(None);
    };
    let payload = serde_json::from_str::<Value>(payload).map_err(|error| {
        TaskProcessingError::runtime(format!(
            "RefreshBookMetadata payload must be valid JSON: {error}"
        ))
    })?;
    let payload = payload.as_object().ok_or_else(|| {
        TaskProcessingError::invalid_task("RefreshBookMetadata payload must be a JSON object")
    })?;
    validate_refresh_book_metadata_book_id(payload.get("bookId"), book_id)?;
    let Some(capabilities) = payload.get("capabilities") else {
        return Ok(None);
    };
    if capabilities.is_null() {
        return Ok(None);
    }
    let capabilities = capabilities.as_array().ok_or_else(|| {
        TaskProcessingError::invalid_task(
            "RefreshBookMetadata payload field 'capabilities' must be an array",
        )
    })?;

    let mut parsed = BTreeSet::new();
    for capability in capabilities {
        let Some(capability) = capability.as_str() else {
            return Err(TaskProcessingError::runtime(
                "RefreshBookMetadata payload field 'capabilities' must contain only strings",
            ));
        };
        parsed.insert(capability.to_string());
    }

    if parsed.is_empty() {
        return Ok(None);
    }
    Ok(Some(parsed.into_iter().collect()))
}

fn validate_refresh_book_metadata_book_id(
    value: Option<&Value>,
    task_book_id: &str,
) -> Result<(), TaskProcessingError> {
    let Some(value) = value else {
        return Ok(());
    };
    let Some(payload_book_id) = value.as_str() else {
        return Err(TaskProcessingError::runtime(
            "RefreshBookMetadata payload field 'bookId' must be a string",
        ));
    };
    if payload_book_id != task_book_id {
        return Err(TaskProcessingError::runtime(format!(
            "RefreshBookMetadata payload field 'bookId' must match the task target: {payload_book_id}"
        )));
    }
    Ok(())
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

    pub fn from_task_record(task: &TaskQueueRecord) -> Result<Self, TaskProcessingError> {
        let payload = task
            .payload
            .as_deref()
            .map(scan_library_payload_fields)
            .transpose()?
            .flatten();
        let task_target = task.target();
        let library_id = payload
            .as_ref()
            .and_then(|fields| fields.library_id.clone())
            .or_else(|| task_target.map(scan_library_target_library_id));
        let Some(library_id) = library_id else {
            return Err(TaskProcessingError::invalid_task(
                "ScanLibrary task must include a library id",
            ));
        };

        let deep_scan = match payload.as_ref().and_then(|fields| fields.deep_scan) {
            Some(deep_scan) => deep_scan,
            None => task_target
                .map(scan_library_target_deep_scan)
                .transpose()?
                .flatten()
                .unwrap_or(false),
        };

        Ok(Self::new(library_id, deep_scan))
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RebuildIndexEntity {
    Book,
    Series,
    Collection,
    ReadList,
}

impl RebuildIndexEntity {
    fn persisted_name(self) -> &'static str {
        match self {
            Self::Book => "Book",
            Self::Series => "Series",
            Self::Collection => "Collection",
            Self::ReadList => "ReadList",
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RebuildIndexPayload {
    pub entities: Option<Vec<RebuildIndexEntity>>,
}

impl RebuildIndexPayload {
    pub fn all_entities() -> Self {
        Self { entities: None }
    }

    pub fn selected(entities: Vec<RebuildIndexEntity>) -> Self {
        Self {
            entities: Some(entities),
        }
    }

    pub fn from_task_record(task: &TaskQueueRecord) -> Result<Self, TaskProcessingError> {
        let Some(payload) = task.payload.as_deref() else {
            return Ok(Self::all_entities());
        };
        let payload = serde_json::from_str::<Value>(payload).map_err(|error| {
            TaskProcessingError::runtime(format!(
                "RebuildIndex payload must be valid JSON: {error}"
            ))
        })?;
        let payload = payload.as_object().ok_or_else(|| {
            TaskProcessingError::invalid_task("RebuildIndex payload must be a JSON object")
        })?;
        let Some(entities) = payload.get("entities") else {
            return Ok(Self::all_entities());
        };
        if entities.is_null() {
            return Ok(Self::all_entities());
        }
        let entity_values = entities.as_array().ok_or_else(|| {
            TaskProcessingError::invalid_task(
                "RebuildIndex payload field 'entities' must be an array",
            )
        })?;

        let mut parsed = Vec::new();
        for entity in entity_values {
            let entity = parse_rebuild_index_entity(entity).ok_or_else(|| {
                TaskProcessingError::runtime(format!(
                    "RebuildIndex payload contains unsupported entity selector: {entity}"
                ))
            })?;
            if !parsed.contains(&entity) {
                parsed.push(entity);
            }
        }

        Ok(Self::selected(parsed))
    }
}

impl TaskPayload for RebuildIndexPayload {
    fn write_task_fields(&self, map: &mut Map<String, serde_json::Value>) {
        map.insert(
            "entities".into(),
            self.entities
                .as_ref()
                .map(|entities| {
                    Value::Array(
                        entities
                            .iter()
                            .map(|entity| Value::String(entity.persisted_name().to_string()))
                            .collect(),
                    )
                })
                .unwrap_or(Value::Null),
        );
    }
}

fn parse_rebuild_index_entity(value: &Value) -> Option<RebuildIndexEntity> {
    let raw = match value {
        Value::String(value) => Some(value.as_str()),
        Value::Object(value) => value.get("type").and_then(Value::as_str),
        _ => None,
    }?;

    match raw.trim().to_ascii_lowercase().as_str() {
        "book" => Some(RebuildIndexEntity::Book),
        "series" => Some(RebuildIndexEntity::Series),
        "collection" => Some(RebuildIndexEntity::Collection),
        "readlist" => Some(RebuildIndexEntity::ReadList),
        _ => None,
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct FindBookThumbnailsToRegeneratePayload {
    pub for_bigger_result_only: bool,
}

impl FindBookThumbnailsToRegeneratePayload {
    pub fn new(for_bigger_result_only: bool) -> Self {
        Self {
            for_bigger_result_only,
        }
    }

    pub fn from_task_record(task: &TaskQueueRecord) -> Result<Self, TaskProcessingError> {
        let Some(payload) = task.payload.as_deref() else {
            return Ok(Self::new(false));
        };
        let payload = serde_json::from_str::<Value>(payload).map_err(|error| {
            TaskProcessingError::runtime(format!(
                "FindBookThumbnailsToRegenerate payload must be valid JSON: {error}"
            ))
        })?;
        let payload = payload.as_object().ok_or_else(|| {
            TaskProcessingError::invalid_task(
                "FindBookThumbnailsToRegenerate payload must be a JSON object",
            )
        })?;
        let for_bigger_result_only = payload
            .get("for_bigger_result_only")
            .map(|value| ("for_bigger_result_only", value))
            .or_else(|| {
                payload
                    .get("forBiggerResultOnly")
                    .map(|value| ("forBiggerResultOnly", value))
            })
            .map(|(key, value)| parse_thumbnail_regeneration_flag(key, value))
            .transpose()?
            .unwrap_or(false);

        Ok(Self::new(for_bigger_result_only))
    }
}

fn parse_thumbnail_regeneration_flag(
    key: &str,
    value: &Value,
) -> Result<bool, TaskProcessingError> {
    value.as_bool().ok_or_else(|| {
        TaskProcessingError::runtime(format!(
            "FindBookThumbnailsToRegenerate payload field '{key}' must be a boolean"
        ))
    })
}

impl TaskPayload for FindBookThumbnailsToRegeneratePayload {
    fn write_task_fields(&self, map: &mut Map<String, serde_json::Value>) {
        map.insert(
            "forBiggerResultOnly".into(),
            Value::Bool(self.for_bigger_result_only),
        );
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ScanLibraryPayloadFields {
    library_id: Option<String>,
    deep_scan: Option<bool>,
}

fn scan_library_payload_fields(
    payload: &str,
) -> Result<Option<ScanLibraryPayloadFields>, TaskProcessingError> {
    let payload = serde_json::from_str::<serde_json::Value>(payload).map_err(|error| {
        TaskProcessingError::runtime(format!("ScanLibrary payload must be valid JSON: {error}"))
    })?;
    let payload = payload.as_object().ok_or_else(|| {
        TaskProcessingError::invalid_task("ScanLibrary payload must be a JSON object")
    })?;

    if payload.is_empty() {
        return Ok(None);
    }

    Ok(Some(ScanLibraryPayloadFields {
        library_id: optional_scan_library_string(payload.get("libraryId"), "libraryId")?,
        deep_scan: scan_library_deep_scan(payload)?,
    }))
}

fn scan_library_deep_scan(
    payload: &serde_json::Map<String, serde_json::Value>,
) -> Result<Option<bool>, TaskProcessingError> {
    match optional_scan_library_bool(payload, "scanDeep")? {
        Some(value) => Ok(Some(value)),
        None => optional_scan_library_bool(payload, "deep"),
    }
}

fn optional_scan_library_string(
    value: Option<&serde_json::Value>,
    key: &str,
) -> Result<Option<String>, TaskProcessingError> {
    let Some(value) = value else {
        return Ok(None);
    };
    value
        .as_str()
        .map(|value| Some(value.to_string()))
        .ok_or_else(|| {
            TaskProcessingError::runtime(format!(
                "ScanLibrary payload field '{key}' must be a string"
            ))
        })
}

fn optional_scan_library_bool(
    payload: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<Option<bool>, TaskProcessingError> {
    let Some(value) = payload.get(key) else {
        return Ok(None);
    };
    value.as_bool().map(Some).ok_or_else(|| {
        TaskProcessingError::runtime(format!(
            "ScanLibrary payload field '{key}' must be a boolean"
        ))
    })
}

fn scan_library_target_library_id(task_target: &str) -> String {
    task_target
        .split_once("_DEEP_")
        .map(|(library_id, _)| library_id)
        .unwrap_or(task_target)
        .to_string()
}

fn scan_library_target_deep_scan(task_target: &str) -> Result<Option<bool>, TaskProcessingError> {
    let Some((_, deep_scan)) = task_target.rsplit_once("_DEEP_") else {
        return Ok(None);
    };
    deep_scan.parse::<bool>().map(Some).map_err(|_| {
        TaskProcessingError::runtime(format!(
            "ScanLibrary legacy task target deep flag must be a boolean: {deep_scan}"
        ))
    })
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HashedPageToDeletePayload {
    pub file_hash: String,
    pub file_size: i64,
    pub file_name: String,
    pub media_type: String,
    pub page_number: i64,
}

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveHashedPagesPayload {
    pub book_id: String,
    pub pages: Vec<HashedPageToDeletePayload>,
}

impl RemoveHashedPagesPayload {
    pub fn new(book_id: impl Into<String>, pages: Vec<HashedPageToDeletePayload>) -> Self {
        Self {
            book_id: book_id.into(),
            pages,
        }
    }

    pub fn from_task_record(
        task: &TaskQueueRecord,
        book_id: &str,
    ) -> Result<Self, TaskProcessingError> {
        let Some(payload) = task.payload.as_deref() else {
            return Err(TaskProcessingError::invalid_task(
                "RemoveHashedPages task requires serialized payload",
            ));
        };
        let parsed =
            serde_json::from_str::<RemoveHashedPagesTaskEnvelope>(payload).map_err(|error| {
                TaskProcessingError::runtime(format!(
                    "failed to parse RemoveHashedPages payload: {error}",
                ))
            })?;
        if parsed.book_id != book_id {
            return Err(TaskProcessingError::invalid_task(
                "RemoveHashedPages payload book id must match task id",
            ));
        }

        if parsed.unique_id != task.id {
            return Err(TaskProcessingError::invalid_task(
                "RemoveHashedPages payload unique id must match task id",
            ));
        }

        Ok(Self {
            book_id: parsed.book_id,
            pages: parsed.pages,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoveHashedPagesTaskEnvelope {
    book_id: String,
    pages: Vec<HashedPageToDeletePayload>,
    unique_id: String,
}

impl TaskPayload for RemoveHashedPagesPayload {
    fn write_task_fields(&self, map: &mut Map<String, serde_json::Value>) {
        map.insert(
            "bookId".into(),
            serde_json::Value::String(self.book_id.clone()),
        );
        map.insert(
            "pages".into(),
            serde_json::to_value(&self.pages)
                .expect("hashed pages payload should serialize to JSON"),
        );
    }

    fn primary_key(&self) -> Option<&str> {
        Some(&self.book_id)
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
    fn task_kind_all_roundtrips_registered_names() {
        for kind in TaskKind::all() {
            let definition = kind.definition();

            assert_eq!(TaskKind::parse(definition.simple_type), Ok(*kind));
            assert_eq!(TaskKind::parse(definition.persisted_class_name), Ok(*kind));
        }
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
    fn remove_hashed_pages_payload_roundtrips_from_task_record() {
        let record = TaskRequest::with_payload(
            TaskKind::RemoveHashedPages,
            RemoveHashedPagesPayload::new(
                "book-1",
                vec![HashedPageToDeletePayload {
                    file_hash: "hash-1".to_string(),
                    file_size: 123,
                    file_name: "page-1.jpg".to_string(),
                    media_type: "image/jpeg".to_string(),
                    page_number: 1,
                }],
            ),
        )
        .priority(12)
        .into_queue_record();

        let payload = RemoveHashedPagesPayload::from_task_record(&record, "book-1")
            .expect("RemoveHashedPages payload should parse from its queue record");

        assert_eq!(payload.book_id, "book-1");
        assert_eq!(
            payload.pages,
            vec![HashedPageToDeletePayload {
                file_hash: "hash-1".to_string(),
                file_size: 123,
                file_name: "page-1.jpg".to_string(),
                media_type: "image/jpeg".to_string(),
                page_number: 1,
            }]
        );
    }

    #[test]
    fn import_book_payload_roundtrips_from_task_record() {
        let record = TaskRequest::with_payload(
            TaskKind::ImportBook,
            ImportBookPayload::new(
                "/tmp/book-a.cbz",
                "series-1",
                ImportBookCopyMode::Hardlink,
                Some("dest-a".to_string()),
                Some("book-1".to_string()),
            ),
        )
        .priority(100)
        .group("series-1")
        .into_queue_record_with_id("series-1_/tmp/book-a.cbz");

        let payload =
            ImportBookPayload::from_task_record(&record).expect("ImportBook payload should parse");

        assert_eq!(payload.source_file, "/tmp/book-a.cbz");
        assert_eq!(payload.series_id, "series-1");
        assert_eq!(payload.copy_mode, ImportBookCopyMode::Hardlink);
        assert_eq!(payload.destination_name.as_deref(), Some("dest-a"));
        assert_eq!(payload.upgrade_book_id.as_deref(), Some("book-1"));
    }

    #[test]
    fn import_book_payload_reads_legacy_flat_and_nested_task_payloads() {
        let flat_record = TaskQueueRecord::new("ImportBook:task-1", 100, None)
            .with_simple_type("ImportBook")
            .with_payload(
                serde_json::json!({
                    "sourceFile": "/tmp/book-a.cbz",
                    "seriesId": "series-1",
                    "copyMode": "COPY",
                    "destinationName": "dest-a",
                    "upgradeBookId": "book-1",
                    "priority": 100,
                    "groupId": "series-1",
                    "uniqueId": "ImportBook:task-1"
                })
                .to_string(),
            );
        let nested_record = TaskQueueRecord::new("ImportBook:task-2", 100, None)
            .with_simple_type("ImportBook")
            .with_payload(
                serde_json::json!({
                    "copy_mode": "HARDLINK",
                    "book": {
                        "source_file": "/tmp/book-b.cbz",
                        "series_id": "series-2",
                        "destination_name": null,
                        "upgrade_book_id": null
                    }
                })
                .to_string(),
            );

        let flat =
            ImportBookPayload::from_task_record(&flat_record).expect("flat payload should parse");
        let nested = ImportBookPayload::from_task_record(&nested_record)
            .expect("nested payload should parse");

        assert_eq!(flat.source_file, "/tmp/book-a.cbz");
        assert_eq!(flat.series_id, "series-1");
        assert_eq!(flat.copy_mode, ImportBookCopyMode::Copy);
        assert_eq!(flat.destination_name.as_deref(), Some("dest-a"));
        assert_eq!(flat.upgrade_book_id.as_deref(), Some("book-1"));
        assert_eq!(nested.source_file, "/tmp/book-b.cbz");
        assert_eq!(nested.series_id, "series-2");
        assert_eq!(nested.copy_mode, ImportBookCopyMode::Hardlink);
        assert_eq!(nested.destination_name, None);
        assert_eq!(nested.upgrade_book_id, None);
    }

    #[test]
    fn import_book_payload_rejects_invalid_flat_payload_without_nested_fallback() {
        let record = TaskQueueRecord::new("ImportBook:task-1", 100, None)
            .with_simple_type("ImportBook")
            .with_payload(
                serde_json::json!({
                    "sourceFile": "/tmp/book-a.cbz",
                    "seriesId": "series-1",
                    "copyMode": "LINK",
                    "copy_mode": "COPY",
                    "book": {
                        "source_file": "/tmp/book-b.cbz",
                        "series_id": "series-2"
                    }
                })
                .to_string(),
            );

        let error = ImportBookPayload::from_task_record(&record)
            .expect_err("invalid flat ImportBook payload should not fall back to nested payload");

        assert!(error.message.contains("copyMode"), "{error}");
    }

    #[test]
    fn scan_library_payload_prefers_payload_over_legacy_task_target() {
        let record = TaskRequest::with_payload(
            TaskKind::ScanLibrary,
            ScanLibraryPayload::new("library-1", false),
        )
        .into_queue_record_with_id("missing-library_DEEP_true");

        let payload = ScanLibraryPayload::from_task_record(&record)
            .expect("ScanLibrary payload should parse from its queue record");

        assert_eq!(payload.library_id, "library-1");
        assert!(!payload.deep_scan);
    }

    #[test]
    fn scan_library_payload_rejects_invalid_persisted_payload_fields() {
        let cases = [
            (r#"{"libraryId":42,"scanDeep":true}"#, "libraryId"),
            (r#"{"libraryId":"library-1","scanDeep":"true"}"#, "scanDeep"),
        ];

        for (payload, expected_error) in cases {
            let record = TaskQueueRecord::new("ScanLibrary_fallback_DEEP_true", 900, None)
                .with_simple_type("ScanLibrary")
                .with_payload(payload);

            let error = ScanLibraryPayload::from_task_record(&record)
                .expect_err("invalid persisted scan-library payload should fail");

            assert!(error.message.contains(expected_error), "{error}");
        }
    }

    #[test]
    fn scan_library_payload_reads_legacy_deep_task_target_without_payload() {
        let record = TaskQueueRecord::new("ScanLibrary_library-1_DEEP_true", 900, None)
            .with_simple_type("ScanLibrary");

        let payload = ScanLibraryPayload::from_task_record(&record)
            .expect("legacy ScanLibrary task target should parse");

        assert_eq!(payload.library_id, "library-1");
        assert!(payload.deep_scan);
    }

    #[test]
    fn scan_library_payload_rejects_invalid_legacy_deep_task_target() {
        let record = TaskQueueRecord::new("ScanLibrary_library-1_DEEP_maybe", 900, None)
            .with_simple_type("ScanLibrary");

        let error = ScanLibraryPayload::from_task_record(&record)
            .expect_err("invalid legacy ScanLibrary deep flag should fail");

        assert!(error.message.contains("deep"), "{error}");
    }

    #[test]
    fn rebuild_index_payload_parses_kotlin_entity_selectors() {
        let record = TaskQueueRecord::new("RebuildIndex", 8, None)
            .with_simple_type("RebuildIndex")
            .with_payload(r#"{"entities":["Collection",{"type":"Series"},"Collection"]}"#);

        let payload = RebuildIndexPayload::from_task_record(&record)
            .expect("RebuildIndex payload should parse from its queue record");

        assert_eq!(
            payload.entities,
            Some(vec![
                RebuildIndexEntity::Collection,
                RebuildIndexEntity::Series
            ])
        );
    }

    #[test]
    fn rebuild_index_payload_rejects_non_object_payload() {
        let record = TaskQueueRecord::new("RebuildIndex", 8, None)
            .with_simple_type("RebuildIndex")
            .with_payload(r#"["Book"]"#);

        let error = RebuildIndexPayload::from_task_record(&record)
            .expect_err("persisted rebuild-index payload must be a JSON object");

        assert!(error.message.contains("JSON object"), "{error}");
    }

    #[test]
    fn thumbnail_regeneration_payload_accepts_legacy_flags() {
        let record = TaskQueueRecord::new("FindBookThumbnailsToRegenerate", 0, None)
            .with_simple_type("FindBookThumbnailsToRegenerate")
            .with_payload(r#"{"for_bigger_result_only":true}"#);

        let payload = FindBookThumbnailsToRegeneratePayload::from_task_record(&record)
            .expect("thumbnail regeneration payload should parse legacy snake-case flag");

        assert!(payload.for_bigger_result_only);
    }

    #[test]
    fn thumbnail_regeneration_payload_rejects_invalid_persisted_payload() {
        let record = TaskQueueRecord::new("FindBookThumbnailsToRegenerate", 0, None)
            .with_simple_type("FindBookThumbnailsToRegenerate")
            .with_payload(r#"{"forBiggerResultOnly":"true"}"#);

        let error = FindBookThumbnailsToRegeneratePayload::from_task_record(&record)
            .expect_err("invalid persisted thumbnail regeneration payload should fail");

        assert!(error.message.contains("forBiggerResultOnly"));
    }

    #[test]
    fn refresh_book_metadata_payload_restores_default_capabilities_for_execution() {
        let record = TaskQueueRecord::new("RefreshBookMetadata_book-1", 5, None)
            .with_simple_type("RefreshBookMetadata")
            .with_payload(r#"{"bookId":"book-1"}"#);

        let payload = RefreshBookMetadataPayload::from_task_record(&record, "book-1")
            .expect("RefreshBookMetadata payload should parse from its queue record");

        assert!(payload.capabilities_for_execution().contains("TITLE"));
        assert!(payload.capabilities_for_execution().contains("AUTHORS"));
    }

    #[test]
    fn refresh_book_metadata_payload_rejects_invalid_persisted_capabilities() {
        let record = TaskQueueRecord::new("RefreshBookMetadata_book-1", 5, None)
            .with_simple_type("RefreshBookMetadata")
            .with_payload(r#"{"bookId":"book-1","capabilities":["TITLE",42]}"#);

        let error = RefreshBookMetadataPayload::from_task_record(&record, "book-1")
            .expect_err("invalid persisted metadata capabilities should fail");

        assert!(error.message.contains("capabilities"));
    }

    #[test]
    fn refresh_book_metadata_payload_rejects_mismatched_book_id() {
        let record = TaskQueueRecord::new("RefreshBookMetadata_book-1", 5, None)
            .with_simple_type("RefreshBookMetadata")
            .with_payload(r#"{"bookId":"book-2","capabilities":["TITLE"]}"#);

        let error = RefreshBookMetadataPayload::from_task_record(&record, "book-1")
            .expect_err("persisted metadata payload book id must match the task target");

        assert!(error.message.contains("bookId"));
    }

    #[test]
    fn task_request_into_queue_record_with_id() {
        let request = TaskRequest::new(TaskKind::EmptyTrash);
        let record = request.into_queue_record_with_id("library-1");
        assert_eq!(record.id, "EmptyTrash_library-1");
        assert_eq!(record.simple_type, "EmptyTrash");
    }
}
