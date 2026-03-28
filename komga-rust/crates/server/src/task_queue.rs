use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::future::Future;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::app::compat_runtime::content::media::{
    hash_book_pages_with_media_content, process_queued_book_import_task,
    process_queued_books_import_task,
};
use crate::config::{RuntimeConfig, WriterDecision, WriterKind};
use crate::search::{SearchDocument, SearchEntityType, SearchEvent, SearchIndexLifecycle};
use komga_persistence::sqlite::connect_pool;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use zip::ZipArchive;

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskQueueRecord {
    pub id: String,
    pub simple_type: String,
    pub priority: i32,
    pub group: Option<String>,
    pub payload: Option<String>,
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
            payload: None,
            owner: None,
            order: 0,
        }
    }

    pub fn with_simple_type(mut self, simple_type: impl Into<String>) -> Self {
        self.simple_type = simple_type.into();
        self
    }

    pub fn with_payload(mut self, payload: impl Into<String>) -> Self {
        self.payload = Some(payload.into());
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
            self.reload_admin_from_store();
            return;
        }
        self.admin.enqueue(task);
    }

    pub fn take_next(&mut self) -> Option<TaskQueueRecord> {
        if !self.consumes_queue {
            return None;
        }

        self.reload_admin_from_store();

        let task = self.admin.take_available(&self.consumer_owner)?;
        if let Some(store) = &self.persisted_store {
            store.claim_task(&task.id, &self.consumer_owner);
            self.reload_admin_from_store();
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

        self.reload_admin_from_store();

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
        self.reload_admin_from_store();
        selected
    }

    pub fn complete(&mut self, task_id: &str) -> bool {
        if let Some(store) = &self.persisted_store {
            let removed = store.delete_task(task_id);
            self.reload_admin_from_store();
            return removed;
        }

        self.admin.complete(task_id)
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
        if self.persisted_store.is_some() {
            self.reload_admin_from_store();
            let disowned = self.admin.disown_all();
            if let Some(store) = &self.persisted_store {
                store.disown_all();
            }
            self.reload_admin_from_store();
            return disowned;
        }

        self.admin.disown_all()
    }

    pub fn clear_unowned(&mut self) -> usize {
        if let Some(store) = &self.persisted_store {
            let deleted = store.clear_unowned();
            self.reload_admin_from_store();
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

            let mut batch_iter = batch.into_iter();
            while let Some(task) = batch_iter.next() {
                match self.execute_claimed_task(runtime, &task) {
                    Ok(()) => {
                        let _ = self.complete(&task.id);
                        processed += 1;
                    }
                    Err(error) => {
                        self.disown_task(&task.id);
                        for remaining in batch_iter {
                            self.disown_task(&remaining.id);
                        }
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
        if let Some(store) = &self.persisted_store {
            store.disown_task(task_id);
            self.reload_admin_from_store();
            return;
        }

        let _ = self.admin.disown(task_id);
    }

    fn reload_admin_from_store(&mut self) {
        if let Some(store) = &self.persisted_store {
            self.admin = store.load_admin();
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
                let deep_scan = task
                    .payload
                    .as_deref()
                    .and_then(parse_scan_library_payload_deep)
                    .unwrap_or(false);
                let scan = scan_library(runtime, &library_id, deep_scan)?;
                let changed_sidecars = load_changed_sidecars(runtime, &library_id, &scan.sidecars)?;
                persist_scanned_library(runtime, &library_id, &scan)?;

                if library_empty_trash_after_scan(runtime, &library_id)? {
                    empty_trash(runtime, &library_id)?;
                }
                cleanup_empty_sets(runtime)?;

                let hashing_flags = load_library_hashing_flags(runtime, &library_id)?;

                let analyzable_book_ids = find_books_requiring_analysis(runtime, &scan.book_ids)?;

                for book_id in &analyzable_book_ids {
                    self.enqueue(TaskQueueRecord::new(
                        format!("ANALYZE_BOOK:{book_id}"),
                        task.priority.saturating_sub(10),
                        Some(book_id.clone()),
                    ));
                }

                if hashing_flags.hash_files {
                    let book_ids = find_books_with_missing_file_hash(runtime, &library_id, false)?;
                    for book_id in book_ids {
                        self.enqueue(TaskQueueRecord::new(
                            format!("HASH_BOOK:{book_id}"),
                            task.priority.saturating_sub(15),
                            Some(book_id),
                        ));
                    }
                }

                if hashing_flags.hash_koreader {
                    let book_ids = find_books_with_missing_file_hash(runtime, &library_id, true)?;
                    for book_id in book_ids {
                        self.enqueue(TaskQueueRecord::new(
                            format!("HASH_BOOK_KOREADER:{book_id}"),
                            task.priority.saturating_sub(15),
                            Some(book_id),
                        ));
                    }
                }

                if hashing_flags.hash_pages {
                    self.enqueue(TaskQueueRecord::new(
                        format!("FIND_BOOKS_WITH_MISSING_PAGE_HASH:{library_id}"),
                        task.priority.saturating_sub(15),
                        Some(library_id.clone()),
                    ));
                }
                self.enqueue(TaskQueueRecord::new(
                    format!("FIND_DUPLICATE_PAGES_TO_DELETE:{library_id}"),
                    task.priority.saturating_sub(15),
                    Some(library_id.clone()),
                ));

                let maintenance_flags = load_library_maintenance_flags(runtime, &library_id)?;
                if maintenance_flags.repair_extensions {
                    self.enqueue(TaskQueueRecord::new(
                        format!("REPAIR_EXTENSIONS:{library_id}"),
                        task.priority.saturating_sub(20),
                        Some(library_id.clone()),
                    ));
                }
                if maintenance_flags.convert_to_cbz {
                    self.enqueue(TaskQueueRecord::new(
                        format!("FIND_BOOKS_TO_CONVERT:{library_id}"),
                        task.priority.saturating_sub(20),
                        Some(library_id.clone()),
                    ));
                }

                enqueue_sidecar_refresh_tasks(
                    self,
                    &scan,
                    &changed_sidecars,
                    task.priority.saturating_sub(12),
                );
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
            "EMPTY_TRASH" => {
                let Some(library_id) = task_target.as_deref() else {
                    return Err(TaskExecutionError::invalid_task(
                        "EMPTY_TRASH task must include a library id",
                    ));
                };
                empty_trash(runtime, library_id)?;
                cleanup_empty_sets(runtime)
            }
            "REFRESH_BOOK_METADATA" => {
                let Some(book_id) = task_target.as_deref() else {
                    return Err(TaskExecutionError::invalid_task(
                        "REFRESH_BOOK_METADATA task must include a book id",
                    ));
                };
                if let Some(series_id) = refresh_book_metadata(runtime, book_id)? {
                    self.enqueue(TaskQueueRecord::new(
                        format!("REFRESH_SERIES_METADATA:{series_id}"),
                        task.priority.saturating_sub(5),
                        Some(series_id),
                    ));
                }
                Ok(())
            }
            "REFRESH_SERIES_METADATA" => {
                let Some(series_id) = task_target.as_deref() else {
                    return Err(TaskExecutionError::invalid_task(
                        "REFRESH_SERIES_METADATA task must include a series id",
                    ));
                };
                refresh_series_metadata(runtime, series_id)?;
                self.enqueue(TaskQueueRecord::new(
                    format!("AGGREGATE_SERIES_METADATA:{series_id}"),
                    task.priority.saturating_sub(5),
                    Some(series_id.to_string()),
                ));
                Ok(())
            }
            "AGGREGATE_SERIES_METADATA" => {
                let Some(series_id) = task_target.as_deref() else {
                    return Err(TaskExecutionError::invalid_task(
                        "AGGREGATE_SERIES_METADATA task must include a series id",
                    ));
                };
                aggregate_series_metadata(runtime, series_id)
            }
            "REFRESH_BOOK_LOCAL_ARTWORK" => {
                let Some(book_id) = task_target.as_deref() else {
                    return Err(TaskExecutionError::invalid_task(
                        "REFRESH_BOOK_LOCAL_ARTWORK task must include a book id",
                    ));
                };
                refresh_book_local_artwork(runtime, book_id)
            }
            "REFRESH_SERIES_LOCAL_ARTWORK" => {
                let Some(series_id) = task_target.as_deref() else {
                    return Err(TaskExecutionError::invalid_task(
                        "REFRESH_SERIES_LOCAL_ARTWORK task must include a series id",
                    ));
                };
                refresh_series_local_artwork(runtime, series_id)
            }
            "GENERATE_BOOK_THUMBNAIL" => {
                let Some(book_id) = task_target.as_deref() else {
                    return Err(TaskExecutionError::invalid_task(
                        "GENERATE_BOOK_THUMBNAIL task must include a book id",
                    ));
                };
                refresh_book_local_artwork(runtime, book_id)
            }
            "HASH_BOOK_PAGES" => {
                let Some(book_id) = task_target.as_deref() else {
                    return Err(TaskExecutionError::invalid_task(
                        "HASH_BOOK_PAGES task must include a book id",
                    ));
                };
                hash_book_pages(runtime, book_id)
            }
            "HASH_BOOK" => {
                let Some(book_id) = task_target.as_deref() else {
                    return Err(TaskExecutionError::invalid_task(
                        "HASH_BOOK task must include a book id",
                    ));
                };
                hash_book(runtime, book_id, false)
            }
            "HASH_BOOK_KOREADER" => {
                let Some(book_id) = task_target.as_deref() else {
                    return Err(TaskExecutionError::invalid_task(
                        "HASH_BOOK_KOREADER task must include a book id",
                    ));
                };
                hash_book(runtime, book_id, true)
            }
            "FIND_BOOK_THUMBNAILS_TO_REGENERATE" => {
                let book_ids = find_books_without_selected_thumbnails(runtime)?;
                for book_id in book_ids {
                    self.enqueue(TaskQueueRecord::new(
                        format!("GENERATE_BOOK_THUMBNAIL:{book_id}"),
                        task.priority.saturating_sub(5),
                        Some(book_id),
                    ));
                }
                Ok(())
            }
            "FIND_BOOKS_WITH_MISSING_PAGE_HASH" => {
                let book_ids = find_books_with_missing_page_hash(runtime, task_target.as_deref())?;
                for book_id in book_ids {
                    self.enqueue(TaskQueueRecord::new(
                        format!("HASH_BOOK_PAGES:{book_id}"),
                        task.priority.saturating_sub(5),
                        Some(book_id),
                    ));
                }
                Ok(())
            }
            "FIND_DUPLICATE_PAGES_TO_DELETE" => {
                let Some(library_id) = task_target.as_deref() else {
                    return Err(TaskExecutionError::invalid_task(
                        "FIND_DUPLICATE_PAGES_TO_DELETE task must include a library id",
                    ));
                };
                let targets = find_duplicate_pages_to_delete(runtime, library_id)?;
                for (book_id, pages) in targets {
                    let payload = serde_json::to_string(&RemoveHashedPagesPayload { pages })
                        .map_err(|error| {
                            TaskExecutionError::runtime(format!(
                                "failed to serialize REMOVE_HASHED_PAGES payload: {error}",
                            ))
                        })?;
                    self.enqueue(
                        TaskQueueRecord::new(
                            format!("REMOVE_HASHED_PAGES:{book_id}"),
                            task.priority.saturating_sub(5),
                            Some(book_id),
                        )
                        .with_payload(payload),
                    );
                }
                Ok(())
            }
            "REMOVE_HASHED_PAGES" => {
                let Some(book_id) = task_target.as_deref() else {
                    return Err(TaskExecutionError::invalid_task(
                        "REMOVE_HASHED_PAGES task must include a book id",
                    ));
                };
                let payload = task.payload.as_deref().ok_or_else(|| {
                    TaskExecutionError::invalid_task(
                        "REMOVE_HASHED_PAGES task requires serialized payload",
                    )
                })?;
                let parsed: RemoveHashedPagesPayload =
                    serde_json::from_str(payload).map_err(|error| {
                        TaskExecutionError::runtime(format!(
                            "failed to parse REMOVE_HASHED_PAGES payload: {error}",
                        ))
                    })?;
                let regenerate_thumbnail = remove_hashed_pages(runtime, book_id, &parsed.pages)?;
                if regenerate_thumbnail {
                    self.enqueue(TaskQueueRecord::new(
                        format!("GENERATE_BOOK_THUMBNAIL:{book_id}"),
                        task.priority.saturating_sub(1),
                        Some(book_id.to_string()),
                    ));
                }
                Ok(())
            }
            "DELETE_BOOK" => {
                let Some(book_id) = task_target.as_deref() else {
                    return Err(TaskExecutionError::invalid_task(
                        "DELETE_BOOK task must include a book id",
                    ));
                };
                delete_book_task(runtime, book_id)
            }
            "DELETE_SERIES" => {
                let Some(series_id) = task_target.as_deref() else {
                    return Err(TaskExecutionError::invalid_task(
                        "DELETE_SERIES task must include a series id",
                    ));
                };
                delete_series(runtime, series_id)
            }
            "REPAIR_EXTENSIONS" => {
                let Some(library_id) = task_target.as_deref() else {
                    return Err(TaskExecutionError::invalid_task(
                        "REPAIR_EXTENSIONS task must include a library id",
                    ));
                };
                repair_extensions(runtime, library_id)
            }
            "FIND_BOOKS_TO_CONVERT" => {
                let Some(library_id) = task_target.as_deref() else {
                    return Err(TaskExecutionError::invalid_task(
                        "FIND_BOOKS_TO_CONVERT task must include a library id",
                    ));
                };
                let book_ids = find_books_to_convert(runtime, library_id)?;
                for book_id in book_ids {
                    self.enqueue(TaskQueueRecord::new(
                        format!("CONVERT_BOOK:{book_id}"),
                        task.priority.saturating_sub(5),
                        Some(book_id),
                    ));
                }
                Ok(())
            }
            "CONVERT_BOOK" => {
                let Some(book_id) = task_target.as_deref() else {
                    return Err(TaskExecutionError::invalid_task(
                        "CONVERT_BOOK task must include a book id",
                    ));
                };
                convert_book(runtime, book_id)
            }
            "IMPORT_BOOKS_BATCH" => {
                let follow_up_tasks = process_import_books_batch_task(runtime, task)?;
                for follow_up in follow_up_tasks {
                    self.enqueue(follow_up);
                }
                Ok(())
            }
            "IMPORT_BOOK" => {
                let follow_up_tasks = process_import_book_task(runtime, task)?;
                for follow_up in follow_up_tasks {
                    self.enqueue(follow_up);
                }
                Ok(())
            }
            other => Err(TaskExecutionError::unsupported_task(other)),
        }
    }
}

fn process_import_books_batch_task(
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
) -> Result<Vec<TaskQueueRecord>, TaskExecutionError> {
    let payload = task.payload.clone().ok_or_else(|| {
        TaskExecutionError::invalid_task("IMPORT_BOOKS_BATCH task requires serialized payload")
    })?;
    let database_file = runtime.database_file.clone();

    std::thread::spawn(move || {
        let async_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "build import books batch runtime failed: {error}"
                ))
            })?;

        async_runtime.block_on(async move {
            process_queued_books_import_task(database_file.as_path(), &payload)
                .await
                .map_err(|error| TaskExecutionError::runtime(error))
        })
    })
    .join()
    .map_err(|_| TaskExecutionError::runtime("import books batch worker thread panicked"))?
}

