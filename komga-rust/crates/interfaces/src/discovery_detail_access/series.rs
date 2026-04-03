use std::collections::HashMap;
use std::future::Future;
use std::path::Path as FsPath;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub struct DiscoveryDetailSeriesAccessBackend {
    pub load_persisted_series_resource: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<PersistedSeriesResourceRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_series_id_by_sorted_position:
        Arc<dyn Fn(PathBuf, usize) -> BoxFuture<Result<Option<String>, String>> + Send + Sync>,
    pub load_persisted_series_detail: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<PersistedSeriesDetailRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_persisted_series_summaries:
        Arc<dyn Fn(PathBuf) -> BoxFuture<Result<Vec<SeriesSummaryRecord>, String>> + Send + Sync>,
    pub load_series_total_book_counts:
        Arc<dyn Fn(PathBuf) -> BoxFuture<Result<HashMap<String, i64>, String>> + Send + Sync>,
    pub load_series_read_progress_counts: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<HashMap<String, (i64, i64)>, String>>
            + Send
            + Sync,
    >,
    pub load_persisted_series_collections: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Vec<PersistedCollectionRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_existing_series_metadata: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<ExistingSeriesMetadataRecord>, String>>
            + Send
            + Sync,
    >,
    pub persist_series_metadata_update: Arc<
        dyn Fn(PathBuf, String, SeriesMetadataUpdateRecord) -> BoxFuture<Result<bool, String>>
            + Send
            + Sync,
    >,
    pub refresh_series_search_documents_after_metadata_update:
        Arc<dyn Fn(PathBuf, PathBuf, String) -> BoxFuture<Result<(), String>> + Send + Sync>,
}

static BACKEND: OnceLock<DiscoveryDetailSeriesAccessBackend> = OnceLock::new();

pub(crate) fn install_backend(backend: DiscoveryDetailSeriesAccessBackend) {
    let _ = BACKEND.set(backend);
}

fn backend() -> &'static DiscoveryDetailSeriesAccessBackend {
    BACKEND.get_or_init(|| DiscoveryDetailSeriesAccessBackend {
        load_persisted_series_resource: Arc::new(|_, _| {
            Box::pin(async { Err("discovery detail series backend is not configured".to_string()) })
        }),
        load_series_id_by_sorted_position: Arc::new(|_, _| {
            Box::pin(async { Err("discovery detail series backend is not configured".to_string()) })
        }),
        load_persisted_series_detail: Arc::new(|_, _| {
            Box::pin(async { Err("discovery detail series backend is not configured".to_string()) })
        }),
        load_persisted_series_summaries: Arc::new(|_| {
            Box::pin(async { Err("discovery detail series backend is not configured".to_string()) })
        }),
        load_series_total_book_counts: Arc::new(|_| {
            Box::pin(async { Err("discovery detail series backend is not configured".to_string()) })
        }),
        load_series_read_progress_counts: Arc::new(|_, _| {
            Box::pin(async { Err("discovery detail series backend is not configured".to_string()) })
        }),
        load_persisted_series_collections: Arc::new(|_, _| {
            Box::pin(async { Err("discovery detail series backend is not configured".to_string()) })
        }),
        load_existing_series_metadata: Arc::new(|_, _| {
            Box::pin(async { Err("discovery detail series backend is not configured".to_string()) })
        }),
        persist_series_metadata_update: Arc::new(|_, _, _| {
            Box::pin(async { Err("discovery detail series backend is not configured".to_string()) })
        }),
        refresh_series_search_documents_after_metadata_update: Arc::new(|_, _, _| {
            Box::pin(async { Err("discovery detail series backend is not configured".to_string()) })
        }),
    })
}

#[derive(Clone)]
pub struct PersistedSeriesResourceRecord {
    pub library_id: String,
    pub age_rating: Option<u32>,
    pub sharing_labels: String,
}

#[derive(Clone)]
pub struct PersistedSeriesDetailRecord {
    pub id: String,
    pub library_id: String,
    pub name: String,
    pub title: String,
    pub title_sort: String,
    pub url: String,
    pub created: String,
    pub last_modified: String,
    pub file_last_modified: String,
    pub books_count: u32,
    pub status: String,
    pub summary: String,
    pub reading_direction: String,
    pub publisher: String,
    pub age_rating: Option<u32>,
    pub language: String,
    pub sharing_labels: String,
    pub metadata_created: String,
    pub metadata_last_modified: String,
    pub deleted: bool,
    pub oneshot: bool,
}

#[derive(Clone)]
pub struct PersistedCollectionRecord {
    pub id: String,
    pub name: String,
    pub ordered: bool,
    pub series_ids: Vec<String>,
    pub created_date: String,
    pub last_modified_date: String,
}

