use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;

use crate::config::{RuntimeConfig, WriterDecision, WriterKind};
use crate::search::{SearchDocument, SearchEntityType, SearchEvent, SearchIndexLifecycle};
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

    pub fn disown(&mut self, task_id: &str) -> bool {
        let Some(task) = self.tasks.iter_mut().find(|task| task.id == task_id) else {
            return false;
        };
        task.owner = None;
        true
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

    pub fn process_available(
        &mut self,
        runtime: &RuntimeConfig,
    ) -> Result<usize, TaskExecutionError> {
        if !self.consumes_queue {
            return Ok(0);
        }

        let mut processed = 0usize;
        loop {
            let batch = self.take_available_batch();
            if batch.is_empty() {
                return Ok(processed);
            }

            for task in batch {
                match self.execute_claimed_task(runtime, &task) {
                    Ok(()) => {
                        let _ = self.complete(&task.id);
                        processed += 1;
                    }
                    Err(error) => {
                        self.disown_task(&task.id);
                        return Err(error);
                    }
                }
            }
        }
    }

    pub fn recover_and_process(
        &mut self,
        runtime: &RuntimeConfig,
    ) -> Result<usize, TaskExecutionError> {
        self.disown_all();
        self.process_available(runtime)
    }

    fn disown_task(&mut self, task_id: &str) {
        let _ = self.admin.disown(task_id);
        if let Some(store) = &self.persisted_store {
            store.disown_task(task_id);
        }
    }

    fn execute_claimed_task(
        &mut self,
        runtime: &RuntimeConfig,
        task: &TaskQueueRecord,
    ) -> Result<(), TaskExecutionError> {
        let task_target = task.id.split_once(':').map(|(_, value)| value.to_string());
        match task.simple_type.as_str() {
            "SCAN_LIBRARY" => {
                let Some(library_id) = task_target else {
                    return Err(TaskExecutionError::invalid_task(
                        "SCAN_LIBRARY task must include a library id",
                    ));
                };
                let scan = scan_library(runtime, &library_id)?;
                persist_scanned_library(runtime, &library_id, &scan)?;

                for book_id in scan.book_ids {
                    self.enqueue(TaskQueueRecord::new(
                        format!("ANALYZE_BOOK:{book_id}"),
                        task.priority.saturating_sub(10),
                        Some(book_id),
                    ));
                }
                Ok(())
            }
            "ANALYZE_BOOK" => {
                let Some(book_id) = task_target else {
                    return Err(TaskExecutionError::invalid_task(
                        "ANALYZE_BOOK task must include a book id",
                    ));
                };
                analyze_book(runtime, &book_id)
            }
            "REBUILD_INDEX" | "UPGRADE_INDEX" => rebuild_index(runtime),
            "EMPTY_TRASH"
            | "REFRESH_BOOK_METADATA"
            | "REFRESH_BOOK_LOCAL_ARTWORK"
            | "REFRESH_SERIES_LOCAL_ARTWORK" => Ok(()),
            other => Err(TaskExecutionError::unsupported_task(other)),
        }
    }
}

#[derive(Debug)]
pub struct TaskExecutionError {
    message: String,
}

impl TaskExecutionError {
    fn invalid_task(message: &str) -> Self {
        Self {
            message: message.to_string(),
        }
    }

    fn unsupported_task(task_type: &str) -> Self {
        Self {
            message: format!("unsupported runtime task type: {task_type}"),
        }
    }

