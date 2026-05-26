use std::path::Path;
use std::path::PathBuf;

use async_trait::async_trait;
use serde_json::Value;

use super::{
    BookMediaRecord, BookPageRecord, CollectionThumbnailRecord, EntityThumbnailBinary,
    EntityThumbnailRecord, PersistedMediaFileRecord, ReadlistThumbnailRecord,
    SeriesThumbnailRecord,
};

pub struct BookProgressionInput {
    pub book_id: String,
    pub user_id: String,
    pub progression: f64,
    pub use_locator_position_for_page: bool,
    pub modified: Option<String>,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub locator: Option<Value>,
}

/// Write operations for read progress (book and series level).
#[async_trait]
pub trait ProgressWriterPort: Send + Sync {
    async fn persist_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
        page: u64,
        completed: bool,
        locator: Option<Value>,
    ) -> Result<(), String>;

    async fn persist_book_progression(&self, input: BookProgressionInput) -> Result<(), String>;

    async fn delete_read_progress(&self, book_id: &str, user_id: &str) -> Result<(), String>;

    async fn persist_readlist_tachiyomi_progress(
        &self,
        ordered_book_ids: &[String],
        user_id: &str,
        last_book_read: usize,
    ) -> Result<Option<()>, String>;

    async fn refresh_series_read_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<(), String>;

    async fn delete_series_read_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<(), String>;
}

