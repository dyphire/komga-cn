use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::application::media_assets::BookMediaRecord;
use komga_rust::config::RuntimeMode;
use komga_rust::infrastructure::filesystem::load_epub_cover_bytes;
use komga_rust::infrastructure::metadata::generate_book_thumbnail;
use komga_rust::infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use sha2::{Digest, Sha512};
use sqlx::Row;
use std::fs::File;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tower::util::ServiceExt;
use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

#[path = "support/runtime_router_contract_support.rs"]
pub mod runtime_router_contract_support;

use runtime_router_contract_support::*;

#[path = "books_media_contract/support.rs"]
mod support;

use support::{
    books_list_ids, fixture_epub_positions_extension_blob,
    fixture_epub_positions_extension_blob_fixed_layout_single_position,
    fixture_epub_positions_extension_blob_total_progression_021,
    fixture_epub_positions_extension_blob_total_progression_0995, kobo_proxy_env_lock,
    restore_env_var, seed_kobo_sync_api_key, seed_router_cbz_book, seed_router_persisted_pdf_page,
    spawn_single_response_server, update_book_search_fixture_title, upsert_server_setting,
    write_router_epub_with_cover,
};

#[path = "books_media_contract/authors_and_list_basics.rs"]
mod authors_and_list_basics;
#[path = "books_media_contract/discovery_additional_filters.rs"]
mod discovery_additional_filters;
#[path = "books_media_contract/discovery_numeric_filters.rs"]
mod discovery_numeric_filters;
#[path = "books_media_contract/discovery_profile_and_string_filters.rs"]
mod discovery_profile_and_string_filters;
#[path = "books_media_contract/discovery_release_date_filters.rs"]
mod discovery_release_date_filters;
#[path = "books_media_contract/file_page_resource_routes.rs"]
mod file_page_resource_routes;
#[path = "books_media_contract/kobo_koreader_detail_metadata_readlists.rs"]
mod kobo_koreader_detail_metadata_readlists;
#[path = "books_media_contract/manifests.rs"]
mod manifests;
#[path = "books_media_contract/ondeck.rs"]
mod ondeck;
#[path = "books_media_contract/positions_and_pdf_pages.rs"]
mod positions_and_pdf_pages;
#[path = "books_media_contract/progression.rs"]
mod progression;
#[path = "books_media_contract/read_progress.rs"]
mod read_progress;
#[path = "books_media_contract/search_parity.rs"]
mod search_parity;
#[path = "books_media_contract/thumbnails_and_generated.rs"]
mod thumbnails_and_generated;

#[test]
fn books_media_contract_target_is_registered() {
    assert_required_target_declared("books/media", "books_media_contract");
}
