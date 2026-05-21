use komga_application::task_processing::{TaskKind, TaskQueueRecord};
use serde_json::{Map, Value, json};

#[derive(Clone, Debug)]
pub(super) struct PersistedTaskStoreRecord {
    pub id: String,
    pub simple_type: String,
    pub priority: i32,
    pub group: Option<String>,
    pub payload: Option<String>,
    pub owner: Option<String>,
}

pub(super) fn task_target_from_id<'a>(id: &'a str, simple_type: &str) -> Option<&'a str> {
    id.strip_prefix(simple_type).and_then(|suffix| {
        suffix
            .strip_prefix(':')
            .or_else(|| suffix.strip_prefix('_'))
    })
}

pub(super) fn task_target(task: &TaskQueueRecord) -> Option<&str> {
    task_target_from_id(&task.id, &task.simple_type)
}

pub(super) fn runtime_task_class_name(simple_type: &str) -> String {
    format!(
        "org.gotson.komga.task.{}.RuntimeTask",
        simple_type.to_ascii_lowercase()
    )
}

pub(super) fn persisted_payload_for_known_task(
    kind: TaskKind,
    task: &PersistedTaskStoreRecord,
) -> String {
    compatibility_payload(kind, task)
        .or_else(|| task.payload.clone())
        .unwrap_or_else(|| fallback_task_payload(task))
}

pub(super) fn fallback_task_payload(task: &PersistedTaskStoreRecord) -> String {
    json!({
        "id": task.id,
        "simpleType": task.simple_type,
        "priority": task.priority,
        "groupId": task.group,
    })
    .to_string()
}

fn persisted_task_target(task: &PersistedTaskStoreRecord) -> Option<&str> {
    task_target_from_id(&task.id, &task.simple_type)
}

fn optional_string_value(value: Option<&str>) -> Value {
    value
        .map(|value| Value::String(value.to_string()))
        .unwrap_or(Value::Null)
}

fn task_group_value(task: &PersistedTaskStoreRecord) -> Value {
    task.group.clone().map(Value::String).unwrap_or(Value::Null)
}

fn task_payload(
    task: &PersistedTaskStoreRecord,
    fields: impl IntoIterator<Item = (&'static str, Value)>,
) -> String {
    let mut payload = Map::new();
    for (key, value) in fields {
        payload.insert(key.to_string(), value);
    }
    payload.insert("priority".to_string(), Value::from(task.priority));
    payload.insert("groupId".to_string(), task_group_value(task));
    payload.insert("uniqueId".to_string(), Value::String(task.id.clone()));
    Value::Object(payload).to_string()
}

fn payload_contains_key(task: &PersistedTaskStoreRecord, key: &str) -> bool {
    payload_json(task)
        .as_ref()
        .and_then(Value::as_object)
        .is_some_and(|payload| payload.contains_key(key))
}

fn payload_json(task: &PersistedTaskStoreRecord) -> Option<Value> {
    task.payload
        .as_deref()
        .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
}

fn legacy_bool_payload_value(
    task: &PersistedTaskStoreRecord,
    primary_key: &str,
    fallback_key: &str,
) -> bool {
    payload_json(task)
        .as_ref()
        .and_then(|payload| {
            payload
                .get(primary_key)
                .or_else(|| payload.get(fallback_key))
        })
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn scan_library_target(task: &PersistedTaskStoreRecord) -> Option<String> {
    payload_json(task)
        .as_ref()
        .and_then(|payload| payload.get("libraryId"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            persisted_task_target(task).map(|target| {
                target
                    .split_once("_DEEP_")
                    .map(|(library_id, _)| library_id)
                    .unwrap_or(target)
                    .to_string()
            })
        })
}

fn scan_library_deep(task: &PersistedTaskStoreRecord) -> bool {
    payload_json(task)
        .as_ref()
        .and_then(|payload| payload.get("scanDeep").or_else(|| payload.get("deep")))
        .and_then(Value::as_bool)
        .or_else(|| {
            task.id
                .rsplit_once("_DEEP_")
                .and_then(|(_, deep_scan)| deep_scan.parse::<bool>().ok())
        })
        .unwrap_or(false)
}

fn target_payload(
    record: &PersistedTaskStoreRecord,
    simple_type: &str,
    target_key: &'static str,
) -> Option<String> {
    (record.simple_type == simple_type).then_some(())?;
    if payload_contains_key(record, target_key) {
        return record.payload.clone();
    }
    Some(task_payload(
        record,
        [(
            target_key,
            optional_string_value(persisted_task_target(record)),
        )],
    ))
}

fn scan_library_payload(record: &PersistedTaskStoreRecord) -> Option<String> {
    (record.simple_type == "ScanLibrary").then_some(())?;
    let library_id = scan_library_target(record)?;
    Some(task_payload(
        record,
        [
            ("libraryId", Value::String(library_id)),
            ("scanDeep", Value::Bool(scan_library_deep(record))),
        ],
    ))
}

fn import_book_payload(record: &PersistedTaskStoreRecord) -> Option<String> {
    (record.simple_type == "ImportBook").then_some(())?;
    let payload = payload_json(record)?;
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

fn find_book_thumbnails_to_regenerate_payload(record: &PersistedTaskStoreRecord) -> Option<String> {
    (record.simple_type == "FindBookThumbnailsToRegenerate").then(|| {
        task_payload(
            record,
            [(
                "forBiggerResultOnly",
                Value::Bool(legacy_bool_payload_value(
                    record,
                    "for_bigger_result_only",
                    "forBiggerResultOnly",
                )),
            )],
        )
    })
}

fn refresh_book_metadata_payload(record: &PersistedTaskStoreRecord) -> Option<String> {
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
            "bookId": persisted_task_target(record),
            "capabilities": capabilities,
            "priority": record.priority,
            "groupId": record.group,
            "uniqueId": record.id,
        })
        .to_string()
    })
}

fn rebuild_index_payload(record: &PersistedTaskStoreRecord) -> Option<String> {
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

fn upgrade_index_payload(record: &PersistedTaskStoreRecord) -> Option<String> {
    (record.simple_type == "UpgradeIndex")
        .then(|| task_payload(record, std::iter::empty::<(&'static str, Value)>()))
}

fn compatibility_payload(kind: TaskKind, record: &PersistedTaskStoreRecord) -> Option<String> {
    match kind {
        TaskKind::ScanLibrary => scan_library_payload(record),
        TaskKind::ImportBook => import_book_payload(record),
        TaskKind::RefreshBookMetadata => refresh_book_metadata_payload(record),
        TaskKind::FindBookThumbnailsToRegenerate => {
            find_book_thumbnails_to_regenerate_payload(record)
        }
        TaskKind::RebuildIndex => rebuild_index_payload(record),
        TaskKind::UpgradeIndex => upgrade_index_payload(record),
        TaskKind::RemoveHashedPages => None,
        _ => kind
            .compat_target_key()
            .and_then(|key| target_payload(record, kind.simple_type(), key)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            compatibility_payload(kind, record).expect("known task should produce payload");
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
