use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

fn discovery_index_dir_mappings() -> &'static RwLock<HashMap<PathBuf, PathBuf>> {
    static DISCOVERY_INDEX_DIR_MAPPINGS: OnceLock<RwLock<HashMap<PathBuf, PathBuf>>> =
        OnceLock::new();
    DISCOVERY_INDEX_DIR_MAPPINGS.get_or_init(|| RwLock::new(HashMap::new()))
}

pub fn register_discovery_index_dir(database_file: &Path, lucene_data_directory: &Path) {
    let key = database_file.to_path_buf();
    let value = lucene_data_directory.to_path_buf();
    let mappings = discovery_index_dir_mappings();
    let mut guard = mappings
        .write()
        .expect("discovery index-dir mapping write lock should not be poisoned");
    guard.insert(key, value);
}

pub fn resolve_discovery_index_dir(
    database_file: &Path,
    default_lucene_data_directory: &Path,
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
