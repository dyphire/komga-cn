#![allow(clippy::type_complexity)]

use std::collections::HashMap;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

#[derive(Clone)]
pub struct ServerSettingsStore {
    load_map_fn:
        Arc<dyn Fn() -> BoxFuture<Result<HashMap<String, Option<String>>, String>> + Send + Sync>,
    apply_changes_fn:
        Arc<dyn Fn(Vec<(String, Option<String>)>) -> BoxFuture<Result<(), String>> + Send + Sync>,
}

impl ServerSettingsStore {
    pub fn new(
        load_map_fn: Arc<
            dyn Fn() -> BoxFuture<Result<HashMap<String, Option<String>>, String>> + Send + Sync,
        >,
        apply_changes_fn: Arc<
            dyn Fn(Vec<(String, Option<String>)>) -> BoxFuture<Result<(), String>> + Send + Sync,
        >,
    ) -> Self {
        Self {
            load_map_fn,
            apply_changes_fn,
        }
    }

    pub async fn load_map(&self) -> Result<HashMap<String, Option<String>>, String> {
        (self.load_map_fn)().await
    }

    pub async fn apply_changes(&self, changes: &[(String, Option<String>)]) -> Result<(), String> {
        (self.apply_changes_fn)(changes.to_vec()).await
    }
}

#[derive(Clone, Default)]
pub struct SseSnapshot {
    pub libraries: HashMap<String, LibrarySnapshot>,
    pub series: HashMap<String, SeriesSnapshot>,
    pub books: HashMap<String, BookSnapshot>,
    pub readlists: HashMap<String, ReadListSnapshot>,
    pub collections: HashMap<String, CollectionSnapshot>,
    pub thumbnails_book: HashMap<String, ThumbnailBookSnapshot>,
    pub thumbnails_series: HashMap<String, ThumbnailSnapshot>,
    pub thumbnails_collection: HashMap<String, ThumbnailCollectionSnapshot>,
    pub thumbnails_readlist: HashMap<String, ThumbnailReadListSnapshot>,
    pub read_progress: HashMap<String, String>,
    pub read_progress_series: HashMap<String, String>,
}

