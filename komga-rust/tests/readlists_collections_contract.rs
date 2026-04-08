use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use sqlx::Row;
use tower::util::ServiceExt;
use zip::CompressionMethod;

#[path = "support/runtime_router_contract_support.rs"]
pub mod runtime_router_contract_support;

use runtime_router_contract_support::*;

#[path = "readlists_collections_contract/support.rs"]
mod support;

use support::*;

#[path = "readlists_collections_contract/comicrack_and_series_list.rs"]
mod comicrack_and_series_list;
#[path = "readlists_collections_contract/list_and_filters.rs"]
mod list_and_filters;
#[path = "readlists_collections_contract/media_assets_visibility.rs"]
mod media_assets_visibility;
#[path = "readlists_collections_contract/readlist_books_siblings_authors.rs"]
mod readlist_books_siblings_authors;
#[path = "readlists_collections_contract/thumbnails.rs"]
mod thumbnails;

#[test]
fn readlists_collections_contract_target_is_registered() {
    assert_required_target_declared("readlists/collections", "readlists_collections_contract");
}
