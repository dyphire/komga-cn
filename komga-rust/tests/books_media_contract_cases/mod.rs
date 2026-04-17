use super::*;

mod support;
use support::{
    books_list_ids, fixture_epub_positions_extension_blob,
    fixture_epub_positions_extension_blob_fixed_layout_single_position,
    fixture_epub_positions_extension_blob_total_progression_021,
    fixture_epub_positions_extension_blob_total_progression_0995, seed_router_persisted_pdf_page,
    update_book_search_fixture_title, write_router_epub_with_cover,
};

mod authors_and_list_basics;
mod discovery_additional_filters;
mod discovery_numeric_filters;
mod discovery_profile_and_string_filters;
mod discovery_release_date_filters;
mod file_page_resource_routes;
mod kobo_koreader_detail_metadata_readlists;
mod manifests;
mod ondeck;
mod positions_and_pdf_pages;
mod progression;
mod read_progress;
mod search_parity;
mod thumbnails_and_generated;
