use std::path::PathBuf;

use komga_application::task_processing::{
    OpaqueTask, PersistedTaskRowShape, TaskKind, TaskQueueRecord,
};
use serde_json::{Map, Value, json};
use sqlx::Row;

use crate::sqlite::connect_private_write_pool;

#[derive(Clone, Debug)]
pub struct PersistedTaskStoreRecord {
    pub id: String,
    pub simple_type: String,
    pub priority: i32,
    pub group: Option<String>,
    pub payload: Option<String>,
    pub owner: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SqliteTaskQueueStore {
    tasks_db_file: PathBuf,
}

fn runtime_record_from_persisted_row(persisted_row: PersistedTaskRowShape) -> TaskQueueRecord {
    if let Ok(kind) = TaskKind::parse(&persisted_row.simple_type) {
        return known_runtime_record(kind, persisted_row);
    }

    OpaqueTask {
        runtime_simple_type: persisted_row.simple_type.clone(),
        persisted_row,
    }
    .into_queue_record()
}

fn persisted_row_from_runtime_record(task: &PersistedTaskStoreRecord) -> PersistedTaskRowShape {
    if let Ok(kind) = TaskKind::parse(&task.simple_type) {
        return known_persisted_row(kind, task);
    }

    PersistedTaskRowShape {
        id: task.id.clone(),
        priority: task.priority,
        group: task.group.clone(),
        class_name: runtime_task_class_name(task.simple_type.as_str()),
        simple_type: task.simple_type.clone(),
        payload: fallback_task_payload(task),
        owner: task.owner.clone(),
    }
}

impl SqliteTaskQueueStore {
    pub fn new(tasks_db_file: PathBuf) -> Option<Self> {
        if !tasks_db_file.exists() {
            return None;
        }

        Some(Self { tasks_db_file })
    }

    pub async fn load_records(&self) -> Vec<PersistedTaskStoreRecord> {
        let pool = connect_private_write_pool(&self.tasks_db_file)
            .await
            .expect("tasks sqlite pool should open for task persistence");

        let rows = sqlx::query(
            r#"SELECT
                ID,
                PRIORITY,
                GROUP_ID,
                CLASS,
                SIMPLE_TYPE,
                PAYLOAD,
                OWNER
            FROM TASK
            ORDER BY PRIORITY DESC, LAST_MODIFIED_DATE ASC, ID ASC"#,
        )
        .fetch_all(&pool)
        .await
        .expect("persisted task queue rows should be readable");
        pool.close().await;

        rows.into_iter()
            .map(persisted_row_shape)
            .map(runtime_record_from_persisted_row)
            .map(store_record_from_runtime_record)
            .collect::<Vec<_>>()
    }

    pub async fn persist_task(&self, task: &PersistedTaskStoreRecord) {
        let row = persisted_row_from_runtime_record(task);
        let pool = connect_private_write_pool(&self.tasks_db_file)
            .await
            .expect("tasks sqlite pool should open for task persistence");
        sqlx::query(
            r#"INSERT INTO TASK (
                ID,
                PRIORITY,
                GROUP_ID,
                CLASS,
                SIMPLE_TYPE,
                PAYLOAD,
                OWNER
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(ID) DO UPDATE
            SET PRIORITY = excluded.PRIORITY,
                GROUP_ID = excluded.GROUP_ID,
                CLASS = excluded.CLASS,
                SIMPLE_TYPE = excluded.SIMPLE_TYPE,
                PAYLOAD = excluded.PAYLOAD,
                OWNER = excluded.OWNER,
                LAST_MODIFIED_DATE = CURRENT_TIMESTAMP"#,
        )
        .bind(row.id)
        .bind(row.priority)
        .bind(row.group)
        .bind(row.class_name)
        .bind(row.simple_type)
        .bind(row.payload)
        .bind(row.owner)
        .execute(&pool)
        .await
        .expect("queued task rows should persist to TASK table");
        pool.close().await;
    }