fn process_import_book_task(
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
) -> Result<Vec<TaskQueueRecord>, TaskExecutionError> {
    let payload = task.payload.clone().ok_or_else(|| {
        TaskExecutionError::invalid_task("IMPORT_BOOK task requires serialized payload")
    })?;
    let database_file = runtime.database_file.clone();

    std::thread::spawn(move || {
        let async_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                TaskExecutionError::runtime(format!("build import book runtime failed: {error}"))
            })?;

        async_runtime.block_on(async move {
            process_queued_book_import_task(database_file.as_path(), &payload)
                .await
                .map_err(TaskExecutionError::runtime)
        })
    })
    .join()
    .map_err(|_| TaskExecutionError::runtime("import book worker thread panicked"))?
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
    root_available: bool,
    series_rows: Vec<ScannedSeriesRow>,
    sidecars: Vec<ScannedSidecarRow>,
    book_ids: Vec<String>,
    discovered_series_ids: HashSet<String>,
    discovered_book_ids: HashSet<String>,
}

#[derive(Clone, Debug)]
struct ScannedSeriesRow {
    series_id: String,
    series_name: String,
    series_url: String,
    series_last_modified_unix_seconds: i64,
    oneshot: bool,
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
    oneshot: bool,
}

#[derive(Clone, Debug)]
struct ScannedSidecarRow {
    url: String,
    parent_url: String,
    last_modified_unix_seconds: i64,
    source: ScannedSidecarSource,
    sidecar_type: ScannedSidecarType,
}

#[derive(Clone, Copy, Debug)]
enum ScannedSidecarSource {
    Series,
    Book,
}

#[derive(Clone, Copy, Debug)]
enum ScannedSidecarType {
    Metadata,
    Artwork,
}

fn enqueue_sidecar_refresh_tasks(
    scheduler: &mut TaskQueueScheduler,
    scanned: &ScannedLibrary,
    changed_sidecar_urls: &[String],
    priority: i32,
) {
    let changed_sidecar_urls = changed_sidecar_urls.iter().cloned().collect::<HashSet<_>>();
    let mut series_by_url = HashMap::new();
    let mut book_by_url = HashMap::new();
    for series in &scanned.series_rows {
        series_by_url.insert(series.series_url.clone(), series.series_id.clone());
        for book in &series.books {
            book_by_url.insert(book.book_url.clone(), book.book_id.clone());
        }
    }

    let mut seen_series_metadata = HashSet::new();
    let mut seen_series_artwork = HashSet::new();
    let mut seen_books_metadata = HashSet::new();
    let mut seen_books_artwork = HashSet::new();
    for sidecar in &scanned.sidecars {
        if !changed_sidecar_urls.contains(&sidecar.url) {
            continue;
        }
        if sidecar.last_modified_unix_seconds < 0 {
            continue;
        }

        match (sidecar.source, sidecar.sidecar_type) {
            (ScannedSidecarSource::Series, ScannedSidecarType::Metadata) => {
                if let Some(series_id) = series_by_url.get(&sidecar.parent_url)
                    && seen_series_metadata.insert(series_id.clone())
                {
                    scheduler.enqueue(TaskQueueRecord::new(
                        format!("REFRESH_SERIES_METADATA:{series_id}"),
                        priority,
                        Some(series_id.clone()),
                    ));
                }
            }
            (ScannedSidecarSource::Series, ScannedSidecarType::Artwork) => {
                if let Some(series_id) = series_by_url.get(&sidecar.parent_url)
                    && seen_series_artwork.insert(series_id.clone())
                {
                    scheduler.enqueue(TaskQueueRecord::new(
                        format!("REFRESH_SERIES_LOCAL_ARTWORK:{series_id}"),
                        priority,
                        Some(series_id.clone()),
                    ));
                }
            }
            (ScannedSidecarSource::Book, ScannedSidecarType::Metadata) => {
                if let Some(book_id) = book_by_url.get(&sidecar.parent_url)
                    && seen_books_metadata.insert(book_id.clone())
                {
                    scheduler.enqueue(TaskQueueRecord::new(
                        format!("REFRESH_BOOK_METADATA:{book_id}"),
                        priority,
                        Some(book_id.clone()),
                    ));
                }
            }
            (ScannedSidecarSource::Book, ScannedSidecarType::Artwork) => {
                if let Some(book_id) = book_by_url.get(&sidecar.parent_url)
                    && seen_books_artwork.insert(book_id.clone())
                {
                    scheduler.enqueue(TaskQueueRecord::new(
                        format!("REFRESH_BOOK_LOCAL_ARTWORK:{book_id}"),
                        priority,
                        Some(book_id.clone()),
                    ));
                }
            }
        }
    }
}

fn load_changed_sidecars(
    runtime: &RuntimeConfig,
    library_id: &str,
    scanned_sidecars: &[ScannedSidecarRow],
) -> Result<Vec<String>, TaskExecutionError> {
    if scanned_sidecars.is_empty() {
        return Ok(Vec::new());
    }

    let database_file = runtime.database_file.clone();
    let library_id = library_id.to_string();
    let scanned_sidecars = scanned_sidecars.to_vec();

    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        let scanned_sidecars = scanned_sidecars.clone();
        Box::pin(async move {
            let existing_rows = sqlx::query(
                "SELECT URL, LAST_MODIFIED_TIME \
                 FROM SIDECAR \
                 WHERE LIBRARY_ID = ?",
            )
            .bind(&library_id)
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to load existing sidecars for '{library_id}': {error}",
                ))
            })?;

            let existing = existing_rows
                .into_iter()
                .map(|row| {
                    (
                        row.get::<String, _>("URL"),
                        row.get::<i64, _>("LAST_MODIFIED_TIME"),
                    )
                })
                .collect::<HashMap<_, _>>();

            Ok::<Vec<String>, TaskExecutionError>(
                scanned_sidecars
                    .into_iter()
                    .filter(|sidecar| {
                        existing.get(&sidecar.url).map_or(true, |timestamp| {
                            *timestamp != sidecar.last_modified_unix_seconds
                        })
                    })
                    .map(|sidecar| sidecar.url)
                    .collect(),
            )
        })
    })
}

fn scan_library(
    runtime: &RuntimeConfig,
    library_id: &str,
    _deep_scan: bool,
) -> Result<ScannedLibrary, TaskExecutionError> {
    let scan_config = load_library_scan_config(runtime, library_id)?;
    let Some(scan_config) = scan_config else {
        return Ok(ScannedLibrary {
            root_available: false,
            series_rows: Vec::new(),
            sidecars: Vec::new(),
            book_ids: Vec::new(),
            discovered_series_ids: HashSet::new(),
            discovered_book_ids: HashSet::new(),
        });
    };

    let root = PathBuf::from(&scan_config.root);
    if !root.exists() {
        return Ok(ScannedLibrary {
            root_available: false,
            series_rows: Vec::new(),
            sidecars: Vec::new(),
            book_ids: Vec::new(),
            discovered_series_ids: HashSet::new(),
            discovered_book_ids: HashSet::new(),
        });
    }

    let mut discovered = Vec::new();
    collect_series_directories(&root, &root, &scan_config, &mut discovered)?;

    let mut sidecars = Vec::new();
    let mut series_rows = Vec::new();
    let mut book_ids = Vec::new();
    let mut discovered_series_ids = HashSet::new();
    let mut discovered_book_ids = HashSet::new();
    for series_dir in discovered {
        let series_url = series_dir.to_string_lossy().to_string();
        let series_dir_last_modified_unix_seconds = fs::metadata(&series_dir)
            .ok()
            .map(|value| metadata_updated_unix_seconds(&value))
            .unwrap_or(0);

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
                if is_supported_book_file(&path, &scan_config) {
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
                        file_last_modified_unix_seconds: metadata_updated_unix_seconds(&metadata),
                        oneshot: false,
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

        let books_last_modified_unix_seconds = books
            .iter()
            .map(|book| book.file_last_modified_unix_seconds)
            .max()
            .unwrap_or(0);
        let series_last_modified_unix_seconds = if scan_config.scan_force_modified_time {
            series_dir_last_modified_unix_seconds.max(books_last_modified_unix_seconds)
        } else {
            series_dir_last_modified_unix_seconds
        };
        books.iter().for_each(|book| {
            discovered_book_ids.insert(book.book_id.clone());
        });

        let oneshots_dir = scan_config
            .oneshots_directory
            .as_ref()
            .map(|value| value.to_ascii_lowercase());
        if let Some(oneshots_dir) = oneshots_dir
            && series_url.to_ascii_lowercase().contains(&oneshots_dir)
        {
            for book in &books {
                let series_id =
                    route_safe_scanner_id("series", PathBuf::from(&book.book_url).as_path());
                discovered_series_ids.insert(series_id.clone());
                series_rows.push(ScannedSeriesRow {
                    series_id,
                    series_name: book.book_name.clone(),
                    series_url: book.book_url.clone(),
                    series_last_modified_unix_seconds: book.file_last_modified_unix_seconds,
                    oneshot: true,
                    books: vec![ScannedBookRow {
                        oneshot: true,
                        ..book.clone()
                    }],
                });
            }
            continue;
        }

        let series_id = route_safe_scanner_id("series", &series_dir);
        discovered_series_ids.insert(series_id.clone());
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

            let file_name_lower = file_name.to_ascii_lowercase();
            let is_image = ["jpg", "jpeg", "png", "webp", "gif", "avif"]
                .iter()
                .any(|ext| file_name_lower.ends_with(&format!(".{ext}")));
            if is_image {
                let base = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                if matches!(base.as_str(), "cover" | "folder" | "poster" | "series") {
                    series_sidecars.push(ScannedSidecarRow {
                        url: path.to_string_lossy().to_string(),
                        parent_url: series_url.clone(),
                        last_modified_unix_seconds: to_unix_seconds(metadata.modified().ok()),
                        source: ScannedSidecarSource::Series,
                        sidecar_type: ScannedSidecarType::Artwork,
                    });
                    continue;
                }
            }

            if file_name.eq_ignore_ascii_case("ComicInfo.xml") {
                series_sidecars.push(ScannedSidecarRow {
                    url: path.to_string_lossy().to_string(),
                    parent_url: series_url.clone(),
                    last_modified_unix_seconds: to_unix_seconds(metadata.modified().ok()),
                    source: ScannedSidecarSource::Series,
                    sidecar_type: ScannedSidecarType::Metadata,
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
                        source: ScannedSidecarSource::Book,
                        sidecar_type: ScannedSidecarType::Metadata,
                    });
                    continue;
                }

                if is_image {
                    let base = path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or_default();
                    if base.eq_ignore_ascii_case(&book.book_name) {
                        series_sidecars.push(ScannedSidecarRow {
                            url: path.to_string_lossy().to_string(),
                            parent_url: book.book_url.clone(),
                            last_modified_unix_seconds: to_unix_seconds(metadata.modified().ok()),
                            source: ScannedSidecarSource::Book,
                            sidecar_type: ScannedSidecarType::Artwork,
                        });
                    }
                }
            }
        }

        sidecars.extend(series_sidecars);
        series_rows.push(ScannedSeriesRow {
            series_id: series_id.clone(),
            series_name,
            series_url,
            series_last_modified_unix_seconds,
            oneshot: false,
            books,
        });
    }

    Ok(ScannedLibrary {
        root_available: true,
        series_rows,
        sidecars,
        book_ids,
        discovered_series_ids,
        discovered_book_ids,
    })
}

fn collect_series_directories(
    current: &PathBuf,
    root: &PathBuf,
    scan_config: &LibraryScanConfig,
    discovered: &mut Vec<PathBuf>,
) -> Result<(), TaskExecutionError> {
    if is_library_path_excluded(
        current.as_path(),
        root.as_path(),
        &scan_config.scan_directory_exclusions,
    ) {
        return Ok(());
    }

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
        if metadata.is_file()
            && is_supported_book_file(&path, scan_config)
            && !is_library_path_excluded(
                path.as_path(),
                root.as_path(),
                &scan_config.scan_directory_exclusions,
            )
        {
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
        collect_series_directories(&child, root, scan_config, discovered)?;
    }

    Ok(())
}

fn is_supported_book_file(path: &PathBuf, scan_config: &LibraryScanConfig) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "cbz" | "zip" | "cbr" | "rar"
            )
            .then_some(scan_config.scan_cbx)
            .unwrap_or_else(|| {
                matches!(extension.to_ascii_lowercase().as_str(), "pdf")
                    .then_some(scan_config.scan_pdf)
                    .or_else(|| {
                        matches!(extension.to_ascii_lowercase().as_str(), "epub")
                            .then_some(scan_config.scan_epub)
                    })
                    .unwrap_or(false)
            })
        })
}

