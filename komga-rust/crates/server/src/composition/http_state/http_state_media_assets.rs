use super::*;
use komga_infrastructure::database_handle::DatabaseHandle;
use komga_infrastructure::filesystem::import;
use komga_infrastructure::filesystem::media_access::db_queries;
use komga_infrastructure::filesystem::media_access::epub;
use komga_infrastructure::filesystem::media_access::hashes;
use komga_infrastructure::filesystem::media_access::page_content;
use komga_infrastructure::filesystem::media_access::read_progress;
use komga_interfaces::state::{
    MediaAssetsService, PersistedMediaFileRecord, RuntimeBookMetadataService,
    RuntimeMediaImportService,
};
use serde_json::Value;

struct ComposedMediaImportService {
    inner: komga_application::media_assets::MediaImportService<import::FilesystemImportPort>,
}

impl RuntimeMediaImportService for ComposedMediaImportService {
    fn enqueue_books(
        &self,
        payload: komga_application::media_assets::BooksImportPayload,
        next_task_id: &mut dyn FnMut() -> String,
    ) -> Result<Vec<komga_application::task_processing::TaskQueueRecord>, String> {
        self.inner.enqueue_books(payload, next_task_id)
    }

    fn process_queued_book_payload<'a>(
        &'a self,
        task_payload: &'a str,
        import_priority: i32,
    ) -> futures_util::future::BoxFuture<
        'a,
        Result<Vec<komga_application::task_processing::TaskQueueRecord>, String>,
    > {
        Box::pin(async move {
            self.inner
                .process_queued_book_payload(task_payload, import_priority)
                .await
        })
    }
}

struct ComposedBookMetadataService {
    inner: komga_application::media_assets::BookMetadataService<metadata::SqliteBookMetadataPort>,
}

impl RuntimeBookMetadataService for ComposedBookMetadataService {
    fn update_book_metadata<'a>(
        &'a self,
        book_id: &'a str,
        patch: &'a komga_application::media_assets::BookMetadataPatch,
    ) -> futures_util::future::BoxFuture<'a, Result<Option<Option<String>>, String>> {
        Box::pin(async move { self.inner.update_book_metadata(book_id, patch).await })
    }

    fn batch_update_book_metadata<'a>(
        &'a self,
        updates: Vec<(String, komga_application::media_assets::BookMetadataPatch)>,
    ) -> futures_util::future::BoxFuture<'a, Result<Vec<String>, String>> {
        Box::pin(async move { self.inner.batch_update_book_metadata(updates).await })
    }
}

#[derive(Clone)]
pub(super) struct RuntimeMediaAssetsService {
    db: DatabaseHandle,
}

pub(super) fn compose_media_assets_service(db: DatabaseHandle) -> Box<dyn MediaAssetsService> {
    Box::new(RuntimeMediaAssetsService { db })
}

#[async_trait::async_trait]
impl MediaAssetsService for RuntimeMediaAssetsService {
    fn media_import_service(&self) -> Box<dyn RuntimeMediaImportService> {
        Box::new(ComposedMediaImportService {
            inner: komga_application::media_assets::MediaImportService::new(
                import::FilesystemImportPort::new(self.db.database_file().to_path_buf()),
            ),
        })
    }

    fn book_metadata_service(&self) -> Box<dyn RuntimeBookMetadataService> {
        Box::new(ComposedBookMetadataService {
            inner: komga_application::media_assets::BookMetadataService::new(
                metadata::SqliteBookMetadataPort::new(self.db.database_file().to_path_buf()),
            ),
        })
    }

    async fn refresh_book_search_documents_after_metadata_update(
        &self,
        index_dir: &Path,
        book_id: &str,
    ) -> Result<(), String> {
        komga_infrastructure::search::runtime_tasks::sync_entity_upsert_from_database(
            self.db.write_pool(),
            self.db.database_file(),
            index_dir,
            komga_infrastructure::search::index_lifecycle::SearchEntityType::Book,
            book_id,
        )
        .await
        .map(|_| ())
    }

    async fn persist_book_page_hashes_with_media_content(
        &self,
        book_id: &str,
    ) -> Result<(), String> {
        hashes::persist_book_page_hashes_from_media_content(self.db.database_file(), book_id).await
    }

    fn decode_epub_positions(&self, blob: &[u8]) -> Result<Vec<Value>, String> {
        epub::decode_epub_positions_blob(blob)
    }

