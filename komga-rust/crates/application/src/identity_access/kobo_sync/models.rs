use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::super::user_models::AuthUser;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KoboStoreSyncMergeResult {
    pub events: Vec<Value>,
    pub raw_sync_token: Option<String>,
    pub should_continue: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KoboLibrarySyncRequest {
    pub user: AuthUser,
    pub current_api_key_id: Option<String>,
    pub sync_token: Option<String>,
    pub store_sync_enabled: bool,
    pub forwarded_headers: Vec<(String, String)>,
    pub query: Option<String>,
    pub base_url: String,
    pub auth_token: String,
    pub limit: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KoboSyncPageRequest {
    pub user: AuthUser,
    pub current_api_key_id: Option<String>,
    pub ongoing_sync_point_id: Option<String>,
    pub last_successful_sync_point_id: Option<String>,
    pub limit: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KoboLibrarySyncResponse {
    pub events: Vec<Value>,
    pub sync_token_payload: String,
    pub should_continue: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KoboLibrarySyncPayload {
    pub events: Vec<Value>,
    pub encoded_sync_token: String,
    pub should_continue: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KoboSyncBookSnapshot {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub release_date: Option<String>,
    pub language: String,
    pub file_size: u64,
    pub page_count: u64,
    pub created: String,
    pub last_modified: String,
    pub contributor_names: Vec<String>,
    pub isbn: Option<String>,
    pub publisher_name: Option<String>,
    pub cover_image_id: Option<String>,
    pub series_id: Option<String>,
    pub series_name: Option<String>,
    pub series_number: Option<String>,
    pub series_number_float: Option<f64>,
    pub oneshot: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct KoboSyncReadProgressSnapshot {
    pub page: i64,
    pub completed: bool,
    pub created: String,
    pub last_modified: String,
    pub locator: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct KoboSyncReadListSnapshot {
    pub id: String,
    pub name: String,
    pub created: String,
    pub last_modified: String,
    pub items: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KoboSyncPointBook {
    pub book_id: String,
    pub created: String,
    pub file_last_modified: String,
    pub file_size: u64,
    pub file_hash: String,
    pub metadata_last_modified: String,
    pub read_progress_last_modified: Option<String>,
    pub cover_image_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KoboSyncPage {
    pub to_sync_point_id: String,
    pub from_sync_point_id: Option<String>,
    pub books_added: Vec<KoboSyncPointBook>,
    pub books_changed: Vec<KoboSyncPointBook>,
    pub books_removed: Vec<KoboSyncPointBook>,
    pub books_read_progress_changed: Vec<KoboSyncPointBook>,
    pub readlists_added: Vec<KoboSyncReadListSnapshot>,
    pub readlists_changed: Vec<KoboSyncReadListSnapshot>,
    pub readlists_removed: Vec<KoboSyncReadListSnapshot>,
    pub should_continue: bool,
}

pub const KOBO_SYNC_ITEM_LIMIT: usize = 200;