fn is_library_path_excluded(
    path: &std::path::Path,
    root: &std::path::Path,
    exclusions: &[String],
) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let relative = relative.to_string_lossy().replace('\\', "/");
    exclusions.iter().any(|entry| {
        let exclusion = entry.trim().replace('\\', "/");
        if exclusion.is_empty() {
            return false;
        }
        relative == exclusion
            || relative.starts_with(&(exclusion.clone() + "/"))
            || relative.contains(&("/".to_string() + &exclusion + "/"))
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
            if !scanned.root_available {
                sqlx::query(
                    "UPDATE LIBRARY \
                     SET UNAVAILABLE_DATE = CURRENT_TIMESTAMP, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                     WHERE ID = ?",
                )
                .bind(&library_id)
                .execute(&pool)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to mark library unavailable for '{library_id}': {error}",
                    ))
                })?;
                return Ok::<(), TaskExecutionError>(());
            }

            sqlx::query(
                "UPDATE LIBRARY \
                 SET UNAVAILABLE_DATE = NULL, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                 WHERE ID = ?",
            )
            .bind(&library_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to clear library unavailable marker for '{library_id}': {error}",
                ))
            })?;

            let discovered_series_ids = scanned.discovered_series_ids.clone();
            let discovered_book_ids = scanned.discovered_book_ids.clone();

            for series in &scanned.series_rows {
                let series_updated = sqlx::query(
                    "UPDATE SERIES \
                     SET FILE_LAST_MODIFIED = ?, NAME = ?, URL = ?, LIBRARY_ID = ?, oneshot = ?, \
                         LAST_MODIFIED_DATE = CURRENT_TIMESTAMP, DELETED_DATE = NULL \
                     WHERE ID = ?",
                )
                .bind(series.series_last_modified_unix_seconds)
                .bind(&series.series_name)
                .bind(&series.series_url)
                .bind(&library_id)
                .bind(series.oneshot)
                .bind(&series.series_id)
                .execute(&pool)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!("failed to update SERIES rows: {error}"))
                })?
                .rows_affected();

                if series_updated == 0 {
                    sqlx::query(
                        "INSERT \
                         OR IGNORE INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot) \
                         VALUES (?, ?, ?, ?, ?, ?)",
                    )
                    .bind(&series.series_id)
                    .bind(series.series_last_modified_unix_seconds)
                    .bind(&series.series_name)
                    .bind(&series.series_url)
                    .bind(&library_id)
                    .bind(series.oneshot)
                    .execute(&pool)
                    .await
                    .map_err(|error| TaskExecutionError::runtime(format!("failed to insert SERIES rows: {error}")))?;
                }

                for book in &series.books {
                    let book_updated = sqlx::query(
                        "UPDATE BOOK \
                         SET FILE_LAST_MODIFIED = ?, URL = ?, SERIES_ID = ?, FILE_SIZE = ?, \
                             LIBRARY_ID = ?, oneshot = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP, DELETED_DATE = NULL \
                         WHERE ID = ?",
                    )
                    .bind(book.file_last_modified_unix_seconds)
                    .bind(&book.book_url)
                    .bind(&series.series_id)
                    .bind(book.file_size)
                    .bind(&library_id)
                    .bind(book.oneshot)
                    .bind(&book.book_id)
                    .execute(&pool)
                    .await
                    .map_err(|error| TaskExecutionError::runtime(format!("failed to update BOOK rows: {error}")))?
                    .rows_affected();

                    if book_updated == 0 {
                        sqlx::query(
                            "INSERT \
                             OR IGNORE INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, \
                                LIBRARY_ID, oneshot) \
                             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                        )
                        .bind(&book.book_id)
                        .bind(book.file_last_modified_unix_seconds)
                        .bind(&book.book_name)
                        .bind(&book.book_url)
                        .bind(&series.series_id)
                        .bind(book.file_size)
                        .bind(&library_id)
                        .bind(book.oneshot)
                        .execute(&pool)
                        .await
                        .map_err(|error| TaskExecutionError::runtime(format!("failed to insert BOOK rows: {error}")))?;
                    }

                    let media_updated = sqlx::query(
                        "UPDATE MEDIA_FILE \
                         SET FILE_SIZE = ? \
                         WHERE FILE_NAME = ? \
                         AND BOOK_ID = ?",
                    )
                    .bind(book.file_size)
                    .bind(&book.file_name)
                    .bind(&book.book_id)
                    .execute(&pool)
                    .await
                    .map_err(|error| {
                        TaskExecutionError::runtime(format!(
                            "failed to update MEDIA_FILE rows: {error}"
                        ))
                    })?
                    .rows_affected();

                    if media_updated == 0 {
                        sqlx::query(
                            "INSERT INTO MEDIA_FILE (FILE_NAME, BOOK_ID, FILE_SIZE) \
                             VALUES (?, ?, ?)",
                        )
                        .bind(&book.file_name)
                        .bind(&book.book_id)
                        .bind(book.file_size)
                        .execute(&pool)
                        .await
                        .map_err(|error| {
                            TaskExecutionError::runtime(format!(
                                "failed to insert MEDIA_FILE rows: {error}"
                            ))
                        })?;
                    }
                }
            }

            for sidecar in &scanned.sidecars {
                let sidecar_updated = sqlx::query(
                    "UPDATE SIDECAR \
                     SET PARENT_URL = ?, LAST_MODIFIED_TIME = ? \
                     WHERE URL = ? \
                     AND LIBRARY_ID = ?",
                )
                .bind(&sidecar.parent_url)
                .bind(sidecar.last_modified_unix_seconds)
                .bind(&sidecar.url)
                .bind(&library_id)
                .execute(&pool)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!("failed to update SIDECAR rows: {error}"))
                })?
                .rows_affected();

                if sidecar_updated == 0 {
                    sqlx::query(
                        "INSERT \
                         OR IGNORE INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) \
                         VALUES (?, ?, ?, ?)",
                    )
                    .bind(&sidecar.url)
                    .bind(&sidecar.parent_url)
                    .bind(sidecar.last_modified_unix_seconds)
                    .bind(&library_id)
                    .execute(&pool)
                    .await
                    .map_err(|error| {
                        TaskExecutionError::runtime(format!(
                            "failed to insert SIDECAR rows: {error}"
                        ))
                    })?;
                }
            }

            if scanned.root_available {
                let existing_series = sqlx::query(
                    "SELECT ID \
                     FROM SERIES \
                     WHERE LIBRARY_ID = ? \
                     AND DELETED_DATE IS NULL",
                )
                .bind(&library_id)
                .fetch_all(&pool)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to query existing SERIES rows for '{library_id}': {error}",
                    ))
                })?;
                for row in existing_series {
                    let series_id = row.get::<String, _>("ID");
                    if discovered_series_ids.contains(&series_id) {
                        continue;
                    }
                    sqlx::query(
                        "UPDATE SERIES \
                         SET DELETED_DATE = CURRENT_TIMESTAMP, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                         WHERE ID = ?",
                    )
                    .bind(&series_id)
                    .execute(&pool)
                    .await
                    .map_err(|error| {
                        TaskExecutionError::runtime(format!(
                            "failed to soft-delete missing SERIES '{series_id}': {error}",
                        ))
                    })?;
                }

                let existing_books = sqlx::query(
                    "SELECT ID \
                     FROM BOOK \
                     WHERE LIBRARY_ID = ? \
                     AND DELETED_DATE IS NULL",
                )
                .bind(&library_id)
                .fetch_all(&pool)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to query existing BOOK rows for '{library_id}': {error}",
                    ))
                })?;
                for row in existing_books {
                    let book_id = row.get::<String, _>("ID");
                    if discovered_book_ids.contains(&book_id) {
                        continue;
                    }
                    sqlx::query(
                        "UPDATE BOOK \
                         SET DELETED_DATE = CURRENT_TIMESTAMP, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                         WHERE ID = ?",
                    )
                    .bind(&book_id)
                    .execute(&pool)
                    .await
                    .map_err(|error| {
                        TaskExecutionError::runtime(format!(
                            "failed to soft-delete missing BOOK '{book_id}': {error}",
                        ))
                    })?;
                }
            }

            sqlx::query(
                "UPDATE SERIES \
                 SET BOOK_COUNT = (SELECT COUNT(*) \
                 FROM BOOK \
                 WHERE BOOK.SERIES_ID = SERIES.ID \
                 AND BOOK.DELETED_DATE IS NULL) \
                 WHERE LIBRARY_ID = ?",
            )
            .bind(&library_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to refresh series book counts after scan for '{library_id}': {error}",
                ))
            })?;

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
                "SELECT b.ID AS ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, b.URL AS URL, l.ROOT AS ROOT \
                 FROM BOOK b \
                 JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
                 LEFT \
                 JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
                 WHERE b.ID = ? \
                 LIMIT 1",
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!("failed to load BOOK row for analyze: {error}"))
            })?;

            let Some(row) = row else {
                return Ok::<Option<SearchDocument>, TaskExecutionError>(None);
            };

            let title = row.get::<String, _>("TITLE");
            let url = row.get::<String, _>("URL");
            let root = row.get::<String, _>("ROOT");
            let file_path = PathBuf::from(root).join(&url);

            let analysis = analyze_book_media_file(&file_path, &url).map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to analyze media file for '{book_id}' ('{}'): {error}",
                    file_path.display(),
                ))
            })?;

            let mut tx = pool.begin().await.map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to start analyze-book transaction for '{book_id}': {error}",
                ))
            })?;

            sqlx::query(
                "DELETE \
                 FROM MEDIA_PAGE \
                 WHERE BOOK_ID = ?",
            )
            .bind(&book_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to clear MEDIA_PAGE rows for '{book_id}': {error}",
                ))
            })?;

            for (index, page) in analysis.pages.iter().enumerate() {
                sqlx::query(
                    "INSERT INTO MEDIA_PAGE (FILE_NAME, MEDIA_TYPE, NUMBER, BOOK_ID, width, height, FILE_HASH, FILE_SIZE) \
                     VALUES (?, ?, ?, ?, NULL, NULL, '', ?)",
                )
                .bind(&page.file_name)
                .bind(&page.media_type)
                .bind(index as i64)
                .bind(&book_id)
                .bind(page.file_size)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to insert MEDIA_PAGE row for '{book_id}': {error}",
                    ))
                })?;
            }

            sqlx::query(
                "INSERT INTO MEDIA (BOOK_ID, STATUS, MEDIA_TYPE, PAGE_COUNT) \
                 VALUES (?, ?, ?, ?) \
                 ON CONFLICT(BOOK_ID) DO UPDATE \
                 SET STATUS = excluded.STATUS, MEDIA_TYPE = excluded.MEDIA_TYPE, \
                      PAGE_COUNT = excluded.PAGE_COUNT, \
                      LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
            )
            .bind(&book_id)
            .bind(&analysis.status)
            .bind(&analysis.media_type)
            .bind(analysis.pages.len() as i32)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to persist MEDIA analyze state: {error}"
                ))
            })?;

            tx.commit().await.map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to commit analyze-book transaction for '{book_id}': {error}",
                ))
            })?;

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
                "SELECT b.ID AS ID, COALESCE(bm.TITLE, b.NAME) AS TITLE \
                 FROM BOOK b \
                 LEFT \
                 JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID",
            )
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to read BOOK rows for index rebuild: {error}"
                ))
            })?;
            for row in book_rows {
                docs.push(SearchDocument {
                    entity_type: SearchEntityType::Book,
                    id: row.get::<String, _>("ID"),
                    title: row.get::<String, _>("TITLE"),
                });
            }

            let series_rows = sqlx::query(
                "SELECT s.ID AS ID, COALESCE(sm.TITLE, s.NAME) AS TITLE \
                 FROM SERIES s \
                 LEFT \
                 JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID",
            )
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to read SERIES rows for index rebuild: {error}"
                ))
            })?;
            for row in series_rows {
                docs.push(SearchDocument {
                    entity_type: SearchEntityType::Series,
                    id: row.get::<String, _>("ID"),
                    title: row.get::<String, _>("TITLE"),
                });
            }

            let collection_rows = sqlx::query(
                "SELECT ID, NAME \
                                               FROM COLLECTION",
            )
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

            let readlist_rows = sqlx::query(
                "SELECT ID, NAME \
                                             FROM READLIST",
            )
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

