use std::path::PathBuf;

use serde_json::{Map, Value, json};
use sqlx::{Row, SqlitePool};

use crate::sqlite::connect_private_pool;

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

impl SqliteTaskQueueStore {
    pub fn new(tasks_db_file: PathBuf) -> Option<Self> {
        if !tasks_db_file.exists() {
            return None;
        }

        Some(Self { tasks_db_file })
    }

    pub fn load_records(&self) -> Vec<PersistedTaskStoreRecord> {
        self.run(|pool| {
            Box::pin(async move {
                let rows = sqlx::query(
                    "SELECT ID, PRIORITY, GROUP_ID, SIMPLE_TYPE, PAYLOAD, OWNER \
                     FROM TASK \
                     ORDER BY PRIORITY DESC, LAST_MODIFIED_DATE ASC, ID ASC",
                )
                .fetch_all(&pool)
                .await
                .expect("persisted task queue rows should be readable");

                rows.into_iter()
                    .map(|row| PersistedTaskStoreRecord {
                        id: row.get::<String, _>("ID"),
                        priority: row.get::<i64, _>("PRIORITY") as i32,
                        group: row.get::<Option<String>, _>("GROUP_ID"),
                        simple_type: runtime_simple_type(&row.get::<String, _>("SIMPLE_TYPE")),
                        payload: row.get::<Option<String>, _>("PAYLOAD"),
                        owner: row.get::<Option<String>, _>("OWNER"),
                    })
                    .collect::<Vec<_>>()
            })
        })
    }

    pub fn persist_task(&self, task: &PersistedTaskStoreRecord) {
        let row = PersistedTaskRow::from_record(task);
        self.run(move |pool| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) \
                     VALUES (?, ?, ?, ?, ?, ?, ?) \
                     ON CONFLICT(ID) DO UPDATE \
                     SET PRIORITY = excluded.PRIORITY, \
                         GROUP_ID = excluded.GROUP_ID, \
                         CLASS = excluded.CLASS, \
                         SIMPLE_TYPE = excluded.SIMPLE_TYPE, \
                         PAYLOAD = excluded.PAYLOAD, \
                         OWNER = excluded.OWNER, \
                         LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
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
            })
        });
    }

    pub fn claim_task(&self, task_id: &str, owner: &str) {
        let task_id = task_id.to_string();
        let owner = owner.to_string();
        self.run(move |pool| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE TASK \
                     SET OWNER = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                     WHERE ID = ?",
                )
                .bind(owner)
                .bind(task_id)
                .execute(&pool)
                .await
                .expect("claimed task owner should persist to TASK table");
            })
        });
    }

    pub fn delete_task(&self, task_id: &str) -> bool {
        let task_id = task_id.to_string();
        self.run(move |pool| {
            Box::pin(async move {
                sqlx::query("DELETE FROM TASK WHERE ID = ?")
                    .bind(task_id)
                    .execute(&pool)
                    .await
                    .expect("completed task rows should be deleted from TASK table")
                    .rows_affected()
                    > 0
            })
        })
    }

    pub fn disown_all(&self) {
        self.run(|pool| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE TASK \
                     SET OWNER = NULL, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                     WHERE OWNER IS NOT NULL",
                )
                .execute(&pool)
                .await
                .expect("owned task rows should be disowned in TASK table");
            })
        });
    }

    pub fn disown_task(&self, task_id: &str) {
        let task_id = task_id.to_string();
        self.run(move |pool| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE TASK \
                     SET OWNER = NULL, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                     WHERE ID = ?",
                )
                .bind(task_id)
                .execute(&pool)
                .await
                .expect("task row should be disowned in TASK table");
            })
        });
    }

    pub fn clear_unowned(&self) -> usize {
        self.run(|pool| {
            Box::pin(async move {
                sqlx::query("DELETE FROM TASK WHERE OWNER IS NULL")
                    .execute(&pool)
                    .await
                    .expect("unowned task rows should be deleted from TASK table")
                    .rows_affected() as usize
            })
        })
    }

    fn run<T>(&self, operation: impl FnOnce(SqlitePool) -> BoxFuture<T> + Send + 'static) -> T
    where
        T: Send + 'static,
    {
        let tasks_db_file = self.tasks_db_file.clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("persisted task runtime should build");

            runtime.block_on(async move {
                let pool = connect_private_pool(&tasks_db_file, 1)
                    .await
                    .expect("tasks sqlite pool should open for task persistence");
                let result = operation(pool.clone()).await;
                pool.close().await;
                result
            })
        })
        .join()
        .expect("persisted task worker thread should complete")
    }
}

type BoxFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;

#[derive(Debug)]
struct PersistedTaskRow {
    id: String,
    priority: i32,
    group: Option<String>,
    class_name: String,
    simple_type: String,
    payload: String,
    owner: Option<String>,
}

impl PersistedTaskRow {
    fn from_record(task: &PersistedTaskStoreRecord) -> Self {
        let simple_type = persisted_simple_type(task.simple_type.as_str());
        Self {
            id: task.id.clone(),
            priority: task.priority,
            group: task.group.clone(),
            class_name: kotlin_task_class_name(simple_type.as_str()),
            simple_type,
            payload: persisted_task_payload(task),
            owner: task.owner.clone(),
        }
    }
}

fn persisted_simple_type(simple_type: &str) -> String {
    match simple_type {
        "SCAN_LIBRARY" => "ScanLibrary".to_string(),
        "EMPTY_TRASH" => "EmptyTrash".to_string(),
        "ANALYZE_BOOK" => "AnalyzeBook".to_string(),
        "IMPORT_BOOK" => "ImportBook".to_string(),
        "FIND_BOOKS_WITH_MISSING_PAGE_HASH" => "FindBooksWithMissingPageHash".to_string(),
        "FIND_DUPLICATE_PAGES_TO_DELETE" => "FindDuplicatePagesToDelete".to_string(),
        "FIND_BOOK_THUMBNAILS_TO_REGENERATE" => "FindBookThumbnailsToRegenerate".to_string(),
        "REFRESH_BOOK_METADATA" => "RefreshBookMetadata".to_string(),
        "REFRESH_BOOK_LOCAL_ARTWORK" => "RefreshBookLocalArtwork".to_string(),
        "REFRESH_SERIES_LOCAL_ARTWORK" => "RefreshSeriesLocalArtwork".to_string(),
        "REPAIR_EXTENSION" => "RepairExtension".to_string(),
        "GENERATE_BOOK_THUMBNAIL" => "GenerateBookThumbnail".to_string(),
        "HASH_BOOK" => "HashBook".to_string(),
        "HASH_BOOK_KOREADER" => "HashBookKoreader".to_string(),
        "HASH_BOOK_PAGES" => "HashBookPages".to_string(),
        "REBUILD_INDEX" => "RebuildIndex".to_string(),
        "UPGRADE_INDEX" => "UpgradeIndex".to_string(),
        "REMOVE_HASHED_PAGES" => "RemoveHashedPages".to_string(),
        "DELETE_BOOK" => "DeleteBook".to_string(),
        "DELETE_SERIES" => "DeleteSeries".to_string(),
        _ => simple_type.to_string(),
    }
}

fn runtime_simple_type(simple_type: &str) -> String {
    match simple_type {
        "ScanLibrary" => "SCAN_LIBRARY".to_string(),
        "EmptyTrash" => "EMPTY_TRASH".to_string(),
        "AnalyzeBook" => "ANALYZE_BOOK".to_string(),
        "ImportBook" => "IMPORT_BOOK".to_string(),
        "FindBooksWithMissingPageHash" => "FIND_BOOKS_WITH_MISSING_PAGE_HASH".to_string(),
        "FindDuplicatePagesToDelete" => "FIND_DUPLICATE_PAGES_TO_DELETE".to_string(),
        "FindBookThumbnailsToRegenerate" => "FIND_BOOK_THUMBNAILS_TO_REGENERATE".to_string(),
        "RefreshBookMetadata" => "REFRESH_BOOK_METADATA".to_string(),
        "RefreshBookLocalArtwork" => "REFRESH_BOOK_LOCAL_ARTWORK".to_string(),
        "RefreshSeriesLocalArtwork" => "REFRESH_SERIES_LOCAL_ARTWORK".to_string(),
        "RepairExtension" => "REPAIR_EXTENSION".to_string(),
        "GenerateBookThumbnail" => "GENERATE_BOOK_THUMBNAIL".to_string(),
        "HashBook" => "HASH_BOOK".to_string(),
        "HashBookKoreader" => "HASH_BOOK_KOREADER".to_string(),
        "HashBookPages" => "HASH_BOOK_PAGES".to_string(),
        "RebuildIndex" => "REBUILD_INDEX".to_string(),
        "UpgradeIndex" => "UPGRADE_INDEX".to_string(),
        "RemoveHashedPages" => "REMOVE_HASHED_PAGES".to_string(),
        "DeleteBook" => "DELETE_BOOK".to_string(),
        "DeleteSeries" => "DELETE_SERIES".to_string(),
        _ => simple_type.to_string(),
    }
}

