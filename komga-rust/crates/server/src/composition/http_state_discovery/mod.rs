use super::*;

use komga_infrastructure::{
    SearchEntityType, sync_entity_delete_from_index, sync_entity_upsert_from_database,
    sync_series_and_oneshot_books_after_metadata_update,
};

mod detail_access;
mod index_dirs;
mod persisted_access;

pub(super) fn compose_discovery_detail_access_backends() -> DiscoveryDetailAccessBackends {
    detail_access::compose_discovery_detail_access_backends()
}

pub(super) fn compose_persisted_discovery_access_backend(
    database_file: &std::path::Path,
    lucene_data_directory: &std::path::Path,
) -> PersistedDiscoveryAccessBackend {
    persisted_access::compose_persisted_discovery_access_backend(
        database_file,
        lucene_data_directory,
    )
}

pub(super) fn resolve_discovery_index_dir(
    database_file: &std::path::Path,
    default_lucene_data_directory: &std::path::Path,
) -> PathBuf {
    index_dirs::resolve_discovery_index_dir(database_file, default_lucene_data_directory)
}
