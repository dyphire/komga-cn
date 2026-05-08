#![allow(unused_variables)]

use super::*;
use std::path::Path;

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
