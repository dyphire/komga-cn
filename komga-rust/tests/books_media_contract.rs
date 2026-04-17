use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_application::media_assets::BookMediaRecord;
use komga_config::profile::RuntimeMode;
use komga_infrastructure::filesystem::media_access::epub::load_epub_cover_bytes;
use komga_infrastructure::metadata::generate_book_thumbnail;
use komga_infrastructure::sqlite::connect_pool;
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

mod support {
    pub mod persistence_contract_fixture;

    pub mod runtime_router_contract_support {
        use super::persistence_contract_fixture;

        pub(crate) use super::persistence_contract_fixture::RuntimeDbPaths;

        pub mod books_media_fixture_bootstrap;
        pub mod books_media_media_file_fixtures;
        pub mod books_media_metadata_series_seeding;
        pub mod contract_seed;
        pub mod response_helpers;
        pub mod user_auth;
    }
}

use support::runtime_router_contract_support::{
    RuntimeDbPaths,
    books_media_fixture_bootstrap::*,
    books_media_media_file_fixtures::*,
    books_media_metadata_series_seeding::*,
    contract_seed::*,
    response_helpers::*,
    user_auth::*,
};

mod books_media_contract_cases;

#[test]
fn books_media_contract_target_is_registered() {
    assert_required_target_declared("books/media", "books_media_contract");
}