    fn runtime(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for TaskExecutionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for TaskExecutionError {}

#[derive(Clone, Debug)]
struct ScannedLibrary {
    series_rows: Vec<ScannedSeriesRow>,
    sidecars: Vec<ScannedSidecarRow>,
    book_ids: Vec<String>,
}

#[derive(Clone, Debug)]
struct ScannedSeriesRow {
    series_id: String,
    series_name: String,
    series_url: String,
    series_last_modified_unix_seconds: i64,
    books: Vec<ScannedBookRow>,
}

#[derive(Clone, Debug)]
struct ScannedBookRow {
    book_id: String,
    book_name: String,
    book_url: String,
    file_name: String,
    file_size: i64,
    file_last_modified_unix_seconds: i64,
}

#[derive(Clone, Debug)]
struct ScannedSidecarRow {
    url: String,
    parent_url: String,
    last_modified_unix_seconds: i64,
}

fn scan_library(
    runtime: &RuntimeConfig,
    library_id: &str,
) -> Result<ScannedLibrary, TaskExecutionError> {
    let root = load_library_root(runtime, library_id)?;
    let Some(root) = root else {
        return Ok(ScannedLibrary {
            series_rows: Vec::new(),
            sidecars: Vec::new(),
            book_ids: Vec::new(),
        });
    };

    let root = PathBuf::from(root);
    if !root.exists() {
        return Ok(ScannedLibrary {
            series_rows: Vec::new(),
            sidecars: Vec::new(),
            book_ids: Vec::new(),
        });
    }

    let mut discovered = Vec::new();
    collect_series_directories(&root, &mut discovered)?;

    let mut sidecars = Vec::new();
    let mut series_rows = Vec::new();
    let mut book_ids = Vec::new();

    for series_dir in discovered {
        let Ok(entries) = fs::read_dir(&series_dir) else {
            continue;
        };

        let mut books = Vec::new();
        let mut sidecar_candidates = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };

            if metadata.is_file() {
                if is_supported_book_file(&path) {
                    let book_id = route_safe_scanner_id("book", &path);
                    let book_url = path.to_string_lossy().to_string();
                    let book_name = path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default()
                        .to_string();
                    let file_name = path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default()
                        .to_string();
                    books.push(ScannedBookRow {
                        book_id: book_id.clone(),
                        book_name,
                        book_url,
                        file_name,
                        file_size: metadata.len() as i64,
                        file_last_modified_unix_seconds: to_unix_seconds(metadata.modified().ok()),
                    });
                    book_ids.push(book_id);
                    continue;
                }

                sidecar_candidates.push((path, metadata));
            }
        }

        if books.is_empty() {
            continue;
        }

        let series_id = route_safe_scanner_id("series", &series_dir);
        let series_url = series_dir.to_string_lossy().to_string();
        let series_name = series_dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();

        let mut series_sidecars = Vec::new();
        for (path, metadata) in &sidecar_candidates {
            let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };

            if file_name.eq_ignore_ascii_case("ComicInfo.xml") {
                series_sidecars.push(ScannedSidecarRow {
                    url: path.to_string_lossy().to_string(),
                    parent_url: series_url.clone(),
                    last_modified_unix_seconds: to_unix_seconds(metadata.modified().ok()),
                });
                continue;
            }

            for book in &books {
                let expected = format!("{}.xml", book.book_name);
                if file_name.eq_ignore_ascii_case(&expected) {
                    series_sidecars.push(ScannedSidecarRow {
                        url: path.to_string_lossy().to_string(),
                        parent_url: book.book_url.clone(),
                        last_modified_unix_seconds: to_unix_seconds(metadata.modified().ok()),
                    });
                }
            }
        }

        sidecars.extend(series_sidecars);
        series_rows.push(ScannedSeriesRow {
            series_id: series_id.clone(),
            series_name,
            series_url,
            series_last_modified_unix_seconds: to_unix_seconds(
                fs::metadata(&series_dir)
                    .ok()
                    .and_then(|value| value.modified().ok()),
            ),
            books,
        });
    }

    Ok(ScannedLibrary {
        series_rows,
        sidecars,
        book_ids,
    })
}

fn collect_series_directories(
    current: &PathBuf,
    discovered: &mut Vec<PathBuf>,
) -> Result<(), TaskExecutionError> {
    let entries = fs::read_dir(current).map_err(|error| {
        TaskExecutionError::runtime(format!(
            "failed to scan directory '{}': {error}",
            current.display()
        ))
    })?;

    let mut has_supported_book = false;
    let mut children = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_file() && is_supported_book_file(&path) {
            has_supported_book = true;
        }
        if metadata.is_dir() {
            children.push(path);
        }
    }

    if has_supported_book {
        discovered.push(current.clone());
    }

    for child in children {
        collect_series_directories(&child, discovered)?;
    }

    Ok(())
}

fn is_supported_book_file(path: &PathBuf) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "cbz" | "zip" | "cbr" | "rar" | "pdf" | "epub"
            )
        })
}

fn route_safe_scanner_id(prefix: &str, path: &std::path::Path) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{prefix}-{:016x}", hasher.finish())
}

