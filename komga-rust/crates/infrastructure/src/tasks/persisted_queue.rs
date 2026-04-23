use std::path::PathBuf;

use komga_application::task_processing::{
    DefaultTaskProtocolCatalog, PersistedTaskRowShape, PlannedTaskKind, TaskProtocolCatalog,
    TaskQueueRecord,
};
use serde_json::{Map, Value, json};
use sqlx::{Row, SqlitePool};

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

#[derive(Clone, Debug)]
struct PersistedTaskCompatibility<C> {
    catalog: C,
}

impl<C: Default> Default for PersistedTaskCompatibility<C> {
    fn default() -> Self {
        Self {
            catalog: C::default(),
        }
    }
}

impl<C: TaskProtocolCatalog> PersistedTaskCompatibility<C> {
    #[cfg(test)]
    fn new(catalog: C) -> Self {
        Self { catalog }
    }

    fn runtime_record_from_persisted_row(
        &self,
        persisted_row: PersistedTaskRowShape,
    ) -> TaskQueueRecord {
        if let Some(kind) = self
            .catalog
            .known_kind_from_persisted_simple_type(&persisted_row.simple_type)
        {
            return known_runtime_record(&self.catalog, kind, persisted_row);
        }

        self.catalog
            .opaque_task(persisted_row.simple_type.clone(), persisted_row)
            .into_queue_record()
    }

