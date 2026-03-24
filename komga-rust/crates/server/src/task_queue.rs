use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use crate::config::{RuntimeConfig, WriterDecision, WriterKind};
use komga_persistence::sqlite::connect_pool;
use serde_json::json;
use sqlx::{Row, SqlitePool};

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskQueueRecord {
    pub id: String,
    pub simple_type: String,
    pub priority: i32,
    pub group: Option<String>,
    pub owner: Option<String>,
    order: usize,
}

impl TaskQueueRecord {
    pub fn new(id: impl Into<String>, priority: i32, group: Option<String>) -> Self {
        let id = id.into();
        Self {
            simple_type: id
                .split_once(':')
                .map(|(task_type, _)| task_type)
                .unwrap_or(id.as_str())
                .to_string(),
            id,
            priority,
            group,
            owner: None,
            order: 0,
        }
    }

    pub fn with_simple_type(mut self, simple_type: impl Into<String>) -> Self {
        self.simple_type = simple_type.into();
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct TaskQueueAdmin {
    tasks: Vec<TaskQueueRecord>,
    next_order: usize,
}

impl TaskQueueAdmin {
    pub fn enqueue(&mut self, mut task: TaskQueueRecord) {
        task.order = self.next_order;
        self.next_order += 1;
        self.tasks.push(task);
    }

    pub fn claim(&mut self, task_id: &str, owner: &str) -> bool {
        match self.tasks.iter_mut().find(|task| task.id == task_id) {
            Some(task) => {
                task.owner = Some(owner.to_string());
                true
            }
            None => false,
        }
    }

    pub fn complete(&mut self, task_id: &str) -> bool {
        let original = self.tasks.len();
        self.tasks.retain(|task| task.id != task_id);
        self.tasks.len() != original
    }

    pub fn clear_unowned(&mut self) -> usize {
        let original = self.tasks.len();
        self.tasks.retain(|task| task.owner.is_some());
        original - self.tasks.len()
    }

    pub fn disown_all(&mut self) -> usize {
        let mut disowned = 0;
        for task in &mut self.tasks {
            if task.owner.take().is_some() {
                disowned += 1;
            }
        }
        disowned
    }

    pub fn count_by_simple_type(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for task in &self.tasks {
            *counts.entry(task.simple_type.clone()).or_insert(0) += 1;
        }
        counts
    }

    pub fn read_grouped_by_owner(&self) -> BTreeMap<Option<String>, Vec<TaskQueueRecord>> {
        let mut grouped: BTreeMap<Option<String>, Vec<TaskQueueRecord>> = BTreeMap::new();
        for task in &self.tasks {
            grouped
                .entry(task.owner.clone())
                .or_default()
                .push(task.clone());
        }
        grouped
    }

    fn take_available(&mut self, owner: &str) -> Option<TaskQueueRecord> {
        let mut locked_groups = std::collections::BTreeSet::new();
        for task in &self.tasks {
            if task.owner.is_some() {
                if let Some(group) = &task.group {
                    locked_groups.insert(group.clone());
                }
            }
        }

        let selected_index = self
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, task)| {
                task.owner.is_none()
                    && task
                        .group
                        .as_ref()
                        .is_none_or(|group| !locked_groups.contains(group))
            })
            .max_by(|(_, left), (_, right)| {
                left.priority
                    .cmp(&right.priority)
                    .then_with(|| right.order.cmp(&left.order))
            })
            .map(|(index, _)| index)?;

        let task = self.tasks.get_mut(selected_index)?;
        task.owner = Some(owner.to_string());
        Some(task.clone())
    }
}

#[derive(Clone, Debug)]
pub struct TaskQueueScheduler {
    admin: TaskQueueAdmin,
    consumer_owner: String,
    consumes_queue: bool,
    persisted_store: Option<PersistedTaskStore>,
    task_pool_size: usize,
}

