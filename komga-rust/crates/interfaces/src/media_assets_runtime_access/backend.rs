#![allow(clippy::type_complexity)]

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use komga_application::media_assets::{
    BookMediaRecord, BookMetadataPatch, BookPageRecord, BooksImportPayload,
    CollectionThumbnailRecord, EntityThumbnailBinary, EntityThumbnailRecord,
    ReadlistThumbnailRecord, SeriesThumbnailRecord,
};
use komga_application::task_processing::TaskQueueRecord;
use serde_json::Value;

pub trait RuntimeMediaImportService: Send + Sync {
    fn enqueue_books(
        &self,
        payload: BooksImportPayload,
        next_task_id: &mut dyn FnMut() -> String,
    ) -> Result<Vec<TaskQueueRecord>, String>;

    fn process_queued_books_payload<'a>(
        &'a self,
        task_payload: &'a str,
    ) -> futures_util::future::BoxFuture<'a, Result<Vec<TaskQueueRecord>, String>>;

    fn process_queued_book_payload<'a>(
        &'a self,
        task_payload: &'a str,
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

#[derive(Clone)]
pub struct MediaAssetsRuntimeAccessBackend {
    pub media_import_service:
        Arc<dyn Fn(PathBuf) -> Box<dyn RuntimeMediaImportService> + Send + Sync>,
    pub book_metadata_service:
        Arc<dyn Fn(PathBuf) -> Box<dyn RuntimeBookMetadataService> + Send + Sync>,
    pub persist_book_page_hashes_with_media_content: Arc<
        dyn Fn(PathBuf, String) -> futures_util::future::BoxFuture<'static, Result<(), String>>
            + Send
            + Sync,
    >,
    pub decode_epub_positions: Arc<dyn Fn(Vec<u8>) -> Result<Vec<Value>, String> + Send + Sync>,
    pub load_epub_archive_positions:
        Arc<dyn Fn(BookMediaRecord) -> Option<Vec<Value>> + Send + Sync>,
    pub read_media_file_bytes: Arc<dyn Fn(PathBuf) -> Option<Vec<u8>> + Send + Sync>,
    pub read_media_file_size: Arc<dyn Fn(PathBuf) -> Option<i64> + Send + Sync>,
    pub load_persisted_book_media: Arc<
        dyn Fn(
                PathBuf,
                String,
            )
                -> futures_util::future::BoxFuture<'static, Result<Option<BookMediaRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_persisted_book_media_files: Arc<
        dyn Fn(
                PathBuf,
                String,
            ) -> futures_util::future::BoxFuture<'static, Result<Vec<String>, String>>
            + Send
            + Sync,
    >,
    pub book_media_is_ready_status: Arc<
        dyn Fn(PathBuf, String) -> futures_util::future::BoxFuture<'static, Result<bool, String>>
            + Send
            + Sync,
    >,
    pub load_persisted_series_thumbnail_media: Arc<
        dyn Fn(
                PathBuf,
                String,
            )
                -> futures_util::future::BoxFuture<'static, Result<Option<BookMediaRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_persisted_book_pages: Arc<
        dyn Fn(
                PathBuf,
                String,
            )
                -> futures_util::future::BoxFuture<'static, Result<Vec<BookPageRecord>, String>>
            + Send
            + Sync,
    >,
    pub load_persisted_book_page_row: Arc<
        dyn Fn(
                PathBuf,
                String,
                u64,
            )
                -> futures_util::future::BoxFuture<'static, Result<Option<BookPageRecord>, String>>
            + Send
            + Sync,
    >,
    pub resolve_book_page_bytes:
        Arc<dyn Fn(BookMediaRecord, BookPageRecord, u64) -> Option<Vec<u8>> + Send + Sync>,
    pub render_book_page_thumbnail:
        Arc<dyn Fn(BookMediaRecord, BookPageRecord, u64, u32) -> Option<Vec<u8>> + Send + Sync>,
    pub load_archive_page_row:
        Arc<dyn Fn(BookMediaRecord, u64) -> Option<BookPageRecord> + Send + Sync>,
    pub load_archive_page_rows:
        Arc<dyn Fn(BookMediaRecord) -> Option<Vec<BookPageRecord>> + Send + Sync>,
    pub load_pdf_page_row:
        Arc<dyn Fn(BookMediaRecord, u64) -> Option<BookPageRecord> + Send + Sync>,
    pub load_generated_pdf_page_rows:
        Arc<dyn Fn(BookMediaRecord) -> Vec<BookPageRecord> + Send + Sync>,
    pub read_pdf_page_as_single_page_pdf:
        Arc<dyn Fn(BookMediaRecord, u64) -> Option<Vec<u8>> + Send + Sync>,
    pub detect_pdf_page_count: Arc<dyn Fn(BookMediaRecord) -> Option<u64> + Send + Sync>,
    pub load_persisted_epub_extension_blob: Arc<
        dyn Fn(
                PathBuf,
                String,
            ) -> futures_util::future::BoxFuture<
                'static,
                Result<Option<(String, Vec<u8>)>, String>,
            > + Send
            + Sync,
    >,
    pub load_series_book_ids: Arc<
        dyn Fn(
                PathBuf,
                String,
            ) -> futures_util::future::BoxFuture<'static, Result<Vec<String>, String>>
            + Send
            + Sync,
    >,
    pub refresh_series_read_progress_row: Arc<
        dyn Fn(
                PathBuf,
                String,
                String,
            ) -> futures_util::future::BoxFuture<'static, Result<(), String>>
            + Send
            + Sync,
    >,
    pub delete_series_read_progress_row: Arc<
        dyn Fn(
                PathBuf,
                String,
                String,
            ) -> futures_util::future::BoxFuture<'static, Result<(), String>>
            + Send
            + Sync,
    >,
    pub load_series_tachiyomi_progress: Arc<
        dyn Fn(
                PathBuf,
                String,
                String,
            )
                -> futures_util::future::BoxFuture<'static, Result<Option<Value>, String>>
            + Send
            + Sync,
    >,
    pub load_book_progression: Arc<
        dyn Fn(
                PathBuf,
                String,
                String,
            )
                -> futures_util::future::BoxFuture<'static, Result<Option<Value>, String>>
            + Send
            + Sync,
    >,
    pub persist_read_progress: Arc<
        dyn Fn(
                PathBuf,
                String,
                String,
                u64,
                bool,
                Option<Value>,
            ) -> futures_util::future::BoxFuture<'static, Result<(), String>>
            + Send
            + Sync,
    >,
    pub delete_persisted_read_progress: Arc<
        dyn Fn(
                PathBuf,
                String,
                String,
            ) -> futures_util::future::BoxFuture<'static, Result<(), String>>
            + Send
            + Sync,
    >,
    pub readlist_tachiyomi_counters: Arc<
        dyn Fn(
                PathBuf,
                Vec<String>,
                String,
            ) -> futures_util::future::BoxFuture<
                'static,
                Result<(u64, u64, u64, u64, u64), String>,
            > + Send
            + Sync,
    >,
    pub persist_readlist_tachiyomi_progress: Arc<
        dyn Fn(
                PathBuf,
                Vec<String>,
                String,
                usize,
            ) -> futures_util::future::BoxFuture<'static, Result<Option<()>, String>>
            + Send
            + Sync,
    >,
    pub load_selected_book_thumbnail: Arc<
        dyn Fn(
                PathBuf,
                String,
            ) -> futures_util::future::BoxFuture<
                'static,
                Result<Option<EntityThumbnailBinary>, String>,
            > + Send
            + Sync,
    >,
    pub load_book_thumbnail_by_id: Arc<
        dyn Fn(
                PathBuf,
                String,
            ) -> futures_util::future::BoxFuture<
                'static,
                Result<Option<EntityThumbnailBinary>, String>,
            > + Send
            + Sync,
    >,
    pub load_persisted_book_thumbnails: Arc<
        dyn Fn(
                PathBuf,
                String,
            ) -> futures_util::future::BoxFuture<
                'static,
                Result<Vec<EntityThumbnailRecord>, String>,
            > + Send
            + Sync,
    >,
    pub insert_book_thumbnail: Arc<
        dyn Fn(
                PathBuf,
                String,
                Vec<u8>,
                String,
                i64,
                i64,
                bool,
            )
                -> futures_util::future::BoxFuture<'static, Result<EntityThumbnailRecord, String>>
            + Send
            + Sync,
    >,
    pub select_book_thumbnail: Arc<
        dyn Fn(PathBuf, String) -> futures_util::future::BoxFuture<'static, Result<bool, String>>
            + Send
            + Sync,
    >,
    pub delete_book_thumbnail: Arc<
        dyn Fn(
                PathBuf,
                String,
                String,
            ) -> futures_util::future::BoxFuture<'static, Result<bool, String>>
            + Send
            + Sync,
    >,
    pub load_persisted_readlist_thumbnails: Arc<
        dyn Fn(
                PathBuf,
                String,
            ) -> futures_util::future::BoxFuture<
                'static,
                Result<Vec<ReadlistThumbnailRecord>, String>,
            > + Send
            + Sync,
    >,
    pub insert_readlist_thumbnail: Arc<
        dyn Fn(
                PathBuf,
                String,
                Vec<u8>,
                String,
                i64,
                i64,
                bool,
            )
                -> futures_util::future::BoxFuture<'static, Result<ReadlistThumbnailRecord, String>>
            + Send
            + Sync,
    >,
    pub select_readlist_thumbnail: Arc<
        dyn Fn(
                PathBuf,
                String,
                String,
            ) -> futures_util::future::BoxFuture<'static, Result<bool, String>>
            + Send
            + Sync,
    >,
    pub delete_readlist_thumbnail: Arc<
        dyn Fn(
                PathBuf,
                String,
                String,
            ) -> futures_util::future::BoxFuture<'static, Result<bool, String>>
            + Send
            + Sync,
    >,
    pub load_persisted_collection_thumbnails: Arc<
        dyn Fn(
                PathBuf,
                String,
            ) -> futures_util::future::BoxFuture<
                'static,
                Result<Vec<CollectionThumbnailRecord>, String>,
            > + Send
            + Sync,
    >,
    pub insert_collection_thumbnail: Arc<
        dyn Fn(
                PathBuf,
                String,
                Vec<u8>,
                String,
                i64,
                i64,
                bool,
            ) -> futures_util::future::BoxFuture<
                'static,
                Result<CollectionThumbnailRecord, String>,
            > + Send
            + Sync,
    >,
    pub select_collection_thumbnail: Arc<
        dyn Fn(PathBuf, String) -> futures_util::future::BoxFuture<'static, Result<bool, String>>
            + Send
            + Sync,
    >,
    pub delete_collection_thumbnail: Arc<
        dyn Fn(
                PathBuf,
                String,
                String,
            ) -> futures_util::future::BoxFuture<'static, Result<bool, String>>
            + Send
            + Sync,
    >,
    pub load_selected_series_thumbnail: Arc<
        dyn Fn(
                PathBuf,
                String,
            ) -> futures_util::future::BoxFuture<
                'static,
                Result<Option<EntityThumbnailBinary>, String>,
            > + Send
            + Sync,
    >,
    pub load_persisted_series_thumbnails: Arc<
        dyn Fn(
                PathBuf,
                String,
            ) -> futures_util::future::BoxFuture<
                'static,
                Result<Vec<SeriesThumbnailRecord>, String>,
            > + Send
            + Sync,
    >,
    pub load_series_thumbnail_by_id: Arc<
        dyn Fn(
                PathBuf,
                String,
            ) -> futures_util::future::BoxFuture<
                'static,
                Result<Option<EntityThumbnailBinary>, String>,
            > + Send
            + Sync,
    >,
    pub insert_series_thumbnail: Arc<
        dyn Fn(
                PathBuf,
                String,
                Vec<u8>,
                String,
                i64,
                i64,
                bool,
            )
                -> futures_util::future::BoxFuture<'static, Result<SeriesThumbnailRecord, String>>
            + Send
            + Sync,
    >,
    pub select_series_thumbnail: Arc<
        dyn Fn(
                PathBuf,
                String,
                String,
            ) -> futures_util::future::BoxFuture<'static, Result<bool, String>>
            + Send
            + Sync,
    >,
    pub delete_series_thumbnail: Arc<
        dyn Fn(
                PathBuf,
                String,
                String,
            ) -> futures_util::future::BoxFuture<'static, Result<bool, String>>
            + Send
            + Sync,
    >,
    pub load_persisted_readlist_name: Arc<
        dyn Fn(
                PathBuf,
                String,
            )
                -> futures_util::future::BoxFuture<'static, Result<Option<String>, String>>
            + Send
            + Sync,
    >,
    pub load_book_restrictions: Arc<
        dyn Fn(
                PathBuf,
                String,
            ) -> futures_util::future::BoxFuture<
                'static,
                Result<Option<(Option<u16>, Vec<String>)>, String>,
            > + Send
            + Sync,
    >,
    pub load_readlist_archive_entries: Arc<
        dyn Fn(
                PathBuf,
                String,
            )
                -> futures_util::future::BoxFuture<'static, Result<Vec<(String, PathBuf)>, String>>
            + Send
            + Sync,
    >,
    pub load_series_archive_entries: Arc<
        dyn Fn(
                PathBuf,
                String,
            ) -> futures_util::future::BoxFuture<
                'static,
                Result<Option<(String, String, Vec<(String, PathBuf)>)>, String>,
            > + Send
            + Sync,
    >,
    pub is_font_resource: Arc<dyn Fn(String) -> bool + Send + Sync>,
    pub read_epub_resource_bytes: Arc<dyn Fn(PathBuf, String) -> Option<Vec<u8>> + Send + Sync>,
    pub load_persisted_manifest_book: Arc<
        dyn Fn(
                PathBuf,
                String,
            ) -> futures_util::future::BoxFuture<
                'static,
                Result<Option<(String, String, String)>, String>,
            > + Send
            + Sync,
    >,
    pub persisted_book_exists: Arc<
        dyn Fn(PathBuf, String) -> futures_util::future::BoxFuture<'static, Result<bool, String>>
            + Send
            + Sync,
    >,
    pub persisted_book_ids: Arc<
        dyn Fn(PathBuf) -> futures_util::future::BoxFuture<'static, Result<Vec<String>, String>>
            + Send
            + Sync,
    >,
    pub persisted_series_exists: Arc<
        dyn Fn(PathBuf, String) -> futures_util::future::BoxFuture<'static, Result<bool, String>>
            + Send
            + Sync,
    >,
    pub load_persisted_series_oneshot: Arc<
        dyn Fn(
                PathBuf,
                String,
            )
                -> futures_util::future::BoxFuture<'static, Result<Option<bool>, String>>
            + Send
            + Sync,
    >,
    pub persisted_readlist_exists: Arc<
        dyn Fn(PathBuf, String) -> futures_util::future::BoxFuture<'static, Result<bool, String>>
            + Send
            + Sync,
    >,
    pub persisted_collection_exists: Arc<
        dyn Fn(PathBuf, String) -> futures_util::future::BoxFuture<'static, Result<bool, String>>
            + Send
            + Sync,
    >,
    pub load_series_book_number_sorts: Arc<
        dyn Fn(
                PathBuf,
                String,
            )
                -> futures_util::future::BoxFuture<'static, Result<Vec<(String, f64)>, String>>
            + Send
            + Sync,
    >,
    pub load_book_page_count: Arc<
        dyn Fn(
                PathBuf,
                String,
            ) -> futures_util::future::BoxFuture<'static, Result<Option<u64>, String>>
            + Send
            + Sync,
    >,
    pub persist_book_progression: Arc<
        dyn Fn(
                PathBuf,
                String,
                String,
                f64,
                bool,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<Value>,
            ) -> futures_util::future::BoxFuture<'static, Result<(), String>>
            + Send
            + Sync,
    >,
}

static BACKEND: OnceLock<MediaAssetsRuntimeAccessBackend> = OnceLock::new();
#[cfg(test)]
static TEST_BACKEND: OnceLock<MediaAssetsRuntimeAccessBackend> = OnceLock::new();

pub fn install_media_assets_runtime_access(backend: MediaAssetsRuntimeAccessBackend) {
    let _ = BACKEND.set(backend);
}

pub(super) fn backend() -> &'static MediaAssetsRuntimeAccessBackend {
    if let Some(backend) = BACKEND.get() {
        return backend;
    }

    #[cfg(test)]
    {
        TEST_BACKEND.get_or_init(super::test_backend::default_test_backend)
    }

    #[cfg(not(test))]
    {
        panic!("media assets runtime access backend should be installed before use");
    }
}
