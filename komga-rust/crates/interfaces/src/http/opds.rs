#[path = "content_opds.rs"]
mod protocol_routes;

pub(super) use protocol_routes::{
    opds_auth_route, opds_catalog_route, opds_manifest_profile_route, opds_manifest_route,
    opds_v1_book_file_route, opds_v1_books_latest_route, opds_v1_catalog_route,
    opds_v1_collection_detail_route, opds_v1_collections_route, opds_v1_keep_reading_route,
    opds_v1_libraries_route, opds_v1_library_detail_route, opds_v1_on_deck_route,
    opds_v1_publishers_route, opds_v1_readlist_detail_route, opds_v1_readlists_route,
    opds_v1_search_route, opds_v1_series_detail_route, opds_v1_series_latest_route,
    opds_v1_series_route, opds_v2_collection_route, opds_v2_libraries_browse_route,
    opds_v2_libraries_collections_route, opds_v2_libraries_keep_reading_route,
    opds_v2_libraries_latest_books_route, opds_v2_libraries_latest_series_route,
    opds_v2_libraries_on_deck_route, opds_v2_libraries_readlists_route, opds_v2_libraries_route,
    opds_v2_library_browse_route, opds_v2_library_collections_route,
    opds_v2_library_keep_reading_route, opds_v2_library_latest_books_route,
    opds_v2_library_latest_series_route, opds_v2_library_on_deck_route,
    opds_v2_library_readlists_route, opds_v2_library_route, opds_v2_readlist_route,
    opds_v2_search_route, opds_v2_series_route,
};