fn persist_scanned_library(
    runtime: &RuntimeConfig,
    library_id: &str,
    scanned: &ScannedLibrary,
) -> Result<(), TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let library_id = library_id.to_string();
    let scanned = scanned.clone();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            for series in &scanned.series_rows {
                sqlx::query(
                    "INSERT OR IGNORE INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot) VALUES (?, ?, ?, ?, ?, 0)",
                )
                .bind(&series.series_id)
                .bind(series.series_last_modified_unix_seconds)
                .bind(&series.series_name)
                .bind(&series.series_url)
                .bind(&library_id)
                .execute(&pool)
                .await
                .map_err(|error| TaskExecutionError::runtime(format!("failed to persist SERIES rows: {error}")))?;

                for book in &series.books {
                    sqlx::query(
                        "INSERT OR IGNORE INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, LIBRARY_ID, oneshot) VALUES (?, ?, ?, ?, ?, ?, ?, 0)",
                    )
                    .bind(&book.book_id)
                    .bind(book.file_last_modified_unix_seconds)
                    .bind(&book.file_name)
                    .bind(&book.book_url)
                    .bind(&series.series_id)
                    .bind(book.file_size)
                    .bind(&library_id)
                    .execute(&pool)
                    .await
                    .map_err(|error| TaskExecutionError::runtime(format!("failed to persist BOOK rows: {error}")))?;

                    sqlx::query(
                        "INSERT INTO MEDIA_FILE (FILE_NAME, BOOK_ID, FILE_SIZE) SELECT ?, ?, ? WHERE NOT EXISTS (SELECT 1 FROM MEDIA_FILE WHERE FILE_NAME = ? AND BOOK_ID = ?)",
                    )
                    .bind(&book.file_name)
                    .bind(&book.book_id)
                    .bind(book.file_size)
                    .bind(&book.file_name)
                    .bind(&book.book_id)
                    .execute(&pool)
                    .await
                    .map_err(|error| TaskExecutionError::runtime(format!("failed to persist MEDIA_FILE rows: {error}")))?;
                }
            }

            for sidecar in &scanned.sidecars {
                sqlx::query(
                    "INSERT OR IGNORE INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
                )
                .bind(&sidecar.url)
                .bind(&sidecar.parent_url)
                .bind(sidecar.last_modified_unix_seconds)
                .bind(&library_id)
                .execute(&pool)
                .await
                .map_err(|error| TaskExecutionError::runtime(format!("failed to persist SIDECAR rows: {error}")))?;
            }

            Ok::<(), TaskExecutionError>(())
        })
    })
}

fn analyze_book(runtime: &RuntimeConfig, book_id: &str) -> Result<(), TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let book_id = book_id.to_string();
    let lucene_data_directory = runtime.lucene_data_directory.clone();

    let doc = run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT b.ID AS ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, b.URL AS URL FROM BOOK b LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID WHERE b.ID = ? LIMIT 1",
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| TaskExecutionError::runtime(format!("failed to load BOOK row for analyze: {error}")))?;

            let Some(row) = row else {
                return Ok::<Option<SearchDocument>, TaskExecutionError>(None);
            };

            let title = row.get::<String, _>("TITLE");
            let url = row.get::<String, _>("URL");
            let media_type = media_type_from_path(&url);

            sqlx::query(
                "INSERT INTO MEDIA (BOOK_ID, STATUS, MEDIA_TYPE, PAGE_COUNT) VALUES (?, 'READY', ?, 1) ON CONFLICT(BOOK_ID) DO UPDATE SET STATUS = 'READY', MEDIA_TYPE = excluded.MEDIA_TYPE, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
            )
            .bind(&book_id)
            .bind(&media_type)
            .execute(&pool)
            .await
            .map_err(|error| TaskExecutionError::runtime(format!("failed to persist MEDIA analyze state: {error}")))?;

            Ok(Some(SearchDocument {
                entity_type: SearchEntityType::Book,
                id: book_id,
                title,
            }))
        })
    })?;

    if let Some(doc) = doc {
        let index =
            SearchIndexLifecycle::bootstrap(lucene_data_directory.as_path()).map_err(|error| {
                TaskExecutionError::runtime(format!("failed to bootstrap search index: {error}"))
            })?;
        index
            .apply_event(SearchEvent::Upsert(doc))
            .map_err(|error| {
                TaskExecutionError::runtime(format!("failed to upsert search document: {error}"))
            })?;
    }

    Ok(())
}

