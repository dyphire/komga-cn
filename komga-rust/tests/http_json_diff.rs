use komga_compat_testkit::cases::{CaseConfig, HarnessConfig};
use komga_compat_testkit::diff_writer::{compare_responses, write_diff_report};
use komga_compat_testkit::normalize::{normalize_headers, normalize_json_body, normalize_xml_body};
use komga_compat_testkit::runtime::{apply_setup_steps, resolve_headers};
use komga_compat_testkit::{ComparisonMode, NormalizedBody, NormalizedResponse};
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[path = "http_json_diff/config.rs"]
mod config;
#[path = "http_json_diff/diff.rs"]
mod diff;
#[path = "http_json_diff/headers.rs"]
mod headers;
#[path = "http_json_diff/helpers.rs"]
mod helpers;

use helpers::{
    execute_case, execute_case_async, live_http_json_case_ids,
    phase11_readlists_blank_search_all_case_ids,
    phase11_readlists_blank_search_negative_case_ids,
    phase11_readlists_blank_search_owned_case_ids,
    phase10_readlists_search_all_case_ids, phase10_readlists_search_negative_case_ids,
    phase10_readlists_search_owned_case_ids,
    phase8_readlist_books_family_all_case_ids, phase8_readlist_books_family_negative_case_ids,
    phase8_readlist_books_family_owned_case_ids,
    phase9_readlists_list_browse_all_case_ids,
    phase9_readlists_list_browse_negative_case_ids,
    phase9_readlists_list_browse_owned_case_ids,
    seeded_localdb_binary_manifest_smoke_harness_config, seeded_localdb_session_vars,
    seeded_localdb_smoke_harness_config, smoke_harness_config, temp_output_root,
};

#[test]
fn phase6_readlist_detail_case_inventory_loads() {
    config::phase6_readlist_detail_case_inventory_loads();
}

#[test]
fn phase7_series_oneshot_case_inventory_loads() {
    config::phase7_series_oneshot_case_inventory_loads();
}

#[test]
fn phase8_readlist_books_family_case_inventory_loads() {
    config::phase8_readlist_books_family_case_inventory_loads();
}

#[test]
fn readlists_list_browse_case_inventory_loads() {
    config::readlists_list_browse_case_inventory_loads();
}

#[test]
fn phase10_readlists_search_case_inventory_loads() {
    config::phase10_readlists_search_case_inventory_loads();
}

#[test]
fn phase11_readlists_blank_search_case_inventory_loads() {
    config::phase11_readlists_blank_search_case_inventory_loads();
}
