use super::*;
use std::sync::OnceLock;

fn discovery_index_dir_mappings() -> &'static std::sync::RwLock<HashMap<PathBuf, PathBuf>> {
    static DISCOVERY_INDEX_DIR_MAPPINGS: OnceLock<std::sync::RwLock<HashMap<PathBuf, PathBuf>>> =
        OnceLock::new();
    DISCOVERY_INDEX_DIR_MAPPINGS.get_or_init(|| std::sync::RwLock::new(HashMap::new()))
}

pub(super) fn register_discovery_index_dir(
    database_file: &std::path::Path,
    lucene_data_directory: &std::path::Path,
) {
    let key = database_file.to_path_buf();
    let value = lucene_data_directory.to_path_buf();
    let mappings = discovery_index_dir_mappings();
    let mut guard = mappings
        .write()
        .expect("discovery index-dir mapping write lock should not be poisoned");
    guard.insert(key, value);
}

pub(super) fn resolve_discovery_index_dir(
    database_file: &std::path::Path,
    default_lucene_data_directory: &std::path::Path,
) -> PathBuf {
    let mappings = discovery_index_dir_mappings();
    let guard = mappings
        .read()
        .expect("discovery index-dir mapping read lock should not be poisoned");
    guard
        .get(database_file)
        .cloned()
        .unwrap_or_else(|| default_lucene_data_directory.to_path_buf())
}