/// Write operations for thumbnails across all entity types.
#[async_trait]
pub trait ThumbnailWriterPort: Send + Sync {
    async fn insert_book(
        &self,
        book_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<EntityThumbnailRecord, String>;

    async fn select_book(&self, thumbnail_id: &str) -> Result<bool, String>;

    async fn delete_book(&self, thumbnail_id: &str) -> Result<bool, String>;

    async fn insert_series(
        &self,
        series_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<SeriesThumbnailRecord, String>;

    async fn select_series(&self, series_id: &str, thumbnail_id: &str) -> Result<bool, String>;

    async fn delete_series(&self, series_id: &str, thumbnail_id: &str) -> Result<bool, String>;

    async fn insert_readlist(
        &self,
        readlist_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<ReadlistThumbnailRecord, String>;

    async fn select_readlist(&self, readlist_id: &str, thumbnail_id: &str) -> Result<bool, String>;

    async fn delete_readlist(&self, readlist_id: &str, thumbnail_id: &str) -> Result<bool, String>;

    async fn insert_collection(
        &self,
        collection_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> Result<CollectionThumbnailRecord, String>;

    async fn select_collection(&self, thumbnail_id: &str) -> Result<bool, String>;

    async fn delete_collection(
        &self,
        collection_id: &str,
        thumbnail_id: &str,
    ) -> Result<bool, String>;
}

/// Stateless filesystem I/O for resolving page/resource content from archives and PDFs.
#[async_trait]
pub trait ContentResolverPort: Send + Sync {
    async fn resolve_page_bytes(
        &self,
        media: &BookMediaRecord,
        page: &BookPageRecord,
        page_number: u64,
    ) -> Option<Vec<u8>>;

    async fn render_page_thumbnail(
        &self,
        media: &BookMediaRecord,
        page: &BookPageRecord,
        page_number: u64,
        max_edge: u32,
    ) -> Option<Vec<u8>>;

    async fn archive_page_row(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> Option<BookPageRecord>;

    async fn archive_page_rows(&self, media: &BookMediaRecord) -> Option<Vec<BookPageRecord>>;

    fn pdf_page_row(&self, media: &BookMediaRecord, page_number: u64) -> Option<BookPageRecord>;

    fn generated_pdf_page_rows(&self, media: &BookMediaRecord) -> Vec<BookPageRecord>;

    fn read_pdf_page_as_single_page_pdf(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> Option<Vec<u8>>;

    fn detect_pdf_page_count(&self, media: &BookMediaRecord) -> Option<u64>;

    async fn read_media_file_bytes(&self, path: &Path) -> Option<Vec<u8>>;

    async fn read_media_file_size(&self, path: &Path) -> Option<i64>;

    fn is_font_resource(&self, resource_name: &str) -> bool;

    async fn read_epub_resource_bytes(
        &self,
        epub_path: &Path,
        resource_name: &str,
    ) -> Option<Vec<u8>>;

    fn decode_epub_positions_blob(&self, blob: &[u8]) -> Result<Vec<Value>, String>;

    async fn epub_archive_positions(&self, media: &BookMediaRecord) -> Option<Vec<Value>>;

    async fn epub_cover_bytes(&self, media: &BookMediaRecord) -> Option<(Vec<u8>, String)>;

    async fn epub_package_document(&self, media: &BookMediaRecord) -> Option<Vec<u8>>;

    fn epub_fixed_layout(&self, package_document: &[u8]) -> bool;

    fn epub_kobo_spans(&self, resource_bytes: &[u8]) -> Vec<(String, f64)>;

    fn normalize_epub_resource_href(&self, rootfile_path: &str, href: &str) -> String;
}

/// Read access to book media metadata.
#[async_trait]
pub trait BookMediaPort: Send + Sync {
    async fn book_media(&self, book_id: &str) -> Result<Option<BookMediaRecord>, String>;
    async fn book_media_files(&self, book_id: &str) -> Result<Vec<String>, String>;
    async fn media_file_records(
        &self,
        book_id: &str,
    ) -> Result<Vec<PersistedMediaFileRecord>, String>;
    async fn book_media_is_ready(&self, book_id: &str) -> Result<bool, String>;
    async fn book_pages(&self, book_id: &str) -> Result<Vec<BookPageRecord>, String>;
    async fn book_page(
        &self,
        book_id: &str,
        page_number: u64,
    ) -> Result<Option<BookPageRecord>, String>;
    async fn epub_extension_blob(&self, book_id: &str)
    -> Result<Option<(String, Vec<u8>)>, String>;
}

/// Read access to series/book relationship data.
#[async_trait]
pub trait SeriesRelationPort: Send + Sync {
    async fn series_book_ids(&self, series_id: &str) -> Result<Vec<String>, String>;
    async fn series_book_number_sorts(&self, series_id: &str)
    -> Result<Vec<(String, f64)>, String>;
    async fn series_oneshot(&self, series_id: &str) -> Result<Option<bool>, String>;
}

/// Existence checks for entities.
#[async_trait]
pub trait EntityExistencePort: Send + Sync {
    async fn book_exists(&self, book_id: &str) -> Result<bool, String>;
    async fn series_exists(&self, series_id: &str) -> Result<bool, String>;
    async fn readlist_exists(&self, readlist_id: &str) -> Result<bool, String>;
    async fn collection_exists(&self, collection_id: &str) -> Result<bool, String>;
}

/// Access control and content manifest queries.
#[async_trait]
pub trait ContentAccessPort: Send + Sync {
    async fn book_restrictions(
        &self,
        book_id: &str,
    ) -> Result<Option<(Option<u16>, Vec<String>)>, String>;
    async fn readlist_archive_entries(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<(String, PathBuf)>, String>;
    async fn series_archive_entries(
        &self,
        series_id: &str,
    ) -> Result<Option<(String, String, Vec<(String, PathBuf)>)>, String>;
    async fn manifest_book(
        &self,
        book_id: &str,
    ) -> Result<Option<(String, String, String)>, String>;
    async fn readlist_name(&self, readlist_id: &str) -> Result<Option<String>, String>;
}

/// Read access to thumbnails across all entity types.
#[async_trait]
pub trait ThumbnailReadPort: Send + Sync {
    async fn selected_book_thumbnail(
        &self,
        book_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String>;
    async fn book_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String>;
    async fn book_thumbnails(&self, book_id: &str) -> Result<Vec<EntityThumbnailRecord>, String>;
    async fn selected_series_thumbnail(
        &self,
        series_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String>;
    async fn series_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String>;
    async fn series_thumbnails(
        &self,
        series_id: &str,
    ) -> Result<Vec<SeriesThumbnailRecord>, String>;
    async fn readlist_thumbnails(
        &self,
        readlist_id: &str,
    ) -> Result<Vec<ReadlistThumbnailRecord>, String>;
    async fn collection_thumbnails(
        &self,
        collection_id: &str,
    ) -> Result<Vec<CollectionThumbnailRecord>, String>;
}

/// Read access to reading progress data.
#[async_trait]
pub trait ReadProgressReadPort: Send + Sync {
    async fn book_progression(&self, book_id: &str, user_id: &str)
    -> Result<Option<Value>, String>;
    async fn book_read_progress_completed(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<bool>, String>;
    async fn series_tachiyomi_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> Result<Option<Value>, String>;
    async fn readlist_tachiyomi_counters(
        &self,
        ordered_book_ids: &[String],
        user_id: &str,
    ) -> Result<(u64, u64, u64, u64, u64), String>;
    async fn book_page_count(&self, book_id: &str) -> Result<Option<u64>, String>;
}

/// Read access needed by read-progress write orchestration.
#[async_trait]
pub trait ReadProgressSurfacePort: Send + Sync {
    async fn series_book_ids(&self, series_id: &str) -> Result<Vec<String>, String>;
    async fn series_book_number_sorts(&self, series_id: &str)
    -> Result<Vec<(String, f64)>, String>;
    async fn book_read_progress_completed(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<bool>, String>;
    async fn book_page_count(&self, book_id: &str) -> Result<Option<u64>, String>;
}

#[async_trait]
impl<T> ReadProgressSurfacePort for T
where
    T: ReadProgressReadPort + SeriesRelationPort + Send + Sync,
{
    async fn series_book_ids(&self, series_id: &str) -> Result<Vec<String>, String> {
        SeriesRelationPort::series_book_ids(self, series_id).await
    }

    async fn series_book_number_sorts(
        &self,
        series_id: &str,
    ) -> Result<Vec<(String, f64)>, String> {
        SeriesRelationPort::series_book_number_sorts(self, series_id).await
    }

    async fn book_read_progress_completed(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<bool>, String> {
        ReadProgressReadPort::book_read_progress_completed(self, book_id, user_id).await
    }

    async fn book_page_count(&self, book_id: &str) -> Result<Option<u64>, String> {
        ReadProgressReadPort::book_page_count(self, book_id).await
    }
}

/// Supertrait aggregating all media reader sub-ports for backward compatibility.
pub trait MediaReaderPort:
    BookMediaPort
    + SeriesRelationPort
    + EntityExistencePort
    + ContentAccessPort
    + ThumbnailReadPort
    + ReadProgressReadPort
{
}

impl<T> MediaReaderPort for T where
    T: BookMediaPort
        + SeriesRelationPort
        + EntityExistencePort
        + ContentAccessPort
        + ThumbnailReadPort
        + ReadProgressReadPort
{
}
