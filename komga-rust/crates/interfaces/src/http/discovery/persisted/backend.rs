use super::*;

type BoxFutureResult<T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send>>;

#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub struct PersistedDiscoveryAccessBackend {
    pub load_persisted_author_names:
        Arc<dyn Fn(PathBuf, String) -> BoxFutureResult<Vec<String>> + Send + Sync>,
    pub load_persisted_author_roles:
        Arc<dyn Fn(PathBuf) -> BoxFutureResult<Vec<String>> + Send + Sync>,
    pub load_persisted_authors_by_scope: Arc<
        dyn Fn(PathBuf, PersistedAuthorsScope) -> BoxFutureResult<Vec<PersistedAuthorEntry>>
            + Send
            + Sync,
    >,
    pub load_book_poster_summaries: Arc<
        dyn Fn(PathBuf) -> BoxFutureResult<HashMap<String, Vec<PersistedBookPosterSummary>>>
            + Send
            + Sync,
    >,
    pub load_persisted_book_summaries: Arc<
        dyn Fn(PathBuf, Option<String>) -> BoxFutureResult<Vec<PersistedBookSummary>> + Send + Sync,
    >,
    pub load_persisted_book_summaries_by_ids: Arc<
        dyn Fn(PathBuf, Option<String>, Vec<String>) -> BoxFutureResult<Vec<PersistedBookSummary>>
            + Send
            + Sync,
    >,
    pub load_persisted_book_count: Arc<dyn Fn(PathBuf) -> BoxFutureResult<usize> + Send + Sync>,
    pub persisted_books_exist: Arc<dyn Fn(PathBuf) -> BoxFutureResult<bool> + Send + Sync>,
    pub load_persisted_genres: Arc<
        dyn Fn(PathBuf, Option<Vec<String>>, Option<String>) -> BoxFutureResult<Vec<String>>
            + Send
            + Sync,
    >,
    pub load_persisted_tags: Arc<
        dyn Fn(PathBuf, Option<Vec<String>>, Option<String>) -> BoxFutureResult<Vec<String>>
            + Send
            + Sync,
    >,
    pub load_persisted_languages: Arc<
        dyn Fn(PathBuf, Option<Vec<String>>, Option<String>) -> BoxFutureResult<Vec<String>>
            + Send
            + Sync,
    >,
    pub load_persisted_publishers: Arc<
        dyn Fn(PathBuf, Option<Vec<String>>, Option<String>) -> BoxFutureResult<Vec<String>>
            + Send
            + Sync,
    >,
    pub load_persisted_age_ratings: Arc<
        dyn Fn(PathBuf, Option<Vec<String>>, Option<String>) -> BoxFutureResult<Vec<u16>>
            + Send
            + Sync,
    >,
    pub load_persisted_sharing_labels: Arc<
        dyn Fn(PathBuf, Option<Vec<String>>, Option<String>) -> BoxFutureResult<Vec<String>>
            + Send
            + Sync,
    >,
    pub load_persisted_series_release_dates: Arc<
        dyn Fn(PathBuf, Option<Vec<String>>, Option<String>) -> BoxFutureResult<Vec<String>>
            + Send
            + Sync,
    >,
    pub load_persisted_series_tags: Arc<
        dyn Fn(PathBuf, Option<Vec<String>>, Option<String>) -> BoxFutureResult<Vec<String>>
            + Send
            + Sync,
    >,
    pub load_persisted_library_ids:
        Arc<dyn Fn(PathBuf) -> BoxFutureResult<Vec<String>> + Send + Sync>,
    pub load_collection_memberships:
        Arc<dyn Fn(PathBuf) -> BoxFutureResult<BTreeMap<String, BTreeSet<String>>> + Send + Sync>,
    pub load_readlist_memberships:
        Arc<dyn Fn(PathBuf) -> BoxFutureResult<BTreeMap<String, BTreeSet<String>>> + Send + Sync>,
    pub load_persisted_ondeck_books: Arc<
        dyn Fn(PathBuf, String) -> BoxFutureResult<Vec<PersistedBookBrowseEntry>> + Send + Sync,
    >,
    pub load_persisted_duplicate_books:
        Arc<dyn Fn(PathBuf) -> BoxFutureResult<Vec<PersistedBookBrowseEntry>> + Send + Sync>,
    pub load_persisted_book_tags: Arc<
        dyn Fn(PathBuf, Option<PersistedBookTagsScope>) -> BoxFutureResult<Vec<String>>
            + Send
            + Sync,
    >,
    pub persisted_utc_date_minus_days:
        Arc<dyn Fn(PathBuf, i64) -> BoxFutureResult<Option<String>> + Send + Sync>,
    pub load_series_read_progress_counts:
        Arc<dyn Fn(PathBuf, String) -> BoxFutureResult<HashMap<String, (i64, i64)>> + Send + Sync>,
    pub load_series_total_book_counts:
        Arc<dyn Fn(PathBuf) -> BoxFutureResult<HashMap<String, i64>> + Send + Sync>,
    pub load_persisted_series_summaries:
        Arc<dyn Fn(PathBuf) -> BoxFutureResult<Vec<PersistedSeriesSummary>> + Send + Sync>,
    pub load_persisted_series_summaries_by_ids: Arc<
        dyn Fn(PathBuf, Vec<String>) -> BoxFutureResult<Vec<PersistedSeriesSummary>> + Send + Sync,
    >,
    pub load_persisted_series_count: Arc<dyn Fn(PathBuf) -> BoxFutureResult<usize> + Send + Sync>,
    pub persisted_series_exist: Arc<dyn Fn(PathBuf) -> BoxFutureResult<bool> + Send + Sync>,
    pub search_book_ids:
        Arc<dyn Fn(PathBuf, String, usize) -> BoxFutureResult<Vec<String>> + Send + Sync>,
    pub search_series_ids:
        Arc<dyn Fn(PathBuf, String, usize) -> BoxFutureResult<Vec<String>> + Send + Sync>,
}