fn kotlin_task_class_name(simple_type: &str) -> String {
    if simple_type == "ScanLibrary" {
        return "org.gotson.komga.application.tasks.Task$ScanLibrary".to_string();
    }
    if simple_type == "EmptyTrash" {
        return "org.gotson.komga.application.tasks.Task$EmptyTrash".to_string();
    }
    if simple_type == "AnalyzeBook" {
        return "org.gotson.komga.application.tasks.Task$AnalyzeBook".to_string();
    }
    if simple_type == "ImportBook" {
        return "org.gotson.komga.application.tasks.Task$ImportBook".to_string();
    }
    if simple_type == "FindBooksWithMissingPageHash" {
        return "org.gotson.komga.application.tasks.Task$FindBooksWithMissingPageHash".to_string();
    }
    if simple_type == "FindDuplicatePagesToDelete" {
        return "org.gotson.komga.application.tasks.Task$FindDuplicatePagesToDelete".to_string();
    }
    if simple_type == "FindBookThumbnailsToRegenerate" {
        return "org.gotson.komga.application.tasks.Task$FindBookThumbnailsToRegenerate"
            .to_string();
    }
    if simple_type == "RefreshBookMetadata" {
        return "org.gotson.komga.application.tasks.Task$RefreshBookMetadata".to_string();
    }
    if simple_type == "RefreshBookLocalArtwork" {
        return "org.gotson.komga.application.tasks.Task$RefreshBookLocalArtwork".to_string();
    }
    if simple_type == "RefreshSeriesLocalArtwork" {
        return "org.gotson.komga.application.tasks.Task$RefreshSeriesLocalArtwork".to_string();
    }
    if simple_type == "RepairExtension" {
        return "org.gotson.komga.application.tasks.Task$RepairExtension".to_string();
    }
    if simple_type == "GenerateBookThumbnail" {
        return "org.gotson.komga.application.tasks.Task$GenerateBookThumbnail".to_string();
    }
    if simple_type == "HashBook" {
        return "org.gotson.komga.application.tasks.Task$HashBook".to_string();
    }
    if simple_type == "HashBookKoreader" {
        return "org.gotson.komga.application.tasks.Task$HashBookKoreader".to_string();
    }
    if simple_type == "HashBookPages" {
        return "org.gotson.komga.application.tasks.Task$HashBookPages".to_string();
    }
    if simple_type == "RebuildIndex" {
        return "org.gotson.komga.application.tasks.Task$RebuildIndex".to_string();
    }
    if simple_type == "UpgradeIndex" {
        return "org.gotson.komga.application.tasks.Task$UpgradeIndex".to_string();
    }
    if simple_type == "RemoveHashedPages" {
        return "org.gotson.komga.application.tasks.Task$RemoveHashedPages".to_string();
    }
    if simple_type == "DeleteBook" {
        return "org.gotson.komga.application.tasks.Task$DeleteBook".to_string();
    }
    if simple_type == "DeleteSeries" {
        return "org.gotson.komga.application.tasks.Task$DeleteSeries".to_string();
    }
    format!(
        "org.gotson.komga.task.{}.RuntimeTask",
        simple_type.to_ascii_lowercase()
    )
}

