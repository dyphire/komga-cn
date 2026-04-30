use komga_application::task_processing::{TaskKind, TaskQueueRecord};
use serde_json::{Value, json};

use super::queue_core::PersistedTaskStoreRecord;
use super::{TaskExecutionError, TaskExecutionOutcome};

pub(crate) trait TaskHandler {
    fn compatibility_payload(record: &PersistedTaskStoreRecord) -> Option<String>;

    #[allow(dead_code)]
    async fn execute(
        runtime: &super::TaskRuntimeContext,
        task: &TaskQueueRecord,
        task_target: Option<&str>,
    ) -> Result<TaskExecutionOutcome, TaskExecutionError>;
}

macro_rules! impl_target_payload {
    ($handler:ident, $simple_type:expr, $target_key:expr) => {
        impl TaskHandler for $handler {
            fn compatibility_payload(record: &PersistedTaskStoreRecord) -> Option<String> {
                (record.simple_type == $simple_type).then_some(())?;
                if super::queue_core::payload_contains_key(record, $target_key) {
                    return record.payload.clone();
                }
                Some(super::queue_core::task_payload(
                    record,
                    [(
                        $target_key,
                        super::queue_core::optional_string_value(
                            super::queue_core::persisted_task_target(record),
                        ),
                    )],
                ))
            }

            /// Execute the task. Currently stub; will be wired to real execution in a future phase.
            #[allow(dead_code)]
            async fn execute(
                _runtime: &super::TaskRuntimeContext,
                _task: &TaskQueueRecord,
                _task_target: Option<&str>,
            ) -> Result<TaskExecutionOutcome, TaskExecutionError> {
                Ok(TaskExecutionOutcome::completed())
            }
        }
    };
}

pub struct ScanLibraryHandler;
pub struct EmptyTrashHandler;
pub struct AnalyzeBookHandler;
pub struct ImportBookHandler;
pub struct FindBooksWithMissingPageHashHandler;
pub struct FindDuplicatePagesToDeleteHandler;
pub struct FindBookThumbnailsToRegenerateHandler;
pub struct RefreshBookMetadataHandler;
pub struct RefreshBookLocalArtworkHandler;
pub struct RefreshSeriesLocalArtworkHandler;
pub struct RefreshSeriesMetadataHandler;
pub struct AggregateSeriesMetadataHandler;
pub struct RepairExtensionHandler;
pub struct GenerateBookThumbnailHandler;
pub struct HashBookHandler;
pub struct HashBookKoreaderHandler;
pub struct HashBookPagesHandler;
pub struct RebuildIndexHandler;
pub struct UpgradeIndexHandler;
pub struct RemoveHashedPagesHandler;
pub struct DeleteBookHandler;
pub struct DeleteSeriesHandler;
pub struct FindBooksToConvertHandler;
pub struct ConvertBookHandler;

impl TaskHandler for ScanLibraryHandler {
    fn compatibility_payload(record: &PersistedTaskStoreRecord) -> Option<String> {
        (record.simple_type == "ScanLibrary").then_some(())?;
        let library_id = super::queue_core::scan_library_target(record)?;
        Some(super::queue_core::task_payload(
            record,
            [
                ("libraryId", Value::String(library_id)),
                (
                    "scanDeep",
                    Value::Bool(super::queue_core::scan_library_deep(record)),
                ),
            ],
        ))
    }

    async fn execute(
        _runtime: &super::TaskRuntimeContext,
        _task: &TaskQueueRecord,
        _task_target: Option<&str>,
    ) -> Result<TaskExecutionOutcome, TaskExecutionError> {
        Ok(TaskExecutionOutcome::completed())
    }
}

impl TaskHandler for ImportBookHandler {
    fn compatibility_payload(record: &PersistedTaskStoreRecord) -> Option<String> {
        (record.simple_type == "ImportBook").then_some(())?;
        let payload = super::queue_core::payload_json(record)?;
        let source_file = payload
            .get("sourceFile")
            .or_else(|| payload.get("book").and_then(|book| book.get("source_file")))
            .and_then(Value::as_str)?;
        let series_id = payload
            .get("seriesId")
            .or_else(|| payload.get("book").and_then(|book| book.get("series_id")))
            .and_then(Value::as_str)?;
        let copy_mode = payload
            .get("copyMode")
            .or_else(|| payload.get("copy_mode"))
            .and_then(Value::as_str)?;
        let destination_name = payload
            .get("destinationName")
            .or_else(|| {
                payload
                    .get("book")
                    .and_then(|book| book.get("destination_name"))
            })
            .cloned()
            .unwrap_or(Value::Null);
        let upgrade_book_id = payload
            .get("upgradeBookId")
            .or_else(|| {
                payload
                    .get("book")
                    .and_then(|book| book.get("upgrade_book_id"))
            })
            .cloned()
            .unwrap_or(Value::Null);

        Some(
            json!({
                "sourceFile": source_file,
                "seriesId": series_id,
                "copyMode": copy_mode,
                "destinationName": destination_name,
                "upgradeBookId": upgrade_book_id,
                "priority": record.priority,
                "groupId": record.group,
                "uniqueId": record.id,
            })
            .to_string(),
        )
    }

