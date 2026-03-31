use std::future::Future;
use std::path::Path as FsPath;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub struct DiscoveryDetailReadlistsAccessBackend {
    pub persisted_readlists_exist:
        Arc<dyn Fn(PathBuf) -> BoxFuture<Result<bool, String>> + Send + Sync>,
    pub load_persisted_readlists: Arc<
        dyn Fn(PathBuf) -> BoxFuture<Result<Vec<PersistedReadlistRecord>, String>> + Send + Sync,
    >,
    pub load_persisted_readlist_detail: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<PersistedReadlistRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_persisted_readlist_book_rows: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Vec<PersistedReadlistBookRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_comicrack_match_candidates: Arc<
        dyn Fn(PathBuf) -> BoxFuture<Result<Vec<PersistedComicrackMatchCandidateRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_persisted_book_authors: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Vec<PersistedBookAuthorRecord>, String>>
            + Send
            + Sync,
    >,
    pub persist_readlist_create: Arc<
        dyn Fn(PathBuf, String, String, String, bool, Vec<String>) -> BoxFuture<Result<(), String>>
            + Send
            + Sync,
    >,
    pub persist_readlist_update: Arc<
        dyn Fn(
                PathBuf,
                String,
                String,
                String,
                bool,
                Vec<String>,
            ) -> BoxFuture<Result<bool, String>>
            + Send
            + Sync,
    >,
    pub delete_persisted_readlist:
        Arc<dyn Fn(PathBuf, String) -> BoxFuture<Result<bool, String>> + Send + Sync>,
    pub upsert_readlist_search_document:
        Arc<dyn Fn(PathBuf, PathBuf, String) -> BoxFuture<Result<bool, String>> + Send + Sync>,
    pub delete_readlist_search_document:
        Arc<dyn Fn(PathBuf, String) -> BoxFuture<Result<(), String>> + Send + Sync>,
}

static BACKEND: OnceLock<DiscoveryDetailReadlistsAccessBackend> = OnceLock::new();

pub(crate) fn install_backend(backend: DiscoveryDetailReadlistsAccessBackend) {
    let _ = BACKEND.set(backend);
}

fn backend() -> &'static DiscoveryDetailReadlistsAccessBackend {
    BACKEND.get_or_init(|| DiscoveryDetailReadlistsAccessBackend {
        persisted_readlists_exist: Arc::new(|_| {
            Box::pin(async {
                Err("discovery detail readlists backend is not configured".to_string())
            })
        }),
        load_persisted_readlists: Arc::new(|_| {
            Box::pin(async {
                Err("discovery detail readlists backend is not configured".to_string())
            })
        }),
        load_persisted_readlist_detail: Arc::new(|_, _| {
            Box::pin(async {
                Err("discovery detail readlists backend is not configured".to_string())
            })
        }),
        load_persisted_readlist_book_rows: Arc::new(|_, _| {
            Box::pin(async {
                Err("discovery detail readlists backend is not configured".to_string())
            })
        }),
        load_comicrack_match_candidates: Arc::new(|_| {
            Box::pin(async {
                Err("discovery detail readlists backend is not configured".to_string())
            })
        }),
        load_persisted_book_authors: Arc::new(|_, _| {
            Box::pin(async {
                Err("discovery detail readlists backend is not configured".to_string())
            })
        }),
        persist_readlist_create: Arc::new(|_, _, _, _, _, _| {
            Box::pin(async {
                Err("discovery detail readlists backend is not configured".to_string())
            })
        }),
        persist_readlist_update: Arc::new(|_, _, _, _, _, _| {
            Box::pin(async {
                Err("discovery detail readlists backend is not configured".to_string())
            })
        }),
        delete_persisted_readlist: Arc::new(|_, _| {
            Box::pin(async {
                Err("discovery detail readlists backend is not configured".to_string())
            })
        }),
        upsert_readlist_search_document: Arc::new(|_, _, _| {
            Box::pin(async {
                Err("discovery detail readlists backend is not configured".to_string())
            })
        }),
        delete_readlist_search_document: Arc::new(|_, _| {
            Box::pin(async {
                Err("discovery detail readlists backend is not configured".to_string())
            })
        }),
    })
}