    pub async fn claim_task(&self, task_id: &str, owner: &str) {
        let task_id = task_id.to_string();
        let owner = owner.to_string();
        let pool = connect_private_write_pool(&self.tasks_db_file)
            .await
            .expect("tasks sqlite pool should open for task persistence");
        sqlx::query(
            r#"UPDATE TASK
            SET OWNER = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
            WHERE ID = ?"#,
        )
        .bind(owner)
        .bind(task_id)
        .execute(&pool)
        .await
        .expect("claimed task owner should persist to TASK table");
        pool.close().await;
    }

    pub async fn delete_task(&self, task_id: &str) -> bool {
        let task_id = task_id.to_string();
        let pool = connect_private_write_pool(&self.tasks_db_file)
            .await
            .expect("tasks sqlite pool should open for task persistence");
        let removed = sqlx::query("DELETE FROM TASK WHERE ID = ?")
            .bind(task_id)
            .execute(&pool)
            .await
            .expect("completed task rows should be deleted from TASK table")
            .rows_affected()
            > 0;
        pool.close().await;
        removed
    }

    pub async fn disown_all(&self) {
        let pool = connect_private_write_pool(&self.tasks_db_file)
            .await
            .expect("tasks sqlite pool should open for task persistence");
        sqlx::query(
            r#"UPDATE TASK
            SET OWNER = NULL, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
            WHERE OWNER IS NOT NULL"#,
        )
        .execute(&pool)
        .await
        .expect("owned task rows should be disowned in TASK table");
        pool.close().await;
    }

    pub async fn clear_unowned(&self) -> usize {
        let pool = connect_private_write_pool(&self.tasks_db_file)
            .await
            .expect("tasks sqlite pool should open for task persistence");
        let deleted = sqlx::query("DELETE FROM TASK WHERE OWNER IS NULL")
            .execute(&pool)
            .await
            .expect("unowned task rows should be deleted from TASK table")
            .rows_affected() as usize;
        pool.close().await;
        deleted
    }
}

fn store_record_from_runtime_record(task: TaskQueueRecord) -> PersistedTaskStoreRecord {
    PersistedTaskStoreRecord {
        id: task.id,
        simple_type: task.simple_type,
        priority: task.priority,
        group: task.group,
        payload: task.payload,
        owner: task.owner,
    }
}

fn persisted_row_shape(row: sqlx::sqlite::SqliteRow) -> PersistedTaskRowShape {
    PersistedTaskRowShape {
        id: row.get::<String, _>("ID"),
        priority: row.get::<i64, _>("PRIORITY") as i32,
        group: row.get::<Option<String>, _>("GROUP_ID"),
        class_name: row.get::<String, _>("CLASS"),
        simple_type: row.get::<String, _>("SIMPLE_TYPE"),
        payload: row.get::<String, _>("PAYLOAD"),
        owner: row.get::<Option<String>, _>("OWNER"),
    }
}

fn known_runtime_record(kind: TaskKind, persisted_row: PersistedTaskRowShape) -> TaskQueueRecord {
    let def = kind.definition();
    let mut runtime_record = TaskQueueRecord::new(
        persisted_row.id,
        persisted_row.priority,
        persisted_row.group,
    )
    .with_simple_type(def.simple_type)
    .with_payload(persisted_row.payload);
    runtime_record.owner = persisted_row.owner;
    runtime_record
}

fn known_persisted_row(kind: TaskKind, task: &PersistedTaskStoreRecord) -> PersistedTaskRowShape {
    let def = kind.definition();
    PersistedTaskRowShape {
        id: task.id.clone(),
        priority: task.priority,
        group: task.group.clone(),
        class_name: def.persisted_class_name.to_string(),
        simple_type: def.simple_type.to_string(),
        payload: persisted_compatibility_payload(kind, task),
        owner: task.owner.clone(),
    }
}

fn runtime_task_class_name(simple_type: &str) -> String {
    format!(
        "org.gotson.komga.task.{}.RuntimeTask",
        simple_type.to_ascii_lowercase()
    )
}

fn fallback_task_payload(task: &PersistedTaskStoreRecord) -> String {
    json!({
        "id": task.id,
        "simpleType": task.simple_type,
        "priority": task.priority,
        "groupId": task.group,
    })
    .to_string()
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

fn target_task_payload(
    task: &PersistedTaskStoreRecord,
    expected_simple_type: &str,
    target_key: &'static str,
) -> Option<String> {
    (task.simple_type == expected_simple_type).then_some(())?;

    if payload_contains_key(task, target_key) {
        return task.payload.clone();
    }

    Some(task_payload(
        task,
        [(target_key, optional_string_value(task_target(task)))],
    ))
}

fn library_target_task_payload(
    task: &PersistedTaskStoreRecord,
    expected_simple_type: &str,
) -> Option<String> {
    (task.simple_type == expected_simple_type).then_some(())?;

    if payload_contains_key(task, "libraryId") {
        return task.payload.clone();
    }

    Some(task_payload(
        task,
        [("libraryId", optional_string_value(task_target(task)))],
    ))
}

fn task_target(task: &PersistedTaskStoreRecord) -> Option<&str> {
    task.id
        .strip_prefix(task.simple_type.as_str())
        .and_then(|suffix| {
            suffix
                .strip_prefix(':')
                .or_else(|| suffix.strip_prefix('_'))
        })
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
            task_target(task).map(|target| {
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

fn scan_library_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "ScanLibrary").then_some(())?;
    let library_id = scan_library_target(task)?;
    Some(task_payload(
        task,
        [
            ("libraryId", Value::String(library_id)),
            ("scanDeep", Value::Bool(scan_library_deep(task))),
        ],
    ))
}

fn empty_trash_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "EmptyTrash", "libraryId")
}

fn analyze_book_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "AnalyzeBook", "bookId")
}

