#![allow(unused_variables)]

use super::*;
use komga_application::discovery::{
    BookTagScope, BooksBrowseQuery, BooksFeedQuery, DiscoveryListService, SeriesBrowseQuery,
};
use komga_application::media_assets::{PageHashDeleteTarget, PageHashThumbnail};
use komga_domain::discovery::PageEnvelope;
use std::path::Path;
#[derive(Default)]
pub(crate) struct NoopOperationalRuntimeService;
#[async_trait]
impl OperationalRuntimeService for NoopOperationalRuntimeService {
    async fn load_task_execution_values(&self) -> Result<Vec<(String, f64)>, String> {
        panic!("unused test service")
    }
    async fn load_libraries_count(&self) -> Result<f64, String> {
        panic!("unused test service")
    }
    async fn load_series_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String> {
        panic!("unused test service")
    }
    async fn load_books_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String> {
        panic!("unused test service")
    }
    async fn load_books_filesize_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String> {
        panic!("unused test service")
    }
    async fn load_sidecars_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String> {
        panic!("unused test service")
    }
    async fn load_collections_count(&self) -> Result<f64, String> {
        panic!("unused test service")
    }
    async fn load_readlists_count(&self) -> Result<f64, String> {
        panic!("unused test service")
    }
    async fn load_task_failure_count(&self) -> Result<f64, String> {
        panic!("unused test service")
    }
    async fn load_sqlite_pool_snapshots(
        &self,
        paths: &[PathBuf],
    ) -> Result<Vec<SqlitePoolSnapshot>, String> {
        panic!("unused test service")
    }
}