    async fn load_epub_archive_positions(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
    ) -> Option<Vec<Value>> {
        epub::load_epub_archive_positions(media).await
    }

    async fn read_media_file_bytes(&self, path: &Path) -> Option<Vec<u8>> {
        page_content::read_media_file_bytes(path).await
    }

    async fn read_media_file_size(&self, path: &Path) -> Option<i64> {
        page_content::read_media_file_size(path).await
    }

    async fn load_epub_cover_bytes(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
    ) -> Option<(Vec<u8>, String)> {
        epub::load_epub_cover_bytes(media).await
    }

    async fn load_persisted_book_media(
        &self,
        book_id: &str,
    ) -> Result<Option<komga_application::media_assets::BookMediaRecord>, String> {
        db_queries::load_persisted_book_media(self.db.database_file(), book_id).await
    }

    async fn load_persisted_book_media_files(&self, book_id: &str) -> Result<Vec<String>, String> {
        db_queries::load_persisted_book_media_files(self.db.database_file(), book_id).await
    }

    async fn load_persisted_media_file_records(
        &self,
        book_id: &str,
    ) -> Result<Vec<PersistedMediaFileRecord>, String> {
        db_queries::load_persisted_media_file_records(self.db.database_file(), book_id)
            .await
            .map(|rows| {
                rows.into_iter()
                    .map(|row| PersistedMediaFileRecord {
                        file_name: row.file_name,
                        media_type: row.media_type,
                        sub_type: row.sub_type,
                    })
                    .collect()
            })
    }

    async fn book_media_is_ready_status(&self, book_id: &str) -> Result<bool, String> {
        db_queries::book_media_is_ready_status(self.db.database_file(), book_id).await
    }

    async fn load_persisted_book_pages(
        &self,
        book_id: &str,
    ) -> Result<Vec<komga_application::media_assets::BookPageRecord>, String> {
        db_queries::load_persisted_book_pages(self.db.database_file(), book_id).await
    }

    async fn load_persisted_book_page_row(
        &self,
        book_id: &str,
        page_number: u64,
    ) -> Result<Option<komga_application::media_assets::BookPageRecord>, String> {
        db_queries::load_persisted_book_page_row(self.db.database_file(), book_id, page_number)
            .await
    }

    async fn resolve_book_page_bytes(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
        page: &komga_application::media_assets::BookPageRecord,
        page_number: u64,
    ) -> Option<Vec<u8>> {
        page_content::resolve_book_page_bytes(media, page, page_number).await
    }

    async fn render_book_page_thumbnail(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
        page: &komga_application::media_assets::BookPageRecord,
        page_number: u64,
        max_edge: u32,
    ) -> Option<Vec<u8>> {
        page_content::render_book_page_thumbnail(media, page, page_number, max_edge).await
    }

    async fn load_archive_page_row(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
        page_number: u64,
    ) -> Option<komga_application::media_assets::BookPageRecord> {
        page_content::load_archive_page_row(media, page_number).await
    }

    async fn load_archive_page_rows(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
    ) -> Option<Vec<komga_application::media_assets::BookPageRecord>> {
        page_content::load_archive_page_rows(media).await
    }

    fn load_pdf_page_row(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
        page_number: u64,
    ) -> Option<komga_application::media_assets::BookPageRecord> {
        page_content::load_pdf_page_row(media, page_number)
    }

    fn load_generated_pdf_page_rows(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
    ) -> Vec<komga_application::media_assets::BookPageRecord> {
        page_content::load_generated_pdf_page_rows(media)
    }

    fn read_pdf_page_as_single_page_pdf(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
        page_number: u64,
    ) -> Option<Vec<u8>> {
        page_content::read_pdf_page_as_single_page_pdf(media, page_number)
    }

    fn detect_pdf_page_count(
        &self,
        media: &komga_application::media_assets::BookMediaRecord,
    ) -> Option<u64> {
        page_content::detect_pdf_page_count(media)
    }

    async fn load_persisted_epub_extension_blob(
        &self,
        book_id: &str,
    ) -> Result<Option<(String, Vec<u8>)>, String> {
        db_queries::load_persisted_epub_extension_blob(self.db.database_file(), book_id).await
    }

    async fn load_series_book_ids(&self, series_id: &str) -> Result<Vec<String>, String> {
        db_queries::load_series_book_ids(self.db.database_file(), series_id).await
    }