impl TaskQueueScheduler {
    pub fn for_runtime(config: RuntimeConfig, consumer_owner: impl Into<String>) -> Self {
        let consumes_queue = matches!(
            config.writer_decision(WriterKind::TasksDatabase),
            WriterDecision::Allowed | WriterDecision::Isolated
        );
        let persisted_store = PersistedTaskStore::new(config.tasks_db_file.clone());
        let admin = persisted_store
            .as_ref()
            .map(PersistedTaskStore::load_admin)
            .unwrap_or_default();

        Self {
            admin,
            consumer_owner: consumer_owner.into(),
            consumes_queue,
            persisted_store,
            task_pool_size: 1,
        }
    }

    pub fn enqueue(&mut self, task: TaskQueueRecord) {
        if let Some(store) = &self.persisted_store {
            store.persist_task(&task);
        }
        self.admin.enqueue(task);
    }

    pub fn take_next(&mut self) -> Option<TaskQueueRecord> {
        if !self.consumes_queue {
            return None;
        }

        let task = self.admin.take_available(&self.consumer_owner)?;
        if let Some(store) = &self.persisted_store {
            store.claim_task(&task.id, &self.consumer_owner);
        }

        Some(task)
    }

    pub fn take_available_batch(&mut self) -> Vec<TaskQueueRecord> {
        if !self.consumes_queue {
            return Vec::new();
        }

        if self.task_pool_size <= 1 {
            return self.take_next().into_iter().collect();
        }

        let mut selected = Vec::new();
        while selected.len() < self.task_pool_size {
            let Some(task) = self.admin.take_available(&self.consumer_owner) else {
                break;
            };
            if let Some(store) = &self.persisted_store {
                store.claim_task(&task.id, &self.consumer_owner);
            }
            selected.push(task);
        }
        selected
    }

    pub fn complete(&mut self, task_id: &str) -> bool {
        let removed = self.admin.complete(task_id);
        if let Some(store) = &self.persisted_store {
            return store.delete_task(task_id);
        }

        removed
    }

    pub fn admin(&self) -> &TaskQueueAdmin {
        &self.admin
    }

    pub fn admin_mut(&mut self) -> &mut TaskQueueAdmin {
        &mut self.admin
    }

    pub fn task_pool_size(&self) -> usize {
        self.task_pool_size
    }

    pub fn set_task_pool_size(&mut self, task_pool_size: usize) {
        self.task_pool_size = task_pool_size.max(1);
    }

    pub fn disown_all(&mut self) -> usize {
        let disowned = self.admin.disown_all();
        if let Some(store) = &self.persisted_store {
            store.disown_all();
        }

        disowned
    }

    pub fn clear_unowned(&mut self) -> usize {
        if let Some(store) = &self.persisted_store {
            let deleted = store.clear_unowned();
            self.admin = store.load_admin();
            return deleted;
        }

        self.admin.clear_unowned()
    }

    pub fn count_by_simple_type(&self) -> BTreeMap<String, usize> {
        self.admin.count_by_simple_type()
    }
}

#[derive(Clone, Debug)]
struct PersistedTaskStore {
    tasks_db_file: PathBuf,
}

impl PersistedTaskStore {
    fn new(tasks_db_file: PathBuf) -> Option<Self> {
        tasks_db_file.exists().then_some(Self { tasks_db_file })
    }

    fn load_admin(&self) -> TaskQueueAdmin {
        let records = self.run(|pool| {
            Box::pin(async move {
                let rows = sqlx::query(
                    "SELECT ID, PRIORITY, GROUP_ID, SIMPLE_TYPE, OWNER\n                     FROM TASK\n                     ORDER BY CREATED_DATE ASC, ID ASC",
                )
                .fetch_all(&pool)
                .await
                .expect("persisted task queue rows should be readable");

                rows.into_iter()
                    .map(|row| TaskQueueRecord {
                        id: row.get::<String, _>("ID"),
                        priority: row.get::<i64, _>("PRIORITY") as i32,
                        group: row.get::<Option<String>, _>("GROUP_ID"),
                        simple_type: row.get::<String, _>("SIMPLE_TYPE"),
                        owner: row.get::<Option<String>, _>("OWNER"),
                        order: 0,
                    })
                    .collect::<Vec<_>>()
            })
        });

        let mut admin = TaskQueueAdmin::default();
        for task in records {
            admin.enqueue(task);
        }
        admin
    }