static PERSISTED_DISCOVERY_ACCESS_BACKEND: OnceLock<PersistedDiscoveryAccessBackend> =
    OnceLock::new();

pub fn install_persisted_discovery_access(backend: PersistedDiscoveryAccessBackend) {
    let _ = PERSISTED_DISCOVERY_ACCESS_BACKEND.set(backend);
}

fn persisted_discovery_backend() -> Result<&'static PersistedDiscoveryAccessBackend, String> {
    PERSISTED_DISCOVERY_ACCESS_BACKEND
        .get()
        .ok_or_else(|| "persisted discovery backend is not installed".to_string())
}

pub(super) async fn persisted_backend_load_persisted_author_names(
    database_file: &FsPath,
    search: &str,
) -> Result<Vec<String>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_author_names)(database_file.to_path_buf(), search.to_string()).await
}

pub(super) async fn persisted_backend_load_persisted_author_roles(
    database_file: &FsPath,
) -> Result<Vec<String>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_author_roles)(database_file.to_path_buf()).await
}

pub(super) async fn persisted_backend_load_persisted_authors_by_scope(
    database_file: &FsPath,
    scope: &PersistedAuthorsScope,
) -> Result<Vec<PersistedAuthorEntry>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_authors_by_scope)(database_file.to_path_buf(), scope.clone()).await
}

pub(super) async fn persisted_backend_load_book_poster_summaries(
    database_file: &FsPath,
) -> Result<HashMap<String, Vec<PersistedBookPosterSummary>>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_book_poster_summaries)(database_file.to_path_buf()).await
}

pub(super) async fn persisted_backend_load_persisted_book_summaries(
    database_file: &FsPath,
    user_id: Option<&str>,
) -> Result<Vec<PersistedBookSummary>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_book_summaries)(
        database_file.to_path_buf(),
        user_id.map(str::to_string),
    )
    .await
}

pub(super) async fn persisted_backend_load_persisted_book_summaries_by_ids(
    database_file: &FsPath,
    user_id: Option<&str>,
    ids: &[String],
) -> Result<Vec<PersistedBookSummary>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_book_summaries_by_ids)(
        database_file.to_path_buf(),
        user_id.map(str::to_string),
        ids.to_vec(),
    )
    .await
}

pub(super) async fn persisted_backend_load_persisted_book_count(
    database_file: &FsPath,
) -> Result<usize, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_book_count)(database_file.to_path_buf()).await
}

pub(super) async fn persisted_backend_persisted_books_exist(
    database_file: &FsPath,
) -> Result<bool, String> {
    let backend = persisted_discovery_backend()?;
    (backend.persisted_books_exist)(database_file.to_path_buf()).await
}

pub(super) async fn persisted_backend_load_persisted_genres(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_genres)(
        database_file.to_path_buf(),
        library_ids.map(|ids| ids.to_vec()),
        collection_id.map(str::to_string),
    )
    .await
}

pub(super) async fn persisted_backend_load_persisted_tags(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_tags)(
        database_file.to_path_buf(),
        library_ids.map(|ids| ids.to_vec()),
        collection_id.map(str::to_string),
    )
    .await
}

pub(super) async fn persisted_backend_load_persisted_languages(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_languages)(
        database_file.to_path_buf(),
        library_ids.map(|ids| ids.to_vec()),
        collection_id.map(str::to_string),
    )
    .await
}