#[derive(Default)]
pub(crate) struct NoopOperationalSettingsService;
#[async_trait]
impl OperationalSettingsService for NoopOperationalSettingsService {
    async fn load_announcement_read_ids(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        panic!("unused test service")
    }
    async fn save_announcements_read(
        &self,
        user_id: &str,
        ids: &[String],
    ) -> Result<(), sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_claim_status(&self) -> Result<bool, sqlx::Error> {
        panic!("unused test service")
    }
    async fn claim_initial_admin_user(
        &self,
        user_id: &str,
        email: &str,
        password_hash: &str,
    ) -> Result<ClaimInitialAdminUserResult, sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_client_settings_global(
        &self,
        allow_unauthorized_only: bool,
    ) -> Result<Value, sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_client_settings_user(&self, user_id: &str) -> Result<Value, sqlx::Error> {
        panic!("unused test service")
    }
    async fn upsert_client_settings_global(
        &self,
        settings: &[(String, String, bool)],
    ) -> Result<(), sqlx::Error> {
        panic!("unused test service")
    }
    async fn upsert_client_settings_user(
        &self,
        user_id: &str,
        settings: &[(String, String)],
    ) -> Result<(), sqlx::Error> {
        panic!("unused test service")
    }
    async fn delete_client_settings_global(&self, keys: &[String]) -> Result<(), sqlx::Error> {
        panic!("unused test service")
    }
    async fn delete_client_settings_user(
        &self,
        user_id: &str,
        keys: &[String],
    ) -> Result<(), sqlx::Error> {
        panic!("unused test service")
    }
    fn list_directory_entries(&self, path: &Path, directories_only: bool) -> Vec<Value> {
        panic!("unused test service")
    }
    fn list_font_families(&self, path: &Path) -> Vec<String> {
        panic!("unused test service")
    }
    fn load_font_family_css(&self, path: &Path, family: &str) -> Option<String> {
        panic!("unused test service")
    }
    fn load_font_file(&self, path: &Path, family: &str, file: &str) -> Option<Vec<u8>> {
        panic!("unused test service")
    }
    async fn delete_syncpoints_by_user(&self, user_id: &str) -> Result<(), sqlx::Error> {
        panic!("unused test service")
    }
    async fn delete_syncpoints_by_user_and_key_ids(
        &self,
        user_id: &str,
        key_ids: &[String],
    ) -> Result<(), sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_history_page(
        &self,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_page_hash_matches_page(
        &self,
        page_hash: &str,
        page: u64,
        size: u64,
        sorts: &[String],
    ) -> Result<Value, sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_page_hash_thumbnail(
        &self,
        page_hash: &str,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_unknown_page_hash_thumbnail(
        &self,
        page_hash: &str,
        resize_to: Option<u32>,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_page_hashes_page(
        &self,
        page: u64,
        size: u64,
        actions: &[String],
        sorts: &[String],
    ) -> Result<Value, sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_page_hashes_unknown_page(
        &self,
        page: u64,
        size: u64,
        sorts: &[String],
    ) -> Result<Value, sqlx::Error> {
        panic!("unused test service")
    }
    async fn load_page_hash_delete_targets(
        &self,
        hash: &str,
    ) -> Result<Vec<PageHashDeleteTarget>, sqlx::Error> {
        panic!("unused test service")
    }
    async fn upsert_page_hash(
        &self,
        hash: &str,
        size: Option<i64>,
        action: &str,
    ) -> Result<(), sqlx::Error> {
        panic!("unused test service")
    }
    fn analyze_transient_book(&self, path: &str) -> TransientBookAnalysis {
        panic!("unused test service")
    }
    async fn infer_transient_series_and_number(
        &self,
        transient_name: &str,
    ) -> (Option<String>, Option<f64>) {
        panic!("unused test service")
    }
    fn list_transient_book_entries(&self, root: &Path) -> Vec<Value> {
        panic!("unused test service")
    }
    async fn validate_transient_scan_root(&self, path: &str) -> Result<(), String> {
        panic!("unused test service")
    }
    fn load_transient_book_file_metadata(&self, path: &str) -> Option<TransientBookFileMetadata> {
        panic!("unused test service")
    }
    fn load_transient_book_media(&self, path: &str) -> Option<Vec<u8>> {
        panic!("unused test service")
    }
    fn transient_book_content_type(&self, path: &str, media_type: &str) -> &'static str {
        panic!("unused test service")
    }
    fn transient_book_page_content(
        &self,
        path: &str,
        media_type: &str,
        pages: &[TransientBookPage],
        page_number: u32,
    ) -> Option<(String, Vec<u8>)> {
        panic!("unused test service")
    }
}

#[derive(Default)]
pub(crate) struct NoopMediaAssetsService;
#[async_trait]
impl MediaAssetsService for NoopMediaAssetsService {
    fn media_import_service(&self) -> Box<dyn RuntimeMediaImportService> {
        panic!("unused test service")
    }
    fn book_metadata_service(&self) -> Box<dyn RuntimeBookMetadataService> {
        panic!("unused test service")
    }
    async fn refresh_book_search_documents_after_metadata_update(
        &self,
        index_dir: &Path,
        book_id: &str,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn persist_book_page_hashes_with_media_content(
        &self,
        book_id: &str,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    fn decode_epub_positions(&self, blob: &[u8]) -> Result<Vec<Value>, String> {
        panic!("unused test service")
    }
    async fn load_epub_archive_positions(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
    ) -> Option<Vec<Value>> {
        panic!("unused test service")
    }
    async fn read_media_file_bytes(&self, path: &Path) -> Option<Vec<u8>> {
        panic!("unused test service")
    }
    async fn read_media_file_size(&self, path: &Path) -> Option<i64> {
        panic!("unused test service")
    }
    async fn load_epub_cover_bytes(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
    ) -> Option<(Vec<u8>, String)> {
        panic!("unused test service")
    }
    async fn load_persisted_book_media(
        &self,
        book_id: &str,
    ) -> Result<Option<komga_application::media_assets::BookMediaRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_media_files(&self, book_id: &str) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_media_file_records(
        &self,
        book_id: &str,
    ) -> Result<Vec<PersistedMediaFileRecord>, String> {
        panic!("unused test service")
    }
    async fn book_media_is_ready_status(&self, book_id: &str) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_pages(
        &self,
        book_id: &str,
    ) -> Result<Vec<komga_application::media_assets::BookPageRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_page_row(
        &self,
        book_id: &str,
        page_number: u64,
    ) -> Result<Option<komga_application::media_assets::BookPageRecord>, String> {
        panic!("unused test service")
    }
    async fn resolve_book_page_bytes(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
        page: &komga_application::media_assets::BookPageRecord,
        page_number: u64,
    ) -> Option<Vec<u8>> {
        panic!("unused test service")
    }
    async fn render_book_page_thumbnail(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
        page: &komga_application::media_assets::BookPageRecord,
        page_number: u64,
        max_edge: u32,
    ) -> Option<Vec<u8>> {
        panic!("unused test service")
    }
    async fn load_archive_page_row(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
        page_number: u64,
    ) -> Option<komga_application::media_assets::BookPageRecord> {
        panic!("unused test service")
    }
    async fn load_archive_page_rows(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
    ) -> Option<Vec<komga_application::media_assets::BookPageRecord>> {
        panic!("unused test service")
    }
    fn load_pdf_page_row(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
        page_number: u64,
    ) -> Option<komga_application::media_assets::BookPageRecord> {
        panic!("unused test service")
    }
    fn load_generated_pdf_page_rows(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
    ) -> Vec<komga_application::media_assets::BookPageRecord> {
        panic!("unused test service")
    }
    fn read_pdf_page_as_single_page_pdf(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
        page_number: u64,
    ) -> Option<Vec<u8>> {
        panic!("unused test service")
    }
    fn detect_pdf_page_count(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
    ) -> Option<u64> {
        panic!("unused test service")
    }
    async fn load_persisted_epub_extension_blob(
        &self,
        book_id: &str,
    ) -> Result<Option<(String, Vec<u8>)>, String> {
        panic!("unused test service")
    }
    async fn load_series_book_ids(&self, series_id: &str) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn refresh_series_read_progress_row(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn delete_series_read_progress_row(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn load_series_tachiyomi_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<Option<Value>, String> {
        panic!("unused test service")
    }
    async fn load_book_progression(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<Value>, String> {
        panic!("unused test service")
    }
    async fn persist_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
        page: u64,
        completed: bool,
        locator: Option<Value>,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn delete_persisted_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn readlist_tachiyomi_counters(
        &self,
        ordered_book_ids: &[String],
        user_id: &str,
    ) -> Result<(u64, u64, u64, u64, u64), String> {
        panic!("unused test service")
    }
    async fn persist_readlist_tachiyomi_progress(
        &self,
        ordered_book_ids: &[String],
        user_id: &str,
        last_book_read: usize,
    ) -> Result<Option<()>, String> {
        panic!("unused test service")
    }
    async fn load_selected_book_thumbnail(
        &self,
        book_id: &str,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
        panic!("unused test service")
    }
    async fn load_book_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_thumbnails(
        &self,
        book_id: &str,
    ) -> Result<Vec<komga_application::media_assets::EntityThumbnailRecord>, String> {
        panic!("unused test service")
    }
    async fn insert_book_thumbnail(
        &self,
        book_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<komga_application::media_assets::EntityThumbnailRecord, String> {
        panic!("unused test service")
    }
    async fn select_book_thumbnail(&self, thumbnail_id: &str) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_book_thumbnail(&self, thumbnail_id: &str) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn load_persisted_readlist_thumbnails(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<komga_application::media_assets::ReadlistThumbnailRecord>, String> {
        panic!("unused test service")
    }
    async fn insert_readlist_thumbnail(
        &self,
        readlist_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<komga_application::media_assets::ReadlistThumbnailRecord, String> {
        panic!("unused test service")
    }
    async fn select_readlist_thumbnail(
        &self,
        readlist_id: &str,
        thumbnail_id: &str,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_readlist_thumbnail(
        &self,
        readlist_id: &str,
        thumbnail_id: &str,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn load_persisted_collection_thumbnails(
        &self,
        collection_id: &str,
    ) -> Result<Vec<komga_application::media_assets::CollectionThumbnailRecord>, String> {
        panic!("unused test service")
    }
    async fn insert_collection_thumbnail(
        &self,
        collection_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<komga_application::media_assets::CollectionThumbnailRecord, String> {
        panic!("unused test service")
    }
    async fn select_collection_thumbnail(&self, thumbnail_id: &str) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_collection_thumbnail(
        &self,
        collection_id: &str,
        thumbnail_id: &str,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn load_selected_series_thumbnail(
        &self,
        series_id: &str,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_thumbnails(
        &self,
        series_id: &str,
    ) -> Result<Vec<komga_application::media_assets::SeriesThumbnailRecord>, String> {
        panic!("unused test service")
    }
    async fn load_series_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
        panic!("unused test service")
    }
    async fn insert_series_thumbnail(
        &self,
        series_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<komga_application::media_assets::SeriesThumbnailRecord, String> {
        panic!("unused test service")
    }
    async fn select_series_thumbnail(
        &self,
        series_id: &str,
        thumbnail_id: &str,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_series_thumbnail(
        &self,
        series_id: &str,
        thumbnail_id: &str,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn load_persisted_readlist_name(
        &self,
        readlist_id: &str,
    ) -> Result<Option<String>, String> {
        panic!("unused test service")
    }
    async fn load_book_restrictions(
        &self,
        book_id: &str,
    ) -> Result<Option<(Option<u16>, Vec<String>)>, String> {
        panic!("unused test service")
    }
    async fn load_readlist_archive_entries(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<(String, PathBuf)>, String> {
        panic!("unused test service")
    }
    async fn load_series_archive_entries(
        &self,
        series_id: &str,
    ) -> Result<Option<(String, String, Vec<(String, PathBuf)>)>, String> {
        panic!("unused test service")
    }
    fn is_font_resource(&self, resource_name: &str) -> bool {
        panic!("unused test service")
    }
    async fn read_epub_resource_bytes(
        &self,
        epub_path: &Path,
        resource_name: &str,
    ) -> Option<Vec<u8>> {
        panic!("unused test service")
    }
    async fn load_persisted_manifest_book(
        &self,
        book_id: &str,
    ) -> Result<Option<(String, String, String)>, String> {
        panic!("unused test service")
    }
    async fn persisted_book_exists(&self, book_id: &str) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn persisted_book_ids(&self) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn persisted_series_exists(&self, series_id: &str) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_oneshot(&self, series_id: &str) -> Result<Option<bool>, String> {
        panic!("unused test service")
    }
    async fn persisted_readlist_exists(&self, readlist_id: &str) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn persisted_collection_exists(&self, collection_id: &str) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn load_series_book_number_sorts(
        &self,
        series_id: &str,
    ) -> Result<Vec<(String, f64)>, String> {
        panic!("unused test service")
    }
    async fn load_book_page_count(&self, book_id: &str) -> Result<Option<u64>, String> {
        panic!("unused test service")
    }
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
        index: usize,
    ) -> Result<Option<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_resource(
        &self,
        book_id: &str,
    ) -> Result<Option<PersistedBookResourceRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_detail(
        &self,
        book_id: &str,
        user_id: Option<&str>,
    ) -> Result<Option<PersistedBookDetailRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_sibling_id(
        &self,
        book_id: &str,
        direction: PersistedBookSiblingDirectionRecord,
    ) -> Result<Option<String>, String> {
        panic!("unused test service")
    }
    async fn persisted_collections_exist(&self) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn load_persisted_collections(
        &self,
    ) -> Result<Vec<PersistedCollectionAccessRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_collection_series_ids(
        &self,
        collection_id: &str,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_collection_detail(
        &self,
        collection_id: &str,
    ) -> Result<Option<PersistedCollectionAccessRecord>, String> {
        panic!("unused test service")
    }
    async fn load_series_library_id(&self, series_id: &str) -> Result<Option<String>, String> {
        panic!("unused test service")
    }
    async fn load_series_restrictions(
        &self,
        series_id: &str,
    ) -> Result<PersistedSeriesRestrictionRecord, String> {
        panic!("unused test service")
    }
    async fn persist_collection_create(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn persist_collection_update(
        &self,
        collection_id: &str,
        name: &str,
        ordered: bool,
        series_ids: &[String],
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_persisted_collection(&self, collection_id: &str) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn upsert_collection_search_document(&self, collection_id: &str) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_collection_search_document(&self, collection_id: &str) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn load_persisted_readlists(
        &self,
    ) -> Result<Vec<DiscoveryPersistedReadlistRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_readlist_detail(
        &self,
        readlist_id: &str,
    ) -> Result<Option<DiscoveryPersistedReadlistRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_readlist_book_rows(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<DiscoveryPersistedReadlistBookRecord>, String> {
        panic!("unused test service")
    }
    async fn load_comicrack_match_candidates(
        &self,
    ) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_book_authors(
        &self,
        book_id: &str,
    ) -> Result<Vec<PersistedBookAuthorRecord>, String> {
        panic!("unused test service")
    }
    async fn persist_readlist_create(
        &self,
        readlist_id: &str,
        name: &str,
        summary: &str,
        ordered: bool,
        book_ids: &[String],
    ) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn persist_readlist_update(
        &self,
        readlist_id: &str,
        name: &str,
        summary: &str,
        ordered: bool,
        book_ids: &[String],
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_persisted_readlist(&self, readlist_id: &str) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn upsert_readlist_search_document(&self, readlist_id: &str) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn delete_readlist_search_document(&self, readlist_id: &str) -> Result<(), String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_resource(
        &self,
        series_id: &str,
    ) -> Result<Option<PersistedSeriesResourceRecord>, String> {
        panic!("unused test service")
    }
    async fn load_series_id_by_sorted_position(
        &self,
        index: usize,
    ) -> Result<Option<String>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_detail(
        &self,
        series_id: &str,
    ) -> Result<Option<PersistedSeriesDetailRecord>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_summaries(&self) -> Result<Vec<SeriesSummaryRecord>, String> {
        panic!("unused test service")
    }
    async fn load_series_total_book_counts(&self) -> Result<HashMap<String, i64>, String> {
        panic!("unused test service")
    }
    async fn load_series_read_progress_counts(
        &self,
        user_id: &str,
    ) -> Result<HashMap<String, (i64, i64)>, String> {
        panic!("unused test service")
    }
    async fn load_persisted_series_collections(
        &self,
        series_id: &str,
    ) -> Result<Vec<PersistedSeriesCollectionRecord>, String> {
        panic!("unused test service")
    }
    async fn load_existing_series_metadata(
        &self,
        series_id: &str,
    ) -> Result<Option<ExistingSeriesMetadataRecord>, String> {
        panic!("unused test service")
    }
    async fn persist_series_metadata_update(
        &self,
        series_id: &str,
        update: SeriesMetadataUpdateRecord,
    ) -> Result<bool, String> {
        panic!("unused test service")
    }
    async fn refresh_series_search_documents_after_metadata_update(
        &self,
        series_id: &str,
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
        allowed_library_ids: Option<&HashSet<String>>,
        library_id: Option<&str>,
        publishers: &[String],
        page: usize,
        size: usize,
    ) -> Result<(Vec<BrowseSeriesNavigationEntry>, usize), String> {
        panic!("unused test service")
    }
    async fn load_browse_publisher_entries(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        library_id: Option<&str>,
    ) -> Result<Vec<BrowsePublisherEntry>, String> {
        panic!("unused test service")
    }
    async fn load_keep_reading_books(
        &self,
        user_id: &str,
        library_id: Option<&str>,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        panic!("unused test service")
    }
    async fn load_on_deck_books(
        &self,
        user_id: &str,
        library_id: Option<&str>,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        panic!("unused test service")
    }
    async fn load_latest_books(
        &self,
        library_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        panic!("unused test service")
    }
    async fn load_latest_books_paged(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        user_id: Option<&str>,
        library_id: Option<&str>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsBookFeedEntry>, String> {
        panic!("unused test service")
    }
    async fn load_latest_series(
        &self,
        library_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        panic!("unused test service")
    }
    async fn load_latest_series_paged(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        library_id: Option<&str>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        panic!("unused test service")
    }
    async fn load_library_series(
        &self,
        library_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        panic!("unused test service")
    }
    async fn load_series_page(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        search: Option<&str>,
        publishers: &[String],
        offset: i64,
        limit: i64,
    ) -> Result<Vec<OpdsSeriesEntry>, String> {
        panic!("unused test service")
    }
    async fn load_all_readlists(&self) -> Result<Vec<OpdsReadlistEntry>, String> {
        panic!("unused test service")
    }
}

#[derive(Default)]
pub(crate) struct NoopOpdsPersistedService;
#[async_trait]
impl OpdsPersistedService for NoopOpdsPersistedService {
    async fn load_libraries(&self) -> Result<Vec<PersistedLibraryRecord>, String> {
        panic!("unused test service")
    }
    async fn load_library(
        &self,
        library_id: &str,
    ) -> Result<Option<PersistedLibraryRecord>, String> {
        panic!("unused test service")
    }
    async fn load_readlists_for_library(
        &self,
        library_id: &str,
    ) -> Result<Vec<PersistedReadlistRecord>, String> {
        panic!("unused test service")
    }
    async fn load_series(&self, series_id: &str) -> Result<Option<PersistedSeriesRecord>, String> {
        panic!("unused test service")
    }
    async fn load_series_books_paged(
        &self,
        series_id: &str,
        user_id: &str,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<PersistedSeriesBookRecord>, String> {
        panic!("unused test service")
    }
    async fn load_series_tags(&self, series_id: &str) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_readlist(
        &self,
        readlist_id: &str,
    ) -> Result<Option<PersistedReadlistRecord>, String> {
        panic!("unused test service")
    }
    async fn load_readlist_books(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<PersistedReadlistBookRecord>, String> {
        panic!("unused test service")
    }
    async fn load_unified_search_results(
        &self,
        query: &str,
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
        allowed_library_ids: Option<&HashSet<String>>,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
    async fn load_collections(
        &self,
        library_id: Option<&str>,
    ) -> Result<Vec<PersistedNamedRecord>, String> {
        panic!("unused test service")
    }
    async fn load_collection(
        &self,
        collection_id: &str,
    ) -> Result<Option<PersistedNamedRecord>, String> {
        panic!("unused test service")
    }
    async fn load_collection_books(
        &self,
        collection_id: &str,
    ) -> Result<Vec<PersistedBookFeedRecord>, String> {
        panic!("unused test service")
    }
    async fn load_collection_series(
        &self,
        collection_id: &str,
        ordered: bool,
    ) -> Result<Vec<PersistedSeriesRecord>, String> {
        panic!("unused test service")
    }
}

#[derive(Default)]
pub(crate) struct NoopDiscoveryAuthorService;

#[async_trait]
impl DiscoveryAuthorService for NoopDiscoveryAuthorService {
    async fn load_author_names(
        &self,
        search: &str,
        authorized_library_ids: Option<&[String]>,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }

    async fn load_author_roles(
        &self,
        authorized_library_ids: Option<&[String]>,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }

    async fn load_authors_by_scope(
        &self,
        scope: PersistedAuthorsScope,
        authorized_library_ids: Option<&[String]>,
    ) -> Result<Vec<PersistedAuthorEntry>, String> {
        panic!("unused test service")
    }
}

#[derive(Default)]
pub(crate) struct NoopDiscoveryLibraryMappingService;

#[async_trait]
impl DiscoveryLibraryMappingService for NoopDiscoveryLibraryMappingService {
    async fn load_persisted_library_ids(&self) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
}

#[derive(Default)]
pub(crate) struct NoopDiscoveryCollectionSearchService;

#[async_trait]
impl DiscoveryCollectionSearchService for NoopDiscoveryCollectionSearchService {
    async fn search_collection_ids(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<String>, String> {
        panic!("unused test service")
    }
}

#[derive(Default)]
pub(crate) struct NoopDiscoveryReadlistSearchService;

#[async_trait]
impl DiscoveryReadlistSearchService for NoopDiscoveryReadlistSearchService {
    async fn search_readlist_scored_ids(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, String> {
        panic!("unused test service")
    }
}

#[derive(Default)]
pub(crate) struct NoopDiscoveryBookFeedService;

#[async_trait]
impl DiscoveryBookFeedService for NoopDiscoveryBookFeedService {
    async fn load_ondeck_books(
        &self,
        user_id: &str,
    ) -> Result<Vec<PersistedBookBrowseEntry>, String> {
        panic!("unused test service")
    }

    async fn load_duplicate_books(&self) -> Result<Vec<PersistedBookBrowseEntry>, String> {
        panic!("unused test service")
    }
}

#[derive(Default)]
pub(crate) struct NoopDiscoveryListService;
#[async_trait]
impl DiscoveryListService for NoopDiscoveryListService {
    async fn list_series(
        &self,
        _context: &DiscoveryQueryContext,
        _query: SeriesBrowseQuery,
    ) -> Result<PageEnvelope<komga_application::discovery::SeriesReadModel>, DiscoveryError> {
        panic!("unused test service")
    }
    async fn list_books(
        &self,
        _context: &DiscoveryQueryContext,
        _query: BooksBrowseQuery,
    ) -> Result<PageEnvelope<komga_application::discovery::BookReadModel>, DiscoveryError> {
        panic!("unused test service")
    }
    async fn list_books_latest(
        &self,
        _context: &DiscoveryQueryContext,
        _query: BooksFeedQuery,
    ) -> Result<PageEnvelope<komga_application::discovery::BookReadModel>, DiscoveryError> {
        panic!("unused test service")
    }
    async fn list_series_alphabetical_groups(
        &self,
        _context: &DiscoveryQueryContext,
        _filter: komga_domain::discovery::SeriesFilter,
        _search: Option<String>,
    ) -> Result<Vec<serde_json::Value>, DiscoveryError> {
        panic!("unused test service")
    }
    async fn list_genres(
        &self,
        _context: &DiscoveryQueryContext,
        _library_ids: Option<Vec<String>>,
        _collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        panic!("unused test service")
    }
    async fn list_tags(
        &self,
        _context: &DiscoveryQueryContext,
        _library_ids: Option<Vec<String>>,
        _collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        panic!("unused test service")
    }
    async fn list_languages(
        &self,
        _context: &DiscoveryQueryContext,
        _library_ids: Option<Vec<String>>,
        _collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        panic!("unused test service")
    }
    async fn list_publishers(
        &self,
        _context: &DiscoveryQueryContext,
        _library_ids: Option<Vec<String>>,
        _collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        panic!("unused test service")
    }
    async fn list_age_ratings(
        &self,
        _context: &DiscoveryQueryContext,
        _library_ids: Option<Vec<String>>,
        _collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        panic!("unused test service")
    }
    async fn list_sharing_labels(
        &self,
        _context: &DiscoveryQueryContext,
        _library_ids: Option<Vec<String>>,
        _collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        panic!("unused test service")
    }
    async fn list_series_tags(
        &self,
        _context: &DiscoveryQueryContext,
        _library_ids: Option<Vec<String>>,
        _collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        panic!("unused test service")
    }
    async fn list_series_release_dates(
        &self,
        _context: &DiscoveryQueryContext,
        _library_ids: Option<Vec<String>>,
        _collection_id: Option<String>,
    ) -> Result<Vec<String>, DiscoveryError> {
        panic!("unused test service")
    }
    async fn list_book_tags(
        &self,
        _context: &DiscoveryQueryContext,
        _scope: Option<BookTagScope>,
        _library_ids: Option<Vec<String>>,
    ) -> Result<Vec<String>, DiscoveryError> {
        panic!("unused test service")
    }
}
