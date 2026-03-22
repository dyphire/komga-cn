use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::LazyLock;
use tower::ServiceExt;

mod compat;

#[path = "minimal_http_surface/auth.rs"]
mod auth;
#[path = "minimal_http_surface/books.rs"]
mod books;
#[path = "minimal_http_surface/helpers.rs"]
mod helpers;
#[path = "minimal_http_surface/libraries.rs"]
mod libraries;
#[path = "minimal_http_surface/opds.rs"]
mod opds;
#[path = "minimal_http_surface/series.rs"]
mod series;

use helpers::{
    JAVA_LIVE_BASE_URL_ENV_LOCK, assert_java_live_manifest_for_host, assert_opds_auth_for_host,
    assert_opds_catalog_challenge_for_host, assert_series_and_books_urls,
    expected_opds_snapshot, expected_snapshot, libraries_json_for_token,
};
use compat::http::session_token_for_basic_auth;