fn rebuild_index(runtime: &RuntimeConfig) -> Result<(), TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let lucene_data_directory = runtime.lucene_data_directory.clone();

    let docs = run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let mut docs = Vec::new();

            let book_rows = sqlx::query(
                "SELECT b.ID AS ID, COALESCE(bm.TITLE, b.NAME) AS TITLE FROM BOOK b LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID",
            )
            .fetch_all(&pool)
            .await
            .map_err(|error| TaskExecutionError::runtime(format!("failed to read BOOK rows for index rebuild: {error}")))?;
            for row in book_rows {
                docs.push(SearchDocument {
                    entity_type: SearchEntityType::Book,
                    id: row.get::<String, _>("ID"),
                    title: row.get::<String, _>("TITLE"),
                });
            }

            let series_rows = sqlx::query(
                "SELECT s.ID AS ID, COALESCE(sm.TITLE, s.NAME) AS TITLE FROM SERIES s LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID",
            )
            .fetch_all(&pool)
            .await
            .map_err(|error| TaskExecutionError::runtime(format!("failed to read SERIES rows for index rebuild: {error}")))?;
            for row in series_rows {
                docs.push(SearchDocument {
                    entity_type: SearchEntityType::Series,
                    id: row.get::<String, _>("ID"),
                    title: row.get::<String, _>("TITLE"),
                });
            }

            let collection_rows = sqlx::query("SELECT ID, NAME FROM COLLECTION")
                .fetch_all(&pool)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to read COLLECTION rows for index rebuild: {error}"
                    ))
                })?;
            for row in collection_rows {
                docs.push(SearchDocument {
                    entity_type: SearchEntityType::Collection,
                    id: row.get::<String, _>("ID"),
                    title: row.get::<String, _>("NAME"),
                });
            }

            let readlist_rows = sqlx::query("SELECT ID, NAME FROM READLIST")
                .fetch_all(&pool)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to read READLIST rows for index rebuild: {error}"
                    ))
                })?;
            for row in readlist_rows {
                docs.push(SearchDocument {
                    entity_type: SearchEntityType::ReadList,
                    id: row.get::<String, _>("ID"),
                    title: row.get::<String, _>("NAME"),
                });
            }

            Ok::<Vec<SearchDocument>, TaskExecutionError>(docs)
        })
    })?;

    let index =
        SearchIndexLifecycle::bootstrap(lucene_data_directory.as_path()).map_err(|error| {
            TaskExecutionError::runtime(format!("failed to bootstrap search index: {error}"))
        })?;
    index.rebuild(&docs).map_err(|error| {
        TaskExecutionError::runtime(format!("failed to rebuild search index: {error}"))
    })?;
    Ok(())
}

fn load_library_root(
    runtime: &RuntimeConfig,
    library_id: &str,
) -> Result<Option<String>, TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let library_id = library_id.to_string();
    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let row = sqlx::query("SELECT ROOT FROM LIBRARY WHERE ID = ? LIMIT 1")
                .bind(library_id)
                .fetch_optional(&pool)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!("failed to load library root: {error}"))
                })?;
            Ok::<Option<String>, TaskExecutionError>(row.map(|row| row.get::<String, _>("ROOT")))
        })
    })
}

fn run_database_query<T>(
    database_file: PathBuf,
    operation: impl FnOnce(SqlitePool) -> BoxFuture<Result<T, TaskExecutionError>> + Send + 'static,
) -> Result<T, TaskExecutionError>
where
    T: Send + 'static,
{
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                TaskExecutionError::runtime(format!("failed to build task runtime: {error}"))
            })?;

        runtime.block_on(async move {
            let pool = connect_pool(&database_file, 1).await.map_err(|error| {
                TaskExecutionError::runtime(format!("failed to open sqlite pool: {error}"))
            })?;
            let result = operation(pool.clone()).await;
            pool.close().await;
            result
        })
    })
    .join()
    .map_err(|_| TaskExecutionError::runtime("database operation worker thread panicked"))?
}

fn media_type_from_path(path: &str) -> String {
    let extension = PathBuf::from(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();

    match extension.as_str() {
        "cbz" | "zip" => "application/zip",
        "cbr" | "rar" => "application/vnd.comicbook-rar",
        "pdf" => "application/pdf",
        "epub" => "application/epub+zip",
        _ => "application/octet-stream",
    }
    .to_string()
}

fn to_unix_seconds(time: Option<std::time::SystemTime>) -> i64 {
    time.and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_secs() as i64)
        .unwrap_or_default()
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

    fn disown_task(&self, task_id: &str) {
        let task_id = task_id.to_string();
        self.run(move |pool| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE TASK\n                     SET OWNER = NULL, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP\n                     WHERE ID = ?",
                )
                .bind(task_id)
                .execute(&pool)
                .await
                .expect("task row should be disowned in TASK table");
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
    format!(
        "org.gotson.komga.task.{}.CompatTask",
        simple_type.to_ascii_lowercase()
    )
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