fn import_book_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "ImportBook").then_some(())?;
    let payload = payload_json(task)?;
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
            "priority": task.priority,
            "groupId": task.group,
            "uniqueId": task.id,
        })
        .to_string(),
    )
}

fn find_duplicate_pages_to_delete_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    library_target_task_payload(task, "FindDuplicatePagesToDelete")
}

fn find_books_with_missing_page_hash_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    library_target_task_payload(task, "FindBooksWithMissingPageHash")
}

fn find_book_thumbnails_to_regenerate_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "FindBookThumbnailsToRegenerate").then(|| {
        task_payload(
            task,
            [(
                "forBiggerResultOnly",
                Value::Bool(legacy_bool_payload_value(
                    task,
                    "for_bigger_result_only",
                    "forBiggerResultOnly",
                )),
            )],
        )
    })
}

fn default_refresh_book_metadata_capabilities() -> Vec<Value> {
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
}

fn refresh_book_metadata_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "RefreshBookMetadata").then(|| {
        let capabilities = task
            .payload
            .as_deref()
            .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
            .and_then(|payload| payload.get("capabilities").cloned())
            .and_then(|capabilities| capabilities.as_array().cloned())
            .unwrap_or_else(default_refresh_book_metadata_capabilities);
        json!({
            "bookId": task_target(task),
            "capabilities": capabilities,
            "priority": task.priority,
            "groupId": task.group,
            "uniqueId": task.id,
        })
        .to_string()
    })
}

fn refresh_book_local_artwork_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "RefreshBookLocalArtwork", "bookId")
}

fn refresh_series_metadata_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "RefreshSeriesMetadata", "seriesId")
}

fn aggregate_series_metadata_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "AggregateSeriesMetadata", "seriesId")
}

fn refresh_series_local_artwork_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "RefreshSeriesLocalArtwork", "seriesId")
}

fn repair_extension_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "RepairExtension", "bookId")
}

fn hash_book_pages_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "HashBookPages", "bookId")
}

fn generate_book_thumbnail_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "GenerateBookThumbnail", "bookId")
}

fn hash_book_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "HashBook", "bookId")
}

fn hash_book_koreader_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "HashBookKoreader", "bookId")
}

fn delete_book_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "DeleteBook", "bookId")
}

fn delete_series_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "DeleteSeries", "seriesId")
}

fn find_books_to_convert_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    library_target_task_payload(task, "FindBooksToConvert")
}

fn convert_book_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "ConvertBook", "bookId")
}

