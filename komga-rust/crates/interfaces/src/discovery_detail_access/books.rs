use std::future::Future;
use std::path::Path as FsPath;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub struct DiscoveryDetailBooksAccessBackend {
    pub load_book_id_by_sorted_position:
        Arc<dyn Fn(PathBuf, usize) -> BoxFuture<Result<Option<String>, String>> + Send + Sync>,
    pub load_persisted_book_resource: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<PersistedBookResourceRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_persisted_book_detail: Arc<
        dyn Fn(
                PathBuf,
                String,
                Option<String>,
            ) -> BoxFuture<Result<Option<PersistedBookDetailRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_persisted_book_sibling_id: Arc<
        dyn Fn(
                PathBuf,
                String,
                PersistedBookSiblingDirectionRecord,
            ) -> BoxFuture<Result<Option<String>, String>>
            + Send
            + Sync,
    >,
}

static BACKEND: OnceLock<DiscoveryDetailBooksAccessBackend> = OnceLock::new();

pub(crate) fn install_backend(backend: DiscoveryDetailBooksAccessBackend) {
    let _ = BACKEND.set(backend);
}

fn backend() -> &'static DiscoveryDetailBooksAccessBackend {
    BACKEND.get_or_init(|| DiscoveryDetailBooksAccessBackend {
        load_book_id_by_sorted_position: Arc::new(|_, _| {
            Box::pin(async { Err("discovery detail books backend is not configured".to_string()) })
        }),
        load_persisted_book_resource: Arc::new(|_, _| {
            Box::pin(async { Err("discovery detail books backend is not configured".to_string()) })
        }),
        load_persisted_book_detail: Arc::new(|_, _, _| {
            Box::pin(async { Err("discovery detail books backend is not configured".to_string()) })
        }),
        load_persisted_book_sibling_id: Arc::new(|_, _, _| {
            Box::pin(async { Err("discovery detail books backend is not configured".to_string()) })
        }),
    })
}

#[derive(Clone)]
pub struct PersistedBookResourceRecord {
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: String,
}

#[derive(Clone)]
pub struct PersistedBookDetailRecord {
    pub id: String,
    pub series_id: String,
    pub series_title: String,
    pub series_title_sort: String,
    pub library_id: String,
    pub name: String,
    pub url: String,
    pub number: i32,
    pub created: String,
    pub last_modified: String,
    pub file_last_modified: String,
    pub size_bytes: u64,
    pub media_status: String,
    pub media_type: String,
    pub media_pages_count: u32,
    pub media_comment: String,
    pub metadata_title: String,
    pub metadata_summary: String,
    pub metadata_number: String,
    pub metadata_number_sort: f64,
    pub metadata_release_date: Option<String>,
    pub metadata_title_lock: bool,
    pub metadata_summary_lock: bool,
    pub metadata_number_lock: bool,
    pub metadata_number_sort_lock: bool,
    pub metadata_release_date_lock: bool,
    pub metadata_authors: String,
    pub metadata_authors_lock: bool,
    pub metadata_tags: String,
    pub metadata_tags_lock: bool,
    pub metadata_isbn: String,
    pub metadata_isbn_lock: bool,
    pub metadata_links: String,
    pub metadata_links_lock: bool,
    pub metadata_created: String,
    pub metadata_last_modified: String,
    pub media_epub_divina_compatible: bool,
    pub media_epub_is_kepub: bool,
    pub read_progress: Option<PersistedReadProgressRecord>,
    pub deleted: bool,
    pub file_hash: String,
    pub oneshot: bool,
}

#[derive(Clone)]
pub struct PersistedReadProgressRecord {
    pub page: i32,
    pub completed: bool,
    pub read_date: Option<String>,
    pub created: String,
    pub last_modified: String,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Clone, Copy)]
pub enum PersistedBookSiblingDirectionRecord {
    Previous,
    Next,
}

pub async fn load_book_id_by_sorted_position(
    database_file: &FsPath,
    index: usize,
) -> Result<Option<String>, String> {
    (backend().load_book_id_by_sorted_position)(database_file.to_path_buf(), index).await
}

pub async fn load_persisted_book_resource(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<PersistedBookResourceRecord>, String> {
    (backend().load_persisted_book_resource)(database_file.to_path_buf(), book_id.to_string()).await
}

pub async fn load_persisted_book_detail(
    database_file: &FsPath,
    book_id: &str,
    user_id: Option<&str>,
) -> Result<Option<PersistedBookDetailRecord>, String> {
    (backend().load_persisted_book_detail)(
        database_file.to_path_buf(),
        book_id.to_string(),
        user_id.map(str::to_string),
    )
    .await
}

pub async fn load_persisted_book_sibling_id(
    database_file: &FsPath,
    book_id: &str,
    direction: PersistedBookSiblingDirectionRecord,
) -> Result<Option<String>, String> {
    (backend().load_persisted_book_sibling_id)(
        database_file.to_path_buf(),
        book_id.to_string(),
        direction,
    )
    .await
}
