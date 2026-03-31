#![allow(clippy::type_complexity)]

use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

#[derive(Clone)]
pub struct PersistedLibraryRecord {
    pub id: String,
    pub name: String,
    pub last_modified: String,
}

pub struct PersistedSeriesRecord {
    pub id: String,
    pub library_id: String,
    pub title: String,
    pub last_modified: String,
}

pub struct PersistedSeriesBookRecord {
    pub id: String,
    pub title: String,
    pub file_name: String,
    pub media_type: String,
    pub last_modified: String,
}

pub struct PersistedReadlistRecord {
    pub id: String,
    pub name: String,
    pub last_modified: String,
}

pub struct PersistedReadlistBookRecord {
    pub id: String,
    pub title: String,
    pub file_name: String,
    pub media_type: String,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
}

pub struct PersistedSeriesSearchRecord {
    pub id: String,
    pub title: String,
    pub library_id: String,
}

pub struct PersistedBookSearchRecord {
    pub id: String,
    pub title: String,
    pub library_id: String,
}

pub struct PersistedNamedRecord {
    pub id: String,
    pub name: String,
}

pub struct PersistedBookFeedRecord {
    pub id: String,
    pub title: String,
    pub file_name: String,
    pub media_type: String,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
}

pub struct OpdsPersistedAccessBackend {
    pub load_libraries: Arc<
        dyn Fn(PathBuf) -> BoxFuture<Result<Vec<PersistedLibraryRecord>, String>> + Send + Sync,
    >,
    pub load_library: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<PersistedLibraryRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_readlists_for_library: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Vec<PersistedReadlistRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_series: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<PersistedSeriesRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_series_books_paged: Arc<
        dyn Fn(
                PathBuf,
                String,
                i64,
                i64,
            ) -> BoxFuture<Result<Vec<PersistedSeriesBookRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_readlist: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<PersistedReadlistRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_readlist_books: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Vec<PersistedReadlistBookRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_unified_search_results: Arc<
        dyn Fn(
                PathBuf,
                String,
            ) -> BoxFuture<
                Result<
                    (
                        Vec<PersistedSeriesSearchRecord>,
                        Vec<PersistedBookSearchRecord>,
                        Vec<PersistedNamedRecord>,
                        Vec<PersistedNamedRecord>,
                    ),
                    String,
                >,
            > + Send
            + Sync,
    >,
    pub load_publishers: Arc<
        dyn Fn(PathBuf, Option<HashSet<String>>) -> BoxFuture<Result<Vec<String>, String>>
            + Send
            + Sync,
    >,
    pub load_collections: Arc<
        dyn Fn(PathBuf, Option<String>) -> BoxFuture<Result<Vec<PersistedNamedRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_collection: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<PersistedNamedRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_collection_books: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Vec<PersistedBookFeedRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_collection_series: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Vec<PersistedSeriesRecord>, String>>
            + Send
            + Sync,
    >,
}

static BACKEND: OnceLock<OpdsPersistedAccessBackend> = OnceLock::new();

pub fn install_opds_persisted_access(backend: OpdsPersistedAccessBackend) {
    let _ = BACKEND.set(backend);
}

fn backend() -> &'static OpdsPersistedAccessBackend {
    BACKEND
        .get()
        .expect("opds persisted access backend should be installed before use")
}

pub async fn load_libraries(database_file: &Path) -> Result<Vec<PersistedLibraryRecord>, String> {
    (backend().load_libraries)(database_file.to_path_buf()).await
}

pub async fn load_library(
    database_file: &Path,
    library_id: &str,
) -> Result<Option<PersistedLibraryRecord>, String> {
    (backend().load_library)(database_file.to_path_buf(), library_id.to_string()).await
}

pub async fn load_readlists_for_library(
    database_file: &Path,
    library_id: &str,
) -> Result<Vec<PersistedReadlistRecord>, String> {
    (backend().load_readlists_for_library)(database_file.to_path_buf(), library_id.to_string())
        .await
}

pub async fn load_series(
    database_file: &Path,
    series_id: &str,
) -> Result<Option<PersistedSeriesRecord>, String> {
    (backend().load_series)(database_file.to_path_buf(), series_id.to_string()).await
}

pub async fn load_series_books_paged(
    database_file: &Path,
    series_id: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<PersistedSeriesBookRecord>, String> {
    (backend().load_series_books_paged)(
        database_file.to_path_buf(),
        series_id.to_string(),
        offset,
        limit,
    )
    .await
}

pub async fn load_readlist(
    database_file: &Path,
    readlist_id: &str,
) -> Result<Option<PersistedReadlistRecord>, String> {
    (backend().load_readlist)(database_file.to_path_buf(), readlist_id.to_string()).await
}

pub async fn load_readlist_books(
    database_file: &Path,
    readlist_id: &str,
) -> Result<Vec<PersistedReadlistBookRecord>, String> {
    (backend().load_readlist_books)(database_file.to_path_buf(), readlist_id.to_string()).await
}

pub async fn load_unified_search_results(
    database_file: &Path,
    query: &str,
) -> Result<
    (
        Vec<PersistedSeriesSearchRecord>,
        Vec<PersistedBookSearchRecord>,
        Vec<PersistedNamedRecord>,
        Vec<PersistedNamedRecord>,
    ),
    String,
> {
    (backend().load_unified_search_results)(database_file.to_path_buf(), query.to_string()).await
}

pub async fn load_publishers(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
) -> Result<Vec<String>, String> {
    (backend().load_publishers)(database_file.to_path_buf(), allowed_library_ids.clone()).await
}

pub async fn load_collections(
    database_file: &Path,
    library_id: Option<&str>,
) -> Result<Vec<PersistedNamedRecord>, String> {
    (backend().load_collections)(database_file.to_path_buf(), library_id.map(str::to_string)).await
}

pub async fn load_collection(
    database_file: &Path,
    collection_id: &str,
) -> Result<Option<PersistedNamedRecord>, String> {
    (backend().load_collection)(database_file.to_path_buf(), collection_id.to_string()).await
}

pub async fn load_collection_books(
    database_file: &Path,
    collection_id: &str,
) -> Result<Vec<PersistedBookFeedRecord>, String> {
    (backend().load_collection_books)(database_file.to_path_buf(), collection_id.to_string()).await
}

pub async fn load_collection_series(
    database_file: &Path,
    collection_id: &str,
) -> Result<Vec<PersistedSeriesRecord>, String> {
    (backend().load_collection_series)(database_file.to_path_buf(), collection_id.to_string()).await
}