fn empty_trash(runtime: &RuntimeConfig, library_id: &str) -> Result<(), TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let library_id = library_id.to_string();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let mut tx = pool.begin().await.map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to start empty-trash transaction: {error}"
                ))
            })?;

            for sql in [
                "DELETE \
                 FROM BOOK_METADATA \
                 WHERE BOOK_ID IN (SELECT ID \
                 FROM BOOK \
                 WHERE LIBRARY_ID = ? \
                 AND DELETED_DATE IS NOT NULL)",
                "DELETE \
                 FROM BOOK_METADATA_AUTHOR \
                 WHERE BOOK_ID IN (SELECT ID \
                 FROM BOOK \
                 WHERE LIBRARY_ID = ? \
                 AND DELETED_DATE IS NOT NULL)",
                "DELETE \
                 FROM BOOK_METADATA_LINK \
                 WHERE BOOK_ID IN (SELECT ID \
                 FROM BOOK \
                 WHERE LIBRARY_ID = ? \
                 AND DELETED_DATE IS NOT NULL)",
                "DELETE \
                 FROM BOOK_METADATA_TAG \
                 WHERE BOOK_ID IN (SELECT ID \
                 FROM BOOK \
                 WHERE LIBRARY_ID = ? \
                 AND DELETED_DATE IS NOT NULL)",
                "DELETE \
                 FROM MEDIA \
                 WHERE BOOK_ID IN (SELECT ID \
                 FROM BOOK \
                 WHERE LIBRARY_ID = ? \
                 AND DELETED_DATE IS NOT NULL)",
                "DELETE \
                 FROM MEDIA_FILE \
                 WHERE BOOK_ID IN (SELECT ID \
                 FROM BOOK \
                 WHERE LIBRARY_ID = ? \
                 AND DELETED_DATE IS NOT NULL)",
                "DELETE \
                 FROM MEDIA_PAGE \
                 WHERE BOOK_ID IN (SELECT ID \
                 FROM BOOK \
                 WHERE LIBRARY_ID = ? \
                 AND DELETED_DATE IS NOT NULL)",
                "DELETE \
                 FROM THUMBNAIL_BOOK \
                 WHERE BOOK_ID IN (SELECT ID \
                 FROM BOOK \
                 WHERE LIBRARY_ID = ? \
                 AND DELETED_DATE IS NOT NULL)",
                "DELETE \
                 FROM READ_PROGRESS \
                 WHERE BOOK_ID IN (SELECT ID \
                 FROM BOOK \
                 WHERE LIBRARY_ID = ? \
                 AND DELETED_DATE IS NOT NULL)",
                "DELETE \
                 FROM READLIST_BOOK \
                 WHERE BOOK_ID IN (SELECT ID \
                 FROM BOOK \
                 WHERE LIBRARY_ID = ? \
                 AND DELETED_DATE IS NOT NULL)",
            ] {
                sqlx::query(sql)
                    .bind(&library_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| {
                        TaskExecutionError::runtime(format!(
                            "failed to delete empty-trash dependent rows for library '{library_id}': {error}",
                        ))
                    })?;
            }

            sqlx::query(
                "DELETE \
                         FROM BOOK \
                         WHERE LIBRARY_ID = ? \
                         AND DELETED_DATE IS NOT NULL",
            )
            .bind(&library_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to delete trashed BOOK rows for library '{library_id}': {error}",
                ))
            })?;

            sqlx::query(
                "UPDATE SERIES \
                 SET BOOK_COUNT = (SELECT COUNT(*) \
                 FROM BOOK \
                 WHERE BOOK.SERIES_ID = SERIES.ID) \
                 WHERE LIBRARY_ID = ?",
            )
            .bind(&library_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to refresh SERIES book counts for library '{library_id}': {error}",
                ))
            })?;

            for sql in [
                "DELETE \
                 FROM SERIES_METADATA \
                 WHERE SERIES_ID IN (SELECT ID \
                 FROM SERIES \
                 WHERE LIBRARY_ID = ? \
                 AND (DELETED_DATE IS NOT NULL \
                 OR BOOK_COUNT = 0))",
                "DELETE \
                 FROM SERIES_METADATA_ALTERNATE_TITLE \
                 WHERE SERIES_ID IN (SELECT ID \
                 FROM SERIES \
                 WHERE LIBRARY_ID = ? \
                 AND (DELETED_DATE IS NOT NULL \
                 OR BOOK_COUNT = 0))",
                "DELETE \
                 FROM SERIES_METADATA_GENRE \
                 WHERE SERIES_ID IN (SELECT ID \
                 FROM SERIES \
                 WHERE LIBRARY_ID = ? \
                 AND (DELETED_DATE IS NOT NULL \
                 OR BOOK_COUNT = 0))",
                "DELETE \
                 FROM SERIES_METADATA_LINK \
                 WHERE SERIES_ID IN (SELECT ID \
                 FROM SERIES \
                 WHERE LIBRARY_ID = ? \
                 AND (DELETED_DATE IS NOT NULL \
                 OR BOOK_COUNT = 0))",
                "DELETE \
                 FROM SERIES_METADATA_SHARING \
                 WHERE SERIES_ID IN (SELECT ID \
                 FROM SERIES \
                 WHERE LIBRARY_ID = ? \
                 AND (DELETED_DATE IS NOT NULL \
                 OR BOOK_COUNT = 0))",
                "DELETE \
                 FROM SERIES_METADATA_TAG \
                 WHERE SERIES_ID IN (SELECT ID \
                 FROM SERIES \
                 WHERE LIBRARY_ID = ? \
                 AND (DELETED_DATE IS NOT NULL \
                 OR BOOK_COUNT = 0))",
                "DELETE \
                 FROM THUMBNAIL_SERIES \
                 WHERE SERIES_ID IN (SELECT ID \
                 FROM SERIES \
                 WHERE LIBRARY_ID = ? \
                 AND (DELETED_DATE IS NOT NULL \
                 OR BOOK_COUNT = 0))",
                "DELETE \
                 FROM COLLECTION_SERIES \
                 WHERE SERIES_ID IN (SELECT ID \
                 FROM SERIES \
                 WHERE LIBRARY_ID = ? \
                 AND (DELETED_DATE IS NOT NULL \
                 OR BOOK_COUNT = 0))",
            ] {
                sqlx::query(sql)
                    .bind(&library_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| {
                        TaskExecutionError::runtime(format!(
                            "failed to delete empty-trash SERIES dependents for library '{library_id}': {error}",
                        ))
                    })?;
            }

            sqlx::query(
                "DELETE \
                 FROM SERIES \
                 WHERE LIBRARY_ID = ? \
                 AND (DELETED_DATE IS NOT NULL \
                 OR BOOK_COUNT = 0)",
            )
            .bind(&library_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to delete trashed SERIES rows for library '{library_id}': {error}",
                ))
            })?;

            tx.commit().await.map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to commit empty-trash transaction for library '{library_id}': {error}",
                ))
            })?;

            Ok::<(), TaskExecutionError>(())
        })
    })
}

fn cleanup_empty_sets(runtime: &RuntimeConfig) -> Result<(), TaskExecutionError> {
    let database_file = runtime.database_file.clone();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let flags = load_cleanup_empty_sets_flags_from_pool(&pool).await?;
            let mut tx = pool.begin().await.map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to start cleanup-empty-sets transaction: {error}",
                ))
            })?;

            let mut deletes = Vec::<&str>::new();
            if flags.delete_collections {
                deletes.push("DELETE FROM COLLECTION WHERE ID NOT IN (SELECT COLLECTION_ID FROM COLLECTION_SERIES)");
                deletes.push("DELETE FROM THUMBNAIL_COLLECTION WHERE COLLECTION_ID NOT IN (SELECT ID FROM COLLECTION)");
            }
            if flags.delete_readlists {
                deletes.push(
                    "DELETE FROM READLIST WHERE ID NOT IN (SELECT READLIST_ID FROM READLIST_BOOK)",
                );
                deletes.push("DELETE FROM THUMBNAIL_READLIST WHERE READLIST_ID NOT IN (SELECT ID FROM READLIST)");
            }

            for sql in deletes {
                sqlx::query(sql).execute(&mut *tx).await.map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to cleanup empty sets rows: {error}",
                    ))
                })?;
            }

            tx.commit().await.map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to commit cleanup-empty-sets transaction: {error}",
                ))
            })?;

            Ok::<(), TaskExecutionError>(())
        })
    })
}

fn refresh_book_metadata(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<Option<String>, TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let book_row = sqlx::query(
                "SELECT b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT \
                 FROM BOOK b \
                 JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
                 WHERE b.ID = ? \
                 LIMIT 1",
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to resolve book path for metadata refresh '{book_id}': {error}",
                ))
            })?;

            if let Some(book_row) = &book_row {
                let book_url = book_row.get::<String, _>("BOOK_URL");
                let library_root = book_row.get::<String, _>("LIBRARY_ROOT");
                if let Some(sidecar_url) =
                    load_sidecar_url_for_parent(&pool, &book_url, true).await?
                {
                    let sidecar_path = PathBuf::from(library_root).join(sidecar_url);
                    if let Ok(xml) = fs::read_to_string(&sidecar_path) {
                        let title = extract_xml_tag(&xml, "Title").unwrap_or_default();
                        let summary = extract_xml_tag(&xml, "Summary").unwrap_or_default();
                        if !title.is_empty() || !summary.is_empty() {
                            sqlx::query(
                                "UPDATE BOOK_METADATA \
                                 SET TITLE = CASE WHEN TITLE_LOCK = 0 AND ? <> '' THEN ? ELSE TITLE END, \
                                     SUMMARY = CASE WHEN SUMMARY_LOCK = 0 AND ? <> '' THEN ? ELSE SUMMARY END, \
                                     LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                                 WHERE BOOK_ID = ?",
                            )
                            .bind(&title)
                            .bind(&title)
                            .bind(&summary)
                            .bind(&summary)
                            .bind(&book_id)
                            .execute(&pool)
                            .await
                            .map_err(|error| {
                                TaskExecutionError::runtime(format!(
                                    "failed to apply sidecar metadata for '{book_id}': {error}",
                                ))
                            })?;
                        }
                    }
                }
            }

            sqlx::query(
                "UPDATE BOOK_METADATA \
                 SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                 WHERE BOOK_ID = ?",
            )
            .bind(&book_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to refresh BOOK_METADATA for '{book_id}': {error}",
                ))
            })?;

            sqlx::query(
                "UPDATE BOOK \
                         SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                         WHERE ID = ?",
            )
            .bind(&book_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to refresh BOOK row timestamp for '{book_id}': {error}",
                ))
            })?;

            let series_id = sqlx::query(
                "SELECT SERIES_ID \
                 FROM BOOK \
                 WHERE ID = ? \
                 LIMIT 1",
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to resolve SERIES_ID for '{book_id}': {error}",
                ))
            })?
            .and_then(|row| row.get::<Option<String>, _>("SERIES_ID"));

            Ok::<Option<String>, TaskExecutionError>(series_id)
        })
    })
}

fn refresh_series_metadata(
    runtime: &RuntimeConfig,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let series_id = series_id.to_string();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let series_row = sqlx::query(
                "SELECT s.URL AS SERIES_URL, l.ROOT AS LIBRARY_ROOT \
                 FROM SERIES s \
                 JOIN LIBRARY l ON l.ID = s.LIBRARY_ID \
                 WHERE s.ID = ? \
                 LIMIT 1",
            )
            .bind(&series_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to resolve series path for metadata refresh '{series_id}': {error}",
                ))
            })?;

            if let Some(series_row) = &series_row {
                let series_url = series_row.get::<String, _>("SERIES_URL");
                let library_root = series_row.get::<String, _>("LIBRARY_ROOT");
                if let Some(sidecar_url) =
                    load_sidecar_url_for_parent(&pool, &series_url, true).await?
                {
                    let sidecar_path = PathBuf::from(library_root).join(sidecar_url);
                    if let Ok(xml) = fs::read_to_string(&sidecar_path) {
                        let title = extract_xml_tag(&xml, "Title").unwrap_or_default();
                        let summary = extract_xml_tag(&xml, "Summary").unwrap_or_default();
                        if !title.is_empty() || !summary.is_empty() {
                            sqlx::query(
                                "UPDATE SERIES_METADATA \
                                 SET TITLE = CASE WHEN TITLE_LOCK = 0 AND ? <> '' THEN ? ELSE TITLE END, \
                                     TITLE_SORT = CASE WHEN TITLE_SORT_LOCK = 0 AND ? <> '' THEN ? ELSE TITLE_SORT END, \
                                     SUMMARY = CASE WHEN SUMMARY_LOCK = 0 AND ? <> '' THEN ? ELSE SUMMARY END, \
                                     LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                                 WHERE SERIES_ID = ?",
                            )
                            .bind(&title)
                            .bind(&title)
                            .bind(&title)
                            .bind(&title)
                            .bind(&summary)
                            .bind(&summary)
                            .bind(&series_id)
                            .execute(&pool)
                            .await
                            .map_err(|error| {
                                TaskExecutionError::runtime(format!(
                                    "failed to apply series sidecar metadata for '{series_id}': {error}",
                                ))
                            })?;
                        }
                    }
                }
            }

            sqlx::query(
                "UPDATE SERIES_METADATA \
                 SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                 WHERE SERIES_ID = ?",
            )
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to refresh SERIES_METADATA for '{series_id}': {error}",
                ))
            })?;

            sqlx::query(
                "UPDATE SERIES \
                 SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                 WHERE ID = ?",
            )
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to refresh SERIES row for '{series_id}': {error}",
                ))
            })?;

            Ok::<(), TaskExecutionError>(())
        })
    })
}

fn aggregate_series_metadata(
    runtime: &RuntimeConfig,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let series_id = series_id.to_string();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT NAME \
                 FROM SERIES \
                 WHERE ID = ? \
                 LIMIT 1",
            )
            .bind(&series_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to load series for aggregation '{series_id}': {error}",
                ))
            })?;

            let Some(row) = row else {
                return Ok::<(), TaskExecutionError>(());
            };

            let series_name = row.get::<String, _>("NAME");

            sqlx::query(
                "UPDATE SERIES_METADATA \
                 SET TITLE = CASE WHEN TITLE_LOCK = 0 THEN ? ELSE TITLE END, \
                     TITLE_SORT = CASE WHEN TITLE_SORT_LOCK = 0 THEN ? ELSE TITLE_SORT END, \
                     LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                 WHERE SERIES_ID = ?",
            )
            .bind(&series_name)
            .bind(&series_name)
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to aggregate SERIES_METADATA for '{series_id}': {error}",
                ))
            })?;

            sqlx::query(
                "UPDATE SERIES \
                 SET BOOK_COUNT = (SELECT COUNT(*) \
                 FROM BOOK \
                 WHERE BOOK.SERIES_ID = SERIES.ID \
                 AND BOOK.DELETED_DATE IS NULL), \
                     LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                 WHERE ID = ?",
            )
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to aggregate SERIES counters for '{series_id}': {error}",
                ))
            })?;

            Ok::<(), TaskExecutionError>(())
        })
    })
}

fn refresh_book_local_artwork(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<(), TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let book_row = sqlx::query(
                "SELECT b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT \
                 FROM BOOK b \
                 JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
                 WHERE b.ID = ? \
                 LIMIT 1",
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to resolve book path for artwork refresh '{book_id}': {error}",
                ))
            })?;

            if let Some(book_row) = &book_row {
                let book_url = book_row.get::<String, _>("BOOK_URL");
                let library_root = book_row.get::<String, _>("LIBRARY_ROOT");
                if let Some(sidecar_url) =
                    load_sidecar_url_for_parent(&pool, &book_url, false).await?
                {
                    let sidecar_path = PathBuf::from(library_root).join(&sidecar_url);
                    if let Ok(meta) = fs::metadata(&sidecar_path) {
                        let media_type = media_type_from_sidecar_path(sidecar_path.as_path());
                        let thumbnail_id = format!("thumbnail-book-sidecar:{book_id}");
                        sqlx::query(
                            "INSERT OR REPLACE INTO THUMBNAIL_BOOK \
                             (ID, URL, SELECTED, TYPE, BOOK_ID, MEDIA_TYPE, FILE_SIZE, LAST_MODIFIED_DATE) \
                             VALUES (?, ?, 1, 'SIDECAR', ?, ?, ?, CURRENT_TIMESTAMP)",
                        )
                        .bind(&thumbnail_id)
                        .bind(sidecar_url)
                        .bind(&book_id)
                        .bind(media_type)
                        .bind(meta.len() as i64)
                        .execute(&pool)
                        .await
                        .map_err(|error| {
                            TaskExecutionError::runtime(format!(
                                "failed to upsert sidecar thumbnail for book '{book_id}': {error}",
                            ))
                        })?;
                    }
                }
            }

            sqlx::query(
                "UPDATE THUMBNAIL_BOOK \
                 SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                 WHERE BOOK_ID = ?",
            )
            .bind(&book_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to refresh THUMBNAIL_BOOK rows for '{book_id}': {error}",
                ))
            })?;

            sqlx::query("UPDATE BOOK \
                         SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                         WHERE ID = ?")
                .bind(&book_id)
                .execute(&pool)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to refresh BOOK row while updating local artwork for '{book_id}': {error}",
                    ))
                })?;

            Ok::<(), TaskExecutionError>(())
        })
    })
}

