use super::*;
use axum::extract::FromRef;
use komga_application::media_assets::{
    BookMediaRecord, BookMetadataPatch, BookPageRecord, BooksImportPayload, EntityThumbnailBinary,
    SeriesThumbnailRecord,
};
use komga_application::task_processing::TaskQueueRecord;
use std::path::Path;

#[derive(Clone)]
pub struct MediaAssetsState {
    pub root: Arc<HttpAppState>,
    pub profile: RuntimeProfile,
    pub read_progress: ReadProgressState,
    pub discovery_auth: DiscoveryAuthState,
    pub auth_db: AuthDatabaseState,
    pub operational: OperationalState,
    pub identity: IdentityState,
    pub media_assets: Arc<dyn MediaAssetsService>,
    pub server_settings: Arc<dyn ServerSettingsService>,
    pub task_queue: TaskQueueState,
    pub discovery_detail: Arc<dyn DiscoveryDetailService>,
}

impl FromRef<Arc<HttpAppState>> for MediaAssetsState {
    fn from_ref(app: &Arc<HttpAppState>) -> Self {
        Self {
            root: app.clone(),
            profile: app.profile,
            read_progress: app.read_progress.clone(),
            discovery_auth: app.discovery_auth.clone(),
            auth_db: app.auth_db.clone(),
            operational: app.operational.clone(),
            identity: IdentityState::from_ref(app),
            media_assets: app.services.media_assets.clone(),
            server_settings: app.services.server_settings.clone(),
            task_queue: TaskQueueState::from_ref(app),
            discovery_detail: app.services.discovery_detail.clone(),
        }
    }
}

pub trait RuntimeMediaImportService: Send + Sync {
    fn enqueue_books(
        &self,
        payload: BooksImportPayload,
        next_task_id: &mut dyn FnMut() -> String,
    ) -> Result<Vec<TaskQueueRecord>, String>;

    fn process_queued_book_payload<'a>(
        &'a self,
        task_payload: &'a str,
        import_priority: i32,
    ) -> futures_util::future::BoxFuture<'a, Result<Vec<TaskQueueRecord>, String>>;
}

pub trait RuntimeBookMetadataService: Send + Sync {
    fn update_book_metadata<'a>(
        &'a self,
        book_id: &'a str,
        patch: &'a BookMetadataPatch,
    ) -> futures_util::future::BoxFuture<'a, Result<Option<Option<String>>, String>>;

    fn batch_update_book_metadata<'a>(
        &'a self,
        updates: Vec<(String, BookMetadataPatch)>,
    ) -> futures_util::future::BoxFuture<'a, Result<Vec<String>, String>>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedMediaFileRecord {
    pub file_name: String,
    pub media_type: String,
    pub sub_type: Option<String>,
}