pub(super) async fn persisted_backend_load_persisted_publishers(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_publishers)(
        database_file.to_path_buf(),
        library_ids.map(|ids| ids.to_vec()),
        collection_id.map(str::to_string),
    )
    .await
}

pub(super) async fn persisted_backend_load_persisted_age_ratings(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<u16>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_age_ratings)(
        database_file.to_path_buf(),
        library_ids.map(|ids| ids.to_vec()),
        collection_id.map(str::to_string),
    )
    .await
}

pub(super) async fn persisted_backend_load_persisted_sharing_labels(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_sharing_labels)(
        database_file.to_path_buf(),
        library_ids.map(|ids| ids.to_vec()),
        collection_id.map(str::to_string),
    )
    .await
}

pub(super) async fn persisted_backend_load_persisted_series_release_dates(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_series_release_dates)(
        database_file.to_path_buf(),
        library_ids.map(|ids| ids.to_vec()),
        collection_id.map(str::to_string),
    )
    .await
}

pub(super) async fn persisted_backend_load_persisted_series_tags(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_series_tags)(
        database_file.to_path_buf(),
        library_ids.map(|ids| ids.to_vec()),
        collection_id.map(str::to_string),
    )
    .await
}

pub(super) async fn persisted_backend_load_persisted_library_ids(
    database_file: &FsPath,
) -> Result<Vec<String>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_library_ids)(database_file.to_path_buf()).await
}

pub(super) async fn persisted_backend_load_collection_memberships(
    database_file: &FsPath,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_collection_memberships)(database_file.to_path_buf()).await
}

pub(super) async fn persisted_backend_load_readlist_memberships(
    database_file: &FsPath,
) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_readlist_memberships)(database_file.to_path_buf()).await
}

pub(super) async fn persisted_backend_load_persisted_ondeck_books(
    database_file: &FsPath,
    user_id: &str,
) -> Result<Vec<PersistedBookBrowseEntry>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_ondeck_books)(database_file.to_path_buf(), user_id.to_string()).await
}

pub(super) async fn persisted_backend_load_persisted_duplicate_books(
    database_file: &FsPath,
) -> Result<Vec<PersistedBookBrowseEntry>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_duplicate_books)(database_file.to_path_buf()).await
}

pub(super) async fn persisted_backend_load_persisted_book_tags(
    database_file: &FsPath,
    scope: Option<&PersistedBookTagsScope>,
) -> Result<Vec<String>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_book_tags)(database_file.to_path_buf(), scope.cloned()).await
}

pub(super) async fn persisted_backend_persisted_utc_date_minus_days(
    database_file: &FsPath,
    days: i64,
) -> Result<Option<String>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.persisted_utc_date_minus_days)(database_file.to_path_buf(), days).await
}

pub(super) async fn persisted_backend_load_series_read_progress_counts(
    database_file: &FsPath,
    user_id: &str,
) -> Result<HashMap<String, (i64, i64)>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_series_read_progress_counts)(database_file.to_path_buf(), user_id.to_string())
        .await
}

pub(super) async fn persisted_backend_load_series_total_book_counts(
    database_file: &FsPath,
) -> Result<HashMap<String, i64>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_series_total_book_counts)(database_file.to_path_buf()).await
}

pub(super) async fn persisted_backend_load_persisted_series_summaries(
    database_file: &FsPath,
) -> Result<Vec<PersistedSeriesSummary>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_series_summaries)(database_file.to_path_buf()).await
}

pub(super) async fn persisted_backend_load_persisted_series_summaries_by_ids(
    database_file: &FsPath,
    ids: &[String],
) -> Result<Vec<PersistedSeriesSummary>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_series_summaries_by_ids)(database_file.to_path_buf(), ids.to_vec())
        .await
}

pub(super) async fn persisted_backend_load_persisted_series_count(
    database_file: &FsPath,
) -> Result<usize, String> {
    let backend = persisted_discovery_backend()?;
    (backend.load_persisted_series_count)(database_file.to_path_buf()).await
}

pub(super) async fn persisted_backend_persisted_series_exist(
    database_file: &FsPath,
) -> Result<bool, String> {
    let backend = persisted_discovery_backend()?;
    (backend.persisted_series_exist)(database_file.to_path_buf()).await
}

pub(super) async fn persisted_backend_search_book_ids(
    database_file: &FsPath,
    query: &str,
    limit: usize,
) -> Result<Vec<String>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.search_book_ids)(database_file.to_path_buf(), query.to_string(), limit).await
}

pub(super) async fn persisted_backend_search_series_ids(
    database_file: &FsPath,
    query: &str,
    limit: usize,
) -> Result<Vec<String>, String> {
    let backend = persisted_discovery_backend()?;
    (backend.search_series_ids)(database_file.to_path_buf(), query.to_string(), limit).await
}
