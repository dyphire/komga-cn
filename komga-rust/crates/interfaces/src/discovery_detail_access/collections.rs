use std::future::Future;
use std::path::Path as FsPath;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub struct DiscoveryDetailCollectionsAccessBackend {
    pub persisted_collections_exist:
        Arc<dyn Fn(PathBuf) -> BoxFuture<Result<bool, String>> + Send + Sync>,
    pub load_persisted_collections: Arc<
        dyn Fn(PathBuf) -> BoxFuture<Result<Vec<PersistedCollectionRecord>, String>> + Send + Sync,
    >,
    pub load_persisted_collection_series_ids:
        Arc<dyn Fn(PathBuf, String) -> BoxFuture<Result<Vec<String>, String>> + Send + Sync>,
    pub load_persisted_collection_detail: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<PersistedCollectionRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_series_library_id:
        Arc<dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<String>, String>> + Send + Sync>,
    pub load_series_restrictions: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<PersistedSeriesRestrictionRecord, String>>
            + Send
            + Sync,
    >,
    pub persist_collection_create: Arc<
        dyn Fn(PathBuf, String, String, bool, Vec<String>) -> BoxFuture<Result<(), String>>
            + Send
            + Sync,
    >,
    pub persist_collection_update: Arc<
        dyn Fn(PathBuf, String, String, bool, Vec<String>) -> BoxFuture<Result<bool, String>>
            + Send
            + Sync,
    >,
    pub delete_persisted_collection:
        Arc<dyn Fn(PathBuf, String) -> BoxFuture<Result<bool, String>> + Send + Sync>,
    pub upsert_collection_search_document:
        Arc<dyn Fn(PathBuf, PathBuf, String) -> BoxFuture<Result<bool, String>> + Send + Sync>,
    pub delete_collection_search_document:
        Arc<dyn Fn(PathBuf, String) -> BoxFuture<Result<(), String>> + Send + Sync>,
}

static BACKEND: OnceLock<DiscoveryDetailCollectionsAccessBackend> = OnceLock::new();

pub(crate) fn install_backend(backend: DiscoveryDetailCollectionsAccessBackend) {
    let _ = BACKEND.set(backend);
}

fn backend() -> &'static DiscoveryDetailCollectionsAccessBackend {
    BACKEND.get_or_init(|| DiscoveryDetailCollectionsAccessBackend {
        persisted_collections_exist: Arc::new(|_| {
            Box::pin(async {
                Err("discovery detail collections backend is not configured".to_string())
            })
        }),
        load_persisted_collections: Arc::new(|_| {
            Box::pin(async {
                Err("discovery detail collections backend is not configured".to_string())
            })
        }),
        load_persisted_collection_series_ids: Arc::new(|_, _| {
            Box::pin(async {
                Err("discovery detail collections backend is not configured".to_string())
            })
        }),
        load_persisted_collection_detail: Arc::new(|_, _| {
            Box::pin(async {
                Err("discovery detail collections backend is not configured".to_string())
            })
        }),
        load_series_library_id: Arc::new(|_, _| {
            Box::pin(async {
                Err("discovery detail collections backend is not configured".to_string())
            })
        }),
        load_series_restrictions: Arc::new(|_, _| {
            Box::pin(async {
                Err("discovery detail collections backend is not configured".to_string())
            })
        }),
        persist_collection_create: Arc::new(|_, _, _, _, _| {
            Box::pin(async {
                Err("discovery detail collections backend is not configured".to_string())
            })
        }),
        persist_collection_update: Arc::new(|_, _, _, _, _| {
            Box::pin(async {
                Err("discovery detail collections backend is not configured".to_string())
            })
        }),
        delete_persisted_collection: Arc::new(|_, _| {
            Box::pin(async {
                Err("discovery detail collections backend is not configured".to_string())
            })
        }),
        upsert_collection_search_document: Arc::new(|_, _, _| {
            Box::pin(async {
                Err("discovery detail collections backend is not configured".to_string())
            })
        }),
        delete_collection_search_document: Arc::new(|_, _| {
            Box::pin(async {
                Err("discovery detail collections backend is not configured".to_string())
            })
        }),
    })
}

#[derive(Clone)]
pub struct PersistedCollectionRecord {
    pub id: String,
    pub name: String,
    pub ordered: bool,
    pub created_date: String,
    pub last_modified_date: String,
}

pub struct PersistedSeriesRestrictionRecord {
    pub age_rating: Option<u16>,
    pub labels: Vec<String>,
}

pub async fn persisted_collections_exist(database_file: &FsPath) -> Result<bool, String> {
    (backend().persisted_collections_exist)(database_file.to_path_buf()).await
}

pub async fn load_persisted_collections(
    database_file: &FsPath,
) -> Result<Vec<PersistedCollectionRecord>, String> {
    (backend().load_persisted_collections)(database_file.to_path_buf()).await
}

pub async fn load_persisted_collection_series_ids(
    database_file: &FsPath,
    collection_id: &str,
) -> Result<Vec<String>, String> {
    (backend().load_persisted_collection_series_ids)(
        database_file.to_path_buf(),
        collection_id.to_string(),
    )
    .await
}

pub async fn load_persisted_collection_detail(
    database_file: &FsPath,
    collection_id: &str,
) -> Result<Option<PersistedCollectionRecord>, String> {
    (backend().load_persisted_collection_detail)(
        database_file.to_path_buf(),
        collection_id.to_string(),
    )
    .await
}

pub async fn load_series_library_id(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<String>, String> {
    (backend().load_series_library_id)(database_file.to_path_buf(), series_id.to_string()).await
}

pub async fn load_series_restrictions(
    database_file: &FsPath,
    series_id: &str,
) -> Result<PersistedSeriesRestrictionRecord, String> {
    (backend().load_series_restrictions)(database_file.to_path_buf(), series_id.to_string()).await
}

pub async fn persist_collection_create(
    database_file: &FsPath,
    collection_id: &str,
    name: &str,
    ordered: bool,
    series_ids: &[String],
) -> Result<(), String> {
    (backend().persist_collection_create)(
        database_file.to_path_buf(),
        collection_id.to_string(),
        name.to_string(),
        ordered,
        series_ids.to_vec(),
    )
    .await
}

pub async fn persist_collection_update(
    database_file: &FsPath,
    collection_id: &str,
    name: &str,
    ordered: bool,
    series_ids: &[String],
) -> Result<bool, String> {
    (backend().persist_collection_update)(
        database_file.to_path_buf(),
        collection_id.to_string(),
        name.to_string(),
        ordered,
        series_ids.to_vec(),
    )
    .await
}

pub async fn delete_persisted_collection(
    database_file: &FsPath,
    collection_id: &str,
) -> Result<bool, String> {
    (backend().delete_persisted_collection)(database_file.to_path_buf(), collection_id.to_string())
        .await
}

pub async fn upsert_collection_search_document(
    database_file: &FsPath,
    index_dir: &FsPath,
    collection_id: &str,
) -> Result<bool, String> {
    (backend().upsert_collection_search_document)(
        database_file.to_path_buf(),
        index_dir.to_path_buf(),
        collection_id.to_string(),
    )
    .await
}

pub async fn delete_collection_search_document(
    index_dir: &FsPath,
    collection_id: &str,
) -> Result<(), String> {
    (backend().delete_collection_search_document)(
        index_dir.to_path_buf(),
        collection_id.to_string(),
    )
    .await
}