fn refresh_series_local_artwork(
    runtime: &RuntimeConfig,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let series_id = series_id.to_string();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let series_row = sqlx::query(
                "SELECT s.URL AS SERIES_URL, l.ROOT AS LIBRARY_ROOT \
                 FROM SERIES s \
                 JOIN LIBRARY l ON l.ID = s.LIBRARY_ID \
                 WHERE s.ID = ? \
                 LIMIT 1",
            )
            .bind(&series_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to resolve series path for artwork refresh '{series_id}': {error}",
                ))
            })?;

            if let Some(series_row) = &series_row {
                let series_url = series_row.get::<String, _>("SERIES_URL");
                let library_root = series_row.get::<String, _>("LIBRARY_ROOT");
                if let Some(sidecar_url) =
                    load_sidecar_url_for_parent(&pool, &series_url, false).await?
                {
                    let sidecar_path = PathBuf::from(library_root).join(&sidecar_url);
                    if let Ok(meta) = fs::metadata(&sidecar_path) {
                        let media_type = media_type_from_sidecar_path(sidecar_path.as_path());
                        let thumbnail_id = format!("thumbnail-series-sidecar:{series_id}");
                        sqlx::query(
                            "INSERT OR REPLACE INTO THUMBNAIL_SERIES \
                             (ID, URL, SELECTED, TYPE, SERIES_ID, MEDIA_TYPE, FILE_SIZE, LAST_MODIFIED_DATE) \
                             VALUES (?, ?, 1, 'SIDECAR', ?, ?, ?, CURRENT_TIMESTAMP)",
                        )
                        .bind(&thumbnail_id)
                        .bind(sidecar_url)
                        .bind(&series_id)
                        .bind(media_type)
                        .bind(meta.len() as i64)
                        .execute(&pool)
                        .await
                        .map_err(|error| {
                            TaskExecutionError::runtime(format!(
                                "failed to upsert sidecar thumbnail for series '{series_id}': {error}",
                            ))
                        })?;
                    }
                }
            }

            sqlx::query(
                "UPDATE THUMBNAIL_SERIES \
                 SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                 WHERE SERIES_ID = ?",
            )
            .bind(&series_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to refresh THUMBNAIL_SERIES rows for '{series_id}': {error}",
                ))
            })?;

            sqlx::query("UPDATE SERIES \
                         SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                         WHERE ID = ?")
                .bind(&series_id)
                .execute(&pool)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to refresh SERIES row while updating local artwork for '{series_id}': {error}",
                    ))
                })?;

            Ok::<(), TaskExecutionError>(())
        })
    })
}

async fn load_sidecar_url_for_parent(
    pool: &SqlitePool,
    parent_url: &str,
    metadata_only: bool,
) -> Result<Option<String>, TaskExecutionError> {
    let sql = if metadata_only {
        "SELECT URL \
         FROM SIDECAR \
         WHERE PARENT_URL = ? \
         AND LOWER(URL) LIKE '%.xml' \
         ORDER BY LAST_MODIFIED_TIME DESC \
         LIMIT 1"
    } else {
        "SELECT URL \
         FROM SIDECAR \
         WHERE PARENT_URL = ? \
         AND (LOWER(URL) LIKE '%.jpg' OR LOWER(URL) LIKE '%.jpeg' OR LOWER(URL) LIKE '%.png' \
              OR LOWER(URL) LIKE '%.webp' OR LOWER(URL) LIKE '%.gif' OR LOWER(URL) LIKE '%.avif') \
         ORDER BY LAST_MODIFIED_TIME DESC \
         LIMIT 1"
    };

    let row = sqlx::query(sql)
        .bind(parent_url)
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            TaskExecutionError::runtime(format!(
                "failed to load sidecar for '{parent_url}': {error}",
            ))
        })?;
    Ok(row.map(|row| row.get::<String, _>("URL")))
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    let value = xml[start..end].trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

fn media_type_from_sidecar_path(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("avif") => "image/avif",
        _ => "application/octet-stream",
    }
}

fn hash_book_pages(runtime: &RuntimeConfig, book_id: &str) -> Result<(), TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let book_id = book_id.to_string();

    std::thread::spawn(move || {
        let async_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "build hash-book-pages runtime failed: {error}"
                ))
            })?;

        async_runtime.block_on(async move {
            hash_book_pages_with_media_content(database_file.as_path(), &book_id)
                .await
                .map_err(TaskExecutionError::runtime)
        })
    })
    .join()
    .map_err(|_| TaskExecutionError::runtime("hash-book-pages worker thread panicked"))?
}

fn hash_book(
    runtime: &RuntimeConfig,
    book_id: &str,
    koreader: bool,
) -> Result<(), TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT b.URL AS URL, l.ROOT AS ROOT \
                 FROM BOOK b \
                 JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
                 WHERE b.ID = ? \
                 LIMIT 1",
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to query book file for hash task '{book_id}': {error}",
                ))
            })?;

            let Some(row) = row else {
                return Ok::<(), TaskExecutionError>(());
            };

            let file_path =
                PathBuf::from(row.get::<String, _>("ROOT")).join(row.get::<String, _>("URL"));
            let bytes = fs::read(&file_path).map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to read book file for hash task '{}': {error}",
                    file_path.display(),
                ))
            })?;

            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let digest = hasher.finalize();
            let hash = digest
                .iter()
                .map(|value| format!("{value:02x}"))
                .collect::<String>();

            let sql = if koreader {
                "UPDATE BOOK \
                 SET FILE_HASH_KOREADER = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                 WHERE ID = ?"
            } else {
                "UPDATE BOOK \
                 SET FILE_HASH = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                 WHERE ID = ?"
            };
            sqlx::query(sql)
                .bind(hash)
                .bind(&book_id)
                .execute(&pool)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to persist book hash for '{book_id}': {error}",
                    ))
                })?;

            Ok::<(), TaskExecutionError>(())
        })
    })
}

fn find_books_without_selected_thumbnails(
    runtime: &RuntimeConfig,
) -> Result<Vec<String>, TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT b.ID \
                 FROM BOOK b \
                 LEFT \
                 JOIN THUMBNAIL_BOOK tb ON tb.BOOK_ID = b.ID \
                 AND tb.SELECTED = 1 \
                 WHERE tb.ID IS NULL \
                 AND b.DELETED_DATE IS NULL",
            )
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to query books without selected thumbnails: {error}"
                ))
            })?;

            Ok::<Vec<String>, TaskExecutionError>(
                rows.into_iter()
                    .map(|row| row.get::<String, _>("ID"))
                    .collect(),
            )
        })
    })
}

fn find_books_with_missing_page_hash(
    runtime: &RuntimeConfig,
    library_id: Option<&str>,
) -> Result<Vec<String>, TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let library_id = library_id.map(str::to_string);
    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let rows = if let Some(library_id) = library_id.as_deref() {
                sqlx::query(
                    "SELECT DISTINCT mp.BOOK_ID AS BOOK_ID \
                     FROM MEDIA_PAGE mp \
                     JOIN BOOK b ON b.ID = mp.BOOK_ID \
                     WHERE b.LIBRARY_ID = ? \
                     AND (mp.FILE_HASH = '' OR mp.FILE_HASH IS NULL)",
                )
                .bind(library_id)
                .fetch_all(&pool)
                .await
            } else {
                sqlx::query(
                    "SELECT DISTINCT BOOK_ID \
                     FROM MEDIA_PAGE \
                     WHERE FILE_HASH = '' \
                     OR FILE_HASH IS NULL",
                )
                .fetch_all(&pool)
                .await
            }
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to query books with missing page hashes: {error}"
                ))
            })?;

            Ok::<Vec<String>, TaskExecutionError>(
                rows.into_iter()
                    .map(|row| row.get::<String, _>("BOOK_ID"))
                    .collect(),
            )
        })
    })
}

fn find_duplicate_pages_to_delete(
    runtime: &RuntimeConfig,
    library_id: &str,
) -> Result<HashMap<String, Vec<HashedPageToDelete>>, TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let library_id = library_id.to_string();
    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT mp.BOOK_ID AS BOOK_ID, mp.FILE_HASH AS FILE_HASH, mp.NUMBER AS PAGE_NUMBER, \
                        mp.FILE_NAME AS FILE_NAME, mp.MEDIA_TYPE AS MEDIA_TYPE \
                 FROM MEDIA_PAGE mp \
                 JOIN BOOK b ON b.ID = mp.BOOK_ID \
                 JOIN PAGE_HASH ph ON ph.HASH = mp.FILE_HASH \
                 WHERE b.LIBRARY_ID = ? \
                 AND b.DELETED_DATE IS NULL \
                 AND mp.FILE_HASH <> '' \
                 AND ph.ACTION = 'DELETE_AUTO' \
                 AND mp.FILE_HASH IN (SELECT mp2.FILE_HASH \
                 FROM MEDIA_PAGE mp2 \
                 JOIN BOOK b2 ON b2.ID = mp2.BOOK_ID \
                 WHERE b2.LIBRARY_ID = ? \
                 AND b2.DELETED_DATE IS NULL \
                 AND mp2.FILE_HASH <> '' \
                 GROUP BY mp2.FILE_HASH \
                 HAVING COUNT(*) > 1) \
                 ORDER BY mp.BOOK_ID ASC, mp.NUMBER ASC",
            )
            .bind(&library_id)
            .bind(&library_id)
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to query duplicate pages to delete for '{library_id}': {error}",
                ))
            })?;

            let mut by_book = HashMap::<String, Vec<HashedPageToDelete>>::new();
            for row in rows {
                let book_id = row.get::<String, _>("BOOK_ID");
                let hash = row.get::<String, _>("FILE_HASH");
                let number = row.get::<i64, _>("PAGE_NUMBER");
                let file_name = row.get::<String, _>("FILE_NAME");
                let media_type = row.get::<String, _>("MEDIA_TYPE");
                by_book
                    .entry(book_id)
                    .or_default()
                    .push(HashedPageToDelete {
                        hash,
                        number,
                        file_name,
                        media_type,
                    });
            }

            Ok::<HashMap<String, Vec<HashedPageToDelete>>, TaskExecutionError>(by_book)
        })
    })
}

fn remove_hashed_pages(
    runtime: &RuntimeConfig,
    book_id: &str,
    pages: &[HashedPageToDelete],
) -> Result<bool, TaskExecutionError> {
    if pages.is_empty() {
        return Ok(false);
    }

    let source = load_book_archive_source(runtime, book_id)?;
    let Some(source) = source else {
        return Ok(false);
    };

    if !source.media_type.eq_ignore_ascii_case("application/zip")
        || !source.media_status.eq_ignore_ascii_case("READY")
    {
        return Ok(false);
    }

    let removed_pages = rewrite_zip_book_without_pages(&source.file_path, pages)?;
    if removed_pages.is_empty() {
        return Ok(false);
    }

    let mut deleted_count_by_hash = HashMap::<String, i64>::new();
    for removed in &removed_pages {
        *deleted_count_by_hash
            .entry(removed.hash.clone())
            .or_insert(0) += 1;
    }

    let file_size = fs::metadata(&source.file_path)
        .map(|metadata| metadata.len() as i64)
        .unwrap_or_default();
    let file_last_modified = fs::metadata(&source.file_path)
        .map(|metadata| metadata_updated_unix_seconds(&metadata))
        .unwrap_or_default();

    let database_file = runtime.database_file.clone();
    let book_id = book_id.to_string();
    let analyze_book_id = book_id.clone();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        let deleted_count_by_hash = deleted_count_by_hash.clone();
        Box::pin(async move {
            let mut tx = pool.begin().await.map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to start remove-hashed-pages transaction for '{book_id}': {error}",
                ))
            })?;

            for (hash, deleted) in deleted_count_by_hash {
                sqlx::query(
                    "UPDATE PAGE_HASH \
                     SET DELETE_COUNT = DELETE_COUNT + ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                     WHERE HASH = ?",
                )
                .bind(deleted)
                .bind(hash)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to update PAGE_HASH delete count for '{book_id}': {error}",
                    ))
                })?;
            }

            sqlx::query(
                "UPDATE BOOK \
                 SET FILE_LAST_MODIFIED = ?, FILE_SIZE = ?, FILE_HASH = '', FILE_HASH_KOREADER = '', \
                     LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                 WHERE ID = ?",
            )
            .bind(file_last_modified)
            .bind(file_size)
            .bind(&book_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to update BOOK metadata after hashed-page removal for '{book_id}': {error}",
                ))
            })?;

            tx.commit().await.map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to commit remove-hashed-pages transaction for '{book_id}': {error}",
                ))
            })?;

            Ok::<(), TaskExecutionError>(())
        })
    })?;

    analyze_book(runtime, analyze_book_id.as_str())?;

    Ok(removed_pages.iter().any(|page| page.number == 0))
}

fn find_books_requiring_analysis(
    runtime: &RuntimeConfig,
    book_ids: &[String],
) -> Result<Vec<String>, TaskExecutionError> {
    if book_ids.is_empty() {
        return Ok(Vec::new());
    }

    let database_file = runtime.database_file.clone();
    let book_ids = book_ids.to_vec();

    run_database_query(database_file, move |pool| {
        let book_ids = book_ids.clone();
        Box::pin(async move {
            let mut result = Vec::new();

            for book_id in book_ids {
                let status = sqlx::query(
                    "SELECT STATUS \
                     FROM MEDIA \
                     WHERE BOOK_ID = ? \
                     LIMIT 1",
                )
                .bind(&book_id)
                .fetch_optional(&pool)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to query media status for '{book_id}': {error}",
                    ))
                })?
                .map(|row| row.get::<String, _>("STATUS"));

                let needs_analysis = match status.as_deref() {
                    None => true,
                    Some(status) => {
                        status.eq_ignore_ascii_case("UNKNOWN")
                            || status.eq_ignore_ascii_case("OUTDATED")
                    }
                };

                if needs_analysis {
                    result.push(book_id);
                }
            }

            Ok::<Vec<String>, TaskExecutionError>(result)
        })
    })
}

fn parse_scan_library_payload_deep(payload: &str) -> Option<bool> {
    serde_json::from_str::<serde_json::Value>(payload)
        .ok()?
        .get("deep")
        .and_then(|value| value.as_bool())
}

struct LibraryHashingFlags {
    hash_files: bool,
    hash_pages: bool,
    hash_koreader: bool,
}

struct LibraryMaintenanceFlags {
    repair_extensions: bool,
    convert_to_cbz: bool,
}