    fn persisted_row_from_runtime_record(
        &self,
        task: &PersistedTaskStoreRecord,
    ) -> PersistedTaskRowShape {
        if let Some(kind) = self
            .catalog
            .known_kind_from_runtime_simple_type(&task.simple_type)
        {
            return known_persisted_row(&self.catalog, kind, task);
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
                let compatibility = compatibility();
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

                rows.into_iter()
                    .map(persisted_row_shape)
                    .map(|row| compatibility.runtime_record_from_persisted_row(row))
                    .map(store_record_from_runtime_record)
                    .collect::<Vec<_>>()
            })
        })
    }

    pub fn persist_task(&self, task: &PersistedTaskStoreRecord) {
        let row = compatibility().persisted_row_from_runtime_record(task);
        self.run(move |pool| {
            Box::pin(async move {
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
            })
        });
    }

    pub fn claim_task(&self, task_id: &str, owner: &str) {
        let task_id = task_id.to_string();
        let owner = owner.to_string();
        self.run(move |pool| {
            Box::pin(async move {
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
                    r#"UPDATE TASK
                    SET OWNER = NULL, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                    WHERE OWNER IS NOT NULL"#,
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
                    r#"UPDATE TASK
                    SET OWNER = NULL, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                    WHERE ID = ?"#,
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
            let runtime = crate::tokio_runtime::current_thread_runtime()
                .expect("persisted task runtime should build");

            runtime.block_on(async move {
                let pool = connect_private_write_pool(&tasks_db_file)
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

fn compatibility() -> PersistedTaskCompatibility<DefaultTaskProtocolCatalog> {
    PersistedTaskCompatibility::default()
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

fn known_runtime_record<C: TaskProtocolCatalog>(
    catalog: &C,
    kind: PlannedTaskKind,
    persisted_row: PersistedTaskRowShape,
) -> TaskQueueRecord {
    let descriptor = catalog.descriptor(kind);
    let mut runtime_record = TaskQueueRecord::new(
        persisted_row.id,
        persisted_row.priority,
        persisted_row.group,
    )
    .with_simple_type(descriptor.runtime_simple_type)
    .with_payload(persisted_row.payload);
    runtime_record.owner = persisted_row.owner;
    runtime_record
}

fn known_persisted_row<C: TaskProtocolCatalog>(
    catalog: &C,
    kind: PlannedTaskKind,
    task: &PersistedTaskStoreRecord,
) -> PersistedTaskRowShape {
    let descriptor = catalog.descriptor(kind);
    PersistedTaskRowShape {
        id: task.id.clone(),
        priority: task.priority,
        group: task.group.clone(),
        class_name: descriptor.persisted_class_name.to_string(),
        simple_type: descriptor.persisted_simple_type.to_string(),
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

fn refresh_series_metadata_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "REFRESH_SERIES_METADATA", "seriesId")
}

fn aggregate_series_metadata_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "AGGREGATE_SERIES_METADATA", "seriesId")
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

fn find_books_to_convert_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    library_target_task_payload(task, "FIND_BOOKS_TO_CONVERT")
}

fn convert_book_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    target_task_payload(task, "CONVERT_BOOK", "bookId")
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

fn upgrade_index_payload(task: &PersistedTaskStoreRecord) -> Option<String> {
    (task.simple_type == "UPGRADE_INDEX")
        .then(|| task_payload(task, std::iter::empty::<(&'static str, Value)>()))
}

fn persisted_compatibility_payload(
    kind: PlannedTaskKind,
    task: &PersistedTaskStoreRecord,
) -> String {
    let compatibility_payload = match kind {
        PlannedTaskKind::ScanLibrary => scan_library_payload(task),
        PlannedTaskKind::EmptyTrash => empty_trash_payload(task),
        PlannedTaskKind::AnalyzeBook => analyze_book_payload(task),
        PlannedTaskKind::ImportBook => import_book_payload(task),
        PlannedTaskKind::FindBooksWithMissingPageHash => {
            find_books_with_missing_page_hash_payload(task)
        }
        PlannedTaskKind::FindDuplicatePagesToDelete => find_duplicate_pages_to_delete_payload(task),
        PlannedTaskKind::FindBookThumbnailsToRegenerate => {
            find_book_thumbnails_to_regenerate_payload(task)
        }
        PlannedTaskKind::RefreshBookMetadata => refresh_book_metadata_payload(task),
        PlannedTaskKind::RefreshBookLocalArtwork => refresh_book_local_artwork_payload(task),
        PlannedTaskKind::RefreshSeriesMetadata => refresh_series_metadata_payload(task),
        PlannedTaskKind::AggregateSeriesMetadata => aggregate_series_metadata_payload(task),
        PlannedTaskKind::RefreshSeriesLocalArtwork => refresh_series_local_artwork_payload(task),
        PlannedTaskKind::RepairExtension => repair_extension_payload(task),
        PlannedTaskKind::GenerateBookThumbnail => generate_book_thumbnail_payload(task),
        PlannedTaskKind::HashBook => hash_book_payload(task),
        PlannedTaskKind::HashBookKoreader => hash_book_koreader_payload(task),
        PlannedTaskKind::HashBookPages => hash_book_pages_payload(task),
        PlannedTaskKind::RebuildIndex => rebuild_index_payload(task),
        PlannedTaskKind::UpgradeIndex => upgrade_index_payload(task),
        PlannedTaskKind::DeleteBook => delete_book_payload(task),
        PlannedTaskKind::DeleteSeries => delete_series_payload(task),
        PlannedTaskKind::FindBooksToConvert => find_books_to_convert_payload(task),
        PlannedTaskKind::ConvertBook => convert_book_payload(task),
        PlannedTaskKind::RemoveHashedPages => None,
    };

    compatibility_payload
        .or_else(|| task.payload.clone())
        .unwrap_or_else(|| fallback_task_payload(task))
}

#[cfg(test)]
mod tests {
    use super::*;
    use komga_application::task_processing::TaskDescriptor;
    use serde_json::Value;

    #[derive(Clone, Copy, Debug)]
    struct UpgradeIndexCatalog;

    impl TaskProtocolCatalog for UpgradeIndexCatalog {
        fn descriptor(&self, kind: PlannedTaskKind) -> TaskDescriptor {
            match kind {
                PlannedTaskKind::UpgradeIndex => TaskDescriptor {
                    runtime_simple_type: "CUSTOM_UPGRADE_INDEX",
                    persisted_simple_type: "LegacyUpgradeIndex",
                    persisted_class_name: "custom.tasks.LegacyUpgradeIndex",
                },
                _ => kind.descriptor(),
            }
        }

        fn known_kind_from_runtime_simple_type(
            &self,
            simple_type: &str,
        ) -> Option<PlannedTaskKind> {
            match simple_type {
                "CUSTOM_UPGRADE_INDEX" => Some(PlannedTaskKind::UpgradeIndex),
                _ => None,
            }
        }

        fn known_kind_from_persisted_simple_type(
            &self,
            simple_type: &str,
        ) -> Option<PlannedTaskKind> {
            match simple_type {
                "LegacyUpgradeIndex" => Some(PlannedTaskKind::UpgradeIndex),
                _ => None,
            }
        }
    }

    fn persisted_row(task: PersistedTaskStoreRecord) -> PersistedTaskRowShape {
        compatibility().persisted_row_from_runtime_record(&task)
    }

    #[test]
    fn compatibility_uses_catalog_descriptor_when_persisting_known_runtime_task() {
        let row = PersistedTaskCompatibility::new(UpgradeIndexCatalog)
            .persisted_row_from_runtime_record(&PersistedTaskStoreRecord {
                id: "UPGRADE_INDEX".to_string(),
                simple_type: "CUSTOM_UPGRADE_INDEX".to_string(),
                priority: 9,
                group: None,
                payload: None,
                owner: None,
            });

        assert_eq!(row.simple_type, "LegacyUpgradeIndex");
        assert_eq!(row.class_name, "custom.tasks.LegacyUpgradeIndex");
    }

    #[test]
    fn compatibility_uses_catalog_descriptor_when_loading_known_persisted_task() {
        let record = PersistedTaskCompatibility::new(UpgradeIndexCatalog)
            .runtime_record_from_persisted_row(PersistedTaskRowShape {
                id: "UPGRADE_INDEX".to_string(),
                priority: 9,
                group: None,
                class_name: "custom.tasks.LegacyUpgradeIndex".to_string(),
                simple_type: "LegacyUpgradeIndex".to_string(),
                payload: json!({
                    "id": "UPGRADE_INDEX",
                    "simpleType": "CUSTOM_UPGRADE_INDEX",
                    "priority": 9,
                    "groupId": Value::Null,
                })
                .to_string(),
                owner: Some("rust-main".to_string()),
            });

        assert_eq!(record.simple_type, "CUSTOM_UPGRADE_INDEX");
        assert_eq!(record.owner.as_deref(), Some("rust-main"));
    }

    #[test]
    fn persisted_find_duplicate_pages_to_delete_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
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
    fn persisted_find_books_to_convert_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "FIND_BOOKS_TO_CONVERT_library-1".to_string(),
            simple_type: "FIND_BOOKS_TO_CONVERT".to_string(),
            priority: 0,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "FIND_BOOKS_TO_CONVERT_library-1");
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
                "uniqueId": "FIND_BOOKS_TO_CONVERT_library-1"
            })
        );
    }

    #[test]
    fn persisted_rebuild_index_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
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
    fn persisted_refresh_series_metadata_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "REFRESH_SERIES_METADATA_series-1".to_string(),
            simple_type: "REFRESH_SERIES_METADATA".to_string(),
            priority: 5,
            group: Some("series-1".to_string()),
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "REFRESH_SERIES_METADATA_series-1");
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
                "uniqueId": "REFRESH_SERIES_METADATA_series-1"
            })
        );
    }

    #[test]
    fn persisted_aggregate_series_metadata_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "AGGREGATE_SERIES_METADATA_series-1".to_string(),
            simple_type: "AGGREGATE_SERIES_METADATA".to_string(),
            priority: 6,
            group: Some("series-1".to_string()),
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "AGGREGATE_SERIES_METADATA_series-1");
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
                "uniqueId": "AGGREGATE_SERIES_METADATA_series-1"
            })
        );
    }

    #[test]
    fn persisted_upgrade_index_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
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
                "priority": 9,
                "groupId": Value::Null,
                "uniqueId": "UPGRADE_INDEX"
            })
        );
    }

    #[test]
    fn persisted_find_book_thumbnails_to_regenerate_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
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
    fn persisted_import_book_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
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
    fn persisted_convert_book_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "CONVERT_BOOK_book-1".to_string(),
            simple_type: "CONVERT_BOOK".to_string(),
            priority: 7,
            group: Some("series-1".to_string()),
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "CONVERT_BOOK_book-1");
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
                "uniqueId": "CONVERT_BOOK_book-1"
            })
        );
    }

    #[test]
    fn persisted_refresh_book_local_artwork_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
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
    fn persisted_refresh_series_local_artwork_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "REFRESH_SERIES_LOCAL_ARTWORK_series-1".to_string(),
            simple_type: "REFRESH_SERIES_LOCAL_ARTWORK".to_string(),
            priority: 80,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "REFRESH_SERIES_LOCAL_ARTWORK_series-1");
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
                "uniqueId": "REFRESH_SERIES_LOCAL_ARTWORK_series-1"
            })
        );
    }

    #[test]
    fn persisted_repair_extension_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
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
    fn persisted_hash_book_pages_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
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
    fn persisted_hash_book_koreader_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
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
    fn persisted_delete_book_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "DELETE_BOOK_book-1".to_string(),
            simple_type: "DELETE_BOOK".to_string(),
            priority: 8,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "DELETE_BOOK_book-1");
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
                "uniqueId": "DELETE_BOOK_book-1"
            })
        );
    }

    #[test]
    fn persisted_delete_series_uses_kotlin_task_shape() {
        let row = persisted_row(PersistedTaskStoreRecord {
            id: "DELETE_SERIES_series-1".to_string(),
            simple_type: "DELETE_SERIES".to_string(),
            priority: 8,
            group: None,
            payload: None,
            owner: None,
        });

        assert_eq!(row.id, "DELETE_SERIES_series-1");
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
                "uniqueId": "DELETE_SERIES_series-1"
            })
        );
    }
}
