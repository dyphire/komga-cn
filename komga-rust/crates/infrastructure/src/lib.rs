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
pub use search::{
    SearchDocument, SearchEntityType, SearchError, SearchEvent, SearchIndexLifecycle,
    SearchQueryLifecycle, SearchStartupLifecycle, decide_startup_lifecycle, prepare_for_rebuild,
    rebuild_index_from_database, sync_entity_delete_from_index, sync_entity_upsert_from_database,
    sync_series_and_oneshot_books_after_metadata_update,
};
pub use sqlite::write_models::ServerSettingsStore;

static PDFIUM: OnceLock<Result<Pdfium, String>> = OnceLock::new();

pub(crate) fn load_pdfium() -> Result<&'static Pdfium, String> {
    match PDFIUM.get_or_init(init_pdfium) {
        Ok(pdfium) => Ok(pdfium),
        Err(error) => Err(error.clone()),
    }
}

fn init_pdfium() -> Result<Pdfium, String> {
    let library_path = env!("KOMGA_PDFIUM_LIB_PATH");
    let bindings = Pdfium::bind_to_library(library_path)
        .or_else(|_| Pdfium::bind_to_system_library())
        .map_err(|error| format!("failed to bind Pdfium at '{library_path}': {error}"))?;
    Ok(Pdfium::new(bindings))
}