    async fn refresh_series_read_progress_row(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<(), String> {
        read_progress::refresh_series_read_progress_row(self.db.database_file(), series_id, user_id)
            .await
    }

    async fn delete_series_read_progress_row(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<(), String> {
        read_progress::delete_series_read_progress_row(self.db.database_file(), series_id, user_id)
            .await
    }

    async fn load_series_tachiyomi_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<Option<Value>, String> {
        read_progress::load_series_tachiyomi_progress(self.db.database_file(), series_id, user_id)
            .await
    }

    async fn load_book_progression(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<Value>, String> {
        metadata::load_book_progression(self.db.database_file(), book_id, user_id).await
    }

    async fn persist_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
        page: u64,
        completed: bool,
        locator: Option<Value>,
    ) -> Result<(), String> {
        metadata::persist_read_progress(
            self.db.database_file(),
            book_id,
            user_id,
            page,
            completed,
            locator,
        )
        .await
    }

    async fn delete_persisted_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<(), String> {
        metadata::delete_persisted_read_progress(self.db.database_file(), book_id, user_id).await
    }

    async fn readlist_tachiyomi_counters(
        &self,
        ordered_book_ids: &[String],
        user_id: &str,
    ) -> Result<(u64, u64, u64, u64, u64), String> {
        metadata::readlist_tachiyomi_counters(self.db.database_file(), ordered_book_ids, user_id)
            .await
    }

    async fn persist_readlist_tachiyomi_progress(
        &self,
        ordered_book_ids: &[String],
        user_id: &str,
        last_book_read: usize,
    ) -> Result<Option<()>, String> {
        metadata::persist_readlist_tachiyomi_progress(
            self.db.database_file(),
            ordered_book_ids,
            user_id,
            last_book_read,
        )
        .await
    }

    async fn load_selected_book_thumbnail(
        &self,
        book_id: &str,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
        metadata::load_selected_book_thumbnail(self.db.database_file(), book_id).await
    }

    async fn load_book_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
        metadata::load_book_thumbnail_by_id(self.db.database_file(), thumbnail_id).await
    }

    async fn load_persisted_book_thumbnails(
        &self,
        book_id: &str,
    ) -> Result<Vec<komga_application::media_assets::EntityThumbnailRecord>, String> {
        metadata::load_persisted_book_thumbnails(self.db.database_file(), book_id).await
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
        metadata::insert_book_thumbnail(
            self.db.database_file(),
            book_id,
            thumbnail,
            media_type,
            width,
            height,
            selected,
        )
        .await
    }

    async fn select_book_thumbnail(&self, thumbnail_id: &str) -> Result<bool, String> {
        metadata::select_book_thumbnail(self.db.database_file(), thumbnail_id).await
    }

    async fn delete_book_thumbnail(&self, thumbnail_id: &str) -> Result<bool, String> {
        metadata::delete_book_thumbnail(self.db.database_file(), thumbnail_id).await
    }

    async fn load_persisted_readlist_thumbnails(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<komga_application::media_assets::ReadlistThumbnailRecord>, String> {
        metadata::load_persisted_readlist_thumbnails(self.db.database_file(), readlist_id).await
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
        metadata::insert_readlist_thumbnail(
            self.db.database_file(),
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
        readlist_id: &str,
        thumbnail_id: &str,
    ) -> Result<bool, String> {
        metadata::select_readlist_thumbnail(self.db.database_file(), readlist_id, thumbnail_id)
            .await
    }

    async fn delete_readlist_thumbnail(
        &self,
        readlist_id: &str,
        thumbnail_id: &str,
    ) -> Result<bool, String> {
        metadata::delete_readlist_thumbnail(self.db.database_file(), readlist_id, thumbnail_id)
            .await
    }

    async fn load_persisted_collection_thumbnails(
        &self,
        collection_id: &str,
    ) -> Result<Vec<komga_application::media_assets::CollectionThumbnailRecord>, String> {
        metadata::load_persisted_collection_thumbnails(self.db.database_file(), collection_id).await
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
        metadata::insert_collection_thumbnail(
            self.db.database_file(),
            collection_id,
            thumbnail,
            media_type,
            width,
            height,
            selected,
        )
        .await
    }

    async fn select_collection_thumbnail(&self, thumbnail_id: &str) -> Result<bool, String> {
        metadata::select_collection_thumbnail(self.db.database_file(), thumbnail_id).await
    }

    async fn delete_collection_thumbnail(
        &self,
        collection_id: &str,
        thumbnail_id: &str,
    ) -> Result<bool, String> {
        metadata::delete_collection_thumbnail(self.db.database_file(), collection_id, thumbnail_id)
            .await
    }

    async fn load_selected_series_thumbnail(
        &self,
        series_id: &str,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
        metadata::load_selected_series_thumbnail(self.db.database_file(), series_id).await
    }

    async fn load_persisted_series_thumbnails(
        &self,
        series_id: &str,
    ) -> Result<Vec<komga_application::media_assets::SeriesThumbnailRecord>, String> {
        metadata::load_persisted_series_thumbnails(self.db.database_file(), series_id).await
    }

    async fn load_series_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> Result<Option<komga_application::media_assets::EntityThumbnailBinary>, String> {
        metadata::load_series_thumbnail_by_id(self.db.database_file(), thumbnail_id).await
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
        metadata::insert_series_thumbnail(
            self.db.database_file(),
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
        series_id: &str,
        thumbnail_id: &str,
    ) -> Result<bool, String> {
        metadata::select_series_thumbnail(self.db.database_file(), series_id, thumbnail_id).await
    }

    async fn delete_series_thumbnail(
        &self,
        series_id: &str,
        thumbnail_id: &str,
    ) -> Result<bool, String> {
        metadata::delete_series_thumbnail(self.db.database_file(), series_id, thumbnail_id).await
    }

    async fn load_persisted_readlist_name(
        &self,
        readlist_id: &str,
    ) -> Result<Option<String>, String> {
        metadata::load_persisted_readlist_name(self.db.database_file(), readlist_id).await
    }

    async fn load_book_restrictions(
        &self,
        book_id: &str,
    ) -> Result<Option<(Option<u16>, Vec<String>)>, String> {
        db_queries::load_book_restrictions(self.db.database_file(), book_id).await
    }

    async fn load_readlist_archive_entries(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<(String, PathBuf)>, String> {
        db_queries::load_readlist_archive_entries(self.db.database_file(), readlist_id).await
    }

    async fn load_series_archive_entries(
        &self,
        series_id: &str,
    ) -> Result<Option<(String, String, Vec<(String, PathBuf)>)>, String> {
        db_queries::load_series_archive_entries(self.db.database_file(), series_id).await
    }

    fn is_font_resource(&self, resource_name: &str) -> bool {
        epub::is_font_resource(resource_name)
    }

    async fn read_epub_resource_bytes(
        &self,
        epub_path: &Path,
        resource_name: &str,
    ) -> Option<Vec<u8>> {
        epub::read_epub_resource_bytes(epub_path, resource_name).await
    }

    async fn load_persisted_manifest_book(
        &self,
        book_id: &str,
    ) -> Result<Option<(String, String, String)>, String> {
        db_queries::load_persisted_manifest_book(self.db.database_file(), book_id).await
    }

    async fn persisted_book_exists(&self, book_id: &str) -> Result<bool, String> {
        db_queries::persisted_book_exists(self.db.database_file(), book_id).await
    }

    async fn persisted_book_ids(&self) -> Result<Vec<String>, String> {
        db_queries::persisted_book_ids(self.db.database_file()).await
    }

    async fn persisted_series_exists(&self, series_id: &str) -> Result<bool, String> {
        db_queries::persisted_series_exists(self.db.database_file(), series_id).await
    }

    async fn load_persisted_series_oneshot(&self, series_id: &str) -> Result<Option<bool>, String> {
        db_queries::load_persisted_series_oneshot(self.db.database_file(), series_id).await
    }

    async fn persisted_readlist_exists(&self, readlist_id: &str) -> Result<bool, String> {
        metadata::persisted_readlist_exists(self.db.database_file(), readlist_id).await
    }

    async fn persisted_collection_exists(&self, collection_id: &str) -> Result<bool, String> {
        metadata::persisted_collection_exists(self.db.database_file(), collection_id).await
    }

    async fn load_series_book_number_sorts(
        &self,
        series_id: &str,
    ) -> Result<Vec<(String, f64)>, String> {
        db_queries::load_series_book_number_sorts(self.db.database_file(), series_id).await
    }

    async fn load_book_page_count(&self, book_id: &str) -> Result<Option<u64>, String> {
        metadata::load_book_page_count(self.db.database_file(), book_id).await
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
        metadata::persist_book_progression(
            self.db.database_file(),
            book_id,
            user_id,
            page,
            use_locator_position_for_page,
            modified.map(str::to_owned),
            device_id.map(str::to_owned),
            device_name.map(str::to_owned),
            locator,
        )
        .await
    }
}
