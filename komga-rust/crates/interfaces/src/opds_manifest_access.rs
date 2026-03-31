#![allow(clippy::type_complexity)]

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, OnceLock};

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

pub struct ManifestBookRecord {
    pub title: String,
    pub file_name: String,
    pub media_type: Option<String>,
    pub page_count: i64,
}

pub struct OpdsManifestAccessBackend {
    pub load_manifest_book_record: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<ManifestBookRecord>, String>>
            + Send
            + Sync,
    >,
}

static BACKEND: OnceLock<OpdsManifestAccessBackend> = OnceLock::new();

pub fn install_opds_manifest_access(backend: OpdsManifestAccessBackend) {
    let _ = BACKEND.set(backend);
}

fn backend() -> &'static OpdsManifestAccessBackend {
    BACKEND
        .get()
        .expect("opds manifest access backend should be installed before use")
}

pub(crate) async fn load_manifest_book_record(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<ManifestBookRecord>, String> {
    (backend().load_manifest_book_record)(database_file.to_path_buf(), book_id.to_string()).await
}