pub struct ExistingSeriesMetadataRecord {
    pub status: String,
    pub status_lock: bool,
    pub title: String,
    pub title_lock: bool,
    pub title_sort: String,
    pub title_sort_lock: bool,
    pub summary: String,
    pub summary_lock: bool,
    pub reading_direction: Option<String>,
    pub reading_direction_lock: bool,
    pub publisher: String,
    pub publisher_lock: bool,
    pub age_rating: Option<u32>,
    pub age_rating_lock: bool,
    pub language: String,
    pub language_lock: bool,
    pub genres: Vec<String>,
    pub genres_lock: bool,
    pub tags: Vec<String>,
    pub tags_lock: bool,
    pub total_book_count: Option<u32>,
    pub total_book_count_lock: bool,
    pub sharing_labels: Vec<String>,
    pub sharing_labels_lock: bool,
    pub links: Vec<SeriesMetadataLinkRecord>,
    pub links_lock: bool,
    pub alternate_titles: Vec<SeriesAlternateTitleRecord>,
    pub alternate_titles_lock: bool,
}

#[derive(Clone)]
pub struct SeriesMetadataLinkRecord {
    pub label: String,
    pub url: String,
}

#[derive(Clone)]
pub struct SeriesAlternateTitleRecord {
    pub label: String,
    pub title: String,
}

#[derive(Clone)]
pub struct SeriesMetadataUpdateRecord {
    pub status: String,
    pub status_lock: bool,
    pub title: String,
    pub title_lock: bool,
    pub title_sort: String,
    pub title_sort_lock: bool,
    pub summary: String,
    pub summary_lock: bool,
    pub reading_direction: Option<String>,
    pub reading_direction_lock: bool,
    pub publisher: String,
    pub publisher_lock: bool,
    pub age_rating: Option<u32>,
    pub age_rating_lock: bool,
    pub language: String,
    pub language_lock: bool,
    pub genres: Vec<String>,
    pub genres_lock: bool,
    pub tags: Vec<String>,
    pub tags_lock: bool,
    pub total_book_count: Option<u32>,
    pub total_book_count_lock: bool,
    pub sharing_labels: Vec<String>,
    pub sharing_labels_lock: bool,
    pub links: Vec<SeriesMetadataLinkRecord>,
    pub links_lock: bool,
    pub alternate_titles: Vec<SeriesAlternateTitleRecord>,
    pub alternate_titles_lock: bool,
}

#[derive(Clone)]
pub struct SeriesSummaryRecord {
    pub id: String,
    pub genres: Vec<String>,
    pub tags: Vec<String>,
    pub alternate_titles: Vec<String>,
    pub books_metadata_authors: Vec<String>,
    pub books_metadata_tags: Vec<String>,
    pub books_metadata_release_date: Option<String>,
    pub books_metadata_summary: String,
    pub books_metadata_summary_number: String,
    pub books_metadata_created: String,
    pub books_metadata_last_modified: String,
}

pub async fn load_persisted_series_resource(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<PersistedSeriesResourceRecord>, String> {
    (backend().load_persisted_series_resource)(database_file.to_path_buf(), series_id.to_string())
        .await
}

pub async fn load_series_id_by_sorted_position(
    database_file: &FsPath,
    index: usize,
) -> Result<Option<String>, String> {
    (backend().load_series_id_by_sorted_position)(database_file.to_path_buf(), index).await
}

pub async fn load_persisted_series_detail(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<PersistedSeriesDetailRecord>, String> {
    (backend().load_persisted_series_detail)(database_file.to_path_buf(), series_id.to_string())
        .await
}

pub async fn load_persisted_series_summaries(
    database_file: &FsPath,
) -> Result<Vec<SeriesSummaryRecord>, String> {
    (backend().load_persisted_series_summaries)(database_file.to_path_buf()).await
}

pub async fn load_series_total_book_counts(
    database_file: &FsPath,
) -> Result<HashMap<String, i64>, String> {
    (backend().load_series_total_book_counts)(database_file.to_path_buf()).await
}

pub async fn load_series_read_progress_counts(
    database_file: &FsPath,
    user_id: &str,
) -> Result<HashMap<String, (i64, i64)>, String> {
    (backend().load_series_read_progress_counts)(database_file.to_path_buf(), user_id.to_string())
        .await
}

pub async fn load_persisted_series_collections(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Vec<PersistedCollectionRecord>, String> {
    (backend().load_persisted_series_collections)(
        database_file.to_path_buf(),
        series_id.to_string(),
    )
    .await
}

pub async fn load_existing_series_metadata(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<ExistingSeriesMetadataRecord>, String> {
    (backend().load_existing_series_metadata)(database_file.to_path_buf(), series_id.to_string())
        .await
}

pub async fn persist_series_metadata_update(
    database_file: &FsPath,
    series_id: &str,
    update: SeriesMetadataUpdateRecord,
) -> Result<bool, String> {
    (backend().persist_series_metadata_update)(
        database_file.to_path_buf(),
        series_id.to_string(),
        update,
    )
    .await
}

pub async fn refresh_series_search_documents_after_metadata_update(
    database_file: &FsPath,
    index_dir: &FsPath,
    series_id: &str,
) -> Result<(), String> {
    (backend().refresh_series_search_documents_after_metadata_update)(
        database_file.to_path_buf(),
        index_dir.to_path_buf(),
        series_id.to_string(),
    )
    .await
}