fn default_task_payload(task: &PersistedTaskStoreRecord) -> String {
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

fn target_task_payload(
    task: &PersistedTaskStoreRecord,
    expected_simple_type: &str,
    target_key: &'static str,
) -> Option<String> {
    (task.simple_type == expected_simple_type).then(|| {
        task_payload(
            task,
            [(target_key, optional_string_value(task_target(task)))],
        )
    })
}

fn library_target_task_payload(
    task: &PersistedTaskStoreRecord,
    expected_simple_type: &str,
) -> Option<String> {
    (task.simple_type == expected_simple_type && task.payload.is_none()).then(|| {
        task_payload(
            task,
            [("libraryId", optional_string_value(task_target(task)))],
        )
    })
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
                    .split_once(":DEEP:")
                    .map(|(library_id, _)| library_id)
                    .or_else(|| {
                        target
                            .split_once("_DEEP_")
                            .map(|(library_id, _)| library_id)
                    })
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
                .rsplit_once(":DEEP:")
                .or_else(|| task.id.rsplit_once("_DEEP_"))
                .and_then(|(_, deep_scan)| deep_scan.parse::<bool>().ok())
        })
        .unwrap_or(false)
}

fn scan_library_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "SCAN_LIBRARY").then_some(())?;
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
    target_task_payload(task, "EMPTY_TRASH", "libraryId")
}

fn analyze_book_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "ANALYZE_BOOK", "bookId")
}

fn import_book_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "IMPORT_BOOK").then_some(())?;
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
    library_target_task_payload(task, "FIND_DUPLICATE_PAGES_TO_DELETE")
}

fn find_books_with_missing_page_hash_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    library_target_task_payload(task, "FIND_BOOKS_WITH_MISSING_PAGE_HASH")
}

fn find_book_thumbnails_to_regenerate_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "FIND_BOOK_THUMBNAILS_TO_REGENERATE").then(|| {
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
    (task.simple_type == "REFRESH_BOOK_METADATA").then(|| {
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
    target_task_payload(task, "REFRESH_BOOK_LOCAL_ARTWORK", "bookId")
}

fn refresh_series_local_artwork_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "REFRESH_SERIES_LOCAL_ARTWORK", "seriesId")
}

fn repair_extension_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "REPAIR_EXTENSION", "bookId")
}

fn hash_book_pages_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "HASH_BOOK_PAGES", "bookId")
}

fn generate_book_thumbnail_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "GENERATE_BOOK_THUMBNAIL", "bookId")
}

fn hash_book_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "HASH_BOOK", "bookId")
}

fn hash_book_koreader_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "HASH_BOOK_KOREADER", "bookId")
}

fn delete_book_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "DELETE_BOOK", "bookId")
}

fn delete_series_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "DELETE_SERIES", "seriesId")
}

