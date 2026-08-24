use crate::support::sqlite::connect_test_pool;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_config::profile::RuntimeMode;
use komga_infrastructure::media::generate_book_thumbnail;
use serde_json::{Value, json};
use sqlx::Row;
use std::fs::File;
use std::io::Write;
use tower::util::ServiceExt;
use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

mod support;

use support::fixture::TestFixture;
use support::runtime_router_contract_support::{
    RuntimeDbPaths, external_service_support::*, media_file_fixtures::*,
    metadata_series_seeding::*, response_helpers::*, user_auth::*,
};

mod books_media_contract_cases;