#[derive(Clone, Eq, PartialEq)]
pub struct LibrarySnapshot {
    pub last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct SeriesSnapshot {
    pub library_id: String,
    pub last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct BookSnapshot {
    pub series_id: String,
    pub library_id: String,
    pub last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ReadListSnapshot {
    pub book_ids: Vec<String>,
    pub last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct CollectionSnapshot {
    pub series_ids: Vec<String>,
    pub last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ThumbnailBookSnapshot {
    pub book_id: String,
    pub series_id: String,
    pub selected: bool,
    pub last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ThumbnailSnapshot {
    pub selected: bool,
    pub last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ThumbnailReadListSnapshot {
    pub readlist_id: String,
    pub selected: bool,
    pub last_modified: String,
}

#[derive(Clone, Eq, PartialEq)]
pub struct ThumbnailCollectionSnapshot {
    pub collection_id: String,
    pub selected: bool,
    pub last_modified: String,
}

#[derive(Clone)]
pub struct OperationalRuntimeAccessBackend {
    pub load_task_execution_values:
        Arc<dyn Fn(PathBuf) -> BoxFuture<Result<Vec<(String, f64)>, String>> + Send + Sync>,
    pub load_libraries_count: Arc<dyn Fn(PathBuf) -> BoxFuture<Result<f64, String>> + Send + Sync>,
    pub load_series_grouped_by_library:
        Arc<dyn Fn(PathBuf) -> BoxFuture<Result<Vec<(String, f64)>, String>> + Send + Sync>,
    pub load_books_grouped_by_library:
        Arc<dyn Fn(PathBuf) -> BoxFuture<Result<Vec<(String, f64)>, String>> + Send + Sync>,
    pub load_books_filesize_grouped_by_library:
        Arc<dyn Fn(PathBuf) -> BoxFuture<Result<Vec<(String, f64)>, String>> + Send + Sync>,
    pub load_sidecars_grouped_by_library:
        Arc<dyn Fn(PathBuf) -> BoxFuture<Result<Vec<(String, f64)>, String>> + Send + Sync>,
    pub load_collections_count:
        Arc<dyn Fn(PathBuf) -> BoxFuture<Result<f64, String>> + Send + Sync>,
    pub load_readlists_count: Arc<dyn Fn(PathBuf) -> BoxFuture<Result<f64, String>> + Send + Sync>,
    pub load_task_failure_count:
        Arc<dyn Fn(PathBuf) -> BoxFuture<Result<f64, String>> + Send + Sync>,
    pub load_sse_snapshot: Arc<dyn Fn(PathBuf, String) -> BoxFuture<SseSnapshot> + Send + Sync>,
}

static BACKEND: OnceLock<OperationalRuntimeAccessBackend> = OnceLock::new();

pub fn install_operational_runtime_access(backend: OperationalRuntimeAccessBackend) {
    let _ = BACKEND.set(backend);
}

fn backend() -> &'static OperationalRuntimeAccessBackend {
    BACKEND
        .get()
        .expect("operational runtime access backend should be installed before use")
}

pub(crate) mod metrics {
    use super::*;

    pub(crate) async fn load_task_execution_values(
        tasks_db_file: &Path,
    ) -> Result<Vec<(String, f64)>, String> {
        (backend().load_task_execution_values)(tasks_db_file.to_path_buf()).await
    }

    pub(crate) async fn load_libraries_count(database_file: &Path) -> Result<f64, String> {
        (backend().load_libraries_count)(database_file.to_path_buf()).await
    }

    pub(crate) async fn load_series_grouped_by_library(
        database_file: &Path,
    ) -> Result<Vec<(String, f64)>, String> {
        (backend().load_series_grouped_by_library)(database_file.to_path_buf()).await
    }

    pub(crate) async fn load_books_grouped_by_library(
        database_file: &Path,
    ) -> Result<Vec<(String, f64)>, String> {
        (backend().load_books_grouped_by_library)(database_file.to_path_buf()).await
    }

    pub(crate) async fn load_books_filesize_grouped_by_library(
        database_file: &Path,
    ) -> Result<Vec<(String, f64)>, String> {
        (backend().load_books_filesize_grouped_by_library)(database_file.to_path_buf()).await
    }

    pub(crate) async fn load_sidecars_grouped_by_library(
        database_file: &Path,
    ) -> Result<Vec<(String, f64)>, String> {
        (backend().load_sidecars_grouped_by_library)(database_file.to_path_buf()).await
    }

    pub(crate) async fn load_collections_count(database_file: &Path) -> Result<f64, String> {
        (backend().load_collections_count)(database_file.to_path_buf()).await
    }

    pub(crate) async fn load_readlists_count(database_file: &Path) -> Result<f64, String> {
        (backend().load_readlists_count)(database_file.to_path_buf()).await
    }

    pub(crate) async fn load_task_failure_count(database_file: &Path) -> Result<f64, String> {
        (backend().load_task_failure_count)(database_file.to_path_buf()).await
    }
}

pub(crate) mod sse_snapshot {
    pub(crate) use super::{
        BookSnapshot, CollectionSnapshot, LibrarySnapshot, ReadListSnapshot, SeriesSnapshot,
        SseSnapshot, ThumbnailBookSnapshot, ThumbnailCollectionSnapshot, ThumbnailReadListSnapshot,
        ThumbnailSnapshot,
    };

    pub(crate) async fn load_sse_snapshot(database_file: &Path, user_id: &str) -> SseSnapshot {
        (super::backend().load_sse_snapshot)(database_file.to_path_buf(), user_id.to_string()).await
    }

    use std::path::Path;
}
