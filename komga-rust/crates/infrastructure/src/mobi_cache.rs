use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use sha2::{Digest, Sha256};

static MOBI_CACHE_ROOT: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();

pub fn configure_mobi_cache_root(root: Option<PathBuf>) {
    let lock = MOBI_CACHE_ROOT.get_or_init(|| RwLock::new(None));
    if let Ok(mut current) = lock.write() {
        *current = root;
    }
}

pub(crate) async fn cached_mobi_epub_path(path: &Path) -> anyhow::Result<Option<PathBuf>> {
    let Some(root) = cache_root() else {
        return Ok(None);
    };
    let source = path.to_path_buf();
    tokio::task::spawn_blocking(move || materialize_cached_mobi_epub_with_root(source, root))
        .await
        .map_err(|error| anyhow::anyhow!(error).context("join MOBI cache materialization"))?
}

pub(crate) fn materialize_cached_mobi_epub(path: &Path) -> anyhow::Result<Option<PathBuf>> {
    let Some(root) = cache_root() else {
        return Ok(None);
    };
    materialize_cached_mobi_epub_with_root(path.to_path_buf(), root)
}

pub(crate) async fn remove_cached_mobi(path: &Path) -> anyhow::Result<()> {
    let Some(root) = cache_root() else {
        return Ok(());
    };
    let source = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let cache_dir = root.join(cache_key(&source));
        match std::fs::remove_dir_all(cache_dir) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(anyhow::anyhow!(error)),
        }
    })
    .await
    .map_err(|error| anyhow::anyhow!(error).context("join MOBI cache cleanup"))?
}

fn materialize_cached_mobi_epub_with_root(
    source: PathBuf,
    root: PathBuf,
) -> anyhow::Result<Option<PathBuf>> {
    let key = cache_key(&source);
    komga_epub::materialize_mobi_cache(&source, &root, key.as_str())
        .map(Some)
        .map_err(|error| anyhow::anyhow!(error))
}

fn cache_root() -> Option<PathBuf> {
    MOBI_CACHE_ROOT
        .get_or_init(|| RwLock::new(None))
        .read()
        .ok()
        .and_then(|value| value.clone())
}

fn cache_key(path: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
