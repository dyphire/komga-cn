use std::path::PathBuf;

use komga_application::task_processing::{
    OpaqueTask, PersistedTaskRowShape, TaskKind, TaskQueueRecord,
};
use sqlx::Row;
use sqlx::SqlitePool;

use super::task_identity::{
    PersistedTaskStoreRecord, fallback_task_payload, persisted_payload_for_known_task,
    runtime_task_class_name,
};
use crate::sqlite::{connect_shared_pool, default_read_max_connections};

#[derive(Clone, Debug)]
pub struct SqliteTaskQueueStore {
    tasks_pool: SqlitePool,
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
    pub async fn new(tasks_db_file: PathBuf) -> Option<Self> {
        if !tasks_db_file.exists() {
            return None;
        }

        let tasks_pool = connect_shared_pool(&tasks_db_file, default_read_max_connections())
            .await
            .expect("tasks sqlite pool should open for task persistence");

        Some(Self { tasks_pool })
    }

    pub async fn load_records(&self) -> Vec<PersistedTaskStoreRecord> {
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
        .fetch_all(&self.tasks_pool)
        .await
        .expect("persisted task queue rows should be readable");

        rows.into_iter()
            .map(persisted_row_shape)
            .map(runtime_record_from_persisted_row)
            .map(store_record_from_runtime_record)
            .collect::<Vec<_>>()
    }

    pub async fn persist_task(&self, task: &PersistedTaskStoreRecord) {
        let row = persisted_row_from_runtime_record(task);
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
        .execute(&self.tasks_pool)
        .await
        .expect("queued task rows should persist to TASK table");
    }

    pub async fn claim_task(&self, task_id: &str, owner: &str) {
        let task_id = task_id.to_string();
        let owner = owner.to_string();
        sqlx::query(
            r#"UPDATE TASK
            SET OWNER = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
            WHERE ID = ?"#,
        )
        .bind(owner)
        .bind(task_id)
        .execute(&self.tasks_pool)
        .await
        .expect("claimed task owner should persist to TASK table");
    }

    pub async fn delete_task(&self, task_id: &str) -> bool {
        let task_id = task_id.to_string();
        sqlx::query("DELETE FROM TASK WHERE ID = ?")
            .bind(task_id)
            .execute(&self.tasks_pool)
            .await
            .expect("completed task rows should be deleted from TASK table")
            .rows_affected()
            > 0
    }

    pub async fn disown_all(&self) {
        sqlx::query(
            r#"UPDATE TASK
            SET OWNER = NULL, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
            WHERE OWNER IS NOT NULL"#,
        )
        .execute(&self.tasks_pool)
        .await
        .expect("owned task rows should be disowned in TASK table");
    }

    pub async fn clear_unowned(&self) -> usize {
        sqlx::query("DELETE FROM TASK WHERE OWNER IS NULL")
            .execute(&self.tasks_pool)
            .await
            .expect("unowned task rows should be deleted from TASK table")
            .rows_affected() as usize
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
        payload: persisted_payload_for_known_task(kind, task),
        owner: task.owner.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_persisted_row_uses_kotlin_class_name() {
        let task = PersistedTaskStoreRecord {
            id: "AnalyzeBook_book-1".to_string(),
            simple_type: "AnalyzeBook".to_string(),
            priority: 6,
            group: None,
            payload: None,
            owner: None,
        };
        let kind = TaskKind::parse(&task.simple_type).unwrap();
        let row = known_persisted_row(kind, &task);
        assert_eq!(
            row.class_name,
            "org.gotson.komga.application.tasks.Task$AnalyzeBook"
        );
        assert_eq!(row.simple_type, "AnalyzeBook");
    }
}
