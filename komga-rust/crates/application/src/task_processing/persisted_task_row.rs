use serde_json::{Map, Value, json};

use super::{
    FindBookThumbnailsToRegeneratePayload, ImportBookPayload, RebuildIndexPayload,
    RefreshBookMetadataPayload, ScanLibraryPayload, TaskKind, TaskPayload, TaskProcessingError,
    TaskQueueRecord,
};

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

impl PersistedTaskRowShape {
    pub fn from_queue_record(record: TaskQueueRecord) -> Result<Self, TaskProcessingError> {
        if let Ok(kind) = TaskKind::parse(&record.simple_type) {
            return known_task_persisted_row(kind, record);
        }

        Ok(Self {
            id: record.id.clone(),
            priority: record.priority,
            group: record.group.clone(),
            class_name: runtime_task_class_name(record.simple_type.as_str()),
            simple_type: record.simple_type.clone(),
            payload: fallback_task_payload(&record),
            owner: record.owner.clone(),
        })
    }

    pub fn into_queue_record(self) -> TaskQueueRecord {
        let simple_type = TaskKind::parse(&self.simple_type)
            .map(|kind| kind.simple_type().to_string())
            .unwrap_or_else(|_| self.simple_type.clone());
        let mut record =
            TaskQueueRecord::new(self.id, self.priority, self.group).with_simple_type(simple_type);
        record = record.with_payload(self.payload);
        record.owner = self.owner;
        record
    }
}

fn known_task_persisted_row(
    kind: TaskKind,
    record: TaskQueueRecord,
) -> Result<PersistedTaskRowShape, TaskProcessingError> {
    let def = kind.definition();
    Ok(PersistedTaskRowShape {
        id: record.id.clone(),
        priority: record.priority,
        group: record.group.clone(),
        class_name: def.persisted_class_name.to_string(),
        simple_type: def.simple_type.to_string(),
        payload: persisted_payload_for_known_task(kind, &record)?,
        owner: record.owner.clone(),
    })
}

fn runtime_task_class_name(simple_type: &str) -> String {
    format!(
        "org.gotson.komga.task.{}.RuntimeTask",
        simple_type.to_ascii_lowercase()
    )
}

fn persisted_payload_for_known_task(
    kind: TaskKind,
    record: &TaskQueueRecord,
) -> Result<String, TaskProcessingError> {
    let payload = compatibility_payload(kind, record)?
        .or_else(|| record.payload.clone())
        .unwrap_or_else(|| fallback_task_payload(record));
    Ok(payload)
}

fn fallback_task_payload(record: &TaskQueueRecord) -> String {
    json!({
        "id": record.id,
        "simpleType": record.simple_type,
        "priority": record.priority,
        "groupId": record.group,
    })
    .to_string()
}

fn optional_string_value(value: Option<&str>) -> Value {
    value
        .map(|value| Value::String(value.to_string()))
        .unwrap_or(Value::Null)
}

fn task_group_value(record: &TaskQueueRecord) -> Value {
    record
        .group
        .clone()
        .map(Value::String)
        .unwrap_or(Value::Null)
}