fn rebuild_index_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "REBUILD_INDEX").then(|| {
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

fn persisted_task_payload(task: &PersistedTaskStoreRecord) -> String {
    scan_library_payload(task)
        .or_else(|| empty_trash_payload(task))
        .or_else(|| analyze_book_payload(task))
        .or_else(|| import_book_payload(task))
        .or_else(|| find_duplicate_pages_to_delete_payload(task))
        .or_else(|| find_books_with_missing_page_hash_payload(task))
        .or_else(|| find_book_thumbnails_to_regenerate_payload(task))
        .or_else(|| refresh_book_metadata_payload(task))
        .or_else(|| refresh_book_local_artwork_payload(task))
        .or_else(|| refresh_series_local_artwork_payload(task))
        .or_else(|| repair_extension_payload(task))
        .or_else(|| generate_book_thumbnail_payload(task))
        .or_else(|| hash_book_payload(task))
        .or_else(|| hash_book_koreader_payload(task))
        .or_else(|| hash_book_pages_payload(task))
        .or_else(|| delete_book_payload(task))
        .or_else(|| delete_series_payload(task))
        .or_else(|| rebuild_index_payload(task))
        .or_else(|| task.payload.clone())
        .unwrap_or_else(|| default_task_payload(task))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn persisted_find_duplicate_pages_to_delete_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "FIND_DUPLICATE_PAGES_TO_DELETE_library-1".to_string(),
            simple_type: "FIND_DUPLICATE_PAGES_TO_DELETE".to_string(),
            priority: 42,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "FIND_DUPLICATE_PAGES_TO_DELETE_library-1");
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
                "uniqueId": "FIND_DUPLICATE_PAGES_TO_DELETE_library-1"
            })
        );
    }

    #[test]
    fn persisted_rebuild_index_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "REBUILD_INDEX".to_string(),
            simple_type: "REBUILD_INDEX".to_string(),
            priority: 8,
            group: None,
            payload: Some(json!({ "entities": ["Collection"] }).to_string()),
            owner: None,
        });

        assert_eq!(row.id, "REBUILD_INDEX");
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
                "uniqueId": "REBUILD_INDEX"
            })
        );
    }

    #[test]
    fn kotlin_rebuild_index_simple_type_round_trips_back_to_runtime_type() {
        assert_eq!(runtime_simple_type("RebuildIndex"), "REBUILD_INDEX");
    }

    #[test]
    fn persisted_upgrade_index_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "UPGRADE_INDEX".to_string(),
            simple_type: "UPGRADE_INDEX".to_string(),
            priority: 9,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "UPGRADE_INDEX");
        assert_eq!(row.simple_type, "UpgradeIndex");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$UpgradeIndex"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("upgrade index payload should be valid JSON"),
            json!({
                "id": "UPGRADE_INDEX",
                "simpleType": "UPGRADE_INDEX",
                "priority": 9,
                "groupId": Value::Null,
            })
        );
    }

    #[test]
    fn kotlin_upgrade_index_simple_type_round_trips_back_to_runtime_type() {
        assert_eq!(runtime_simple_type("UpgradeIndex"), "UPGRADE_INDEX");
    }

    #[test]
    fn persisted_find_book_thumbnails_to_regenerate_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "FIND_BOOK_THUMBNAILS_TO_REGENERATE".to_string(),
            simple_type: "FIND_BOOK_THUMBNAILS_TO_REGENERATE".to_string(),
            priority: 0,
            group: None,
            payload: Some(json!({ "for_bigger_result_only": true }).to_string()),
            owner: None,
        });

        assert_eq!(row.id, "FIND_BOOK_THUMBNAILS_TO_REGENERATE");
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
                "uniqueId": "FIND_BOOK_THUMBNAILS_TO_REGENERATE"
            })
        );
    }

    #[test]
    fn persisted_scan_library_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "SCAN_LIBRARY:library-1:DEEP:true".to_string(),
            simple_type: "SCAN_LIBRARY".to_string(),
            priority: 100,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "SCAN_LIBRARY:library-1:DEEP:true");
        assert_eq!(row.simple_type, "ScanLibrary");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$ScanLibrary"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("scan-library payload should be valid JSON"),
            json!({
                "libraryId": "library-1",
                "scanDeep": true,
                "priority": 100,
                "groupId": Value::Null,
                "uniqueId": "SCAN_LIBRARY:library-1:DEEP:true"
            })
        );
    }

    #[test]
    fn kotlin_scan_library_simple_type_round_trips_back_to_runtime_type() {
        assert_eq!(runtime_simple_type("ScanLibrary"), "SCAN_LIBRARY");
    }

    #[test]
    fn persisted_empty_trash_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "EMPTY_TRASH:library-1".to_string(),
            simple_type: "EMPTY_TRASH".to_string(),
            priority: 70,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "EMPTY_TRASH:library-1");
        assert_eq!(row.simple_type, "EmptyTrash");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$EmptyTrash"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("empty-trash payload should be valid JSON"),
            json!({
                "libraryId": "library-1",
                "priority": 70,
                "groupId": Value::Null,
                "uniqueId": "EMPTY_TRASH:library-1"
            })
        );
    }

    #[test]
    fn kotlin_empty_trash_simple_type_round_trips_back_to_runtime_type() {
        assert_eq!(runtime_simple_type("EmptyTrash"), "EMPTY_TRASH");
    }

    #[test]
    fn persisted_analyze_book_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "ANALYZE_BOOK:book-1".to_string(),
            simple_type: "ANALYZE_BOOK".to_string(),
            priority: 90,
            group: Some("series-1".to_string()),
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "ANALYZE_BOOK:book-1");
        assert_eq!(row.simple_type, "AnalyzeBook");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$AnalyzeBook"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("analyze-book payload should be valid JSON"),
            json!({
                "bookId": "book-1",
                "priority": 90,
                "groupId": "series-1",
                "uniqueId": "ANALYZE_BOOK:book-1"
            })
        );
    }

    #[test]
    fn kotlin_analyze_book_simple_type_round_trips_back_to_runtime_type() {
        assert_eq!(runtime_simple_type("AnalyzeBook"), "ANALYZE_BOOK");
    }

    #[test]
    fn persisted_import_book_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "IMPORT_BOOK:task-1".to_string(),
            simple_type: "IMPORT_BOOK".to_string(),
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

        assert_eq!(row.id, "IMPORT_BOOK:task-1");
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
                "uniqueId": "IMPORT_BOOK:task-1"
            })
        );
    }

    #[test]
    fn kotlin_import_book_simple_type_round_trips_back_to_runtime_type() {
        assert_eq!(runtime_simple_type("ImportBook"), "IMPORT_BOOK");
    }

    #[test]
    fn kotlin_find_book_thumbnails_to_regenerate_simple_type_round_trips_back_to_runtime_type() {
        assert_eq!(
            runtime_simple_type("FindBookThumbnailsToRegenerate"),
            "FIND_BOOK_THUMBNAILS_TO_REGENERATE"
        );
    }

    #[test]
    fn persisted_refresh_book_metadata_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "REFRESH_BOOK_METADATA_book-1".to_string(),
            simple_type: "REFRESH_BOOK_METADATA".to_string(),
            priority: 80,
            group: Some("series-1".to_string()),
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "REFRESH_BOOK_METADATA_book-1");
        assert_eq!(row.simple_type, "RefreshBookMetadata");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$RefreshBookMetadata"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("refresh-book-metadata payload should be valid JSON"),
            json!({
                "bookId": "book-1",
                "capabilities": [
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
                    "LINKS"
                ],
                "priority": 80,
                "groupId": "series-1",
                "uniqueId": "REFRESH_BOOK_METADATA_book-1"
            })
        );
    }

    #[test]
    fn kotlin_refresh_book_metadata_simple_type_round_trips_back_to_runtime_type() {
        assert_eq!(
            runtime_simple_type("RefreshBookMetadata"),
            "REFRESH_BOOK_METADATA"
        );
    }

    #[test]
    fn persisted_refresh_book_local_artwork_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "REFRESH_BOOK_LOCAL_ARTWORK_book-1".to_string(),
            simple_type: "REFRESH_BOOK_LOCAL_ARTWORK".to_string(),
            priority: 80,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "REFRESH_BOOK_LOCAL_ARTWORK_book-1");
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
                "uniqueId": "REFRESH_BOOK_LOCAL_ARTWORK_book-1"
            })
        );
    }

    #[test]
    fn kotlin_refresh_book_local_artwork_simple_type_round_trips_back_to_runtime_type() {
        assert_eq!(
            runtime_simple_type("RefreshBookLocalArtwork"),
            "REFRESH_BOOK_LOCAL_ARTWORK"
        );
    }

    #[test]
    fn persisted_refresh_series_local_artwork_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "REFRESH_SERIES_LOCAL_ARTWORK:series-1".to_string(),
            simple_type: "REFRESH_SERIES_LOCAL_ARTWORK".to_string(),
            priority: 80,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "REFRESH_SERIES_LOCAL_ARTWORK:series-1");
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
                "uniqueId": "REFRESH_SERIES_LOCAL_ARTWORK:series-1"
            })
        );
    }

    #[test]
    fn kotlin_refresh_series_local_artwork_simple_type_round_trips_back_to_runtime_type() {
        assert_eq!(
            runtime_simple_type("RefreshSeriesLocalArtwork"),
            "REFRESH_SERIES_LOCAL_ARTWORK"
        );
    }

    #[test]
    fn persisted_repair_extension_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "REPAIR_EXTENSION_book-1".to_string(),
            simple_type: "REPAIR_EXTENSION".to_string(),
            priority: 12,
            group: Some("series-1".to_string()),
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "REPAIR_EXTENSION_book-1");
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
                "uniqueId": "REPAIR_EXTENSION_book-1"
            })
        );
    }

    #[test]
    fn kotlin_repair_extension_simple_type_round_trips_back_to_runtime_type() {
        assert_eq!(runtime_simple_type("RepairExtension"), "REPAIR_EXTENSION");
    }

    #[test]
    fn persisted_hash_book_pages_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "HASH_BOOK_PAGES_book-1".to_string(),
            simple_type: "HASH_BOOK_PAGES".to_string(),
            priority: 5,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "HASH_BOOK_PAGES_book-1");
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
                "uniqueId": "HASH_BOOK_PAGES_book-1"
            })
        );
    }

    #[test]
    fn kotlin_hash_book_pages_simple_type_round_trips_back_to_runtime_type() {
        assert_eq!(runtime_simple_type("HashBookPages"), "HASH_BOOK_PAGES");
    }

    #[test]
    fn persisted_hash_book_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "HASH_BOOK_book-1".to_string(),
            simple_type: "HASH_BOOK".to_string(),
            priority: 0,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "HASH_BOOK_book-1");
        assert_eq!(row.simple_type, "HashBook");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$HashBook"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("hash book payload should be valid JSON"),
            json!({
                "bookId": "book-1",
                "priority": 0,
                "groupId": Value::Null,
                "uniqueId": "HASH_BOOK_book-1"
            })
        );
    }

    #[test]
    fn kotlin_hash_book_simple_type_round_trips_back_to_runtime_type() {
        assert_eq!(runtime_simple_type("HashBook"), "HASH_BOOK");
    }

    #[test]
    fn persisted_hash_book_koreader_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "HASH_BOOK_KOREADER_book-1".to_string(),
            simple_type: "HASH_BOOK_KOREADER".to_string(),
            priority: 5,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "HASH_BOOK_KOREADER_book-1");
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
                "uniqueId": "HASH_BOOK_KOREADER_book-1"
            })
        );
    }

    #[test]
    fn kotlin_hash_book_koreader_simple_type_round_trips_back_to_runtime_type() {
        assert_eq!(
            runtime_simple_type("HashBookKoreader"),
            "HASH_BOOK_KOREADER"
        );
    }

    #[test]
    fn persisted_delete_book_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "DELETE_BOOK:book-1".to_string(),
            simple_type: "DELETE_BOOK".to_string(),
            priority: 100,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "DELETE_BOOK:book-1");
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
                "priority": 100,
                "groupId": Value::Null,
                "uniqueId": "DELETE_BOOK:book-1"
            })
        );
    }

    #[test]
    fn kotlin_delete_book_simple_type_round_trips_back_to_runtime_type() {
        assert_eq!(runtime_simple_type("DeleteBook"), "DELETE_BOOK");
    }

    #[test]
    fn persisted_delete_series_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "DELETE_SERIES:series-1".to_string(),
            simple_type: "DELETE_SERIES".to_string(),
            priority: 100,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "DELETE_SERIES:series-1");
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
                "priority": 100,
                "groupId": Value::Null,
                "uniqueId": "DELETE_SERIES:series-1"
            })
        );
    }

    #[test]
    fn kotlin_delete_series_simple_type_round_trips_back_to_runtime_type() {
        assert_eq!(runtime_simple_type("DeleteSeries"), "DELETE_SERIES");
    }

    #[test]
    fn persisted_find_books_with_missing_page_hash_uses_kotlin_task_shape() {
        let row = PersistedTaskRow::from_record(&PersistedTaskStoreRecord {
            id: "FIND_BOOKS_WITH_MISSING_PAGE_HASH_library-1".to_string(),
            simple_type: "FIND_BOOKS_WITH_MISSING_PAGE_HASH".to_string(),
            priority: 7,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "FIND_BOOKS_WITH_MISSING_PAGE_HASH_library-1");
        assert_eq!(row.simple_type, "FindBooksWithMissingPageHash");
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$FindBooksWithMissingPageHash"
        );
        assert_eq!(
            serde_json::from_str::<Value>(&row.payload)
                .expect("missing page hash finder payload should be valid JSON"),
            json!({
                "libraryId": "library-1",
                "priority": 7,
                "groupId": Value::Null,
                "uniqueId": "FIND_BOOKS_WITH_MISSING_PAGE_HASH_library-1"
            })
        );
    }

    #[test]
    fn kotlin_find_books_with_missing_page_hash_simple_type_round_trips_back_to_runtime_type() {
        assert_eq!(
            runtime_simple_type("FindBooksWithMissingPageHash"),
            "FIND_BOOKS_WITH_MISSING_PAGE_HASH"
        );
    }
}