    fn persist_task(&self, task: &TaskQueueRecord) {
        let row = PersistedTaskRow::from_record(task);
        self.run(move |pool| {
            Box::pin(async move {
                sqlx::query(
                    "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER)\n                     VALUES (?, ?, ?, ?, ?, ?, ?)\n                     ON CONFLICT(ID) DO UPDATE SET\n                       PRIORITY = excluded.PRIORITY,\n                       GROUP_ID = excluded.GROUP_ID,\n                       CLASS = excluded.CLASS,\n                       SIMPLE_TYPE = excluded.SIMPLE_TYPE,\n                       PAYLOAD = excluded.PAYLOAD,\n                       OWNER = excluded.OWNER,\n                       LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
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

    fn claim_task(&self, task_id: &str, owner: &str) {
        let task_id = task_id.to_string();
        let owner = owner.to_string();
        self.run(move |pool| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE TASK\n                     SET OWNER = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP\n                     WHERE ID = ?",
                )
                .bind(owner)
                .bind(task_id)
                .execute(&pool)
                .await
                .expect("claimed task owner should persist to TASK table");
            })
        });
    }

    fn delete_task(&self, task_id: &str) -> bool {
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

    fn disown_all(&self) {
        self.run(|pool| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE TASK\n                     SET OWNER = NULL, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP\n                     WHERE OWNER IS NOT NULL",
                )
                .execute(&pool)
                .await
                .expect("owned task rows should be disowned in TASK table");
            })
        });
    }

    fn clear_unowned(&self) -> usize {
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

    fn run<T>(
        &self,
        operation: impl FnOnce(SqlitePool) -> BoxFuture<T> + Send + 'static,
    ) -> T
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
                let pool = connect_pool(&tasks_db_file, 1)
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
    fn from_record(task: &TaskQueueRecord) -> Self {
        Self {
            id: task.id.clone(),
            priority: task.priority,
            group: task.group.clone(),
            class_name: kotlin_compat_class_name(&task.simple_type),
            simple_type: task.simple_type.clone(),
            payload: task_payload(task),
            owner: task.owner.clone(),
        }
    }
}

fn kotlin_compat_class_name(simple_type: &str) -> String {
    format!("org.gotson.komga.task.{}.CompatTask", simple_type.to_ascii_lowercase())
}

fn task_payload(task: &TaskQueueRecord) -> String {
    json!({
        "id": task.id,
        "simpleType": task.simple_type,
        "priority": task.priority,
        "groupId": task.group,
    })
    .to_string()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LibraryScanInterval {
    Disabled,
    Hourly,
    Every6h,
    Every12h,
    Daily,
    Weekly,
}

impl LibraryScanInterval {
    pub fn duration(self) -> Option<Duration> {
        match self {
            Self::Disabled => None,
            Self::Hourly => Some(Duration::from_secs(60 * 60)),
            Self::Every6h => Some(Duration::from_secs(6 * 60 * 60)),
            Self::Every12h => Some(Duration::from_secs(12 * 60 * 60)),
            Self::Daily => Some(Duration::from_secs(24 * 60 * 60)),
            Self::Weekly => Some(Duration::from_secs(7 * 24 * 60 * 60)),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledLibraryScan {
    pub library_id: String,
    pub interval: LibraryScanInterval,
}

#[derive(Clone, Debug, Default)]
pub struct LibraryScanScheduler {
    registry: HashMap<String, ScheduledLibraryScan>,
}

impl LibraryScanScheduler {
    pub fn schedule_scan(&mut self, library_id: impl Into<String>, interval: LibraryScanInterval) {
        let library_id = library_id.into();
        if interval == LibraryScanInterval::Disabled {
            self.registry.remove(&library_id);
            return;
        }

        self.registry.insert(
            library_id.clone(),
            ScheduledLibraryScan {
                library_id,
                interval,
            },
        );
    }

    pub fn scheduled_tasks(&self) -> Vec<ScheduledLibraryScan> {
        let mut tasks = self.registry.values().cloned().collect::<Vec<_>>();
        tasks.sort_by(|left, right| left.library_id.cmp(&right.library_id));
        tasks
    }
}
