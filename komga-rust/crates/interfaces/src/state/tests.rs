#![allow(unused_variables)]

use std::collections::BTreeSet;

use super::*;
#[derive(Default)]
pub(crate) struct NoopOperationalRuntimeService;
#[async_trait]
impl OperationalRuntimeService for NoopOperationalRuntimeService {
    async fn load_task_execution_values(
        &self,
        tasks_db_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String> {
        panic!("unused test service")
    }
    async fn load_libraries_count(&self, database_file: PathBuf) -> Result<f64, String> {
        panic!("unused test service")
    }
    async fn load_series_grouped_by_library(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String> {
        panic!("unused test service")
    }
    async fn load_books_grouped_by_library(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String> {
        panic!("unused test service")
    }
    async fn load_books_filesize_grouped_by_library(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String> {
        panic!("unused test service")
    }
    async fn load_sidecars_grouped_by_library(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String> {
        panic!("unused test service")
    }
    async fn load_collections_count(&self, database_file: PathBuf) -> Result<f64, String> {
        panic!("unused test service")
    }
    async fn load_readlists_count(&self, database_file: PathBuf) -> Result<f64, String> {
        panic!("unused test service")
    }
    async fn load_task_failure_count(&self, database_file: PathBuf) -> Result<f64, String> {
        panic!("unused test service")
    }
    async fn load_sqlite_pool_snapshots(
        &self,
        paths: Vec<PathBuf>,
    ) -> Result<Vec<SqlitePoolSnapshot>, String> {
        panic!("unused test service")
    }
}

#[derive(Default)]
pub(crate) struct NoopOperationalSettingsService;
#[async_trait]
impl OperationalSettingsService for NoopOperationalSettingsService {
    async fn load_announcement_read_ids(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<Vec<String>, sqlx::Error> {
        panic!("unused test service")
    }
    async fn save_announcements_read(
        &self,
        database_file: PathBuf,
        user_id: String,
        ids: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_claim_status(&self, database_file: PathBuf) -> Result<bool, sqlx::Error> {
        panic!("unused test service")
    }
    async fn claim_initial_admin_user(
        &self,
        database_file: PathBuf,
        user_id: String,
        email: String,
        password_hash: String,
    ) -> Result<ClaimInitialAdminUserResult, sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_client_settings_global(
        &self,
        database_file: PathBuf,
        allow_unauthorized_only: bool,
    ) -> Result<Value, sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_client_settings_user(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<Value, sqlx::Error> {
        panic!("unused test service")
    }
    async fn upsert_client_settings_global(
        &self,
        database_file: PathBuf,
        settings: Vec<(String, String, bool)>,
    ) -> Result<(), sqlx::Error> {
        panic!("unused test service")
    }
    async fn upsert_client_settings_user(
        &self,
        database_file: PathBuf,
        user_id: String,
        settings: Vec<(String, String)>,
    ) -> Result<(), sqlx::Error> {
        panic!("unused test service")
    }
    async fn delete_client_settings_global(
        &self,
        database_file: PathBuf,
        keys: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        panic!("unused test service")
    }
    async fn delete_client_settings_user(
        &self,
        database_file: PathBuf,
        user_id: String,
        keys: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        panic!("unused test service")
    }
    fn list_directory_entries(&self, path: PathBuf, directories_only: bool) -> Vec<Value> {
        panic!("unused test service")
    }
    fn list_font_families(&self, path: PathBuf) -> Vec<String> {
        panic!("unused test service")
    }
    fn load_font_family_css(&self, path: PathBuf, family: String) -> Option<String> {
        panic!("unused test service")
    }
    fn load_font_file(&self, path: PathBuf, family: String, file: String) -> Option<Vec<u8>> {
        panic!("unused test service")
    }
    async fn delete_syncpoints_by_user(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<(), sqlx::Error> {
        panic!("unused test service")
    }
    async fn delete_syncpoints_by_user_and_key_ids(
        &self,
        database_file: PathBuf,
        user_id: String,
        key_ids: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_history_page(
        &self,
        database_file: PathBuf,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_page_hash_matches_page(
        &self,
        database_file: PathBuf,
        page_hash: String,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_page_hash_thumbnail(
        &self,
        database_file: PathBuf,
        page_hash: String,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_unknown_page_hash_thumbnail(
        &self,
        database_file: PathBuf,
        page_hash: String,
        resize_to: Option<u32>,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_page_hashes_page(
        &self,
        database_file: PathBuf,
        page: u64,
        size: u64,
        actions: Vec<String>,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_page_hashes_unknown_page(
        &self,
        database_file: PathBuf,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_page_hash_delete_targets(
        &self,
        database_file: PathBuf,
        hash: String,
    ) -> Result<Vec<PageHashDeleteTarget>, sqlx::Error> {
        panic!("unused test service")
    }
    async fn upsert_page_hash(
        &self,
        database_file: PathBuf,
        hash: String,
        size: Option<i64>,
        action: String,
    ) -> Result<(), sqlx::Error> {
        panic!("unused test service")
    }
    fn analyze_transient_book(&self, path: String) -> TransientBookAnalysis {
        panic!("unused test service")
    }
    async fn infer_transient_series_and_number(
        &self,
        database_file: PathBuf,
        transient_name: String,
    ) -> (Option<String>, Option<f64>) {
        panic!("unused test service")
    }
    fn list_transient_book_entries(&self, root: PathBuf) -> Vec<Value> {
        panic!("unused test service")
    }
    async fn validate_transient_scan_root(
        &self,
        database_file: PathBuf,
        path: String,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    fn load_transient_book_file_metadata(&self, path: String) -> Option<TransientBookFileMetadata> {
        panic!("unused test service")
    }
    fn load_transient_book_media(&self, path: String) -> Option<Vec<u8>> {
        panic!("unused test service")
    }
    fn transient_book_content_type(&self, path: String, media_type: String) -> &'static str {
        panic!("unused test service")
    }
    fn transient_book_page_content(
        &self,
        path: String,
        media_type: String,
        pages: Vec<TransientBookPage>,
        page_number: u32,
    ) -> Option<(String, Vec<u8>)> {
        panic!("unused test service")
    }
}

#[derive(Default)]
pub(crate) struct NoopMediaAssetsService;
#[async_trait]
impl MediaAssetsService for NoopMediaAssetsService {
    fn media_import_service(&self, database_file: PathBuf) -> Box<dyn RuntimeMediaImportService> {
        panic!("unused test service")
    }
    fn book_metadata_service(&self, database_file: PathBuf) -> Box<dyn RuntimeBookMetadataService> {
        panic!("unused test service")
    }
    async fn refresh_book_search_documents_after_metadata_update(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        book_id: String,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn persist_book_page_hashes_with_media_content(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    fn decode_epub_positions(&self, blob: Vec<u8>) -> Result<Vec<Value>, String> {
        panic!("unused test service")
    }
    async fn load_epub_archive_positions(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
    ) -> Option<Vec<Value>> {
        panic!("unused test service")
    }
    async fn read_media_file_bytes(&self, path: PathBuf) -> Option<Vec<u8>> {
        panic!("unused test service")
    }
    async fn read_media_file_size(&self, path: PathBuf) -> Option<i64> {
        panic!("unused test service")
    }
    async fn load_epub_cover_bytes(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
    ) -> Option<(Vec<u8>, String)> {
        panic!("unused test service")
    }
    async fn load_persisted_book_media(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<komga_application::media_assets::BookMediaRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_media_files(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_media_file_records(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Vec<PersistedMediaFileRecord>, String> {
        panic!("unused test service")
    }
    async fn book_media_is_ready_status(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_pages(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Vec<komga_application::media_assets::BookPageRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_page_row(
        &self,
        database_file: PathBuf,
        book_id: String,
        page_number: u64,
    ) -> Result<Option<komga_application::media_assets::BookPageRecord>, String> {
        panic!("unused test service")
    }
    async fn resolve_book_page_bytes(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
        page: komga_application::media_assets::BookPageRecord,
        page_number: u64,
    ) -> Option<Vec<u8>> {
        panic!("unused test service")
    }
    async fn render_book_page_thumbnail(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
        page: komga_application::media_assets::BookPageRecord,
        page_number: u64,
        max_edge: u32,
    ) -> Option<Vec<u8>> {
        panic!("unused test service")
    }
    async fn load_archive_page_row(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
        page_number: u64,
    ) -> Option<komga_application::media_assets::BookPageRecord> {
        panic!("unused test service")
    }
    async fn load_archive_page_rows(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
    ) -> Option<Vec<komga_application::media_assets::BookPageRecord>> {
        panic!("unused test service")
    }
    fn load_pdf_page_row(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
        page_number: u64,
    ) -> Option<komga_application::media_assets::BookPageRecord> {
        panic!("unused test service")
    }
    fn load_generated_pdf_page_rows(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
    ) -> Vec<komga_application::media_assets::BookPageRecord> {
        panic!("unused test service")
    }
    fn read_pdf_page_as_single_page_pdf(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
        page_number: u64,
    ) -> Option<Vec<u8>> {
        panic!("unused test service")
    }
    fn detect_pdf_page_count(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
    ) -> Option<u64> {
        panic!("unused test service")
    }
    async fn load_persisted_epub_extension_blob(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<(String, Vec<u8>)>, String> {
        panic!("unused test service")
    }
    async fn load_series_book_ids(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn refresh_series_read_progress_row(
        &self,
        database_file: PathBuf,
        series_id: String,
        user_id: String,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn delete_series_read_progress_row(
        &self,
        database_file: PathBuf,
        series_id: String,
        user_id: String,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn load_series_tachiyomi_progress(
        &self,
        database_file: PathBuf,
        series_id: String,
        user_id: String,
    ) -> Result<Option<Value>, String> {
        panic!("unused test service")
    }
    async fn load_book_progression(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: String,
    ) -> Result<Option<Value>, String> {
        panic!("unused test service")
    }
    async fn persist_read_progress(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: String,
        page: u64,
        completed: bool,
        locator: Option<Value>,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn delete_persisted_read_progress(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: String,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn readlist_tachiyomi_counters(
        &self,
        database_file: PathBuf,
        ordered_book_ids: Vec<String>,
        user_id: String,
    ) -> Result<(u64, u64, u64, u64, u64), String> {
        panic!("unused test service")
    }
    async fn persist_readlist_tachiyomi_progress(
        &self,
        database_file: PathBuf,
        ordered_book_ids: Vec<String>,
        user_id: String,
        last_book_read: usize,
    ) -> Result<Option<()>, String> {
        panic!("unused test service")
    }
    async fn load_selected_book_thumbnail(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
        panic!("unused test service")
    }
    async fn load_book_thumbnail_by_id(
        &self,
        database_file: PathBuf,
        thumbnail_id: String,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_thumbnails(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Vec<komga_application::media_assets::EntityThumbnailRecord>, String> {
        panic!("unused test service")
    }
    async fn insert_book_thumbnail(
        &self,
        database_file: PathBuf,
        book_id: String,
        thumbnail: Vec<u8>,
        media_type: String,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<komga_application::media_assets::EntityThumbnailRecord, String> {
        panic!("unused test service")
    }
    async fn select_book_thumbnail(
        &self,
        database_file: PathBuf,
        thumbnail_id: String,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_book_thumbnail(
        &self,
        database_file: PathBuf,
        book_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn load_persisted_readlist_thumbnails(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Vec<komga_application::media_assets::ReadlistThumbnailRecord>, String> {
        panic!("unused test service")
    }
    async fn insert_readlist_thumbnail(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        thumbnail: Vec<u8>,
        media_type: String,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<komga_application::media_assets::ReadlistThumbnailRecord, String> {
        panic!("unused test service")
    }
    async fn select_readlist_thumbnail(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_readlist_thumbnail(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn load_persisted_collection_thumbnails(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Vec<komga_application::media_assets::CollectionThumbnailRecord>, String> {
        panic!("unused test service")
    }
    async fn insert_collection_thumbnail(
        &self,
        database_file: PathBuf,
        collection_id: String,
        thumbnail: Vec<u8>,
        media_type: String,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<komga_application::media_assets::CollectionThumbnailRecord, String> {
        panic!("unused test service")
    }
    async fn select_collection_thumbnail(
        &self,
        database_file: PathBuf,
        thumbnail_id: String,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_collection_thumbnail(
        &self,
        database_file: PathBuf,
        collection_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn load_selected_series_thumbnail(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_thumbnails(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<komga_application::media_assets::SeriesThumbnailRecord>, String> {
        panic!("unused test service")
    }
    async fn load_series_thumbnail_by_id(
        &self,
        database_file: PathBuf,
        thumbnail_id: String,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
        panic!("unused test service")
    }
    async fn insert_series_thumbnail(
        &self,
        database_file: PathBuf,
        series_id: String,
        thumbnail: Vec<u8>,
        media_type: String,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<komga_application::media_assets::SeriesThumbnailRecord, String> {
        panic!("unused test service")
    }
    async fn select_series_thumbnail(
        &self,
        database_file: PathBuf,
        series_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_series_thumbnail(
        &self,
        database_file: PathBuf,
        series_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn load_persisted_readlist_name(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Option<String>, String> {
        panic!("unused test service")
    }
    async fn load_book_restrictions(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<(Option<u16>, Vec<String>)>, String> {
        panic!("unused test service")
    }
    async fn load_readlist_archive_entries(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Vec<(String, PathBuf)>, String> {
        panic!("unused test service")
    }
    async fn load_series_archive_entries(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<(String, String, Vec<(String, PathBuf)>)>, String> {
        panic!("unused test service")
    }
    fn is_font_resource(&self, resource_name: String) -> bool {
        panic!("unused test service")
    }
    async fn read_epub_resource_bytes(
        &self,
        epub_path: PathBuf,
        resource_name: String,
    ) -> Option<Vec<u8>> {
        panic!("unused test service")
    }
    async fn load_persisted_manifest_book(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<(String, String, String)>, String> {
        panic!("unused test service")
    }
    async fn persisted_book_exists(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn persisted_book_ids(&self, database_file: PathBuf) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn persisted_series_exists(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_oneshot(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<bool>, String> {
        panic!("unused test service")
    }
    async fn persisted_readlist_exists(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn persisted_collection_exists(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn load_series_book_number_sorts(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<(String, f64)>, String> {
        panic!("unused test service")
    }
    async fn load_book_page_count(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<u64>, String> {
        panic!("unused test service")
    }
    async fn persist_book_progression(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: String,
        page: f64,
        use_locator_position_for_page: bool,
        modified: Option<String>,
        device_id: Option<String>,
        device_name: Option<String>,
        locator: Option<Value>,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
}

#[derive(Default)]
pub(crate) struct NoopDiscoveryDetailService;
#[async_trait]
impl DiscoveryDetailService for NoopDiscoveryDetailService {
    async fn load_book_id_by_sorted_position(
        &self,
        database_file: PathBuf,
        index: usize,
    ) -> Result<Option<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_resource(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<PersistedBookResourceRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_detail(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: Option<String>,
    ) -> Result<Option<PersistedBookDetailRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_sibling_id(
        &self,
        database_file: PathBuf,
        book_id: String,
        direction: PersistedBookSiblingDirectionRecord,
    ) -> Result<Option<String>, String> {
        panic!("unused test service")
    }
    async fn persisted_collections_exist(&self, database_file: PathBuf) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn load_persisted_collections(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedCollectionAccessRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_collection_series_ids(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_collection_detail(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Option<PersistedCollectionAccessRecord>, String> {
        panic!("unused test service")
    }
    async fn load_series_library_id(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<String>, String> {
        panic!("unused test service")
    }
    async fn load_series_restrictions(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<PersistedSeriesRestrictionRecord, String> {
        panic!("unused test service")
    }
    async fn persist_collection_create(
        &self,
        database_file: PathBuf,
        collection_id: String,
        name: String,
        ordered: bool,
        series_ids: Vec<String>,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn persist_collection_update(
        &self,
        database_file: PathBuf,
        collection_id: String,
        name: String,
        ordered: bool,
        series_ids: Vec<String>,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_persisted_collection(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn upsert_collection_search_document(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        collection_id: String,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_collection_search_document(
        &self,
        index_dir: PathBuf,
        collection_id: String,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn load_persisted_readlists(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<DiscoveryPersistedReadlistRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_readlist_detail(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Option<DiscoveryPersistedReadlistRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_readlist_book_rows(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Vec<DiscoveryPersistedReadlistBookRecord>, String> {
        panic!("unused test service")
    }
    async fn load_comicrack_match_candidates(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_authors(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Vec<PersistedBookAuthorRecord>, String> {
        panic!("unused test service")
    }
    async fn persist_readlist_create(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        name: String,
        summary: String,
        ordered: bool,
        book_ids: Vec<String>,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn persist_readlist_update(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        name: String,
        summary: String,
        ordered: bool,
        book_ids: Vec<String>,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_persisted_readlist(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn upsert_readlist_search_document(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        readlist_id: String,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_readlist_search_document(
        &self,
        index_dir: PathBuf,
        readlist_id: String,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_resource(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<PersistedSeriesResourceRecord>, String> {
        panic!("unused test service")
    }
    async fn load_series_id_by_sorted_position(
        &self,
        database_file: PathBuf,
        index: usize,
    ) -> Result<Option<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_detail(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<PersistedSeriesDetailRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_summaries(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<SeriesSummaryRecord>, String> {
        panic!("unused test service")
    }
    async fn load_series_total_book_counts(
        &self,
        database_file: PathBuf,
    ) -> Result<HashMap<String, i64>, String> {
        panic!("unused test service")
    }
    async fn load_series_read_progress_counts(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<HashMap<String, (i64, i64)>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_collections(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<PersistedSeriesCollectionRecord>, String> {
        panic!("unused test service")
    }
    async fn load_existing_series_metadata(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<ExistingSeriesMetadataRecord>, String> {
        panic!("unused test service")
    }
    async fn persist_series_metadata_update(
        &self,
        database_file: PathBuf,
        series_id: String,
        update: SeriesMetadataUpdateRecord,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn refresh_series_search_documents_after_metadata_update(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        series_id: String,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
}

#[derive(Default)]
pub(crate) struct NoopOpdsCatalogService;
#[async_trait]
impl OpdsCatalogService for NoopOpdsCatalogService {
    async fn load_browse_series_navigation_entries(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        library_id: Option<String>,
        publishers: Vec<String>,
        page: usize,
        size: usize,
    ) -> Result<(Vec<BrowseSeriesNavigationEntry>, usize), String> {
        panic!("unused test service")
    }
    async fn load_browse_publisher_entries(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        library_id: Option<String>,
    ) -> Result<Vec<BrowsePublisherEntry>, String> {
        panic!("unused test service")
    }
    async fn load_keep_reading_books(
        &self,
        database_file: PathBuf,
        user_id: String,
        library_id: Option<String>,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        panic!("unused test service")
    }
    async fn load_on_deck_books(
        &self,
        database_file: PathBuf,
        user_id: String,
        library_id: Option<String>,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        panic!("unused test service")
    }
    async fn load_latest_books(
        &self,
        database_file: PathBuf,
        library_id: Option<String>,
        limit: i64,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        panic!("unused test service")
    }
    async fn load_latest_books_paged(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        user_id: Option<String>,
        library_id: Option<String>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        panic!("unused test service")
    }
    async fn load_latest_series(
        &self,
        database_file: PathBuf,
        library_id: Option<String>,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        panic!("unused test service")
    }
    async fn load_latest_series_paged(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        library_id: Option<String>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        panic!("unused test service")
    }
    async fn load_library_series(
        &self,
        database_file: PathBuf,
        library_id: String,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        panic!("unused test service")
    }
    async fn load_series_page(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
        search: Option<String>,
        publishers: Vec<String>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        panic!("unused test service")
    }
    async fn load_all_readlists(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<OpdsReadlistEntry>, String> {
        panic!("unused test service")
    }
}

#[derive(Default)]
pub(crate) struct NoopOpdsPersistedService;
#[async_trait]
impl OpdsPersistedService for NoopOpdsPersistedService {
    async fn load_libraries(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedLibraryRecord>, String> {
        panic!("unused test service")
    }
    async fn load_library(
        &self,
        database_file: PathBuf,
        library_id: String,
    ) -> Result<Option<PersistedLibraryRecord>, String> {
        panic!("unused test service")
    }
    async fn load_readlists_for_library(
        &self,
        database_file: PathBuf,
        library_id: String,
    ) -> Result<Vec<PersistedReadlistRecord>, String> {
        panic!("unused test service")
    }
    async fn load_series(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<PersistedSeriesRecord>, String> {
        panic!("unused test service")
    }
    async fn load_series_books_paged(
        &self,
        database_file: PathBuf,
        series_id: String,
        user_id: String,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<PersistedSeriesBookRecord>, String> {
        panic!("unused test service")
    }
    async fn load_series_tags(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_readlist(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Option<PersistedReadlistRecord>, String> {
        panic!("unused test service")
    }
    async fn load_readlist_books(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Vec<PersistedReadlistBookRecord>, String> {
        panic!("unused test service")
    }
    async fn load_unified_search_results(
        &self,
        database_file: PathBuf,
        query: String,
    ) -> Result<
        (
            Vec<PersistedSeriesSearchRecord>,
            Vec<PersistedBookSearchRecord>,
            Vec<PersistedNamedRecord>,
            Vec<PersistedNamedRecord>,
        ),
        String,
    > {
        panic!("unused test service")
    }
    async fn load_publishers(
        &self,
        database_file: PathBuf,
        allowed_library_ids: Option<HashSet<String>>,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_collections(
        &self,
        database_file: PathBuf,
        library_id: Option<String>,
    ) -> Result<Vec<PersistedNamedRecord>, String> {
        panic!("unused test service")
    }
    async fn load_collection(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Option<PersistedNamedRecord>, String> {
        panic!("unused test service")
    }
    async fn load_collection_books(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Vec<PersistedBookFeedRecord>, String> {
        panic!("unused test service")
    }
    async fn load_collection_series(
        &self,
        database_file: PathBuf,
        collection_id: String,
        ordered: bool,
    ) -> Result<Vec<PersistedSeriesRecord>, String> {
        panic!("unused test service")
    }
}

#[derive(Default)]
pub(crate) struct NoopPersistedDiscoveryService;
#[async_trait]
impl PersistedDiscoveryService for NoopPersistedDiscoveryService {
    async fn load_persisted_author_names(
        &self,
        database_file: PathBuf,
        search: String,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_author_roles(
        &self,
        database_file: PathBuf,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_authors_by_scope(
        &self,
        database_file: PathBuf,
        scope: PersistedAuthorsScope,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<PersistedAuthorEntry>, String> {
        panic!("unused test service")
    }
    async fn load_book_poster_summaries(
        &self,
        database_file: PathBuf,
    ) -> Result<HashMap<String, Vec<PersistedBookPosterSummary>>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_summaries(
        &self,
        database_file: PathBuf,
        user_id: Option<String>,
    ) -> Result<Vec<PersistedBookSummary>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_summaries_by_ids(
        &self,
        database_file: PathBuf,
        user_id: Option<String>,
        ids: Vec<String>,
    ) -> Result<Vec<PersistedBookSummary>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_count(&self, database_file: PathBuf) -> Result<usize, String> {
        panic!("unused test service")
    }
    async fn load_persisted_genres(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_tags(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_languages(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_publishers(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_age_ratings(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_sharing_labels(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_release_dates(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_tags(
        &self,
        database_file: PathBuf,
        library_ids: Option<Vec<String>>,
        collection_id: Option<String>,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_library_ids(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_collection_memberships(
        &self,
        database_file: PathBuf,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
        panic!("unused test service")
    }
    async fn load_collection_ordering(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<HashMap<String, i64>, String> {
        panic!("unused test service")
    }
    async fn load_readlist_memberships(
        &self,
        database_file: PathBuf,
    ) -> Result<BTreeMap<String, BTreeSet<String>>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_ondeck_books(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<Vec<PersistedBookBrowseEntry>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_duplicate_books(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedBookBrowseEntry>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_tags(
        &self,
        database_file: PathBuf,
        scope: Option<PersistedBookTagsScope>,
        authorized_library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn persisted_utc_date_minus_days(
        &self,
        database_file: PathBuf,
        days: i64,
    ) -> Result<Option<String>, String> {
        panic!("unused test service")
    }
    async fn load_series_read_progress_counts(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<HashMap<String, (i64, i64)>, String> {
        panic!("unused test service")
    }
    async fn load_series_read_dates(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<HashMap<String, String>, String> {
        panic!("unused test service")
    }
    async fn load_series_total_book_counts(
        &self,
        database_file: PathBuf,
    ) -> Result<HashMap<String, i64>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_summaries(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<PersistedSeriesSummary>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_summaries_by_ids(
        &self,
        database_file: PathBuf,
        ids: Vec<String>,
    ) -> Result<Vec<PersistedSeriesSummary>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_count(&self, database_file: PathBuf) -> Result<usize, String> {
        panic!("unused test service")
    }
    async fn persisted_series_exist(&self, database_file: PathBuf) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn search_book_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn search_collection_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn search_readlist_scored_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String> {
        panic!("unused test service")
    }
    async fn search_series_scored_ids(
        &self,
        database_file: PathBuf,
        query: String,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String> {
        panic!("unused test service")
    }
}