    async fn execute(
        _runtime: &super::TaskRuntimeContext,
        _task: &TaskQueueRecord,
        _task_target: Option<&str>,
    ) -> Result<TaskExecutionOutcome, TaskExecutionError> {
        Ok(TaskExecutionOutcome::completed())
    }
}

impl TaskHandler for FindBookThumbnailsToRegenerateHandler {
    fn compatibility_payload(record: &PersistedTaskStoreRecord) -> Option<String> {
        (record.simple_type == "FindBookThumbnailsToRegenerate").then(|| {
            super::queue_core::task_payload(
                record,
                [(
                    "forBiggerResultOnly",
                    Value::Bool(super::queue_core::legacy_bool_payload_value(
                        record,
                        "for_bigger_result_only",
                        "forBiggerResultOnly",
                    )),
                )],
            )
        })
    }

    async fn execute(
        _runtime: &super::TaskRuntimeContext,
        _task: &TaskQueueRecord,
        _task_target: Option<&str>,
    ) -> Result<TaskExecutionOutcome, TaskExecutionError> {
        Ok(TaskExecutionOutcome::completed())
    }
}

impl TaskHandler for RefreshBookMetadataHandler {
    fn compatibility_payload(record: &PersistedTaskStoreRecord) -> Option<String> {
        (record.simple_type == "RefreshBookMetadata").then(|| {
            let capabilities = record
                .payload
                .as_deref()
                .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
                .and_then(|payload| payload.get("capabilities").cloned())
                .and_then(|capabilities| capabilities.as_array().cloned())
                .unwrap_or_else(|| {
                    vec![
                        Value::String("TITLE".to_string()),
                        Value::String("SUMMARY".to_string()),
                        Value::String("NUMBER".to_string()),
                        Value::String("NUMBER_SORT".to_string()),
                        Value::String("RELEASE_DATE".to_string()),
                        Value::String("AUTHORS".to_string()),
                        Value::String("TAGS".to_string()),
                        Value::String("ISBN".to_string()),
                        Value::String("READ_LISTS".to_string()),
                        Value::String("THUMBNAILS".to_string()),
                        Value::String("LINKS".to_string()),
                    ]
                });
            json!({
                "bookId": super::queue_core::persisted_task_target(record),
                "capabilities": capabilities,
                "priority": record.priority,
                "groupId": record.group,
                "uniqueId": record.id,
            })
            .to_string()
        })
    }

    async fn execute(
        _runtime: &super::TaskRuntimeContext,
        _task: &TaskQueueRecord,
        _task_target: Option<&str>,
    ) -> Result<TaskExecutionOutcome, TaskExecutionError> {
        Ok(TaskExecutionOutcome::completed())
    }
}

impl TaskHandler for RebuildIndexHandler {
    fn compatibility_payload(record: &PersistedTaskStoreRecord) -> Option<String> {
        (record.simple_type == "RebuildIndex").then(|| {
            let entities = record
                .payload
                .as_deref()
                .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
                .and_then(|payload| payload.get("entities").cloned())
                .unwrap_or(Value::Null);
            json!({
                "entities": entities,
                "priority": record.priority,
                "groupId": record.group,
                "uniqueId": record.id,
            })
            .to_string()
        })
    }

    async fn execute(
        _runtime: &super::TaskRuntimeContext,
        _task: &TaskQueueRecord,
        _task_target: Option<&str>,
    ) -> Result<TaskExecutionOutcome, TaskExecutionError> {
        Ok(TaskExecutionOutcome::completed())
    }
}

