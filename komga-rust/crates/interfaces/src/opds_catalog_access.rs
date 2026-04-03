#![allow(clippy::type_complexity)]

use std::collections::HashSet;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

pub struct BrowseSeriesNavigationEntry {
    pub id: String,
    pub title: String,
}

pub struct BrowsePublisherEntry {
    pub publisher: String,
}

pub struct OpdsBookFeedEntry {
    pub id: String,
    pub title: String,
    pub file_name: String,
    pub media_type: String,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
}

pub struct OpdsSeriesEntry {
    pub id: String,
    pub library_id: String,
    pub title: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
}

pub struct OpdsReadlistEntry {
    pub id: String,
    pub name: String,
    pub last_modified: String,
}

pub struct OpdsCatalogAccessBackend {
    pub load_browse_series_navigation_entries: Arc<
        dyn Fn(
                PathBuf,
                Option<HashSet<String>>,
                Option<String>,
                Vec<String>,
                usize,
                usize,
            ) -> BoxFuture<Result<(Vec<BrowseSeriesNavigationEntry>, usize), String>>
            + Send
            + Sync,
    >,
    pub load_browse_publisher_entries: Arc<
        dyn Fn(
                PathBuf,
                Option<HashSet<String>>,
                Option<String>,
            ) -> BoxFuture<Result<Vec<BrowsePublisherEntry>, String>>
            + Send
            + Sync,
    >,
    pub load_keep_reading_books: Arc<
        dyn Fn(PathBuf, String, Option<String>) -> BoxFuture<Result<Vec<OpdsBookFeedEntry>, String>>
            + Send
            + Sync,
    >,
    pub load_on_deck_books: Arc<
        dyn Fn(PathBuf, String, Option<String>) -> BoxFuture<Result<Vec<OpdsBookFeedEntry>, String>>
            + Send
            + Sync,
    >,
    pub load_latest_books: Arc<
        dyn Fn(PathBuf, Option<String>, i64) -> BoxFuture<Result<Vec<OpdsBookFeedEntry>, String>>
            + Send
            + Sync,
    >,
    pub load_latest_books_paged: Arc<
        dyn Fn(
                PathBuf,
                Option<HashSet<String>>,
                Option<String>,
                i64,
                i64,
            ) -> BoxFuture<Result<Vec<OpdsBookFeedEntry>, String>>
            + Send
            + Sync,
    >,
    pub load_latest_series: Arc<
        dyn Fn(PathBuf, Option<String>, i64) -> BoxFuture<Result<Vec<OpdsSeriesEntry>, String>>
            + Send
            + Sync,
    >,
    pub load_latest_series_paged: Arc<
        dyn Fn(
                PathBuf,
                Option<HashSet<String>>,
                Option<String>,
                i64,
                i64,
            ) -> BoxFuture<Result<Vec<OpdsSeriesEntry>, String>>
            + Send
            + Sync,
    >,
    pub load_library_series: Arc<
        dyn Fn(PathBuf, String, i64, i64) -> BoxFuture<Result<Vec<OpdsSeriesEntry>, String>>
            + Send
            + Sync,
    >,
    pub load_series_page: Arc<
        dyn Fn(
                PathBuf,
                Option<HashSet<String>>,
                Option<String>,
                Vec<String>,
                i64,
                i64,
            ) -> BoxFuture<Result<Vec<OpdsSeriesEntry>, String>>
            + Send
            + Sync,
    >,
    pub load_all_readlists:
        Arc<dyn Fn(PathBuf) -> BoxFuture<Result<Vec<OpdsReadlistEntry>, String>> + Send + Sync>,
}

static BACKEND: OnceLock<OpdsCatalogAccessBackend> = OnceLock::new();

pub fn install_opds_catalog_access(backend: OpdsCatalogAccessBackend) {
    let _ = BACKEND.set(backend);
}

fn backend() -> &'static OpdsCatalogAccessBackend {
    BACKEND
        .get()
        .expect("opds catalog access backend should be installed before use")
}

pub(crate) async fn load_browse_series_navigation_entries(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
    publishers: &[String],
    page: usize,
    size: usize,
) -> Result<(Vec<BrowseSeriesNavigationEntry>, usize), String> {
    (backend().load_browse_series_navigation_entries)(
        database_file.to_path_buf(),
        allowed_library_ids.clone(),
        library_id.map(str::to_string),
        publishers.to_vec(),
        page,
        size,
    )
    .await
}

pub(crate) async fn load_browse_publisher_entries(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
) -> Result<Vec<BrowsePublisherEntry>, String> {
    (backend().load_browse_publisher_entries)(
        database_file.to_path_buf(),
        allowed_library_ids.clone(),
        library_id.map(str::to_string),
    )
    .await
}

pub(crate) async fn load_keep_reading_books(
    database_file: &Path,
    user_id: &str,
    library_id: Option<&str>,
) -> Result<Vec<OpdsBookFeedEntry>, String> {
    (backend().load_keep_reading_books)(
        database_file.to_path_buf(),
        user_id.to_string(),
        library_id.map(str::to_string),
    )
    .await
}

pub(crate) async fn load_on_deck_books(
    database_file: &Path,
    user_id: &str,
    library_id: Option<&str>,
) -> Result<Vec<OpdsBookFeedEntry>, String> {
    (backend().load_on_deck_books)(
        database_file.to_path_buf(),
        user_id.to_string(),
        library_id.map(str::to_string),
    )
    .await
}

pub(crate) async fn load_latest_books(
    database_file: &Path,
    library_id: Option<&str>,
    limit: i64,
) -> Result<Vec<OpdsBookFeedEntry>, String> {
    (backend().load_latest_books)(
        database_file.to_path_buf(),
        library_id.map(str::to_string),
        limit,
    )
    .await
}

pub(crate) async fn load_latest_books_paged(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<OpdsBookFeedEntry>, String> {
    (backend().load_latest_books_paged)(
        database_file.to_path_buf(),
        allowed_library_ids.clone(),
        library_id.map(str::to_string),
        offset,
        limit,
    )
    .await
}

pub(crate) async fn load_latest_series(
    database_file: &Path,
    library_id: Option<&str>,
    limit: i64,
) -> Result<Vec<OpdsSeriesEntry>, String> {
    (backend().load_latest_series)(
        database_file.to_path_buf(),
        library_id.map(str::to_string),
        limit,
    )
    .await
}

pub(crate) async fn load_latest_series_paged(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<OpdsSeriesEntry>, String> {
    (backend().load_latest_series_paged)(
        database_file.to_path_buf(),
        allowed_library_ids.clone(),
        library_id.map(str::to_string),
        offset,
        limit,
    )
    .await
}

pub(crate) async fn load_library_series(
    database_file: &Path,
    library_id: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<OpdsSeriesEntry>, String> {
    (backend().load_library_series)(
        database_file.to_path_buf(),
        library_id.to_string(),
        offset,
        limit,
    )
    .await
}

pub(crate) async fn load_series_page(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    search: Option<&str>,
    publishers: &[String],
    offset: i64,
    limit: i64,
) -> Result<Vec<OpdsSeriesEntry>, String> {
    (backend().load_series_page)(
        database_file.to_path_buf(),
        allowed_library_ids.clone(),
        search.map(str::to_string),
        publishers.to_vec(),
        offset,
        limit,
    )
    .await
}

pub(crate) async fn load_all_readlists(
    database_file: &Path,
) -> Result<Vec<OpdsReadlistEntry>, String> {
    (backend().load_all_readlists)(database_file.to_path_buf()).await
}