#[derive(Clone)]
pub struct PersistedReadlistRecord {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub ordered: bool,
    pub created_date: String,
    pub last_modified_date: String,
}

#[derive(Clone)]
pub struct PersistedReadlistBookRecord {
    pub book_id: String,
    pub library_id: String,
}

#[derive(Clone)]
pub struct PersistedComicrackMatchCandidateRecord {
    pub series_id: String,
    pub series_title: String,
    pub series_release_date: Option<String>,
    pub book_id: String,
    pub book_title: String,
    pub book_number: String,
}

#[derive(Clone)]
pub struct PersistedBookAuthorRecord {
    pub name: String,
    pub role: String,
}

pub async fn persisted_readlists_exist(database_file: &FsPath) -> Result<bool, String> {
    (backend().persisted_readlists_exist)(database_file.to_path_buf()).await
}

pub async fn load_persisted_readlists(
    database_file: &FsPath,
) -> Result<Vec<PersistedReadlistRecord>, String> {
    (backend().load_persisted_readlists)(database_file.to_path_buf()).await
}

pub async fn load_persisted_readlist_detail(
    database_file: &FsPath,
    readlist_id: &str,
) -> Result<Option<PersistedReadlistRecord>, String> {
    (backend().load_persisted_readlist_detail)(database_file.to_path_buf(), readlist_id.to_string())
        .await
}

pub async fn load_persisted_readlist_book_rows(
    database_file: &FsPath,
    readlist_id: &str,
) -> Result<Vec<PersistedReadlistBookRecord>, String> {
    (backend().load_persisted_readlist_book_rows)(
        database_file.to_path_buf(),
        readlist_id.to_string(),
    )
    .await
}

pub async fn load_comicrack_match_candidates(
    database_file: &FsPath,
) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String> {
    (backend().load_comicrack_match_candidates)(database_file.to_path_buf()).await
}

pub async fn load_persisted_book_authors(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Vec<PersistedBookAuthorRecord>, String> {
    (backend().load_persisted_book_authors)(database_file.to_path_buf(), book_id.to_string()).await
}

pub async fn persist_readlist_create(
    database_file: &FsPath,
    readlist_id: &str,
    name: &str,
    summary: &str,
    ordered: bool,
    book_ids: &[String],
) -> Result<(), String> {
    (backend().persist_readlist_create)(
        database_file.to_path_buf(),
        readlist_id.to_string(),
        name.to_string(),
        summary.to_string(),
        ordered,
        book_ids.to_vec(),
    )
    .await
}

pub async fn persist_readlist_update(
    database_file: &FsPath,
    readlist_id: &str,
    name: &str,
    summary: &str,
    ordered: bool,
    book_ids: &[String],
) -> Result<bool, String> {
    (backend().persist_readlist_update)(
        database_file.to_path_buf(),
        readlist_id.to_string(),
        name.to_string(),
        summary.to_string(),
        ordered,
        book_ids.to_vec(),
    )
    .await
}

pub async fn delete_persisted_readlist(
    database_file: &FsPath,
    readlist_id: &str,
) -> Result<bool, String> {
    (backend().delete_persisted_readlist)(database_file.to_path_buf(), readlist_id.to_string())
        .await
}

pub async fn upsert_readlist_search_document(
    database_file: &FsPath,
    index_dir: &FsPath,
    readlist_id: &str,
) -> Result<bool, String> {
    (backend().upsert_readlist_search_document)(
        database_file.to_path_buf(),
        index_dir.to_path_buf(),
        readlist_id.to_string(),
    )
    .await
}

pub async fn delete_readlist_search_document(
    index_dir: &FsPath,
    readlist_id: &str,
) -> Result<(), String> {
    (backend().delete_readlist_search_document)(index_dir.to_path_buf(), readlist_id.to_string())
        .await
}