struct CleanupEmptySetsFlags {
    delete_collections: bool,
    delete_readlists: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct RemoveHashedPagesPayload {
    pages: Vec<HashedPageToDelete>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct HashedPageToDelete {
    hash: String,
    number: i64,
    file_name: String,
    media_type: String,
}

struct BookArchiveSource {
    file_path: PathBuf,
    media_type: String,
    media_status: String,
}

fn load_book_archive_source(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<Option<BookArchiveSource>, TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT, \
                        COALESCE(m.MEDIA_TYPE, '') AS MEDIA_TYPE, COALESCE(m.STATUS, '') AS MEDIA_STATUS \
                 FROM BOOK b \
                 JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
                 LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID \
                 WHERE b.ID = ? \
                 LIMIT 1",
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to load archive source for '{book_id}': {error}",
                ))
            })?;

            let Some(row) = row else {
                return Ok::<Option<BookArchiveSource>, TaskExecutionError>(None);
            };

            let file_path = PathBuf::from(row.get::<String, _>("LIBRARY_ROOT"))
                .join(row.get::<String, _>("BOOK_URL"));
            Ok::<Option<BookArchiveSource>, TaskExecutionError>(Some(BookArchiveSource {
                file_path,
                media_type: row.get::<String, _>("MEDIA_TYPE"),
                media_status: row.get::<String, _>("MEDIA_STATUS"),
            }))
        })
    })
}

fn rewrite_zip_book_without_pages(
    archive_path: &PathBuf,
    pages_to_delete: &[HashedPageToDelete],
) -> Result<Vec<HashedPageToDelete>, TaskExecutionError> {
    let source_file = fs::File::open(archive_path).map_err(|error| {
        TaskExecutionError::runtime(format!(
            "failed to open archive '{}' for page deletion: {error}",
            archive_path.display(),
        ))
    })?;
    let mut archive = ZipArchive::new(source_file).map_err(|error| {
        TaskExecutionError::runtime(format!(
            "failed to read zip archive '{}' for page deletion: {error}",
            archive_path.display(),
        ))
    })?;

    let mut delete_by_name = HashMap::<String, Vec<HashedPageToDelete>>::new();
    for page in pages_to_delete {
        delete_by_name
            .entry(page.file_name.clone())
            .or_default()
            .push(page.clone());
    }

    let mut kept_entries = Vec::<(String, Vec<u8>)>::new();
    let mut removed_pages = Vec::<HashedPageToDelete>::new();

    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            TaskExecutionError::runtime(format!(
                "failed to read zip entry index {index} for '{}': {error}",
                archive_path.display(),
            ))
        })?;
        if entry.is_dir() {
            continue;
        }

        let entry_name = entry.name().to_string();
        let should_remove = delete_by_name
            .get(&entry_name)
            .and_then(|candidates| {
                candidates.iter().find(|candidate| {
                    candidate.media_type == media_type_from_entry_name(&entry_name)
                })
            })
            .cloned();

        if let Some(removed) = should_remove {
            removed_pages.push(removed);
            continue;
        }

        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|error| {
            TaskExecutionError::runtime(format!(
                "failed to read zip entry '{}' bytes for '{}': {error}",
                entry_name,
                archive_path.display(),
            ))
        })?;
        kept_entries.push((entry_name, bytes));
    }

    if removed_pages.is_empty() {
        return Ok(Vec::new());
    }

    if kept_entries.is_empty() {
        return Err(TaskExecutionError::runtime(format!(
            "refused to rewrite '{}' with zero entries after page deletion",
            archive_path.display(),
        )));
    }

    let rewritten = build_stored_zip_archive(kept_entries)?;
    let temp_path = archive_path.with_extension("komga-page-removal.tmp");
    fs::write(&temp_path, rewritten).map_err(|error| {
        TaskExecutionError::runtime(format!(
            "failed to write temporary rewritten archive '{}': {error}",
            temp_path.display(),
        ))
    })?;
    fs::rename(&temp_path, archive_path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        TaskExecutionError::runtime(format!(
            "failed to replace archive '{}' with rewritten file '{}': {error}",
            archive_path.display(),
            temp_path.display(),
        ))
    })?;

    Ok(removed_pages)
}

fn load_library_hashing_flags(
    runtime: &RuntimeConfig,
    library_id: &str,
) -> Result<LibraryHashingFlags, TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let library_id = library_id.to_string();

    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT COALESCE(HASH_FILES, 0) AS HASH_FILES, \
                        COALESCE(HASH_PAGES, 0) AS HASH_PAGES, \
                        COALESCE(HASH_KOREADER, 0) AS HASH_KOREADER \
                 FROM LIBRARY \
                 WHERE ID = ? \
                 LIMIT 1",
            )
            .bind(&library_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to load library hashing flags for '{library_id}': {error}",
                ))
            })?;

            let Some(row) = row else {
                return Ok::<LibraryHashingFlags, TaskExecutionError>(LibraryHashingFlags {
                    hash_files: false,
                    hash_pages: false,
                    hash_koreader: false,
                });
            };

            Ok::<LibraryHashingFlags, TaskExecutionError>(LibraryHashingFlags {
                hash_files: row.get::<i64, _>("HASH_FILES") != 0,
                hash_pages: row.get::<i64, _>("HASH_PAGES") != 0,
                hash_koreader: row.get::<i64, _>("HASH_KOREADER") != 0,
            })
        })
    })
}

fn load_library_maintenance_flags(
    runtime: &RuntimeConfig,
    library_id: &str,
) -> Result<LibraryMaintenanceFlags, TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let library_id = library_id.to_string();

    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT COALESCE(REPAIR_EXTENSIONS, 0) AS REPAIR_EXTENSIONS, \
                        COALESCE(CONVERT_TO_CBZ, 0) AS CONVERT_TO_CBZ \
                 FROM LIBRARY \
                 WHERE ID = ? \
                 LIMIT 1",
            )
            .bind(&library_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to load library maintenance flags for '{library_id}': {error}",
                ))
            })?;

            let Some(row) = row else {
                return Ok::<LibraryMaintenanceFlags, TaskExecutionError>(
                    LibraryMaintenanceFlags {
                        repair_extensions: false,
                        convert_to_cbz: false,
                    },
                );
            };

            Ok::<LibraryMaintenanceFlags, TaskExecutionError>(LibraryMaintenanceFlags {
                repair_extensions: row.get::<i64, _>("REPAIR_EXTENSIONS") != 0,
                convert_to_cbz: row.get::<i64, _>("CONVERT_TO_CBZ") != 0,
            })
        })
    })
}

fn repair_extensions(runtime: &RuntimeConfig, library_id: &str) -> Result<(), TaskExecutionError> {
    let flags = load_library_maintenance_flags(runtime, library_id)?;
    if !flags.repair_extensions {
        return Ok(());
    }

    let database_file = runtime.database_file.clone();
    let library_id = library_id.to_string();

    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT b.ID AS BOOK_ID, b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT, m.MEDIA_TYPE AS MEDIA_TYPE \
                 FROM BOOK b \
                 JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
                 JOIN MEDIA m ON m.BOOK_ID = b.ID \
                 WHERE b.LIBRARY_ID = ? \
                 AND b.DELETED_DATE IS NULL",
            )
            .bind(&library_id)
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to query books for extension repair in '{library_id}': {error}",
                ))
            })?;

            for row in rows {
                let book_id = row.get::<String, _>("BOOK_ID");
                let book_url = row.get::<String, _>("BOOK_URL");
                let library_root = row.get::<String, _>("LIBRARY_ROOT");
                let media_type = row.get::<String, _>("MEDIA_TYPE");

                let Some(correct_extension) = expected_extension_for_media_type(&media_type) else {
                    continue;
                };

                let source_path = PathBuf::from(&library_root).join(&book_url);
                if !source_path.exists() {
                    continue;
                }

                let current_extension = source_path
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(|value| value.to_ascii_lowercase())
                    .unwrap_or_default();
                if current_extension == correct_extension {
                    continue;
                }

                if media_type == "application/zip" && current_extension == "epub" {
                    continue;
                }

                let destination_path = source_path.with_extension(correct_extension);
                if destination_path.exists() {
                    return Err(TaskExecutionError::runtime(format!(
                        "failed to repair extension for '{book_id}': destination already exists '{}'",
                        destination_path.display(),
                    )));
                }

                fs::rename(&source_path, &destination_path).map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to rename book file for extension repair '{}' -> '{}': {error}",
                        source_path.display(),
                        destination_path.display(),
                    ))
                })?;

                let destination_url = normalize_library_relative_url(
                    &PathBuf::from(&library_root),
                    &destination_path,
                )?;
                let file_size = fs::metadata(&destination_path)
                    .map(|metadata| metadata.len() as i64)
                    .unwrap_or_default();
                let file_last_modified = fs::metadata(&destination_path)
                    .map(|metadata| metadata_updated_unix_seconds(&metadata))
                    .unwrap_or_default();

                let repair_result: Result<(), TaskExecutionError> = async {
                    let mut tx = pool.begin().await.map_err(|error| {
                        TaskExecutionError::runtime(format!(
                            "failed to start extension-repair transaction for '{book_id}': {error}",
                        ))
                    })?;

                    sqlx::query(
                        "UPDATE BOOK \
                         SET URL = ?, FILE_LAST_MODIFIED = ?, FILE_SIZE = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                         WHERE ID = ?",
                    )
                    .bind(&destination_url)
                    .bind(file_last_modified)
                    .bind(file_size)
                    .bind(&book_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| {
                        TaskExecutionError::runtime(format!(
                            "failed to update BOOK row during extension repair for '{book_id}': {error}",
                        ))
                    })?;

                    sqlx::query(
                        "UPDATE SIDECAR \
                         SET PARENT_URL = ? \
                         WHERE LIBRARY_ID = ? \
                         AND PARENT_URL = ?",
                    )
                    .bind(&destination_url)
                    .bind(&library_id)
                    .bind(&book_url)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| {
                        TaskExecutionError::runtime(format!(
                            "failed to update SIDECAR rows during extension repair for '{book_id}': {error}",
                        ))
                    })?;

                    tx.commit().await.map_err(|error| {
                        TaskExecutionError::runtime(format!(
                            "failed to commit extension-repair transaction for '{book_id}': {error}",
                        ))
                    })?;

                    Ok(())
                }
                .await;

                if let Err(error) = repair_result {
                    let _ = fs::rename(&destination_path, &source_path);
                    return Err(error);
                }
            }

            Ok::<(), TaskExecutionError>(())
        })
    })
}

fn find_books_to_convert(
    runtime: &RuntimeConfig,
    library_id: &str,
) -> Result<Vec<String>, TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let library_id = library_id.to_string();

    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT ID \
                 FROM BOOK \
                 JOIN MEDIA ON MEDIA.BOOK_ID = BOOK.ID \
                 WHERE LIBRARY_ID = ? \
                 AND DELETED_DATE IS NULL \
                 AND LOWER(MEDIA.MEDIA_TYPE) IN ('application/vnd.comicbook-rar', 'application/x-rar-compressed')",
            )
            .bind(&library_id)
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to query books to convert for '{library_id}': {error}",
                ))
            })?;

            Ok::<Vec<String>, TaskExecutionError>(
                rows.into_iter()
                    .map(|row| row.get::<String, _>("ID"))
                    .collect(),
            )
        })
    })
}

fn convert_book(runtime: &RuntimeConfig, book_id: &str) -> Result<(), TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let book_id = book_id.to_string();
    let analyze_book_id = book_id.clone();

    let converted = run_database_query(database_file, move |pool| {
        let book_id = book_id.clone();
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT b.URL AS BOOK_URL, b.LIBRARY_ID AS LIBRARY_ID, l.ROOT AS LIBRARY_ROOT, \
                        COALESCE(l.CONVERT_TO_CBZ, 0) AS CONVERT_TO_CBZ, \
                        COALESCE(m.MEDIA_TYPE, '') AS MEDIA_TYPE, COALESCE(m.STATUS, '') AS MEDIA_STATUS \
                 FROM BOOK b \
                 JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
                 LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID \
                 WHERE b.ID = ? \
                 LIMIT 1",
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to load convert-book source row for '{book_id}': {error}",
                ))
            })?;

            let Some(row) = row else {
                return Ok::<bool, TaskExecutionError>(false);
            };

            let convert_to_cbz = row.get::<i64, _>("CONVERT_TO_CBZ") != 0;
            if !convert_to_cbz {
                return Ok::<bool, TaskExecutionError>(false);
            }

            let media_type = row.get::<String, _>("MEDIA_TYPE");
            let media_status = row.get::<String, _>("MEDIA_STATUS");
            if !media_status.eq_ignore_ascii_case("READY") {
                return Ok::<bool, TaskExecutionError>(false);
            }
            if !is_rar_media_type(&media_type) {
                return Ok::<bool, TaskExecutionError>(false);
            }

            let book_url = row.get::<String, _>("BOOK_URL");
            let library_root = row.get::<String, _>("LIBRARY_ROOT");
            let source_path = PathBuf::from(&library_root).join(&book_url);
            if !source_path.exists() {
                return Ok::<bool, TaskExecutionError>(false);
            }

            let destination_path = source_path.with_extension("cbz");
            if destination_path.exists() {
                return Err(TaskExecutionError::runtime(format!(
                    "failed to convert book '{book_id}' to CBZ: destination already exists '{}'",
                    destination_path.display(),
                )));
            }

            let archive_entries = load_rar_entries_for_conversion(&source_path)?;
            if archive_entries.is_empty() {
                return Err(TaskExecutionError::runtime(format!(
                    "failed to convert book '{book_id}' to CBZ: no archive entries extracted",
                )));
            }

            let payload = build_stored_zip_archive(archive_entries)?;
            fs::write(&destination_path, payload).map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to write converted CBZ file for '{book_id}' to '{}': {error}",
                    destination_path.display(),
                ))
            })?;

            let destination_file = fs::File::open(&destination_path).map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to open converted file for '{book_id}' ('{}'): {error}",
                    destination_path.display(),
                ))
            })?;
            ZipArchive::new(destination_file).map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to validate converted CBZ for '{book_id}': {error}",
                ))
            })?;

            let destination_url =
                normalize_library_relative_url(&PathBuf::from(&library_root), &destination_path)?;
            let file_size = fs::metadata(&destination_path)
                .map(|metadata| metadata.len() as i64)
                .unwrap_or_default();
            let file_last_modified = fs::metadata(&destination_path)
                .map(|metadata| metadata_updated_unix_seconds(&metadata))
                .unwrap_or_default();

            let convert_result: Result<(), TaskExecutionError> = async {
                let mut tx = pool.begin().await.map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to start convert-book transaction for '{book_id}': {error}",
                    ))
                })?;

                sqlx::query(
                    "UPDATE BOOK \
                     SET URL = ?, FILE_LAST_MODIFIED = ?, FILE_SIZE = ?, FILE_HASH = '', FILE_HASH_KOREADER = '', \
                         LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                     WHERE ID = ?",
                )
                .bind(&destination_url)
                .bind(file_last_modified)
                .bind(file_size)
                .bind(&book_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to update BOOK row during conversion for '{book_id}': {error}",
                    ))
                })?;

                sqlx::query(
                    "UPDATE SIDECAR \
                     SET PARENT_URL = ? \
                     WHERE LIBRARY_ID = ? \
                     AND PARENT_URL = ?",
                )
                .bind(&destination_url)
                .bind(row.get::<String, _>("LIBRARY_ID"))
                .bind(&book_url)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to update SIDECAR rows during conversion for '{book_id}': {error}",
                    ))
                })?;

                sqlx::query(
                    "UPDATE MEDIA \
                     SET STATUS = 'OUTDATED', MEDIA_TYPE = 'application/zip', LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                     WHERE BOOK_ID = ?",
                )
                .bind(&book_id)
                .execute(&mut *tx)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to refresh MEDIA row during conversion for '{book_id}': {error}",
                    ))
                })?;

                tx.commit().await.map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to commit convert-book transaction for '{book_id}': {error}",
                    ))
                })?;

                Ok(())
            }
            .await;

            if let Err(error) = convert_result {
                let _ = fs::remove_file(&destination_path);
                return Err(error);
            }

            let _ = fs::remove_file(&source_path);

            Ok::<bool, TaskExecutionError>(true)
        })
    })?;

    if converted {
        analyze_book(runtime, &analyze_book_id)?;
    }

    Ok(())
}

