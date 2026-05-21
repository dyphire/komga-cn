use std::path::Path;

use async_trait::async_trait;
use serde_json::Value;

use crate::media_assets::{PageHashDeleteTarget, PageHashThumbnail};

// Record types migrated from infrastructure

#[derive(Clone, Debug)]
pub struct CreatedClaimedUser {
    pub id: String,
    pub email: String,
}

#[derive(Clone, Debug)]
pub enum ClaimInitialAdminUserResult {
    Created(CreatedClaimedUser),
    AlreadyClaimed,
}

#[derive(Clone, Debug)]
pub struct TransientBookFileMetadata {
    pub file_last_modified_unix_nanos: i128,
    pub size_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct TransientBookAnalysis {
    pub status: String,
    pub media_type: String,
    pub page_count: u32,
    pub pages: Vec<TransientBookPage>,
    pub files: Vec<String>,
    pub comment: String,
    pub number: Option<f64>,
    pub series_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TransientBookPage {
    pub number: u32,
    pub file_name: String,
    pub media_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size_bytes: Option<u64>,
}

/// Port for operational settings: announcements, claims, client settings,
/// filesystem browsing, fonts, syncpoints, history, page hashes, and transient books.
#[async_trait]
pub trait OperationalSettingsPort: Send + Sync {
    // Announcements
    async fn load_announcement_read_ids(&self, user_id: &str) -> Result<Vec<String>, String>;
    async fn save_announcements_read(&self, user_id: &str, ids: &[String]) -> Result<(), String>;

    // Claims
    async fn load_claim_status(&self) -> Result<bool, String>;
    async fn claim_initial_admin_user(
        &self,
        user_id: &str,
        email: &str,
        password_hash: &str,
    ) -> Result<ClaimInitialAdminUserResult, String>;

    // Client settings
    async fn load_client_settings_global(
        &self,
        allow_unauthorized_only: bool,
    ) -> Result<Value, String>;
    async fn load_client_settings_user(&self, user_id: &str) -> Result<Value, String>;
    async fn upsert_client_settings_global(
        &self,
        settings: &[(String, String, bool)],
    ) -> Result<(), String>;
    async fn upsert_client_settings_user(
        &self,
        user_id: &str,
        settings: &[(String, String)],
    ) -> Result<(), String>;
    async fn delete_client_settings_global(&self, keys: &[String]) -> Result<(), String>;
    async fn delete_client_settings_user(
        &self,
        user_id: &str,
        keys: &[String],
    ) -> Result<(), String>;

    // Filesystem browsing
    fn list_directory_entries(&self, path: &Path, directories_only: bool) -> Vec<Value>;

    // Fonts
    fn list_font_families(&self, path: &Path) -> Vec<String>;
    fn load_font_family_css(&self, path: &Path, family: &str) -> Option<String>;
    fn load_font_file(&self, path: &Path, family: &str, file: &str) -> Option<Vec<u8>>;

    // Syncpoints
    async fn delete_syncpoints_by_user(&self, user_id: &str) -> Result<(), String>;
    async fn delete_syncpoints_by_user_and_key_ids(
        &self,
        user_id: &str,
        key_ids: &[String],
    ) -> Result<(), String>;

    // History
    async fn load_history_page(
        &self,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, String>;

    // Page hashes
    async fn load_page_hash_matches_page(
        &self,
        page_hash: &str,
        page: u64,
        size: u64,
        sorts: &[String],
    ) -> Result<Value, String>;
    async fn load_page_hash_thumbnail(
        &self,
        page_hash: &str,
    ) -> Result<Option<PageHashThumbnail>, String>;
    async fn load_unknown_page_hash_thumbnail(
        &self,
        page_hash: &str,
        resize_to: Option<u32>,
    ) -> Result<Option<PageHashThumbnail>, String>;
    async fn load_page_hashes_page(
        &self,
        page: u64,
        size: u64,
        actions: &[String],
        sorts: &[String],
    ) -> Result<Value, String>;
    async fn load_page_hashes_unknown_page(
        &self,
        page: u64,
        size: u64,
        sorts: &[String],
    ) -> Result<Value, String>;
    async fn load_page_hash_delete_targets(
        &self,
        hash: &str,
    ) -> Result<Vec<PageHashDeleteTarget>, String>;
    async fn upsert_page_hash(
        &self,
        hash: &str,
        size: Option<i64>,
        action: &str,
    ) -> Result<(), String>;

    // Transient books
    fn analyze_transient_book(&self, path: &str) -> TransientBookAnalysis;
    async fn infer_transient_series_and_number(
        &self,
        transient_name: &str,
    ) -> (Option<String>, Option<f64>);
    fn list_transient_book_entries(&self, root: &Path) -> Vec<Value>;
    async fn validate_transient_scan_root(&self, path: &str) -> Result<(), String>;
    fn load_transient_book_file_metadata(&self, path: &str) -> Option<TransientBookFileMetadata>;
    fn load_transient_book_media(&self, path: &str) -> Option<Vec<u8>>;
    fn transient_book_content_type(&self, path: &str, media_type: &str) -> &'static str;
    fn transient_book_page_content(
        &self,
        path: &str,
        media_type: &str,
        pages: &[TransientBookPage],
        page_number: u32,
    ) -> Option<(String, Vec<u8>)>;
}
