use std::path::PathBuf;

use serde_json::{Value, json};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Row, SqlitePool};

use crate::sqlite::file_backed_connect_options;

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
                let pool = SqlitePoolOptions::new()
                    .max_connections(1)
                    .connect_with(file_backed_connect_options(&tasks_db_file))
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
        "FIND_BOOKS_WITH_MISSING_PAGE_HASH" => "FindBooksWithMissingPageHash".to_string(),
        "FIND_DUPLICATE_PAGES_TO_DELETE" => "FindDuplicatePagesToDelete".to_string(),
        "FIND_BOOK_THUMBNAILS_TO_REGENERATE" => "FindBookThumbnailsToRegenerate".to_string(),
        "REFRESH_BOOK_METADATA" => "RefreshBookMetadata".to_string(),
        "REFRESH_BOOK_LOCAL_ARTWORK" => "RefreshBookLocalArtwork".to_string(),
        "REPAIR_EXTENSION" => "RepairExtension".to_string(),
        "GENERATE_BOOK_THUMBNAIL" => "GenerateBookThumbnail".to_string(),
        "HASH_BOOK" => "HashBook".to_string(),
        "HASH_BOOK_KOREADER" => "HashBookKoreader".to_string(),
        "HASH_BOOK_PAGES" => "HashBookPages".to_string(),
        "REBUILD_INDEX" => "RebuildIndex".to_string(),
        "UPGRADE_INDEX" => "UpgradeIndex".to_string(),
        "REMOVE_HASHED_PAGES" => "RemoveHashedPages".to_string(),
        _ => simple_type.to_string(),
    }
}

fn runtime_simple_type(simple_type: &str) -> String {
    match simple_type {
        "FindBooksWithMissingPageHash" => "FIND_BOOKS_WITH_MISSING_PAGE_HASH".to_string(),
        "FindDuplicatePagesToDelete" => "FIND_DUPLICATE_PAGES_TO_DELETE".to_string(),
        "FindBookThumbnailsToRegenerate" => "FIND_BOOK_THUMBNAILS_TO_REGENERATE".to_string(),
        "RefreshBookMetadata" => "REFRESH_BOOK_METADATA".to_string(),
        "RefreshBookLocalArtwork" => "REFRESH_BOOK_LOCAL_ARTWORK".to_string(),
        "RepairExtension" => "REPAIR_EXTENSION".to_string(),
        "GenerateBookThumbnail" => "GENERATE_BOOK_THUMBNAIL".to_string(),
        "HashBook" => "HASH_BOOK".to_string(),
        "HashBookKoreader" => "HASH_BOOK_KOREADER".to_string(),
        "HashBookPages" => "HASH_BOOK_PAGES".to_string(),
        "RebuildIndex" => "REBUILD_INDEX".to_string(),
        "UpgradeIndex" => "UPGRADE_INDEX".to_string(),
        "RemoveHashedPages" => "REMOVE_HASHED_PAGES".to_string(),
        _ => simple_type.to_string(),
    }
}

fn kotlin_task_class_name(simple_type: &str) -> String {
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

fn task_target(task: &PersistedTaskStoreRecord) -> Option<&str> {
    task.id
        .strip_prefix(task.simple_type.as_str())
        .and_then(|suffix| {
            suffix
                .strip_prefix(':')
                .or_else(|| suffix.strip_prefix('_'))
        })
}

fn find_duplicate_pages_to_delete_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "FIND_DUPLICATE_PAGES_TO_DELETE" && task.payload.is_none()).then(|| {
        json!({
            "libraryId": task_target(task),
            "priority": task.priority,
            "groupId": task.group,
            "uniqueId": task.id,
        })
        .to_string()
    })
}

fn find_books_with_missing_page_hash_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "FIND_BOOKS_WITH_MISSING_PAGE_HASH" && task.payload.is_none()).then(|| {
        json!({
            "libraryId": task_target(task),
            "priority": task.priority,
            "groupId": task.group,
            "uniqueId": task.id,
        })
        .to_string()
    })
}

fn find_book_thumbnails_to_regenerate_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "FIND_BOOK_THUMBNAILS_TO_REGENERATE").then(|| {
        let for_bigger_result_only = task
            .payload
            .as_deref()
            .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
            .and_then(|payload| {
                payload
                    .get("for_bigger_result_only")
                    .or_else(|| payload.get("forBiggerResultOnly"))
                    .and_then(Value::as_bool)
            })
            .unwrap_or(false);
        json!({
            "forBiggerResultOnly": for_bigger_result_only,
            "priority": task.priority,
            "groupId": task.group,
            "uniqueId": task.id,
        })
        .to_string()
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
    (task.simple_type == "REFRESH_BOOK_LOCAL_ARTWORK").then(|| {
        json!({
            "bookId": task_target(task),
            "priority": task.priority,
            "groupId": task.group,
            "uniqueId": task.id,
        })
        .to_string()
    })
}

fn repair_extension_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "REPAIR_EXTENSION").then(|| {
        json!({
            "bookId": task_target(task),
            "priority": task.priority,
            "groupId": task.group,
            "uniqueId": task.id,
        })
        .to_string()
    })
}

fn hash_book_pages_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "HASH_BOOK_PAGES").then(|| {
        json!({
            "bookId": task_target(task),
            "priority": task.priority,
            "groupId": task.group,
            "uniqueId": task.id,
        })
        .to_string()
    })
}

fn generate_book_thumbnail_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "GENERATE_BOOK_THUMBNAIL").then(|| {
        json!({
            "bookId": task_target(task),
            "priority": task.priority,
            "groupId": task.group,
            "uniqueId": task.id,
        })
        .to_string()
    })
}

fn hash_book_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "HASH_BOOK").then(|| {
        json!({
            "bookId": task_target(task),
            "priority": task.priority,
            "groupId": task.group,
            "uniqueId": task.id,
        })
        .to_string()
    })
}

fn hash_book_koreader_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "HASH_BOOK_KOREADER").then(|| {
        json!({
            "bookId": task_target(task),
            "priority": task.priority,
            "groupId": task.group,
            "uniqueId": task.id,
        })
        .to_string()
    })
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
    find_duplicate_pages_to_delete_payload(task)
        .or_else(|| find_books_with_missing_page_hash_payload(task))
        .or_else(|| find_book_thumbnails_to_regenerate_payload(task))
        .or_else(|| refresh_book_metadata_payload(task))
        .or_else(|| refresh_book_local_artwork_payload(task))
        .or_else(|| repair_extension_payload(task))
        .or_else(|| generate_book_thumbnail_payload(task))
        .or_else(|| hash_book_payload(task))
        .or_else(|| hash_book_koreader_payload(task))
        .or_else(|| hash_book_pages_payload(task))
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