fn find_books_with_missing_file_hash(
    runtime: &RuntimeConfig,
    library_id: &str,
    koreader: bool,
) -> Result<Vec<String>, TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let library_id = library_id.to_string();

    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
            let sql = if koreader {
                "SELECT ID \
                 FROM BOOK \
                 WHERE LIBRARY_ID = ? \
                 AND DELETED_DATE IS NULL \
                 AND (FILE_HASH_KOREADER = '' OR FILE_HASH_KOREADER IS NULL)"
            } else {
                "SELECT ID \
                 FROM BOOK \
                 WHERE LIBRARY_ID = ? \
                 AND DELETED_DATE IS NULL \
                 AND (FILE_HASH = '' OR FILE_HASH IS NULL)"
            };

            let rows = sqlx::query(sql)
                .bind(&library_id)
                .fetch_all(&pool)
                .await
                .map_err(|error| {
                    TaskExecutionError::runtime(format!(
                        "failed to query books with missing file hash for '{library_id}': {error}",
                    ))
                })?;

            Ok::<Vec<String>, TaskExecutionError>(
                rows.into_iter()
                    .map(|row| row.get::<String, _>("ID"))
                    .collect(),
            )
        })
    })
}

fn delete_book_task(runtime: &RuntimeConfig, book_id: &str) -> Result<(), TaskExecutionError> {
    let target = load_book_delete_target(runtime, book_id)?;
    let Some((series_id, oneshot)) = target else {
        return Ok(());
    };

    if oneshot {
        delete_series(runtime, &series_id)
    } else {
        delete_book(runtime, book_id)
    }
}

fn load_book_delete_target(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<Option<(String, bool)>, TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT SERIES_ID, COALESCE(oneshot, 0) AS ONESHOT \
                 FROM BOOK \
                 WHERE ID = ? \
                 LIMIT 1",
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to resolve delete-book target for '{book_id}': {error}",
                ))
            })?;

            Ok::<Option<(String, bool)>, TaskExecutionError>(row.map(|row| {
                (
                    row.get::<String, _>("SERIES_ID"),
                    row.get::<i64, _>("ONESHOT") != 0,
                )
            }))
        })
    })
}

fn delete_book(runtime: &RuntimeConfig, book_id: &str) -> Result<(), TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let book_id = book_id.to_string();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT b.SERIES_ID AS SERIES_ID, b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT \
                 FROM BOOK b \
                 JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
                 WHERE b.ID = ? \
                 LIMIT 1",
            )
            .bind(&book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to load book delete target for '{book_id}': {error}",
                ))
            })?;

            let Some(row) = row else {
                return Ok::<(), TaskExecutionError>(());
            };

            let series_id = row.get::<String, _>("SERIES_ID");
            let library_root = row.get::<String, _>("LIBRARY_ROOT");
            let book_url = row.get::<String, _>("BOOK_URL");

            let book_path = PathBuf::from(library_root).join(book_url);
            let _ = fs::remove_file(book_path);

            let mut tx = pool.begin().await.map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to start delete-book transaction for '{book_id}': {error}",
                ))
            })?;

            for sql in [
                "DELETE \
                 FROM BOOK_METADATA \
                 WHERE BOOK_ID = ?",
                "DELETE \
                 FROM BOOK_METADATA_AUTHOR \
                 WHERE BOOK_ID = ?",
                "DELETE \
                 FROM BOOK_METADATA_LINK \
                 WHERE BOOK_ID = ?",
                "DELETE \
                 FROM BOOK_METADATA_TAG \
                 WHERE BOOK_ID = ?",
                "DELETE \
                 FROM MEDIA \
                 WHERE BOOK_ID = ?",
                "DELETE \
                 FROM MEDIA_FILE \
                 WHERE BOOK_ID = ?",
                "DELETE \
                 FROM MEDIA_PAGE \
                 WHERE BOOK_ID = ?",
                "DELETE \
                 FROM THUMBNAIL_BOOK \
                 WHERE BOOK_ID = ?",
                "DELETE \
                 FROM READ_PROGRESS \
                 WHERE BOOK_ID = ?",
                "DELETE \
                 FROM READLIST_BOOK \
                 WHERE BOOK_ID = ?",
            ] {
                sqlx::query(sql)
                    .bind(&book_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| {
                        TaskExecutionError::runtime(format!(
                            "failed to delete dependent rows while deleting book '{book_id}': {error}",
                        ))
                    })?;
            }

            sqlx::query(
                "DELETE \
                         FROM BOOK \
                         WHERE ID = ?",
            )
            .bind(&book_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to delete BOOK row for '{book_id}': {error}",
                ))
            })?;

            sqlx::query(
                "UPDATE SERIES \
                 SET BOOK_COUNT = (SELECT COUNT(*) \
                 FROM BOOK \
                 WHERE BOOK.SERIES_ID = SERIES.ID) \
                 WHERE ID = ?",
            )
            .bind(&series_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to refresh series count for '{series_id}' while deleting book '{book_id}': {error}",
                ))
            })?;

            tx.commit().await.map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to commit delete-book transaction for '{book_id}': {error}",
                ))
            })?;

            Ok::<(), TaskExecutionError>(())
        })
    })
}

fn delete_series(runtime: &RuntimeConfig, series_id: &str) -> Result<(), TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let series_id = series_id.to_string();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT b.ID AS BOOK_ID, b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT \
                 FROM BOOK b \
                 JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
                 WHERE b.SERIES_ID = ?",
            )
            .bind(&series_id)
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to load series books for delete '{series_id}': {error}",
                ))
            })?;

            let book_ids = rows
                .iter()
                .map(|row| row.get::<String, _>("BOOK_ID"))
                .collect::<Vec<_>>();

            for row in &rows {
                let library_root = row.get::<String, _>("LIBRARY_ROOT");
                let book_url = row.get::<String, _>("BOOK_URL");
                let _ = fs::remove_file(PathBuf::from(library_root).join(book_url));
            }

            let mut tx = pool.begin().await.map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to start delete-series transaction for '{series_id}': {error}",
                ))
            })?;

            for book_id in &book_ids {
                for sql in [
                    "DELETE \
                     FROM BOOK_METADATA \
                     WHERE BOOK_ID = ?",
                    "DELETE \
                     FROM BOOK_METADATA_AUTHOR \
                     WHERE BOOK_ID = ?",
                    "DELETE \
                     FROM BOOK_METADATA_LINK \
                     WHERE BOOK_ID = ?",
                    "DELETE \
                     FROM BOOK_METADATA_TAG \
                     WHERE BOOK_ID = ?",
                    "DELETE \
                     FROM MEDIA \
                     WHERE BOOK_ID = ?",
                    "DELETE \
                     FROM MEDIA_FILE \
                     WHERE BOOK_ID = ?",
                    "DELETE \
                     FROM MEDIA_PAGE \
                     WHERE BOOK_ID = ?",
                    "DELETE \
                     FROM THUMBNAIL_BOOK \
                     WHERE BOOK_ID = ?",
                    "DELETE \
                     FROM READ_PROGRESS \
                     WHERE BOOK_ID = ?",
                    "DELETE \
                     FROM READLIST_BOOK \
                     WHERE BOOK_ID = ?",
                ] {
                    sqlx::query(sql)
                        .bind(book_id)
                        .execute(&mut *tx)
                        .await
                        .map_err(|error| {
                            TaskExecutionError::runtime(format!(
                                "failed to delete dependent rows while deleting series '{series_id}': {error}",
                            ))
                        })?;
                }
            }

            sqlx::query(
                "DELETE \
                         FROM BOOK \
                         WHERE SERIES_ID = ?",
            )
            .bind(&series_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to delete BOOK rows for series '{series_id}': {error}",
                ))
            })?;

            for sql in [
                "DELETE \
                 FROM SERIES_METADATA_ALTERNATE_TITLE \
                 WHERE SERIES_ID = ?",
                "DELETE \
                 FROM SERIES_METADATA_GENRE \
                 WHERE SERIES_ID = ?",
                "DELETE \
                 FROM SERIES_METADATA_LINK \
                 WHERE SERIES_ID = ?",
                "DELETE \
                 FROM SERIES_METADATA_SHARING \
                 WHERE SERIES_ID = ?",
                "DELETE \
                 FROM SERIES_METADATA_TAG \
                 WHERE SERIES_ID = ?",
                "DELETE \
                 FROM THUMBNAIL_SERIES \
                 WHERE SERIES_ID = ?",
                "DELETE \
                 FROM COLLECTION_SERIES \
                 WHERE SERIES_ID = ?",
                "DELETE \
                 FROM SERIES_METADATA \
                 WHERE SERIES_ID = ?",
            ] {
                sqlx::query(sql)
                    .bind(&series_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|error| {
                        TaskExecutionError::runtime(format!(
                            "failed to delete series dependent rows for '{series_id}': {error}",
                        ))
                    })?;
            }

            sqlx::query(
                "DELETE \
                         FROM SERIES \
                         WHERE ID = ?",
            )
            .bind(&series_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to delete SERIES row '{series_id}': {error}",
                ))
            })?;

            tx.commit().await.map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to commit delete-series transaction for '{series_id}': {error}",
                ))
            })?;

            Ok::<(), TaskExecutionError>(())
        })
    })
}

#[derive(Clone, Debug)]
struct LibraryScanConfig {
    root: String,
    scan_cbx: bool,
    scan_pdf: bool,
    scan_epub: bool,
    scan_force_modified_time: bool,
    oneshots_directory: Option<String>,
    scan_directory_exclusions: Vec<String>,
}

fn load_library_scan_config(
    runtime: &RuntimeConfig,
    library_id: &str,
) -> Result<Option<LibraryScanConfig>, TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let library_id = library_id.to_string();
    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT ROOT, SCAN_CBX, SCAN_PDF, SCAN_EPUB, SCAN_FORCE_MODIFIED_TIME, ONESHOTS_DIRECTORY \
                 FROM LIBRARY \
                 WHERE ID = ? \
                 LIMIT 1",
            )
            .bind(&library_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!("failed to load library root: {error}"))
            })?;

            let Some(row) = row else {
                return Ok::<Option<LibraryScanConfig>, TaskExecutionError>(None);
            };

            let exclusions = sqlx::query(
                "SELECT EXCLUSION \
                 FROM LIBRARY_EXCLUSIONS \
                 WHERE LIBRARY_ID = ?",
            )
            .bind(&library_id)
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to load library exclusions for '{library_id}': {error}",
                ))
            })?
            .into_iter()
            .map(|row| row.get::<String, _>("EXCLUSION"))
            .collect::<Vec<_>>();

            Ok::<Option<LibraryScanConfig>, TaskExecutionError>(Some(LibraryScanConfig {
                root: row.get::<String, _>("ROOT"),
                scan_cbx: row.get::<bool, _>("SCAN_CBX"),
                scan_pdf: row.get::<bool, _>("SCAN_PDF"),
                scan_epub: row.get::<bool, _>("SCAN_EPUB"),
                scan_force_modified_time: row.get::<bool, _>("SCAN_FORCE_MODIFIED_TIME"),
                oneshots_directory: row.get::<Option<String>, _>("ONESHOTS_DIRECTORY"),
                scan_directory_exclusions: exclusions,
            }))
        })
    })
}

fn library_empty_trash_after_scan(
    runtime: &RuntimeConfig,
    library_id: &str,
) -> Result<bool, TaskExecutionError> {
    let database_file = runtime.database_file.clone();
    let library_id = library_id.to_string();
    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            let value = sqlx::query(
                "SELECT EMPTY_TRASH_AFTER_SCAN \
                 FROM LIBRARY \
                 WHERE ID = ? \
                 LIMIT 1",
            )
            .bind(&library_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to load empty-trash-after-scan flag for '{library_id}': {error}",
                ))
            })?
            .map(|row| row.get::<bool, _>("EMPTY_TRASH_AFTER_SCAN"))
            .unwrap_or(false);

            Ok::<bool, TaskExecutionError>(value)
        })
    })
}

