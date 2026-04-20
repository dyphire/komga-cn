use super::*;

#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait MediaAssetsService: Send + Sync {
    fn media_import_service(&self, database_file: PathBuf) -> Box<dyn RuntimeMediaImportService>;
    fn book_metadata_service(&self, database_file: PathBuf) -> Box<dyn RuntimeBookMetadataService>;
    async fn refresh_book_search_documents_after_metadata_update(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        book_id: String,
    ) -> Result<(), String>;
    async fn persist_book_page_hashes_with_media_content(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<(), String>;
    fn decode_epub_positions(&self, blob: Vec<u8>) -> Result<Vec<Value>, String>;
    fn load_epub_archive_positions(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
    ) -> Option<Vec<Value>>;
    fn read_media_file_bytes(&self, path: PathBuf) -> Option<Vec<u8>>;
    fn read_media_file_size(&self, path: PathBuf) -> Option<i64>;
    fn load_epub_cover_bytes(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
    ) -> Option<(Vec<u8>, String)>;
    async fn load_persisted_book_media(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<komga_application::media_assets::BookMediaRecord>, String>;
    async fn load_persisted_book_media_files(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Vec<String>, String>;
    async fn load_persisted_media_file_records(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Vec<PersistedMediaFileRecord>, String>;
    async fn book_media_is_ready_status(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<bool, String>;
    async fn load_persisted_book_pages(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Vec<komga_application::media_assets::BookPageRecord>, String>;
    async fn load_persisted_book_page_row(
        &self,
        database_file: PathBuf,
        book_id: String,
        page_number: u64,
    ) -> Result<Option<komga_application::media_assets::BookPageRecord>, String>;
    fn resolve_book_page_bytes(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
        page: komga_application::media_assets::BookPageRecord,
        page_number: u64,
    ) -> Option<Vec<u8>>;
    fn render_book_page_thumbnail(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
        page: komga_application::media_assets::BookPageRecord,
        page_number: u64,
        max_edge: u32,
    ) -> Option<Vec<u8>>;
    fn load_archive_page_row(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
        page_number: u64,
    ) -> Option<komga_application::media_assets::BookPageRecord>;
    fn load_archive_page_rows(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
    ) -> Option<Vec<komga_application::media_assets::BookPageRecord>>;
    fn load_pdf_page_row(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
        page_number: u64,
    ) -> Option<komga_application::media_assets::BookPageRecord>;
    fn load_generated_pdf_page_rows(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
    ) -> Vec<komga_application::media_assets::BookPageRecord>;
    fn read_pdf_page_as_single_page_pdf(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
        page_number: u64,
    ) -> Option<Vec<u8>>;
    fn detect_pdf_page_count(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
    ) -> Option<u64>;
    async fn load_persisted_epub_extension_blob(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<(String, Vec<u8>)>, String>;
    async fn load_series_book_ids(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<String>, String>;
    async fn refresh_series_read_progress_row(
        &self,
        database_file: PathBuf,
        series_id: String,
        user_id: String,
    ) -> Result<(), String>;
    async fn delete_series_read_progress_row(
        &self,
        database_file: PathBuf,
        series_id: String,
        user_id: String,
    ) -> Result<(), String>;
    async fn load_series_tachiyomi_progress(
        &self,
        database_file: PathBuf,
        series_id: String,
        user_id: String,
    ) -> Result<Option<Value>, String>;
    async fn load_book_progression(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: String,
    ) -> Result<Option<Value>, String>;
    async fn persist_read_progress(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: String,
        page: u64,
        completed: bool,
        locator: Option<Value>,
    ) -> Result<(), String>;
    async fn delete_persisted_read_progress(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: String,
    ) -> Result<(), String>;
    async fn readlist_tachiyomi_counters(
        &self,
        database_file: PathBuf,
        ordered_book_ids: Vec<String>,
        user_id: String,
    ) -> Result<(u64, u64, u64, u64, u64), String>;
    async fn persist_readlist_tachiyomi_progress(
        &self,
        database_file: PathBuf,
        ordered_book_ids: Vec<String>,
        user_id: String,
        last_book_read: usize,
    ) -> Result<Option<()>, String>;
    async fn load_selected_book_thumbnail(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String>;
    async fn load_book_thumbnail_by_id(
        &self,
        database_file: PathBuf,
        thumbnail_id: String,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String>;
    async fn load_persisted_book_thumbnails(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Vec<komga_application::media_assets::EntityThumbnailRecord>, String>;
    async fn insert_book_thumbnail(
        &self,
        database_file: PathBuf,
        book_id: String,
        thumbnail: Vec<u8>,
        media_type: String,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<komga_application::media_assets::EntityThumbnailRecord, String>;
    async fn select_book_thumbnail(
        &self,
        database_file: PathBuf,
        thumbnail_id: String,
    ) -> Result<bool, String>;
    async fn delete_book_thumbnail(
        &self,
        database_file: PathBuf,
        book_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String>;
    async fn load_persisted_readlist_thumbnails(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Vec<komga_application::media_assets::ReadlistThumbnailRecord>, String>;
    async fn insert_readlist_thumbnail(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        thumbnail: Vec<u8>,
        media_type: String,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<komga_application::media_assets::ReadlistThumbnailRecord, String>;
    async fn select_readlist_thumbnail(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String>;
    async fn delete_readlist_thumbnail(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String>;
    async fn load_persisted_collection_thumbnails(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Vec<komga_application::media_assets::CollectionThumbnailRecord>, String>;
    async fn insert_collection_thumbnail(
        &self,
        database_file: PathBuf,
        collection_id: String,
        thumbnail: Vec<u8>,
        media_type: String,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<komga_application::media_assets::CollectionThumbnailRecord, String>;
    async fn select_collection_thumbnail(
        &self,
        database_file: PathBuf,
        thumbnail_id: String,
    ) -> Result<bool, String>;
    async fn delete_collection_thumbnail(
        &self,
        database_file: PathBuf,
        collection_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String>;
    async fn load_selected_series_thumbnail(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String>;
    async fn load_persisted_series_thumbnails(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<komga_application::media_assets::SeriesThumbnailRecord>, String>;
    async fn load_series_thumbnail_by_id(
        &self,
        database_file: PathBuf,
        thumbnail_id: String,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String>;
    async fn insert_series_thumbnail(
        &self,
        database_file: PathBuf,
        series_id: String,
        thumbnail: Vec<u8>,
        media_type: String,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<komga_application::media_assets::SeriesThumbnailRecord, String>;
    async fn select_series_thumbnail(
        &self,
        database_file: PathBuf,
        series_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String>;
    async fn delete_series_thumbnail(
        &self,
        database_file: PathBuf,
        series_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String>;
    async fn load_persisted_readlist_name(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Option<String>, String>;
    async fn load_book_restrictions(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<(Option<u16>, Vec<String>)>, String>;
    async fn load_readlist_archive_entries(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Vec<(String, PathBuf)>, String>;
    async fn load_series_archive_entries(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<(String, String, Vec<(String, PathBuf)>)>, String>;
    fn is_font_resource(&self, resource_name: String) -> bool;
    fn read_epub_resource_bytes(
        &self,
        epub_path: PathBuf,
        resource_name: String,
    ) -> Option<Vec<u8>>;
    async fn load_persisted_manifest_book(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<(String, String, String)>, String>;
    async fn persisted_book_exists(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<bool, String>;
    async fn persisted_book_ids(&self, database_file: PathBuf) -> Result<Vec<String>, String>;
    async fn persisted_series_exists(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<bool, String>;
    async fn load_persisted_series_oneshot(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<bool>, String>;
    async fn persisted_readlist_exists(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<bool, String>;
    async fn persisted_collection_exists(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<bool, String>;
    async fn load_series_book_number_sorts(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<(String, f64)>, String>;
    async fn load_book_page_count(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<u64>, String>;
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
    ) -> Result<(), String>;
}

#[async_trait]
impl<T> MediaAssetsService for Arc<T>
where
    T: MediaAssetsService + ?Sized,
{
    fn media_import_service(&self, database_file: PathBuf) -> Box<dyn RuntimeMediaImportService> {
        (**self).media_import_service(database_file)
    }

    fn book_metadata_service(&self, database_file: PathBuf) -> Box<dyn RuntimeBookMetadataService> {
        (**self).book_metadata_service(database_file)
    }

    async fn refresh_book_search_documents_after_metadata_update(
        &self,
        database_file: PathBuf,
        index_dir: PathBuf,
        book_id: String,
    ) -> Result<(), String> {
        (**self)
            .refresh_book_search_documents_after_metadata_update(database_file, index_dir, book_id)
            .await
    }

    async fn persist_book_page_hashes_with_media_content(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<(), String> {
        (**self)
            .persist_book_page_hashes_with_media_content(database_file, book_id)
            .await
    }

    fn decode_epub_positions(&self, blob: Vec<u8>) -> Result<Vec<Value>, String> {
        (**self).decode_epub_positions(blob)
    }

    fn load_epub_archive_positions(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
    ) -> Option<Vec<Value>> {
        (**self).load_epub_archive_positions(media)
    }

    fn read_media_file_bytes(&self, path: PathBuf) -> Option<Vec<u8>> {
        (**self).read_media_file_bytes(path)
    }

    fn read_media_file_size(&self, path: PathBuf) -> Option<i64> {
        (**self).read_media_file_size(path)
    }

    fn load_epub_cover_bytes(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
    ) -> Option<(Vec<u8>, String)> {
        (**self).load_epub_cover_bytes(media)
    }

    async fn load_persisted_book_media(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<komga_application::media_assets::BookMediaRecord>, String> {
        (**self)
            .load_persisted_book_media(database_file, book_id)
            .await
    }

    async fn load_persisted_book_media_files(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Vec<String>, String> {
        (**self)
            .load_persisted_book_media_files(database_file, book_id)
            .await
    }

    async fn load_persisted_media_file_records(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Vec<PersistedMediaFileRecord>, String> {
        (**self)
            .load_persisted_media_file_records(database_file, book_id)
            .await
    }

    async fn book_media_is_ready_status(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<bool, String> {
        (**self)
            .book_media_is_ready_status(database_file, book_id)
            .await
    }

    async fn load_persisted_book_pages(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Vec<komga_application::media_assets::BookPageRecord>, String> {
        (**self)
            .load_persisted_book_pages(database_file, book_id)
            .await
    }

    async fn load_persisted_book_page_row(
        &self,
        database_file: PathBuf,
        book_id: String,
        page_number: u64,
    ) -> Result<Option<komga_application::media_assets::BookPageRecord>, String> {
        (**self)
            .load_persisted_book_page_row(database_file, book_id, page_number)
            .await
    }

    fn resolve_book_page_bytes(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
        page: komga_application::media_assets::BookPageRecord,
        page_number: u64,
    ) -> Option<Vec<u8>> {
        (**self).resolve_book_page_bytes(media, page, page_number)
    }

    fn render_book_page_thumbnail(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
        page: komga_application::media_assets::BookPageRecord,
        page_number: u64,
        max_edge: u32,
    ) -> Option<Vec<u8>> {
        (**self).render_book_page_thumbnail(media, page, page_number, max_edge)
    }

    fn load_archive_page_row(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
        page_number: u64,
    ) -> Option<komga_application::media_assets::BookPageRecord> {
        (**self).load_archive_page_row(media, page_number)
    }

    fn load_archive_page_rows(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
    ) -> Option<Vec<komga_application::media_assets::BookPageRecord>> {
        (**self).load_archive_page_rows(media)
    }

    fn load_pdf_page_row(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
        page_number: u64,
    ) -> Option<komga_application::media_assets::BookPageRecord> {
        (**self).load_pdf_page_row(media, page_number)
    }

    fn load_generated_pdf_page_rows(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
    ) -> Vec<komga_application::media_assets::BookPageRecord> {
        (**self).load_generated_pdf_page_rows(media)
    }

    fn read_pdf_page_as_single_page_pdf(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
        page_number: u64,
    ) -> Option<Vec<u8>> {
        (**self).read_pdf_page_as_single_page_pdf(media, page_number)
    }

    fn detect_pdf_page_count(
        &self,
        media: komga_application::media_assets::BookMediaRecord,
    ) -> Option<u64> {
        (**self).detect_pdf_page_count(media)
    }

    async fn load_persisted_epub_extension_blob(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<(String, Vec<u8>)>, String> {
        (**self)
            .load_persisted_epub_extension_blob(database_file, book_id)
            .await
    }

    async fn load_series_book_ids(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<String>, String> {
        (**self)
            .load_series_book_ids(database_file, series_id)
            .await
    }

    async fn refresh_series_read_progress_row(
        &self,
        database_file: PathBuf,
        series_id: String,
        user_id: String,
    ) -> Result<(), String> {
        (**self)
            .refresh_series_read_progress_row(database_file, series_id, user_id)
            .await
    }

    async fn delete_series_read_progress_row(
        &self,
        database_file: PathBuf,
        series_id: String,
        user_id: String,
    ) -> Result<(), String> {
        (**self)
            .delete_series_read_progress_row(database_file, series_id, user_id)
            .await
    }

    async fn load_series_tachiyomi_progress(
        &self,
        database_file: PathBuf,
        series_id: String,
        user_id: String,
    ) -> Result<Option<Value>, String> {
        (**self)
            .load_series_tachiyomi_progress(database_file, series_id, user_id)
            .await
    }

    async fn load_book_progression(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: String,
    ) -> Result<Option<Value>, String> {
        (**self)
            .load_book_progression(database_file, book_id, user_id)
            .await
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
        (**self)
            .persist_read_progress(database_file, book_id, user_id, page, completed, locator)
            .await
    }

    async fn delete_persisted_read_progress(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: String,
    ) -> Result<(), String> {
        (**self)
            .delete_persisted_read_progress(database_file, book_id, user_id)
            .await
    }

    async fn readlist_tachiyomi_counters(
        &self,
        database_file: PathBuf,
        ordered_book_ids: Vec<String>,
        user_id: String,
    ) -> Result<(u64, u64, u64, u64, u64), String> {
        (**self)
            .readlist_tachiyomi_counters(database_file, ordered_book_ids, user_id)
            .await
    }

    async fn persist_readlist_tachiyomi_progress(
        &self,
        database_file: PathBuf,
        ordered_book_ids: Vec<String>,
        user_id: String,
        last_book_read: usize,
    ) -> Result<Option<()>, String> {
        (**self)
            .persist_readlist_tachiyomi_progress(
                database_file,
                ordered_book_ids,
                user_id,
                last_book_read,
            )
            .await
    }

    async fn load_selected_book_thumbnail(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
        (**self)
            .load_selected_book_thumbnail(database_file, book_id)
            .await
    }

    async fn load_book_thumbnail_by_id(
        &self,
        database_file: PathBuf,
        thumbnail_id: String,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
        (**self)
            .load_book_thumbnail_by_id(database_file, thumbnail_id)
            .await
    }

    async fn load_persisted_book_thumbnails(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Vec<komga_application::media_assets::EntityThumbnailRecord>, String> {
        (**self)
            .load_persisted_book_thumbnails(database_file, book_id)
            .await
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
        (**self)
            .insert_book_thumbnail(
                database_file,
                book_id,
                thumbnail,
                media_type,
                width,
                height,
                selected,
            )
            .await
    }

    async fn select_book_thumbnail(
        &self,
        database_file: PathBuf,
        thumbnail_id: String,
    ) -> Result<bool, String> {
        (**self)
            .select_book_thumbnail(database_file, thumbnail_id)
            .await
    }

    async fn delete_book_thumbnail(
        &self,
        database_file: PathBuf,
        book_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String> {
        (**self)
            .delete_book_thumbnail(database_file, book_id, thumbnail_id)
            .await
    }

    async fn load_persisted_readlist_thumbnails(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Vec<komga_application::media_assets::ReadlistThumbnailRecord>, String> {
        (**self)
            .load_persisted_readlist_thumbnails(database_file, readlist_id)
            .await
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
        (**self)
            .insert_readlist_thumbnail(
                database_file,
                readlist_id,
                thumbnail,
                media_type,
                width,
                height,
                selected,
            )
            .await
    }

    async fn select_readlist_thumbnail(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String> {
        (**self)
            .select_readlist_thumbnail(database_file, readlist_id, thumbnail_id)
            .await
    }

    async fn delete_readlist_thumbnail(
        &self,
        database_file: PathBuf,
        readlist_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String> {
        (**self)
            .delete_readlist_thumbnail(database_file, readlist_id, thumbnail_id)
            .await
    }

    async fn load_persisted_collection_thumbnails(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<Vec<komga_application::media_assets::CollectionThumbnailRecord>, String> {
        (**self)
            .load_persisted_collection_thumbnails(database_file, collection_id)
            .await
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
        (**self)
            .insert_collection_thumbnail(
                database_file,
                collection_id,
                thumbnail,
                media_type,
                width,
                height,
                selected,
            )
            .await
    }

    async fn select_collection_thumbnail(
        &self,
        database_file: PathBuf,
        thumbnail_id: String,
    ) -> Result<bool, String> {
        (**self)
            .select_collection_thumbnail(database_file, thumbnail_id)
            .await
    }

    async fn delete_collection_thumbnail(
        &self,
        database_file: PathBuf,
        collection_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String> {
        (**self)
            .delete_collection_thumbnail(database_file, collection_id, thumbnail_id)
            .await
    }

    async fn load_selected_series_thumbnail(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
        (**self)
            .load_selected_series_thumbnail(database_file, series_id)
            .await
    }

    async fn load_persisted_series_thumbnails(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<komga_application::media_assets::SeriesThumbnailRecord>, String> {
        (**self)
            .load_persisted_series_thumbnails(database_file, series_id)
            .await
    }

    async fn load_series_thumbnail_by_id(
        &self,
        database_file: PathBuf,
        thumbnail_id: String,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
        (**self)
            .load_series_thumbnail_by_id(database_file, thumbnail_id)
            .await
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
        (**self)
            .insert_series_thumbnail(
                database_file,
                series_id,
                thumbnail,
                media_type,
                width,
                height,
                selected,
            )
            .await
    }

    async fn select_series_thumbnail(
        &self,
        database_file: PathBuf,
        series_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String> {
        (**self)
            .select_series_thumbnail(database_file, series_id, thumbnail_id)
            .await
    }

    async fn delete_series_thumbnail(
        &self,
        database_file: PathBuf,
        series_id: String,
        thumbnail_id: String,
    ) -> Result<bool, String> {
        (**self)
            .delete_series_thumbnail(database_file, series_id, thumbnail_id)
            .await
    }

    async fn load_persisted_readlist_name(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Option<String>, String> {
        (**self)
            .load_persisted_readlist_name(database_file, readlist_id)
            .await
    }

    async fn load_book_restrictions(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<(Option<u16>, Vec<String>)>, String> {
        (**self)
            .load_book_restrictions(database_file, book_id)
            .await
    }

    async fn load_readlist_archive_entries(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<Vec<(String, PathBuf)>, String> {
        (**self)
            .load_readlist_archive_entries(database_file, readlist_id)
            .await
    }

    async fn load_series_archive_entries(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<(String, String, Vec<(String, PathBuf)>)>, String> {
        (**self)
            .load_series_archive_entries(database_file, series_id)
            .await
    }

    fn is_font_resource(&self, resource_name: String) -> bool {
        (**self).is_font_resource(resource_name)
    }

    fn read_epub_resource_bytes(
        &self,
        epub_path: PathBuf,
        resource_name: String,
    ) -> Option<Vec<u8>> {
        (**self).read_epub_resource_bytes(epub_path, resource_name)
    }

    async fn load_persisted_manifest_book(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<(String, String, String)>, String> {
        (**self)
            .load_persisted_manifest_book(database_file, book_id)
            .await
    }

    async fn persisted_book_exists(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<bool, String> {
        (**self).persisted_book_exists(database_file, book_id).await
    }

    async fn persisted_book_ids(&self, database_file: PathBuf) -> Result<Vec<String>, String> {
        (**self).persisted_book_ids(database_file).await
    }

    async fn persisted_series_exists(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<bool, String> {
        (**self)
            .persisted_series_exists(database_file, series_id)
            .await
    }

    async fn load_persisted_series_oneshot(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Option<bool>, String> {
        (**self)
            .load_persisted_series_oneshot(database_file, series_id)
            .await
    }

    async fn persisted_readlist_exists(
        &self,
        database_file: PathBuf,
        readlist_id: String,
    ) -> Result<bool, String> {
        (**self)
            .persisted_readlist_exists(database_file, readlist_id)
            .await
    }

    async fn persisted_collection_exists(
        &self,
        database_file: PathBuf,
        collection_id: String,
    ) -> Result<bool, String> {
        (**self)
            .persisted_collection_exists(database_file, collection_id)
            .await
    }

    async fn load_series_book_number_sorts(
        &self,
        database_file: PathBuf,
        series_id: String,
    ) -> Result<Vec<(String, f64)>, String> {
        (**self)
            .load_series_book_number_sorts(database_file, series_id)
            .await
    }

    async fn load_book_page_count(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<u64>, String> {
        (**self).load_book_page_count(database_file, book_id).await
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
        (**self)
            .persist_book_progression(
                database_file,
                book_id,
                user_id,
                page,
                use_locator_position_for_page,
                modified,
                device_id,
                device_name,
                locator,
            )
            .await
    }
}
