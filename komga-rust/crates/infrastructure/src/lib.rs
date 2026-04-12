use std::env;
use std::path::PathBuf;
use std::sync::OnceLock;

use pdfium_render::prelude::*;

pub mod announcements_access;
pub mod auth;
pub mod claims_access;
mod context;
pub mod discovery_detail_access;
pub mod discovery_persisted_access;
pub mod filesystem;
pub mod library_catalog;
pub mod metadata;
pub mod opds_catalog_access;
pub mod opds_manifest_access;
pub mod opds_persisted_access;
pub mod operational_metrics_access;
pub mod operational_settings_access;
pub mod page_hashes_access;
mod persisted_paths;
mod rar_support;
pub mod read_models;
pub mod runtime_identity_access;
pub mod search;
pub mod sql;
pub mod sqlite;
#[path = "tasks/runtime/mod.rs"]
pub mod task_queue;
pub mod tasks;

pub use context::{SqlitePersistenceConnection, SqlitePersistenceContext, SqliteUnitOfWork};
pub(crate) use persisted_paths::{
    resolve_library_item_path, resolve_optional_library_item_path, resolve_rooted_path,
    resolve_stored_path,
};
pub use search::{
    SearchDocument, SearchEntityType, SearchError, SearchEvent, SearchIndexLifecycle,
    SearchQueryLifecycle, SearchStartupLifecycle, decide_startup_lifecycle, prepare_for_rebuild,
    rebuild_index_from_database, sync_entity_delete_from_index, sync_entity_upsert_from_database,
    sync_series_and_oneshot_books_after_metadata_update,
};
pub use sqlite::write_models::ServerSettingsStore;

static PDFIUM: OnceLock<Result<Pdfium, String>> = OnceLock::new();
const DEFAULT_PDFIUM_LIBRARY_PATH: &str = env!("KOMGA_PDFIUM_LIB_PATH");

pub(crate) fn load_pdfium() -> Result<&'static Pdfium, String> {
    match PDFIUM.get_or_init(init_pdfium) {
        Ok(pdfium) => Ok(pdfium),
        Err(error) => Err(error.clone()),
    }
}

fn init_pdfium() -> Result<Pdfium, String> {
    let mut attempted_paths = Vec::new();

    for library_path in pdfium_library_candidates(
        env::var_os("KOMGA_PDFIUM_LIB_PATH").map(PathBuf::from),
        env::current_exe().ok(),
    ) {
        attempted_paths.push(library_path.display().to_string());
        match Pdfium::bind_to_library(&library_path) {
            Ok(bindings) => return Ok(Pdfium::new(bindings)),
            Err(_) => continue,
        }
    }

    Pdfium::bind_to_system_library()
        .map(Pdfium::new)
        .map_err(|error| {
            if attempted_paths.is_empty() {
                format!("failed to bind Pdfium from system libraries: {error}")
            } else {
                format!(
                    "failed to bind Pdfium from bundled candidates [{}] and system libraries: {error}",
                    attempted_paths.join(", ")
                )
            }
        })
}

fn pdfium_library_candidates(
    runtime_override: Option<PathBuf>,
    executable: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    // Prefer colocated libraries so packaged releases and container images stay portable
    // after leaving the build machine. Fall back to the build-time vendor path for local
    // development, then finally to the system loader.
    if let Some(runtime_override) = runtime_override {
        candidates.push(runtime_override);
    }

    if let Some(bundled_path) = executable.and_then(bundled_pdfium_library_path) {
        candidates.push(bundled_path);
    }

    candidates.push(PathBuf::from(DEFAULT_PDFIUM_LIBRARY_PATH));
    candidates
}

fn bundled_pdfium_library_path(executable: PathBuf) -> Option<PathBuf> {
    Some(executable.parent()?.join(pdfium_library_file_name()))
}

fn pdfium_library_file_name() -> &'static str {
    match env::consts::OS {
        "linux" => "libpdfium.so",
        "macos" => "libpdfium.dylib",
        "windows" => "pdfium.dll",
        other => panic!("unsupported target os for Pdfium library file name: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_PDFIUM_LIBRARY_PATH, pdfium_library_candidates, pdfium_library_file_name};
    use std::path::{Path, PathBuf};

    #[test]
    fn pdfium_candidates_prefer_override_then_bundled_then_build_vendor() {
        let candidates = pdfium_library_candidates(
            Some(PathBuf::from("/runtime/pdfium/custom.so")),
            Some(PathBuf::from("/opt/komga/komga-riir")),
        );

        assert_eq!(
            candidates,
            vec![
                PathBuf::from("/runtime/pdfium/custom.so"),
                Path::new("/opt/komga").join(pdfium_library_file_name()),
                PathBuf::from(DEFAULT_PDFIUM_LIBRARY_PATH),
            ]
        );
    }
}