async fn load_cleanup_empty_sets_flags_from_pool(
    pool: &SqlitePool,
) -> Result<CleanupEmptySetsFlags, TaskExecutionError> {
    let rows = sqlx::query(
        "SELECT KEY, VALUE \
         FROM SERVER_SETTINGS \
         WHERE KEY IN ('DELETE_EMPTY_COLLECTIONS', 'DELETE_EMPTY_READLISTS')",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| {
        TaskExecutionError::runtime(format!(
            "failed to load cleanup-empty-sets flags from server settings: {error}",
        ))
    })?;

    let mut delete_collections = false;
    let mut delete_readlists = false;

    for row in rows {
        let key = row.get::<String, _>("KEY");
        let value = row.get::<Option<String>, _>("VALUE").unwrap_or_default();
        let enabled = value.trim().eq_ignore_ascii_case("true");
        match key.as_str() {
            "DELETE_EMPTY_COLLECTIONS" => delete_collections = enabled,
            "DELETE_EMPTY_READLISTS" => delete_readlists = enabled,
            _ => {}
        }
    }

    Ok(CleanupEmptySetsFlags {
        delete_collections,
        delete_readlists,
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

#[derive(Clone, Debug)]
struct AnalyzedMediaPageRow {
    file_name: String,
    media_type: String,
    file_size: i64,
}

#[derive(Clone, Debug)]
struct BookMediaAnalysis {
    status: String,
    media_type: String,
    pages: Vec<AnalyzedMediaPageRow>,
}

fn analyze_book_media_file(
    file_path: &PathBuf,
    book_url: &str,
) -> Result<BookMediaAnalysis, String> {
    let media_type = media_type_from_path(book_url);

    if !file_path.exists() {
        return Ok(BookMediaAnalysis {
            status: "ERROR".to_string(),
            media_type,
            pages: Vec::new(),
        });
    }

    let pages = match media_type.as_str() {
        "application/zip" => analyze_zip_media_pages(file_path, false)
            .unwrap_or_else(|_| fallback_media_analysis_pages(file_path, media_type.as_str())),
        "application/epub+zip" => analyze_zip_media_pages(file_path, true)
            .unwrap_or_else(|_| fallback_media_analysis_pages(file_path, media_type.as_str())),
        "application/vnd.comicbook-rar" | "application/x-rar-compressed" => {
            analyze_rar_media_pages(file_path)
                .unwrap_or_else(|_| fallback_media_analysis_pages(file_path, media_type.as_str()))
        }
        "application/pdf" => analyze_pdf_media_pages(file_path)
            .unwrap_or_else(|_| fallback_media_analysis_pages(file_path, media_type.as_str())),
        _ => {
            return Ok(BookMediaAnalysis {
                status: "UNSUPPORTED".to_string(),
                media_type,
                pages: Vec::new(),
            });
        }
    };

    let status = if pages.is_empty() { "ERROR" } else { "READY" }.to_string();

    Ok(BookMediaAnalysis {
        status,
        media_type,
        pages,
    })
}

fn fallback_media_analysis_pages(
    file_path: &PathBuf,
    media_type: &str,
) -> Vec<AnalyzedMediaPageRow> {
    let file_name = file_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    let file_size = fs::metadata(file_path)
        .ok()
        .map(|metadata| metadata.len())
        .and_then(|value| i64::try_from(value).ok())
        .unwrap_or_default();

    vec![AnalyzedMediaPageRow {
        file_name,
        media_type: media_type.to_string(),
        file_size,
    }]
}

fn analyze_zip_media_pages(
    file_path: &PathBuf,
    include_epub_resources: bool,
) -> Result<Vec<AnalyzedMediaPageRow>, String> {
    let file = fs::File::open(file_path)
        .map_err(|error| format!("open zip file '{}': {error}", file_path.display()))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| format!("open zip archive '{}': {error}", file_path.display()))?;

    let mut pages = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| format!("read zip entry at index {index}: {error}"))?;
        if entry.is_dir() {
            continue;
        }

        let file_name = entry.name().to_string();
        let include = if include_epub_resources {
            is_epub_page_resource_file_name(&file_name)
        } else {
            is_supported_page_image_file_name(&file_name)
        };
        if !include {
            continue;
        }

        let file_size = i64::try_from(entry.size()).unwrap_or(i64::MAX);
        pages.push(AnalyzedMediaPageRow {
            media_type: media_type_from_entry_name(&file_name),
            file_name,
            file_size,
        });
    }

    Ok(pages)
}

fn analyze_rar_media_pages(file_path: &PathBuf) -> Result<Vec<AnalyzedMediaPageRow>, String> {
    let output = Command::new("unrar")
        .arg("lb")
        .arg(file_path)
        .output()
        .map_err(|error| format!("run 'unrar lb' for '{}': {error}", file_path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "'unrar lb' failed for '{}': status {}",
            file_path.display(),
            output.status,
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && is_supported_page_image_file_name(line))
        .map(|file_name| AnalyzedMediaPageRow {
            file_name: file_name.to_string(),
            media_type: media_type_from_entry_name(file_name),
            file_size: 0,
        })
        .collect::<Vec<_>>())
}

fn analyze_pdf_media_pages(file_path: &PathBuf) -> Result<Vec<AnalyzedMediaPageRow>, String> {
    let document = lopdf::Document::load(file_path)
        .map_err(|error| format!("load pdf '{}': {error}", file_path.display()))?;
    let page_count = document.get_pages().len();
    Ok((0..page_count)
        .map(|index| AnalyzedMediaPageRow {
            file_name: format!("page-{index:04}.pdf"),
            media_type: "application/pdf".to_string(),
            file_size: 0,
        })
        .collect::<Vec<_>>())
}

fn is_supported_page_image_file_name(file_name: &str) -> bool {
    PathBuf::from(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "bmp"
            )
        })
}

fn is_epub_page_resource_file_name(file_name: &str) -> bool {
    PathBuf::from(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "xhtml" | "html" | "htm"
            )
        })
}

fn media_type_from_entry_name(file_name: &str) -> String {
    PathBuf::from(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .map(|extension| match extension.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "avif" => "image/avif",
            "bmp" => "image/bmp",
            "xhtml" | "html" | "htm" => "application/xhtml+xml",
            "pdf" => "application/pdf",
            _ => "application/octet-stream",
        })
        .unwrap_or("application/octet-stream")
        .to_string()
}

fn expected_extension_for_media_type(media_type: &str) -> Option<&'static str> {
    match media_type {
        "application/vnd.comicbook-rar" | "application/x-rar-compressed" => Some("cbr"),
        "application/zip" => Some("cbz"),
        "application/pdf" => Some("pdf"),
        "application/epub+zip" => Some("epub"),
        _ => None,
    }
}

fn is_rar_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        "application/vnd.comicbook-rar" | "application/x-rar-compressed"
    )
}

fn normalize_library_relative_url(
    library_root: &PathBuf,
    absolute_path: &PathBuf,
) -> Result<String, TaskExecutionError> {
    let relative = absolute_path.strip_prefix(library_root).map_err(|error| {
        TaskExecutionError::runtime(format!(
            "failed to derive relative path '{}' from library root '{}': {error}",
            absolute_path.display(),
            library_root.display(),
        ))
    })?;
    Ok(relative.to_string_lossy().replace('\\', "/"))
}

fn load_rar_entries_for_conversion(
    source_path: &PathBuf,
) -> Result<Vec<(String, Vec<u8>)>, TaskExecutionError> {
    let output = Command::new("unrar")
        .arg("lb")
        .arg(source_path)
        .output()
        .map_err(|error| {
            TaskExecutionError::runtime(format!(
                "failed to run 'unrar lb' for '{}': {error}",
                source_path.display(),
            ))
        })?;
    if !output.status.success() {
        return Err(TaskExecutionError::runtime(format!(
            "'unrar lb' failed for '{}': status {}",
            source_path.display(),
            output.status,
        )));
    }

    let mut entries = Vec::new();
    for entry_name in String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.ends_with('/'))
    {
        let entry_output = Command::new("unrar")
            .arg("p")
            .arg("-inul")
            .arg(source_path)
            .arg(entry_name)
            .output()
            .map_err(|error| {
                TaskExecutionError::runtime(format!(
                    "failed to run 'unrar p' for '{}' entry '{}': {error}",
                    source_path.display(),
                    entry_name,
                ))
            })?;
        if !entry_output.status.success() {
            return Err(TaskExecutionError::runtime(format!(
                "'unrar p' failed for '{}' entry '{}': status {}",
                source_path.display(),
                entry_name,
                entry_output.status,
            )));
        }
        entries.push((entry_name.to_string(), entry_output.stdout));
    }

    Ok(entries)
}

fn build_stored_zip_archive(
    entries: Vec<(String, Vec<u8>)>,
) -> Result<Vec<u8>, TaskExecutionError> {
    let mut payload = Vec::new();
    let mut central_directory = Vec::new();
    let mut entries_count: usize = 0;

    for (file_name, bytes) in entries {
        let file_name_bytes = file_name.as_bytes();
        let name_len = u16::try_from(file_name_bytes.len()).map_err(|_| {
            TaskExecutionError::runtime(format!("zip entry name too long: {file_name}"))
        })?;
        let size = u32::try_from(bytes.len()).map_err(|_| {
            TaskExecutionError::runtime(format!("zip entry too large: {file_name}"))
        })?;
        let local_header_offset = u32::try_from(payload.len()).map_err(|_| {
            TaskExecutionError::runtime("zip archive too large for legacy zip format")
        })?;
        let crc32 = crc32_ieee(&bytes);

        push_u32_le(&mut payload, 0x0403_4b50);
        push_u16_le(&mut payload, 20);
        push_u16_le(&mut payload, 0);
        push_u16_le(&mut payload, 0);
        push_u16_le(&mut payload, 0);
        push_u16_le(&mut payload, 0);
        push_u32_le(&mut payload, crc32);
        push_u32_le(&mut payload, size);
        push_u32_le(&mut payload, size);
        push_u16_le(&mut payload, name_len);
        push_u16_le(&mut payload, 0);
        payload.extend_from_slice(file_name_bytes);
        payload.extend_from_slice(&bytes);

        push_u32_le(&mut central_directory, 0x0201_4b50);
        push_u16_le(&mut central_directory, 20);
        push_u16_le(&mut central_directory, 20);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u32_le(&mut central_directory, crc32);
        push_u32_le(&mut central_directory, size);
        push_u32_le(&mut central_directory, size);
        push_u16_le(&mut central_directory, name_len);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u16_le(&mut central_directory, 0);
        push_u32_le(&mut central_directory, 0);
        push_u32_le(&mut central_directory, local_header_offset);
        central_directory.extend_from_slice(file_name_bytes);
        entries_count += 1;
    }

    let central_directory_offset = u32::try_from(payload.len())
        .map_err(|_| TaskExecutionError::runtime("zip archive too large for legacy zip format"))?;
    let central_directory_size = u32::try_from(central_directory.len()).map_err(|_| {
        TaskExecutionError::runtime("zip central directory too large for legacy zip format")
    })?;
    let entries_count = u16::try_from(entries_count)
        .map_err(|_| TaskExecutionError::runtime("too many zip entries for legacy zip format"))?;

    payload.extend_from_slice(&central_directory);
    push_u32_le(&mut payload, 0x0605_4b50);
    push_u16_le(&mut payload, 0);
    push_u16_le(&mut payload, 0);
    push_u16_le(&mut payload, entries_count);
    push_u16_le(&mut payload, entries_count);
    push_u32_le(&mut payload, central_directory_size);
    push_u32_le(&mut payload, central_directory_offset);
    push_u16_le(&mut payload, 0);

    Ok(payload)
}

fn crc32_ieee(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            let lsb = crc & 1;
            crc >>= 1;
            if lsb != 0 {
                crc ^= 0xedb8_8320;
            }
        }
    }
    !crc
}

fn push_u16_le(buffer: &mut Vec<u8>, value: u16) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

fn push_u32_le(buffer: &mut Vec<u8>, value: u32) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

fn to_unix_seconds(time: Option<std::time::SystemTime>) -> i64 {
    time.and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_secs() as i64)
        .unwrap_or_default()
}

fn metadata_updated_unix_seconds(metadata: &fs::Metadata) -> i64 {
    let created = metadata.created().ok();
    let modified = metadata.modified().ok();
    let updated = match (created, modified) {
        (Some(created), Some(modified)) => Some(created.max(modified)),
        (Some(created), None) => Some(created),
        (None, Some(modified)) => Some(modified),
        (None, None) => None,
    };
    to_unix_seconds(updated)
}

#[derive(Clone, Debug)]
struct PersistedTaskStore {
    tasks_db_file: PathBuf,
    pool: Arc<Mutex<SqlitePool>>,
}

impl PersistedTaskStore {
    fn new(tasks_db_file: PathBuf) -> Option<Self> {
        if !tasks_db_file.exists() {
            return None;
        }

        let pool = open_sqlite_pool_blocking(tasks_db_file.clone())?;

        Some(Self {
            tasks_db_file,
            pool: Arc::new(Mutex::new(pool)),
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

    fn load_admin(&self) -> TaskQueueAdmin {
        let records = self.run(|pool| {
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
                    .map(|row| TaskQueueRecord {
                        id: row.get::<String, _>("ID"),
                        priority: row.get::<i64, _>("PRIORITY") as i32,
                        group: row.get::<Option<String>, _>("GROUP_ID"),
                        simple_type: row.get::<String, _>("SIMPLE_TYPE"),
                        payload: row.get::<Option<String>, _>("PAYLOAD"),
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
                    "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) \
                     VALUES (?, ?, ?, ?, ?, ?, ?) \
                     ON CONFLICT(ID) DO UPDATE \
                     SET PRIORITY = excluded.PRIORITY, GROUP_ID = excluded.GROUP_ID, CLASS = excluded.CLASS, \
                         SIMPLE_TYPE = excluded.SIMPLE_TYPE, PAYLOAD = excluded.PAYLOAD, \
                         OWNER = excluded.OWNER, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP",
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

    fn delete_task(&self, task_id: &str) -> bool {
        let task_id = task_id.to_string();
        self.run(move |pool| {
            Box::pin(async move {
                sqlx::query(
                    "DELETE \
                             FROM TASK \
                             WHERE ID = ?",
                )
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

    fn disown_task(&self, task_id: &str) {
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

    fn clear_unowned(&self) -> usize {
        self.run(|pool| {
            Box::pin(async move {
                sqlx::query(
                    "DELETE \
                             FROM TASK \
                             WHERE OWNER IS NULL",
                )
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
    fn from_record(task: &TaskQueueRecord) -> Self {
        Self {
            id: task.id.clone(),
            priority: task.priority,
            group: task.group.clone(),
            class_name: kotlin_compat_class_name(&task.simple_type),
            simple_type: task.simple_type.clone(),
            payload: persisted_task_payload(task),
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

fn default_task_payload(task: &TaskQueueRecord) -> String {
    json!({
        "id": task.id,
        "simpleType": task.simple_type,
        "priority": task.priority,
        "groupId": task.group,
    })
    .to_string()
}

fn persisted_task_payload(task: &TaskQueueRecord) -> String {
    task.payload
        .clone()
        .unwrap_or_else(|| default_task_payload(task))
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