impl TaskHandler for UpgradeIndexHandler {
    fn compatibility_payload(record: &PersistedTaskStoreRecord) -> Option<String> {
        (record.simple_type == "UpgradeIndex").then(|| {
            super::queue_core::task_payload(record, std::iter::empty::<(&'static str, Value)>())
        })
    }

    async fn execute(
        _runtime: &super::TaskRuntimeContext,
        _task: &TaskQueueRecord,
        _task_target: Option<&str>,
    ) -> Result<TaskExecutionOutcome, TaskExecutionError> {
        Ok(TaskExecutionOutcome::completed())
    }
}

impl_target_payload!(EmptyTrashHandler, "EmptyTrash", "libraryId");
impl_target_payload!(AnalyzeBookHandler, "AnalyzeBook", "bookId");
impl_target_payload!(
    FindBooksWithMissingPageHashHandler,
    "FindBooksWithMissingPageHash",
    "libraryId"
);
impl_target_payload!(
    FindDuplicatePagesToDeleteHandler,
    "FindDuplicatePagesToDelete",
    "libraryId"
);
impl_target_payload!(
    RefreshBookLocalArtworkHandler,
    "RefreshBookLocalArtwork",
    "bookId"
);
impl_target_payload!(
    RefreshSeriesMetadataHandler,
    "RefreshSeriesMetadata",
    "seriesId"
);
impl_target_payload!(
    AggregateSeriesMetadataHandler,
    "AggregateSeriesMetadata",
    "seriesId"
);
impl_target_payload!(
    RefreshSeriesLocalArtworkHandler,
    "RefreshSeriesLocalArtwork",
    "seriesId"
);
impl_target_payload!(RepairExtensionHandler, "RepairExtension", "bookId");
impl_target_payload!(
    GenerateBookThumbnailHandler,
    "GenerateBookThumbnail",
    "bookId"
);
impl_target_payload!(HashBookHandler, "HashBook", "bookId");
impl_target_payload!(HashBookKoreaderHandler, "HashBookKoreader", "bookId");
impl_target_payload!(HashBookPagesHandler, "HashBookPages", "bookId");
impl_target_payload!(DeleteBookHandler, "DeleteBook", "bookId");
impl_target_payload!(DeleteSeriesHandler, "DeleteSeries", "seriesId");
impl_target_payload!(FindBooksToConvertHandler, "FindBooksToConvert", "libraryId");
impl_target_payload!(ConvertBookHandler, "ConvertBook", "bookId");

impl TaskHandler for RemoveHashedPagesHandler {
    fn compatibility_payload(_record: &PersistedTaskStoreRecord) -> Option<String> {
        None
    }

    async fn execute(
        _runtime: &super::TaskRuntimeContext,
        _task: &TaskQueueRecord,
        _task_target: Option<&str>,
    ) -> Result<TaskExecutionOutcome, TaskExecutionError> {
        Ok(TaskExecutionOutcome::completed())
    }
}

pub fn compatibility_payload(kind: TaskKind, record: &PersistedTaskStoreRecord) -> Option<String> {
    match kind {
        TaskKind::ScanLibrary => ScanLibraryHandler::compatibility_payload(record),
        TaskKind::EmptyTrash => EmptyTrashHandler::compatibility_payload(record),
        TaskKind::AnalyzeBook => AnalyzeBookHandler::compatibility_payload(record),
        TaskKind::ImportBook => ImportBookHandler::compatibility_payload(record),
        TaskKind::FindBooksWithMissingPageHash => {
            FindBooksWithMissingPageHashHandler::compatibility_payload(record)
        }
        TaskKind::FindDuplicatePagesToDelete => {
            FindDuplicatePagesToDeleteHandler::compatibility_payload(record)
        }
        TaskKind::FindBookThumbnailsToRegenerate => {
            FindBookThumbnailsToRegenerateHandler::compatibility_payload(record)
        }
        TaskKind::RefreshBookMetadata => RefreshBookMetadataHandler::compatibility_payload(record),
        TaskKind::RefreshBookLocalArtwork => {
            RefreshBookLocalArtworkHandler::compatibility_payload(record)
        }
        TaskKind::RefreshSeriesMetadata => {
            RefreshSeriesMetadataHandler::compatibility_payload(record)
        }
        TaskKind::AggregateSeriesMetadata => {
            AggregateSeriesMetadataHandler::compatibility_payload(record)
        }
        TaskKind::RefreshSeriesLocalArtwork => {
            RefreshSeriesLocalArtworkHandler::compatibility_payload(record)
        }
        TaskKind::RepairExtension => RepairExtensionHandler::compatibility_payload(record),
        TaskKind::GenerateBookThumbnail => {
            GenerateBookThumbnailHandler::compatibility_payload(record)
        }
        TaskKind::HashBook => HashBookHandler::compatibility_payload(record),
        TaskKind::HashBookKoreader => HashBookKoreaderHandler::compatibility_payload(record),
        TaskKind::HashBookPages => HashBookPagesHandler::compatibility_payload(record),
        TaskKind::RebuildIndex => RebuildIndexHandler::compatibility_payload(record),
        TaskKind::UpgradeIndex => UpgradeIndexHandler::compatibility_payload(record),
        TaskKind::RemoveHashedPages => RemoveHashedPagesHandler::compatibility_payload(record),
        TaskKind::DeleteBook => DeleteBookHandler::compatibility_payload(record),
        TaskKind::DeleteSeries => DeleteSeriesHandler::compatibility_payload(record),
        TaskKind::FindBooksToConvert => FindBooksToConvertHandler::compatibility_payload(record),
        TaskKind::ConvertBook => ConvertBookHandler::compatibility_payload(record),
    }
}

pub(crate) async fn execute(
    runtime: &super::TaskRuntimeContext,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    if let Some(result) = super::scanner_jobs::try_execute(runtime, task, task_target).await {
        return result;
    }
    if let Some(result) = super::maintenance_jobs::try_execute(runtime, task, task_target).await {
        return result;
    }
    if let Some(result) = super::index_jobs::try_execute(runtime, task, task_target).await {
        return result;
    }
    if let Some(result) = super::import_jobs::try_execute(runtime, task).await {
        return result;
    }

    Err(TaskExecutionError::unsupported_task(&task.simple_type))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn record(
        id: &str,
        simple_type: &str,
        priority: i32,
        group: Option<&str>,
    ) -> PersistedTaskStoreRecord {
        PersistedTaskStoreRecord {
            id: id.to_string(),
            simple_type: simple_type.to_string(),
            priority,
            group: group.map(str::to_string),
            payload: None,
            owner: None,
        }
    }

    fn record_with_payload(
        id: &str,
        simple_type: &str,
        priority: i32,
        group: Option<&str>,
        payload: Value,
    ) -> PersistedTaskStoreRecord {
        PersistedTaskStoreRecord {
            id: id.to_string(),
            simple_type: simple_type.to_string(),
            priority,
            group: group.map(str::to_string),
            payload: Some(payload.to_string()),
            owner: None,
        }
    }

    fn parse_payload(record: &PersistedTaskStoreRecord) -> Value {
        let kind = TaskKind::parse(&record.simple_type).expect("known task kind");
        let payload_str =
            compatibility_payload(kind, record).expect("handler should return Some payload");
        serde_json::from_str(&payload_str).expect("payload should be valid JSON")
    }

    #[test]
    fn find_duplicate_pages_to_delete_payload() {
        let r = record(
            "FindDuplicatePagesToDelete_library-1",
            "FindDuplicatePagesToDelete",
            42,
            None,
        );
        let payload = parse_payload(&r);
        assert_eq!(payload["libraryId"], "library-1");
        assert_eq!(payload["priority"], 42);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(payload["uniqueId"], "FindDuplicatePagesToDelete_library-1");
    }

    #[test]
    fn find_books_to_convert_payload() {
        let r = record(
            "FindBooksToConvert_library-1",
            "FindBooksToConvert",
            0,
            None,
        );
        let payload = parse_payload(&r);
        assert_eq!(payload["libraryId"], "library-1");
        assert_eq!(payload["priority"], 0);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(payload["uniqueId"], "FindBooksToConvert_library-1");
    }

    #[test]
    fn rebuild_index_payload() {
        let r = record_with_payload(
            "RebuildIndex",
            "RebuildIndex",
            8,
            None,
            json!({ "entities": ["Collection"] }),
        );
        let payload = parse_payload(&r);
        assert_eq!(payload["entities"], json!(["Collection"]));
        assert_eq!(payload["priority"], 8);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(payload["uniqueId"], "RebuildIndex");
    }

    #[test]
    fn refresh_series_metadata_payload() {
        let r = record(
            "RefreshSeriesMetadata_series-1",
            "RefreshSeriesMetadata",
            5,
            Some("series-1"),
        );
        let payload = parse_payload(&r);
        assert_eq!(payload["seriesId"], "series-1");
        assert_eq!(payload["priority"], 5);
        assert_eq!(payload["groupId"], "series-1");
        assert_eq!(payload["uniqueId"], "RefreshSeriesMetadata_series-1");
    }

    #[test]
    fn aggregate_series_metadata_payload() {
        let r = record(
            "AggregateSeriesMetadata_series-1",
            "AggregateSeriesMetadata",
            6,
            Some("series-1"),
        );
        let payload = parse_payload(&r);
        assert_eq!(payload["seriesId"], "series-1");
        assert_eq!(payload["priority"], 6);
        assert_eq!(payload["groupId"], "series-1");
        assert_eq!(payload["uniqueId"], "AggregateSeriesMetadata_series-1");
    }

    #[test]
    fn upgrade_index_payload() {
        let r = record("UpgradeIndex", "UpgradeIndex", 9, None);
        let payload = parse_payload(&r);
        assert_eq!(payload["priority"], 9);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(payload["uniqueId"], "UpgradeIndex");
    }

    #[test]
    fn find_book_thumbnails_to_regenerate_payload() {
        let r = record_with_payload(
            "FindBookThumbnailsToRegenerate",
            "FindBookThumbnailsToRegenerate",
            0,
            None,
            json!({ "for_bigger_result_only": true }),
        );
        let payload = parse_payload(&r);
        assert_eq!(payload["forBiggerResultOnly"], true);
        assert_eq!(payload["priority"], 0);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(payload["uniqueId"], "FindBookThumbnailsToRegenerate");
    }

    #[test]
    fn import_book_payload() {
        let r = record_with_payload(
            "ImportBook:task-1",
            "ImportBook",
            100,
            Some("series-1"),
            json!({
                "copy_mode": "COPY",
                "book": {
                    "source_file": "/tmp/book.cbz",
                    "series_id": "series-1",
                    "destination_name": "dest-a",
                    "upgrade_book_id": "book-1"
                }
            }),
        );
        let payload = parse_payload(&r);
        assert_eq!(payload["sourceFile"], "/tmp/book.cbz");
        assert_eq!(payload["seriesId"], "series-1");
        assert_eq!(payload["copyMode"], "COPY");
        assert_eq!(payload["destinationName"], "dest-a");
        assert_eq!(payload["upgradeBookId"], "book-1");
        assert_eq!(payload["priority"], 100);
        assert_eq!(payload["groupId"], "series-1");
        assert_eq!(payload["uniqueId"], "ImportBook:task-1");
    }

    #[test]
    fn convert_book_payload() {
        let r = record("ConvertBook_book-1", "ConvertBook", 7, Some("series-1"));
        let payload = parse_payload(&r);
        assert_eq!(payload["bookId"], "book-1");
        assert_eq!(payload["priority"], 7);
        assert_eq!(payload["groupId"], "series-1");
        assert_eq!(payload["uniqueId"], "ConvertBook_book-1");
    }

    #[test]
    fn refresh_book_local_artwork_payload() {
        let r = record(
            "RefreshBookLocalArtwork_book-1",
            "RefreshBookLocalArtwork",
            80,
            None,
        );
        let payload = parse_payload(&r);
        assert_eq!(payload["bookId"], "book-1");
        assert_eq!(payload["priority"], 80);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(payload["uniqueId"], "RefreshBookLocalArtwork_book-1");
    }

    #[test]
    fn refresh_series_local_artwork_payload() {
        let r = record(
            "RefreshSeriesLocalArtwork_series-1",
            "RefreshSeriesLocalArtwork",
            80,
            None,
        );
        let payload = parse_payload(&r);
        assert_eq!(payload["seriesId"], "series-1");
        assert_eq!(payload["priority"], 80);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(payload["uniqueId"], "RefreshSeriesLocalArtwork_series-1");
    }

    #[test]
    fn repair_extension_payload() {
        let r = record(
            "RepairExtension_book-1",
            "RepairExtension",
            12,
            Some("series-1"),
        );
        let payload = parse_payload(&r);
        assert_eq!(payload["bookId"], "book-1");
        assert_eq!(payload["priority"], 12);
        assert_eq!(payload["groupId"], "series-1");
        assert_eq!(payload["uniqueId"], "RepairExtension_book-1");
    }

    #[test]
    fn hash_book_pages_payload() {
        let r = record("HashBookPages_book-1", "HashBookPages", 5, None);
        let payload = parse_payload(&r);
        assert_eq!(payload["bookId"], "book-1");
        assert_eq!(payload["priority"], 5);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(payload["uniqueId"], "HashBookPages_book-1");
    }

    #[test]
    fn hash_book_koreader_payload() {
        let r = record("HashBookKoreader_book-1", "HashBookKoreader", 5, None);
        let payload = parse_payload(&r);
        assert_eq!(payload["bookId"], "book-1");
        assert_eq!(payload["priority"], 5);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(payload["uniqueId"], "HashBookKoreader_book-1");
    }

    #[test]
    fn delete_book_payload() {
        let r = record("DeleteBook_book-1", "DeleteBook", 8, None);
        let payload = parse_payload(&r);
        assert_eq!(payload["bookId"], "book-1");
        assert_eq!(payload["priority"], 8);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(payload["uniqueId"], "DeleteBook_book-1");
    }

    #[test]
    fn delete_series_payload() {
        let r = record("DeleteSeries_series-1", "DeleteSeries", 8, None);
        let payload = parse_payload(&r);
        assert_eq!(payload["seriesId"], "series-1");
        assert_eq!(payload["priority"], 8);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(payload["uniqueId"], "DeleteSeries_series-1");
    }

    #[test]
    fn scan_library_payload() {
        let r = record("ScanLibrary_library-1", "ScanLibrary", 4, None);
        let payload = parse_payload(&r);
        assert_eq!(payload["libraryId"], "library-1");
        assert_eq!(payload["scanDeep"], false);
        assert_eq!(payload["priority"], 4);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(payload["uniqueId"], "ScanLibrary_library-1");
    }

    #[test]
    fn empty_trash_payload() {
        let r = record("EmptyTrash_library-1", "EmptyTrash", 6, None);
        let payload = parse_payload(&r);
        assert_eq!(payload["libraryId"], "library-1");
        assert_eq!(payload["priority"], 6);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(payload["uniqueId"], "EmptyTrash_library-1");
    }

    #[test]
    fn analyze_book_payload() {
        let r = record("AnalyzeBook_book-1", "AnalyzeBook", 6, None);
        let payload = parse_payload(&r);
        assert_eq!(payload["bookId"], "book-1");
        assert_eq!(payload["priority"], 6);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(payload["uniqueId"], "AnalyzeBook_book-1");
    }

    #[test]
    fn hash_book_payload() {
        let r = record("HashBook_book-1", "HashBook", 5, None);
        let payload = parse_payload(&r);
        assert_eq!(payload["bookId"], "book-1");
        assert_eq!(payload["priority"], 5);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(payload["uniqueId"], "HashBook_book-1");
    }

    #[test]
    fn generate_book_thumbnail_payload() {
        let r = record(
            "GenerateBookThumbnail_book-1",
            "GenerateBookThumbnail",
            4,
            None,
        );
        let payload = parse_payload(&r);
        assert_eq!(payload["bookId"], "book-1");
        assert_eq!(payload["priority"], 4);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(payload["uniqueId"], "GenerateBookThumbnail_book-1");
    }

    #[test]
    fn find_books_with_missing_page_hash_payload() {
        let r = record(
            "FindBooksWithMissingPageHash_library-1",
            "FindBooksWithMissingPageHash",
            4,
            None,
        );
        let payload = parse_payload(&r);
        assert_eq!(payload["libraryId"], "library-1");
        assert_eq!(payload["priority"], 4);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(
            payload["uniqueId"],
            "FindBooksWithMissingPageHash_library-1"
        );
    }

    #[test]
    fn refresh_book_metadata_payload() {
        let r = record("RefreshBookMetadata_book-1", "RefreshBookMetadata", 5, None);
        let payload = parse_payload(&r);
        assert_eq!(payload["bookId"], "book-1");
        assert_eq!(payload["priority"], 5);
        assert_eq!(payload["groupId"], Value::Null);
        assert_eq!(payload["uniqueId"], "RefreshBookMetadata_book-1");
        let capabilities = payload["capabilities"]
            .as_array()
            .expect("capabilities should be array");
        assert!(capabilities.contains(&Value::String("TITLE".to_string())));
        assert!(capabilities.contains(&Value::String("AUTHORS".to_string())));
    }

    #[test]
    fn remove_hashed_pages_returns_none() {
        let r = record("RemoveHashedPages_book-1", "RemoveHashedPages", 5, None);
        let kind = TaskKind::parse("RemoveHashedPages").unwrap();
        assert!(compatibility_payload(kind, &r).is_none());
    }
}
