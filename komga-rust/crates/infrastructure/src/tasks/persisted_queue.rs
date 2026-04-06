use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::json;
use sqlx::{Row, SqlitePool};

use crate::sqlite::connect_pool;

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
    pool: Arc<Mutex<SqlitePool>>,
}

impl SqliteTaskQueueStore {
    pub fn new(tasks_db_file: PathBuf) -> Option<Self> {
        if !tasks_db_file.exists() {
            return None;
        }

        let pool = open_sqlite_pool_blocking(tasks_db_file.clone())?;
        Some(Self {
            tasks_db_file,
            pool: Arc::new(Mutex::new(pool)),
        })
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

    fn shared_pool(&self) -> SqlitePool {
        let mut pool = self
            .pool
            .lock()
            .expect("persisted task pool lock should not be poisoned");
        if pool.is_closed() {
            *pool = open_sqlite_pool_blocking(self.tasks_db_file.clone())
                .expect("tasks sqlite pool should reopen for task persistence");
        }
        pool.clone()
    }

    fn run<T>(&self, operation: impl FnOnce(SqlitePool) -> BoxFuture<T> + Send + 'static) -> T
    where
        T: Send + 'static,
    {
        let pool = self.shared_pool();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("persisted task runtime should build");

            runtime.block_on(async move { operation(pool).await })
        })
        .join()
        .expect("persisted task worker thread should complete")
    }
}

type BoxFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send>>;

fn open_sqlite_pool_blocking(database_file: PathBuf) -> Option<SqlitePool> {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .ok()?;

        runtime.block_on(async move { connect_pool(&database_file, 1).await.ok() })
    })
    .join()
    .ok()
    .flatten()
}

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
        "REMOVE_HASHED_PAGES" => "RemoveHashedPages".to_string(),
        _ => simple_type.to_string(),
    }
}

fn runtime_simple_type(simple_type: &str) -> String {
    match simple_type {
        "RemoveHashedPages" => "REMOVE_HASHED_PAGES".to_string(),
        _ => simple_type.to_string(),
    }
}

fn kotlin_task_class_name(simple_type: &str) -> String {
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

fn persisted_task_payload(task: &PersistedTaskStoreRecord) -> String {
    task.payload
        .clone()
        .unwrap_or_else(|| default_task_payload(task))
}