#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait MediaAssetsService: Send + Sync {
    fn media_import_service(&self) -> Box<dyn RuntimeMediaImportService>;
    fn book_metadata_service(&self) -> Box<dyn RuntimeBookMetadataService>;
    async fn refresh_book_search_documents_after_metadata_update(
        &self,
        index_dir: &Path,
        book_id: &str,
    ) -> Result<(), String>;
    async fn persist_book_page_hashes_with_media_content(
        &self,
        book_id: &str,
    ) -> Result<(), String>;
    fn decode_epub_positions(&self, blob: &[u8]) -> Result<Vec<Value>, String>;
    async fn load_epub_archive_positions(&self, media: &BookMediaRecord) -> Option<Vec<Value>>;
    async fn read_media_file_bytes(&self, path: &Path) -> Option<Vec<u8>>;
    async fn read_media_file_size(&self, path: &Path) -> Option<i64>;
    async fn load_epub_cover_bytes(&self, media: &BookMediaRecord) -> Option<(Vec<u8>, String)>;
    async fn load_persisted_book_media(
        &self,
        book_id: &str,
    ) -> Result<Option<BookMediaRecord>, String>;
    async fn load_persisted_book_media_files(&self, book_id: &str) -> Result<Vec<String>, String>;
    async fn load_persisted_media_file_records(
        &self,
        book_id: &str,
    ) -> Result<Vec<PersistedMediaFileRecord>, String>;
    async fn book_media_is_ready_status(&self, book_id: &str) -> Result<bool, String>;
    async fn load_persisted_book_pages(&self, book_id: &str)
    -> Result<Vec<BookPageRecord>, String>;
    async fn load_persisted_book_page_row(
        &self,
        book_id: &str,
        page_number: u64,
    ) -> Result<Option<BookPageRecord>, String>;
    async fn resolve_book_page_bytes(
        &self,
        media: &BookMediaRecord,
        page: &BookPageRecord,
        page_number: u64,
    ) -> Option<Vec<u8>>;
    async fn render_book_page_thumbnail(
        &self,
        media: &BookMediaRecord,
        page: &BookPageRecord,
        page_number: u64,
        max_edge: u32,
    ) -> Option<Vec<u8>>;
    async fn load_archive_page_row(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> Option<BookPageRecord>;
    async fn load_archive_page_rows(&self, media: &BookMediaRecord) -> Option<Vec<BookPageRecord>>;
    fn load_pdf_page_row(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> Option<BookPageRecord>;
    fn load_generated_pdf_page_rows(&self, media: &BookMediaRecord) -> Vec<BookPageRecord>;
    fn read_pdf_page_as_single_page_pdf(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> Option<Vec<u8>>;
    fn detect_pdf_page_count(&self, media: &BookMediaRecord) -> Option<u64>;
    async fn load_persisted_epub_extension_blob(
        &self,
        book_id: &str,
    ) -> Result<Option<(String, Vec<u8>)>, String>;
    async fn load_series_book_ids(&self, series_id: &str) -> Result<Vec<String>, String>;
    async fn refresh_series_read_progress_row(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<(), String>;
    async fn delete_series_read_progress_row(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<(), String>;
    async fn load_series_tachiyomi_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<Option<Value>, String>;
    async fn load_book_progression(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<Value>, String>;
    async fn persist_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
        page: u64,
        completed: bool,
        locator: Option<Value>,
    ) -> Result<(), String>;
    async fn delete_persisted_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<(), String>;
    async fn readlist_tachiyomi_counters(
        &self,
        ordered_book_ids: &[String],
        user_id: &str,
    ) -> Result<(u64, u64, u64, u64, u64), String>;
    async fn persist_readlist_tachiyomi_progress(
        &self,
        ordered_book_ids: &[String],
        user_id: &str,
        last_book_read: usize,
    ) -> Result<Option<()>, String>;
    async fn load_selected_book_thumbnail(
        &self,
        book_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String>;
    async fn load_book_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String>;
    async fn load_persisted_book_thumbnails(
        &self,
        book_id: &str,
    ) -> Result<Vec<komga_application::media_assets::EntityThumbnailRecord>, String>;
    async fn insert_book_thumbnail(
        &self,
        book_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<komga_application::media_assets::EntityThumbnailRecord, String>;
    async fn select_book_thumbnail(&self, thumbnail_id: &str) -> Result<bool, String>;
    async fn delete_book_thumbnail(&self, thumbnail_id: &str) -> Result<bool, String>;
    async fn load_persisted_readlist_thumbnails(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<komga_application::media_assets::ReadlistThumbnailRecord>, String>;
    async fn insert_readlist_thumbnail(
        &self,
        readlist_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<komga_application::media_assets::ReadlistThumbnailRecord, String>;
    async fn select_readlist_thumbnail(
        &self,
        readlist_id: &str,
        thumbnail_id: &str,
    ) -> Result<bool, String>;
    async fn delete_readlist_thumbnail(
        &self,
        readlist_id: &str,
        thumbnail_id: &str,
    ) -> Result<bool, String>;
    async fn load_persisted_collection_thumbnails(
        &self,
        collection_id: &str,
    ) -> Result<Vec<komga_application::media_assets::CollectionThumbnailRecord>, String>;
    async fn insert_collection_thumbnail(
        &self,
        collection_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<komga_application::media_assets::CollectionThumbnailRecord, String>;
    async fn select_collection_thumbnail(&self, thumbnail_id: &str) -> Result<bool, String>;
    async fn delete_collection_thumbnail(
        &self,
        collection_id: &str,
        thumbnail_id: &str,
    ) -> Result<bool, String>;
    async fn load_selected_series_thumbnail(
        &self,
        series_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String>;
    async fn load_persisted_series_thumbnails(
        &self,
        series_id: &str,
    ) -> Result<Vec<SeriesThumbnailRecord>, String>;
    async fn load_series_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String>;
    async fn insert_series_thumbnail(
        &self,
        series_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<SeriesThumbnailRecord, String>;
    async fn select_series_thumbnail(
        &self,
        series_id: &str,
        thumbnail_id: &str,
    ) -> Result<bool, String>;
    async fn delete_series_thumbnail(
        &self,
        series_id: &str,
        thumbnail_id: &str,
    ) -> Result<bool, String>;
    async fn load_persisted_readlist_name(
        &self,
        readlist_id: &str,
    ) -> Result<Option<String>, String>;
    async fn load_book_restrictions(
        &self,
        book_id: &str,
    ) -> Result<Option<(Option<u16>, Vec<String>)>, String>;
    async fn load_readlist_archive_entries(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<(String, PathBuf)>, String>;
    async fn load_series_archive_entries(
        &self,
        series_id: &str,
    ) -> Result<Option<(String, String, Vec<(String, PathBuf)>)>, String>;
    fn is_font_resource(&self, resource_name: &str) -> bool;
    async fn read_epub_resource_bytes(
        &self,
        epub_path: &Path,
        resource_name: &str,
    ) -> Option<Vec<u8>>;
    async fn load_persisted_manifest_book(
        &self,
        book_id: &str,
    ) -> Result<Option<(String, String, String)>, String>;
    async fn persisted_book_exists(&self, book_id: &str) -> Result<bool, String>;
    async fn persisted_book_ids(&self) -> Result<Vec<String>, String>;
    async fn persisted_series_exists(&self, series_id: &str) -> Result<bool, String>;
    async fn load_persisted_series_oneshot(&self, series_id: &str) -> Result<Option<bool>, String>;
    async fn persisted_readlist_exists(&self, readlist_id: &str) -> Result<bool, String>;
    async fn persisted_collection_exists(&self, collection_id: &str) -> Result<bool, String>;
    async fn load_series_book_number_sorts(
        &self,
        series_id: &str,
    ) -> Result<Vec<(String, f64)>, String>;
    async fn load_book_page_count(&self, book_id: &str) -> Result<Option<u64>, String>;
    async fn persist_book_progression(
        &self,
        book_id: &str,
        user_id: &str,
        page: f64,
        use_locator_position_for_page: bool,
        modified: Option<&str>,
        device_id: Option<&str>,
        device_name: Option<&str>,
        locator: Option<Value>,
    ) -> Result<(), String>;
}