fn rebuild_index_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "RebuildIndex").then(|| {
        let entities = task
            .payload
            .as_deref()
            .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
            .and_then(|payload| payload.get("entities").cloned())
            .unwrap_or(Value::Null);
        json!({
            "entities": entities,
            "priority": task.priority,
            "groupId": task.group,
            "uniqueId": task.id,
        })
        .to_string()
    })
}

fn upgrade_index_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "UpgradeIndex")
        .then(|| task_payload(task, std::iter::empty::<(&'static str, Value)>()))
}

fn persisted_compatibility_payload(kind: TaskKind, task: &PersistedTaskStoreRecord) -> String {
    let compatibility_payload = match kind {
        TaskKind::ScanLibrary => scan_library_payload(task),
        TaskKind::EmptyTrash => empty_trash_payload(task),
        TaskKind::AnalyzeBook => analyze_book_payload(task),
        TaskKind::ImportBook => import_book_payload(task),
        TaskKind::FindBooksWithMissingPageHash => find_books_with_missing_page_hash_payload(task),
        TaskKind::FindDuplicatePagesToDelete => find_duplicate_pages_to_delete_payload(task),
        TaskKind::FindBookThumbnailsToRegenerate => {
            find_book_thumbnails_to_regenerate_payload(task)
        }
        TaskKind::RefreshBookMetadata => refresh_book_metadata_payload(task),
        TaskKind::RefreshBookLocalArtwork => refresh_book_local_artwork_payload(task),
        TaskKind::RefreshSeriesMetadata => refresh_series_metadata_payload(task),
        TaskKind::AggregateSeriesMetadata => aggregate_series_metadata_payload(task),
        TaskKind::RefreshSeriesLocalArtwork => refresh_series_local_artwork_payload(task),
        TaskKind::RepairExtension => repair_extension_payload(task),
        TaskKind::GenerateBookThumbnail => generate_book_thumbnail_payload(task),
        TaskKind::HashBook => hash_book_payload(task),
        TaskKind::HashBookKoreader => hash_book_koreader_payload(task),
        TaskKind::HashBookPages => hash_book_pages_payload(task),
        TaskKind::RebuildIndex => rebuild_index_payload(task),
        TaskKind::UpgradeIndex => upgrade_index_payload(task),
        TaskKind::DeleteBook => delete_book_payload(task),
        TaskKind::DeleteSeries => delete_series_payload(task),
        TaskKind::FindBooksToConvert => find_books_to_convert_payload(task),
        TaskKind::ConvertBook => convert_book_payload(task),
        TaskKind::RemoveHashedPages => None,
    };

    compatibility_payload
        .or_else(|| task.payload.clone())
        .unwrap_or_else(|| fallback_task_payload(task))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn persisted_row(task: PersistedTaskStoreRecord) -> PersistedTaskRowShape {
        persisted_row_from_runtime_record(&task)
    }

    #[test]
    fn persisted_find_duplicate_pages_to_delete_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "FindDuplicatePagesToDelete_library-1".to_string(),
            simple_type: "FindDuplicatePagesToDelete".to_string(),
            priority: 42,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "FindDuplicatePagesToDelete_library-1");
        assert_eq!(row.simple_type, "FindDuplicatePagesToDelete");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$FindDuplicatePagesToDelete"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("find duplicate pages payload should be valid JSON"),
            json!({
                "libraryId": "library-1",
                "priority": 42,
                "groupId": Value::Null,
                "uniqueId": "FindDuplicatePagesToDelete_library-1"
            })
        );
    }

    #[test]
    fn persisted_find_books_to_convert_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "FindBooksToConvert_library-1".to_string(),
            simple_type: "FindBooksToConvert".to_string(),
            priority: 0,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "FindBooksToConvert_library-1");
        assert_eq!(row.simple_type, "FindBooksToConvert");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$FindBooksToConvert"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("find books to convert payload should be valid JSON"),
            json!({
                "libraryId": "library-1",
                "priority": 0,
                "groupId": Value::Null,
                "uniqueId": "FindBooksToConvert_library-1"
            })
        );
    }

    #[test]
    fn persisted_rebuild_index_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "RebuildIndex".to_string(),
            simple_type: "RebuildIndex".to_string(),
            priority: 8,
            group: None,
            payload: Some(json!({ "entities": ["Collection"] }).to_string()),
            owner: None,
        });

        assert_eq!(row.id, "RebuildIndex");
        assert_eq!(row.simple_type, "RebuildIndex");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$RebuildIndex"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("rebuild index payload should be valid JSON"),
            json!({
                "entities": ["Collection"],
                "priority": 8,
                "groupId": Value::Null,
                "uniqueId": "RebuildIndex"
            })
        );
    }

    #[test]
    fn persisted_refresh_series_metadata_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "RefreshSeriesMetadata_series-1".to_string(),
            simple_type: "RefreshSeriesMetadata".to_string(),
            priority: 5,
            group: Some("series-1".to_string()),
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "RefreshSeriesMetadata_series-1");
        assert_eq!(row.simple_type, "RefreshSeriesMetadata");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$RefreshSeriesMetadata"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("refresh series metadata payload should be valid JSON"),
            json!({
                "seriesId": "series-1",
                "priority": 5,
                "groupId": "series-1",
                "uniqueId": "RefreshSeriesMetadata_series-1"
            })
        );
    }

    #[test]
    fn persisted_aggregate_series_metadata_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "AggregateSeriesMetadata_series-1".to_string(),
            simple_type: "AggregateSeriesMetadata".to_string(),
            priority: 6,
            group: Some("series-1".to_string()),
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "AggregateSeriesMetadata_series-1");
        assert_eq!(row.simple_type, "AggregateSeriesMetadata");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$AggregateSeriesMetadata"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("aggregate series metadata payload should be valid JSON"),
            json!({
                "seriesId": "series-1",
                "priority": 6,
                "groupId": "series-1",
                "uniqueId": "AggregateSeriesMetadata_series-1"
            })
        );
    }

    #[test]
    fn persisted_upgrade_index_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "UpgradeIndex".to_string(),
            simple_type: "UpgradeIndex".to_string(),
            priority: 9,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "UpgradeIndex");
        assert_eq!(row.simple_type, "UpgradeIndex");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$UpgradeIndex"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("upgrade index payload should be valid JSON"),
            json!({
                "priority": 9,
                "groupId": Value::Null,
                "uniqueId": "UpgradeIndex"
            })
        );
    }

    #[test]
    fn persisted_find_book_thumbnails_to_regenerate_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "FindBookThumbnailsToRegenerate".to_string(),
            simple_type: "FindBookThumbnailsToRegenerate".to_string(),
            priority: 0,
            group: None,
            payload: Some(json!({ "for_bigger_result_only": true }).to_string()),
            owner: None,
        });

        assert_eq!(row.id, "FindBookThumbnailsToRegenerate");
        assert_eq!(row.simple_type, "FindBookThumbnailsToRegenerate");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$FindBookThumbnailsToRegenerate"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("thumbnail regenerate payload should be valid JSON"),
            json!({
                "forBiggerResultOnly": true,
                "priority": 0,
                "groupId": Value::Null,
                "uniqueId": "FindBookThumbnailsToRegenerate"
            })
        );
    }

    #[test]
    fn persisted_import_book_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "ImportBook:task-1".to_string(),
            simple_type: "ImportBook".to_string(),
            priority: 100,
            group: Some("series-1".to_string()),
            payload: Some(
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
            ),
            owner: None,
        });

        assert_eq!(row.id, "ImportBook:task-1");
        assert_eq!(row.simple_type, "ImportBook");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$ImportBook"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("import-book payload should be valid JSON"),
            json!({
                "sourceFile": "/tmp/book.cbz",
                "seriesId": "series-1",
                "copyMode": "COPY",
                "destinationName": "dest-a",
                "upgradeBookId": "book-1",
                "priority": 100,
                "groupId": "series-1",
                "uniqueId": "ImportBook:task-1"
            })
        );
    }

    #[test]
    fn persisted_convert_book_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "ConvertBook_book-1".to_string(),
            simple_type: "ConvertBook".to_string(),
            priority: 7,
            group: Some("series-1".to_string()),
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "ConvertBook_book-1");
        assert_eq!(row.simple_type, "ConvertBook");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$ConvertBook"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("convert book payload should be valid JSON"),
            json!({
                "bookId": "book-1",
                "priority": 7,
                "groupId": "series-1",
                "uniqueId": "ConvertBook_book-1"
            })
        );
    }

    #[test]
    fn persisted_refresh_book_local_artwork_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "RefreshBookLocalArtwork_book-1".to_string(),
            simple_type: "RefreshBookLocalArtwork".to_string(),
            priority: 80,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "RefreshBookLocalArtwork_book-1");
        assert_eq!(row.simple_type, "RefreshBookLocalArtwork");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$RefreshBookLocalArtwork"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("refresh-book-local-artwork payload should be valid JSON"),
            json!({
                "bookId": "book-1",
                "priority": 80,
                "groupId": Value::Null,
                "uniqueId": "RefreshBookLocalArtwork_book-1"
            })
        );
    }

    #[test]
    fn persisted_refresh_series_local_artwork_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "RefreshSeriesLocalArtwork_series-1".to_string(),
            simple_type: "RefreshSeriesLocalArtwork".to_string(),
            priority: 80,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "RefreshSeriesLocalArtwork_series-1");
        assert_eq!(row.simple_type, "RefreshSeriesLocalArtwork");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$RefreshSeriesLocalArtwork"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("refresh-series-local-artwork payload should be valid JSON"),
            json!({
                "seriesId": "series-1",
                "priority": 80,
                "groupId": Value::Null,
                "uniqueId": "RefreshSeriesLocalArtwork_series-1"
            })
        );
    }

    #[test]
    fn persisted_repair_extension_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "RepairExtension_book-1".to_string(),
            simple_type: "RepairExtension".to_string(),
            priority: 12,
            group: Some("series-1".to_string()),
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "RepairExtension_book-1");
        assert_eq!(row.simple_type, "RepairExtension");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$RepairExtension"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("repair-extension payload should be valid JSON"),
            json!({
                "bookId": "book-1",
                "priority": 12,
                "groupId": "series-1",
                "uniqueId": "RepairExtension_book-1"
            })
        );
    }

    #[test]
    fn persisted_hash_book_pages_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "HashBookPages_book-1".to_string(),
            simple_type: "HashBookPages".to_string(),
            priority: 5,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "HashBookPages_book-1");
        assert_eq!(row.simple_type, "HashBookPages");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$HashBookPages"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("hash book pages payload should be valid JSON"),
            json!({
                "bookId": "book-1",
                "priority": 5,
                "groupId": Value::Null,
                "uniqueId": "HashBookPages_book-1"
            })
        );
    }

    #[test]
    fn persisted_hash_book_koreader_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "HashBookKoreader_book-1".to_string(),
            simple_type: "HashBookKoreader".to_string(),
            priority: 5,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "HashBookKoreader_book-1");
        assert_eq!(row.simple_type, "HashBookKoreader");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$HashBookKoreader"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("hash book koreader payload should be valid JSON"),
            json!({
                "bookId": "book-1",
                "priority": 5,
                "groupId": Value::Null,
                "uniqueId": "HashBookKoreader_book-1"
            })
        );
    }

    #[test]
    fn persisted_delete_book_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "DeleteBook_book-1".to_string(),
            simple_type: "DeleteBook".to_string(),
            priority: 8,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "DeleteBook_book-1");
        assert_eq!(row.simple_type, "DeleteBook");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$DeleteBook"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("delete-book payload should be valid JSON"),
            json!({
                "bookId": "book-1",
                "priority": 8,
                "groupId": Value::Null,
                "uniqueId": "DeleteBook_book-1"
            })
        );
    }

    #[test]
    fn persisted_delete_series_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "DeleteSeries_series-1".to_string(),
            simple_type: "DeleteSeries".to_string(),
            priority: 8,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "DeleteSeries_series-1");
        assert_eq!(row.simple_type, "DeleteSeries");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$DeleteSeries"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("delete-series payload should be valid JSON"),
            json!({
                "seriesId": "series-1",
                "priority": 8,
                "groupId": Value::Null,
                "uniqueId": "DeleteSeries_series-1"
            })
        );
    }
}
