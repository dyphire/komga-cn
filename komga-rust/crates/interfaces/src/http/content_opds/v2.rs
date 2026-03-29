use super::*;

#[path = "v2/catalog_browse.rs"]
mod catalog_browse;
#[path = "v2/library_entities.rs"]
mod library_entities;

pub(crate) use self::catalog_browse::{
    opds_catalog, opds_v2_libraries, opds_v2_libraries_browse, opds_v2_libraries_keep_reading,
    opds_v2_libraries_latest_books, opds_v2_libraries_latest_series, opds_v2_libraries_on_deck,
    opds_v2_library, opds_v2_library_browse, opds_v2_library_keep_reading,
    opds_v2_library_latest_books, opds_v2_library_latest_series, opds_v2_library_on_deck,
};
pub(crate) use self::library_entities::{
    opds_v2_book_thumbnail_small, opds_v2_collection, opds_v2_libraries_collections,
    opds_v2_libraries_readlists, opds_v2_library_collections, opds_v2_library_readlists,
    opds_v2_readlist, opds_v2_search, opds_v2_series,
};
