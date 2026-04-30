use super::*;

use komga_application::media_assets::{PageHashDeleteTarget, PageHashThumbnail};

#[derive(Clone)]
pub struct TransientBookPage {
    pub number: u32,
    pub file_name: String,
    pub media_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size_bytes: Option<u64>,
}

#[derive(Clone)]
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

#[derive(Clone)]
pub struct TransientBookFileMetadata {
    pub file_last_modified_unix_nanos: i128,
    pub size_bytes: u64,
}

pub enum ClaimInitialAdminUserResult {
    Created(Box<AuthUser>),
    AlreadyClaimed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SqlitePoolSnapshot {
    pub path: PathBuf,
    pub max_connections: u32,
    pub min_connections: u32,
    pub total_connections: u32,
    pub idle_connections: u32,
    pub in_use_connections: u32,
    pub is_closed: bool,
}

#[async_trait]
pub trait OperationalRuntimeService: Send + Sync {
    async fn load_task_execution_values(&self) -> Result<Vec<(String, f64)>, String>;
    async fn load_libraries_count(&self) -> Result<f64, String>;
    async fn load_series_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String>;
    async fn load_books_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String>;
    async fn load_books_filesize_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String>;
    async fn load_sidecars_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String>;
    async fn load_collections_count(&self) -> Result<f64, String>;
    async fn load_readlists_count(&self) -> Result<f64, String>;
    async fn load_task_failure_count(&self) -> Result<f64, String>;
    async fn load_sqlite_pool_snapshots(
        &self,
        paths: Vec<PathBuf>,
    ) -> Result<Vec<SqlitePoolSnapshot>, String>;
}

#[async_trait]
pub trait OperationalSettingsService: Send + Sync {
    async fn load_announcement_read_ids(&self, user_id: String)
    -> Result<Vec<String>, sqlx::Error>;
    async fn save_announcements_read(
        &self,
        user_id: String,
        ids: Vec<String>,
    ) -> Result<(), sqlx::Error>;
    async fn load_claim_status(&self) -> Result<bool, sqlx::Error>;
    async fn claim_initial_admin_user(
        &self,
        user_id: String,
        email: String,
        password_hash: String,
    ) -> Result<ClaimInitialAdminUserResult, sqlx::Error>;
    async fn load_client_settings_global(
        &self,
        allow_unauthorized_only: bool,
    ) -> Result<Value, sqlx::Error>;
    async fn load_client_settings_user(&self, user_id: String) -> Result<Value, sqlx::Error>;
    async fn upsert_client_settings_global(
        &self,
        settings: Vec<(String, String, bool)>,
    ) -> Result<(), sqlx::Error>;
    async fn upsert_client_settings_user(
        &self,
        user_id: String,
        settings: Vec<(String, String)>,
    ) -> Result<(), sqlx::Error>;
    async fn delete_client_settings_global(&self, keys: Vec<String>) -> Result<(), sqlx::Error>;
    async fn delete_client_settings_user(
        &self,
        user_id: String,
        keys: Vec<String>,
    ) -> Result<(), sqlx::Error>;
    fn list_directory_entries(&self, path: PathBuf, directories_only: bool) -> Vec<Value>;
    fn list_font_families(&self, path: PathBuf) -> Vec<String>;
    fn load_font_family_css(&self, path: PathBuf, family: String) -> Option<String>;
    fn load_font_file(&self, path: PathBuf, family: String, file: String) -> Option<Vec<u8>>;
    async fn delete_syncpoints_by_user(&self, user_id: String) -> Result<(), sqlx::Error>;
    async fn delete_syncpoints_by_user_and_key_ids(
        &self,
        user_id: String,
        key_ids: Vec<String>,
    ) -> Result<(), sqlx::Error>;
    async fn load_history_page(
        &self,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error>;
    async fn load_page_hash_matches_page(
        &self,
        page_hash: String,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error>;
    async fn load_page_hash_thumbnail(
        &self,
        page_hash: String,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error>;
    async fn load_unknown_page_hash_thumbnail(
        &self,
        page_hash: String,
        resize_to: Option<u32>,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error>;
    async fn load_page_hashes_page(
        &self,
        page: u64,
        size: u64,
        actions: Vec<String>,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error>;
    async fn load_page_hashes_unknown_page(
        &self,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error>;
    async fn load_page_hash_delete_targets(
        &self,
        hash: String,
    ) -> Result<Vec<PageHashDeleteTarget>, sqlx::Error>;
    async fn upsert_page_hash(
        &self,
        hash: String,
        size: Option<i64>,
        action: String,
    ) -> Result<(), sqlx::Error>;
    fn analyze_transient_book(&self, path: String) -> TransientBookAnalysis;
    async fn infer_transient_series_and_number(
        &self,
        transient_name: String,
    ) -> (Option<String>, Option<f64>);
    fn list_transient_book_entries(&self, root: PathBuf) -> Vec<Value>;
    async fn validate_transient_scan_root(&self, path: String) -> Result<(), String>;
    fn load_transient_book_file_metadata(&self, path: String) -> Option<TransientBookFileMetadata>;
    fn load_transient_book_media(&self, path: String) -> Option<Vec<u8>>;
    fn transient_book_content_type(&self, path: String, media_type: String) -> &'static str;
    fn transient_book_page_content(
        &self,
        path: String,
        media_type: String,
        pages: Vec<TransientBookPage>,
        page_number: u32,
    ) -> Option<(String, Vec<u8>)>;
}