fn task_payload(
    record: &TaskQueueRecord,
    fields: impl IntoIterator<Item = (&'static str, Value)>,
) -> String {
    let mut payload = Map::new();
    for (key, value) in fields {
        payload.insert(key.to_string(), value);
    }
    payload.insert("priority".to_string(), Value::from(record.priority));
    payload.insert("groupId".to_string(), task_group_value(record));
    payload.insert("uniqueId".to_string(), Value::String(record.id.clone()));
    Value::Object(payload).to_string()
}

fn contract_task_payload(record: &TaskQueueRecord, task_payload: &impl TaskPayload) -> String {
    let mut payload = Map::new();
    task_payload.write_task_fields(&mut payload);
    payload.insert("priority".to_string(), Value::from(record.priority));
    payload.insert("groupId".to_string(), task_group_value(record));
    payload.insert("uniqueId".to_string(), Value::String(record.id.clone()));
    Value::Object(payload).to_string()
}

fn target_payload(record: &TaskQueueRecord, target_key: &'static str) -> String {
    task_payload(
        record,
        [(target_key, optional_string_value(record.target()))],
    )
}

fn scan_library_payload(record: &TaskQueueRecord) -> Result<Option<String>, TaskProcessingError> {
    let payload = ScanLibraryPayload::from_task_record(record)?;
    Ok(Some(task_payload(
        record,
        [
            ("libraryId", Value::String(payload.library_id)),
            ("scanDeep", Value::Bool(payload.deep_scan)),
        ],
    )))
}

fn import_book_payload(record: &TaskQueueRecord) -> Result<Option<String>, TaskProcessingError> {
    let payload = ImportBookPayload::from_task_record(record)?;
    Ok(Some(contract_task_payload(record, &payload)))
}

fn find_book_thumbnails_to_regenerate_payload(
    record: &TaskQueueRecord,
) -> Result<Option<String>, TaskProcessingError> {
    let payload = FindBookThumbnailsToRegeneratePayload::from_task_record(record)?;
    Ok(Some(contract_task_payload(record, &payload)))
}

fn refresh_book_metadata_payload(
    record: &TaskQueueRecord,
) -> Result<Option<String>, TaskProcessingError> {
    let book_id = record.target().ok_or_else(|| {
        TaskProcessingError::invalid_task("RefreshBookMetadata task must include a book id")
    })?;
    let payload =
        RefreshBookMetadataPayload::from_task_record(record, book_id)?.with_default_capabilities();
    Ok(Some(contract_task_payload(record, &payload)))
}

fn rebuild_index_payload(record: &TaskQueueRecord) -> Result<Option<String>, TaskProcessingError> {
    let payload = RebuildIndexPayload::from_task_record(record)?;
    Ok(Some(contract_task_payload(record, &payload)))
}

fn upgrade_index_payload(record: &TaskQueueRecord) -> Result<Option<String>, TaskProcessingError> {
    Ok(Some(task_payload(
        record,
        std::iter::empty::<(&'static str, Value)>(),
    )))
}

fn compatibility_payload(
    kind: TaskKind,
    record: &TaskQueueRecord,
) -> Result<Option<String>, TaskProcessingError> {
    match kind {
        TaskKind::ScanLibrary => scan_library_payload(record),
        TaskKind::ImportBook => import_book_payload(record),
        TaskKind::RefreshBookMetadata => refresh_book_metadata_payload(record),
        TaskKind::FindBookThumbnailsToRegenerate => {
            find_book_thumbnails_to_regenerate_payload(record)
        }
        TaskKind::RebuildIndex => rebuild_index_payload(record),
        TaskKind::UpgradeIndex => upgrade_index_payload(record),
        TaskKind::RemoveHashedPages => Ok(None),
        _ => kind
            .compat_target_key()
            .map(|key| target_payload(record, key))
            .map(Some)
            .ok_or_else(|| TaskProcessingError::invalid_task("known task has no payload shape")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_processing::{BookPayload, TaskRequest};
    use serde_json::json;

    #[test]
    fn known_task_persisted_row_uses_application_task_contract_shape() {
        let record = TaskRequest::new(TaskKind::AnalyzeBook).into_queue_record_with_id("book-1");

        let row = PersistedTaskRowShape::from_queue_record(record)
            .expect("known task should persist with Kotlin row shape");

        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$AnalyzeBook"
        );
        assert_eq!(row.simple_type, "AnalyzeBook");
        let payload = parse_payload(&row);
        assert_eq!(payload["bookId"], "book-1");
        assert_eq!(payload["uniqueId"], "AnalyzeBook_book-1");
    }

    #[test]
    fn known_target_tasks_restore_kotlin_payload_shape_without_payload() {
        let cases = [
            (TaskKind::AnalyzeBook, "book-1", "bookId", "book-1"),
            (TaskKind::EmptyTrash, "library-1", "libraryId", "library-1"),
            (
                TaskKind::FindBooksWithMissingPageHash,
                "library-1",
                "libraryId",
                "library-1",
            ),
            (
                TaskKind::FindDuplicatePagesToDelete,
                "library-1",
                "libraryId",
                "library-1",
            ),
            (
                TaskKind::RefreshBookLocalArtwork,
                "book-1",
                "bookId",
                "book-1",
            ),
            (
                TaskKind::RefreshSeriesLocalArtwork,
                "series-1",
                "seriesId",
                "series-1",
            ),
            (
                TaskKind::RefreshSeriesMetadata,
                "series-1",
                "seriesId",
                "series-1",
            ),
            (
                TaskKind::AggregateSeriesMetadata,
                "series-1",
                "seriesId",
                "series-1",
            ),
            (TaskKind::RepairExtension, "book-1", "bookId", "book-1"),
            (
                TaskKind::GenerateBookThumbnail,
                "book-1",
                "bookId",
                "book-1",
            ),
            (TaskKind::HashBook, "book-1", "bookId", "book-1"),
            (TaskKind::HashBookKoreader, "book-1", "bookId", "book-1"),
            (TaskKind::HashBookPages, "book-1", "bookId", "book-1"),
            (TaskKind::DeleteBook, "book-1", "bookId", "book-1"),
            (TaskKind::DeleteSeries, "series-1", "seriesId", "series-1"),
            (
                TaskKind::FindBooksToConvert,
                "library-1",
                "libraryId",
                "library-1",
            ),
            (TaskKind::ConvertBook, "book-1", "bookId", "book-1"),
        ];

        for (kind, target, payload_key, payload_value) in cases {
            let record = TaskRequest::new(kind)
                .priority(12)
                .group("group-1")
                .into_queue_record_with_id(target);

            let row = PersistedTaskRowShape::from_queue_record(record)
                .expect("known target task should persist with Kotlin row shape");
            let payload = parse_payload(&row);

            assert_eq!(row.class_name, kind.definition().persisted_class_name);
            assert_eq!(row.simple_type, kind.simple_type());
            assert_eq!(payload[payload_key], payload_value);
            assert_eq!(payload["priority"], 12);
            assert_eq!(payload["groupId"], "group-1");
            assert_eq!(
                payload["uniqueId"],
                format!("{}_{}", kind.simple_type(), target)
            );
        }
    }

    #[test]
    fn known_target_tasks_ignore_mismatched_runtime_payload_target() {
        let record = TaskQueueRecord::new("AnalyzeBook_book-1", 6, None)
            .with_simple_type("AnalyzeBook")
            .with_payload(r#"{"bookId":"book-2"}"#);

        let row = PersistedTaskRowShape::from_queue_record(record)
            .expect("known target task should persist with canonical target payload");

        assert_eq!(parse_payload(&row)["bookId"], "book-1");
    }

    #[test]
    fn special_known_tasks_restore_kotlin_payload_shapes() {
        let scan_row = PersistedTaskRowShape::from_queue_record(
            TaskQueueRecord::new("ScanLibrary_library-1_DEEP_true", 900, None)
                .with_simple_type("ScanLibrary"),
        )
        .expect("scan library task should persist with Kotlin payload shape");
        assert_eq!(
            parse_payload(&scan_row),
            json!({
                "libraryId": "library-1",
                "scanDeep": true,
                "priority": 900,
                "groupId": null,
                "uniqueId": "ScanLibrary_library-1_DEEP_true",
            })
        );

        let metadata_row = PersistedTaskRowShape::from_queue_record(
            TaskRequest::with_payload(TaskKind::RefreshBookMetadata, BookPayload::new("book-1"))
                .priority(5)
                .into_queue_record(),
        )
        .expect("refresh metadata task should persist with Kotlin payload shape");
        let metadata_payload = parse_payload(&metadata_row);
        assert_eq!(metadata_payload["bookId"], "book-1");
        assert!(
            metadata_payload["capabilities"]
                .as_array()
                .expect("capabilities should be an array")
                .contains(&Value::String("AUTHORS".to_string()))
        );

        let thumbnail_row = PersistedTaskRowShape::from_queue_record(
            TaskQueueRecord::new("FindBookThumbnailsToRegenerate", 0, None)
                .with_simple_type("FindBookThumbnailsToRegenerate")
                .with_payload(r#"{"for_bigger_result_only":true}"#),
        )
        .expect("thumbnail task should persist with Kotlin payload shape");
        assert_eq!(
            parse_payload(&thumbnail_row)["forBiggerResultOnly"],
            Value::Bool(true)
        );

        let rebuild_row = PersistedTaskRowShape::from_queue_record(
            TaskQueueRecord::new("RebuildIndex", 8, None)
                .with_simple_type("RebuildIndex")
                .with_payload(r#"{"entities":["Collection"]}"#),
        )
        .expect("rebuild index task should persist with Kotlin payload shape");
        assert_eq!(
            parse_payload(&rebuild_row)["entities"],
            json!(["Collection"])
        );

        let upgrade_row = PersistedTaskRowShape::from_queue_record(
            TaskQueueRecord::new("UpgradeIndex", 9, None).with_simple_type("UpgradeIndex"),
        )
        .expect("upgrade index task should persist with Kotlin payload shape");
        assert_eq!(parse_payload(&upgrade_row)["uniqueId"], "UpgradeIndex");
    }

    #[test]
    fn import_book_persisted_row_restores_kotlin_payload_shape() {
        let record = TaskQueueRecord::new("ImportBook:task-1", 100, Some("series-1".to_string()))
            .with_simple_type("ImportBook")
            .with_payload(
                json!({
                    "copy_mode": "COPY",
                    "book": {
                        "source_file": "/tmp/book.cbz",
                        "series_id": "series-1",
                        "destination_name": "dest-a",
                        "upgrade_book_id": "book-1"
                    }
                })
                .to_string(),
            );

        let row = PersistedTaskRowShape::from_queue_record(record)
            .expect("import book task should persist with Kotlin payload shape");
        let payload = parse_payload(&row);

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
    fn special_known_task_persisted_row_rejects_invalid_runtime_payload() {
        let record = TaskQueueRecord::new("FindBookThumbnailsToRegenerate", 0, None)
            .with_simple_type("FindBookThumbnailsToRegenerate")
            .with_payload(r#"{"forBiggerResultOnly":"true"}"#);

        let error = PersistedTaskRowShape::from_queue_record(record)
            .expect_err("invalid special task payload should not fall back to raw persistence");

        assert!(error.message.contains("forBiggerResultOnly"));
    }

    #[test]
    fn legacy_scan_library_row_restores_payload_shape_in_application() {
        let persisted_row = PersistedTaskRowShape {
            id: "ScanLibrary_library-1_DEEP_true".to_string(),
            priority: 900,
            group: None,
            class_name: "org.gotson.komga.application.tasks.Task$ScanLibrary".to_string(),
            simple_type: "ScanLibrary".to_string(),
            payload: "{}".to_string(),
            owner: None,
        };

        let runtime_record = persisted_row.into_queue_record();

        let payload = ScanLibraryPayload::from_task_record(&runtime_record)
            .expect("legacy persisted scan task should restore canonical payload");
        assert_eq!(payload.library_id, "library-1");
        assert!(payload.deep_scan);
    }

    #[test]
    fn unknown_task_persisted_row_keeps_runtime_class_and_fallback_payload() {
        let record = TaskQueueRecord::new("UnknownTask_target-1", 3, Some("group-1".to_string()))
            .with_simple_type("UnknownTask");

        let row = PersistedTaskRowShape::from_queue_record(record)
            .expect("unknown task should persist with fallback payload");

        assert_eq!(
            row.class_name,
            "org.gotson.komga.task.unknowntask.RuntimeTask"
        );
        assert_eq!(row.simple_type, "UnknownTask");
        assert_eq!(row.group.as_deref(), Some("group-1"));
        assert_eq!(parse_payload(&row)["simpleType"], "UnknownTask");
    }

    #[test]
    fn remove_hashed_pages_keeps_application_payload_without_compatibility_rewrite() {
        let record = TaskRequest::new(TaskKind::RemoveHashedPages)
            .into_queue_record_with_id("book-1")
            .with_payload(
                r#"{"bookId":"book-1","pages":[],"uniqueId":"RemoveHashedPages_book-1"}"#,
            );

        let row = PersistedTaskRowShape::from_queue_record(record)
            .expect("remove hashed pages should preserve application payload");

        assert_eq!(
            parse_payload(&row),
            json!({
                "bookId": "book-1",
                "pages": [],
                "uniqueId": "RemoveHashedPages_book-1"
            })
        );
    }

    fn parse_payload(row: &PersistedTaskRowShape) -> Value {
        serde_json::from_str(&row.payload).expect("persisted payload should be valid JSON")
    }
}
