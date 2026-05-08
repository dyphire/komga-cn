use super::*;
use axum::extract::FromRef;

use komga_application::media_assets::{PageHashDeleteTarget, PageHashThumbnail};
use std::path::Path;

#[derive(Clone)]
pub struct OperationalApiState {
    pub root: Arc<HttpAppState>,
    pub profile: RuntimeProfile,
    pub auth_db: AuthDatabaseState,
    pub operational: OperationalState,
    pub identity: IdentityState,
    pub task_queue: TaskQueueState,
    pub server_settings: Arc<dyn ServerSettingsService>,
    pub operational_runtime: Arc<dyn OperationalRuntimeService>,
    pub operational_settings: Arc<dyn OperationalSettingsService>,
}

impl FromRef<Arc<HttpAppState>> for OperationalApiState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            root: app.clone(),
            profile: app.profile,
            auth_db: app.auth_db.clone(),
            operational: app.operational.clone(),
            identity: IdentityState::from_ref(app),
            task_queue: TaskQueueState::from_ref(app),
            server_settings: app.services.server_settings.clone(),
            operational_runtime: app.services.operational_runtime.clone(),
            operational_settings: app.services.operational_settings.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ServerSettingsState {
    pub identity: IdentityState,
    pub runtime: RuntimeState,
    pub server_settings: Arc<dyn ServerSettingsService>,
    pub task_queue: TaskQueueState,
}

impl FromRef<Arc<HttpAppState>> for ServerSettingsState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            identity: IdentityState::from_ref(app),
            runtime: app.operational.runtime.clone(),
            server_settings: app.services.server_settings.clone(),
            task_queue: TaskQueueState::from_ref(app),
        }
    }
}

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
        paths: &[PathBuf],
    ) -> Result<Vec<SqlitePoolSnapshot>, String>;
}

#[async_trait]
pub trait OperationalSettingsService: Send + Sync {
    async fn load_announcement_read_ids(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error>;
    async fn save_announcements_read(
        &self,
        user_id: &str,
        ids: &[String],
    ) -> Result<(), sqlx::Error>;
    async fn load_claim_status(&self) -> Result<bool, sqlx::Error>;
    async fn claim_initial_admin_user(
        &self,
        user_id: &str,
        email: &str,
        password_hash: &str,
    ) -> Result<ClaimInitialAdminUserResult, sqlx::Error>;
    async fn load_client_settings_global(
        &self,
        allow_unauthorized_only: bool,
    ) -> Result<Value, sqlx::Error>;
    async fn load_client_settings_user(&self, user_id: &str) -> Result<Value, sqlx::Error>;
    async fn upsert_client_settings_global(
        &self,
        settings: &[(String, String, bool)],
    ) -> Result<(), sqlx::Error>;
    async fn upsert_client_settings_user(
        &self,
        user_id: &str,
        settings: &[(String, String)],
    ) -> Result<(), sqlx::Error>;
    async fn delete_client_settings_global(&self, keys: &[String]) -> Result<(), sqlx::Error>;
    async fn delete_client_settings_user(
        &self,
        user_id: &str,
        keys: &[String],
    ) -> Result<(), sqlx::Error>;
    fn list_directory_entries(&self, path: &Path, directories_only: bool) -> Vec<Value>;
    fn list_font_families(&self, path: &Path) -> Vec<String>;
    fn load_font_family_css(&self, path: &Path, family: &str) -> Option<String>;
    fn load_font_file(&self, path: &Path, family: &str, file: &str) -> Option<Vec<u8>>;
    async fn delete_syncpoints_by_user(&self, user_id: &str) -> Result<(), sqlx::Error>;
    async fn delete_syncpoints_by_user_and_key_ids(
        &self,
        user_id: &str,
        key_ids: &[String],
    ) -> Result<(), sqlx::Error>;
    async fn load_history_page(
        &self,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error>;
    async fn load_page_hash_matches_page(
        &self,
        page_hash: &str,
        page: u64,
        size: u64,
        sorts: &[String],
    ) -> Result<Value, sqlx::Error>;
    async fn load_page_hash_thumbnail(
        &self,
        page_hash: &str,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error>;
    async fn load_unknown_page_hash_thumbnail(
        &self,
        page_hash: &str,
        resize_to: Option<u32>,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error>;
    async fn load_page_hashes_page(
        &self,
        page: u64,
        size: u64,
        actions: &[String],
        sorts: &[String],
    ) -> Result<Value, sqlx::Error>;
    async fn load_page_hashes_unknown_page(
        &self,
        page: u64,
        size: u64,
        sorts: &[String],
    ) -> Result<Value, sqlx::Error>;
    async fn load_page_hash_delete_targets(
        &self,
        hash: &str,
    ) -> Result<Vec<PageHashDeleteTarget>, sqlx::Error>;
    async fn upsert_page_hash(
        &self,
        hash: &str,
        size: Option<i64>,
        action: &str,
    ) -> Result<(), sqlx::Error>;
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
