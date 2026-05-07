use super::*;

use komga_application::discovery::DiscoveryListService;
use komga_infrastructure::database_handle::DatabaseHandle;
use komga_infrastructure::search::index_lifecycle::SearchEntityType;
use komga_infrastructure::search::runtime_tasks::{
    sync_entity_delete_from_index, sync_entity_upsert_from_database,
    sync_series_and_oneshot_books_after_metadata_update,
};
use komga_interfaces::state::{
    DiscoveryAuthorService, DiscoveryBookFeedService, DiscoveryCollectionSearchService,
    DiscoveryLibraryMappingService, DiscoveryReadlistSearchService,
};

mod detail_access;
mod index_dirs;
mod persisted_access;

pub(super) fn compose_discovery_detail_service(
    db: DatabaseHandle,
    index_dir: PathBuf,
) -> Box<dyn DiscoveryDetailService> {
    detail_access::compose_discovery_detail_service(db, index_dir)
}

pub(super) fn compose_discovery_list_service(
    db: DatabaseHandle,
    lucene_data_directory: PathBuf,
) -> Box<dyn DiscoveryListService> {
    let persisted =
        persisted_access::compose_persisted_discovery_list_data_source(db, lucene_data_directory);
    komga_interfaces::discovery::compose_persisted_discovery_list_service(persisted)
}

pub(super) fn compose_discovery_author_service(
    db: DatabaseHandle,
    lucene_data_directory: PathBuf,
) -> Box<dyn DiscoveryAuthorService> {
    persisted_access::compose_discovery_author_service(db, lucene_data_directory)
}

pub(super) fn compose_discovery_library_mapping_service(
    db: DatabaseHandle,
    lucene_data_directory: PathBuf,
) -> Box<dyn DiscoveryLibraryMappingService> {
    persisted_access::compose_discovery_library_mapping_service(db, lucene_data_directory)
}

pub(super) fn compose_discovery_collection_search_service(
    db: DatabaseHandle,
    lucene_data_directory: PathBuf,
) -> Box<dyn DiscoveryCollectionSearchService> {
    persisted_access::compose_discovery_collection_search_service(db, lucene_data_directory)
}

pub(super) fn compose_discovery_readlist_search_service(
    db: DatabaseHandle,
    lucene_data_directory: PathBuf,
) -> Box<dyn DiscoveryReadlistSearchService> {
    persisted_access::compose_discovery_readlist_search_service(db, lucene_data_directory)
}

pub(super) fn compose_discovery_book_feed_service(
    db: DatabaseHandle,
    lucene_data_directory: PathBuf,
) -> Box<dyn DiscoveryBookFeedService> {
    persisted_access::compose_discovery_book_feed_service(db, lucene_data_directory)
}

pub(super) fn resolve_discovery_index_dir(
    database_file: &std::path::Path,
    default_lucene_data_directory: &std::path::Path,
) -> PathBuf {
    index_dirs::resolve_discovery_index_dir(database_file, default_lucene_data_directory)
}
