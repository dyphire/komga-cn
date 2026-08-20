use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use serde_json::Value;

use super::{
    BookMediaRecord, BookPageRecord, CollectionThumbnailRecord, EntityThumbnailBinary,
    EntityThumbnailRecord, PersistedMediaFileRecord, ReadlistThumbnailRecord,
    SeriesThumbnailRecord,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageOutputFormat {
    Avif,
    Webp,
    Jpeg,
}

impl ImageOutputFormat {
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Avif => "image/avif",
            Self::Webp => "image/webp",
            Self::Jpeg => "image/jpeg",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedImage {
    pub bytes: Vec<u8>,
    pub format: ImageOutputFormat,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EpubNavigationExtension {
    pub positions: Vec<EpubNavigationPosition>,
    pub is_fixed_layout: bool,
    pub toc: Vec<EpubNavigationLink>,
    pub landmarks: Vec<EpubNavigationLink>,
    pub page_list: Vec<EpubNavigationLink>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EpubNavigationPosition {
    raw: Value,
    href: Option<String>,
    media_type: Option<Value>,
    kobo_span: Option<Value>,
    progression: Option<f64>,
    total_progression: Option<Value>,
    position: Option<i64>,
}

impl EpubNavigationPosition {
    pub fn from_raw(raw: Value) -> Self {
        let href = raw.get("href").and_then(Value::as_str).map(str::to_string);
        let media_type = raw.get("type").cloned();
        let kobo_span = raw.get("koboSpan").cloned();
        let locations = raw.get("locations");
        let progression = locations
            .and_then(|value| value.get("progression"))
            .and_then(Value::as_f64);
        let total_progression = locations
            .and_then(|value| value.get("totalProgression"))
            .cloned();
        let position = locations
            .and_then(|value| value.get("position"))
            .and_then(Value::as_i64);

        Self {
            raw,
            href,
            media_type,
            kobo_span,
            progression,
            total_progression,
            position,
        }
    }

    pub fn raw(&self) -> &Value {
        &self.raw
    }

    pub fn into_raw(self) -> Value {
        self.raw
    }

    pub fn href(&self) -> Option<&str> {
        self.href.as_deref()
    }

    pub fn media_type(&self) -> Option<&Value> {
        self.media_type.as_ref()
    }

    pub fn kobo_span(&self) -> Option<&Value> {
        self.kobo_span.as_ref()
    }

    pub fn progression(&self) -> Option<f64> {
        self.progression
    }

    pub fn total_progression(&self) -> Option<f64> {
        self.total_progression.as_ref().and_then(Value::as_f64)
    }

    pub fn total_progression_value(&self) -> Option<&Value> {
        self.total_progression.as_ref()
    }

    pub fn position(&self) -> Option<i64> {
        self.position
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EpubNavigationLink {
    pub title: Option<String>,
    pub href: Option<String>,
    pub children: Vec<EpubNavigationLink>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArchiveEntry {
    pub file_name: String,
    pub file_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesArchiveEntries {
    pub series_title: String,
    pub entries: Vec<ArchiveEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestBookRecord {
    pub library_id: String,
    pub title: String,
    pub media_type: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadlistTachiyomiCounters {
    pub books_count: u64,
    pub books_read_count: u64,
    pub books_unread_count: u64,
    pub books_in_progress_count: u64,
    pub last_read_continuous_index: u64,
}

impl ReadlistTachiyomiCounters {
    pub fn empty() -> Self {
        Self {
            books_count: 0,
            books_read_count: 0,
            books_unread_count: 0,
            books_in_progress_count: 0,
            last_read_continuous_index: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeriesTachiyomiProgress {
    pub books_count: u64,
    pub books_read_count: u64,
    pub books_unread_count: u64,
    pub books_in_progress_count: u64,
    pub last_read_continuous_number_sort: f64,
    pub max_number_sort: f64,
}

impl SeriesTachiyomiProgress {
    pub fn from_books(books: impl IntoIterator<Item = SeriesTachiyomiProgressBook>) -> Self {
        let mut progress = Self {
            books_count: 0,
            books_read_count: 0,
            books_unread_count: 0,
            books_in_progress_count: 0,
            last_read_continuous_number_sort: 0.0,
            max_number_sort: 0.0,
        };
        let mut all_previous_completed = true;

        for book in books {
            progress.books_count += 1;
            if book.number_sort > progress.max_number_sort {
                progress.max_number_sort = book.number_sort;
            }

            match book.completed {
                Some(true) => {
                    progress.books_read_count += 1;
                    if all_previous_completed {
                        progress.last_read_continuous_number_sort = book.number_sort;
                    }
                }
                Some(false) => {
                    progress.books_in_progress_count += 1;
                    all_previous_completed = false;
                }
                None => {
                    progress.books_unread_count += 1;
                    all_previous_completed = false;
                }
            }
        }

        progress
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeriesTachiyomiProgressBook {
    pub number_sort: f64,
    pub completed: Option<bool>,
}

pub struct BookProgressionInput {
    pub book_id: String,
    pub user_id: String,
    pub page: u64,
    pub completed: bool,
    pub modified: Option<String>,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub locator: Option<Value>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BookProgressionRecord {
    pub modified: String,
    pub device_id: String,
    pub device_name: String,
    pub locator: Value,
}

/// Write operations for read progress (book and series level).
#[async_trait::async_trait]
pub trait ProgressWriterPort: Send + Sync {
    async fn persist_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
        page: u64,
        completed: bool,
        locator: Option<Value>,
    ) -> anyhow::Result<()>;

    async fn persist_book_progression(&self, input: BookProgressionInput) -> anyhow::Result<()>;

    async fn delete_read_progress(&self, book_id: &str, user_id: &str) -> anyhow::Result<()>;

    async fn refresh_series_read_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> anyhow::Result<()>;

    async fn delete_series_read_progress(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> anyhow::Result<()>;
}

/// Write operations for thumbnails across all entity types.
#[async_trait::async_trait]
pub trait ThumbnailWriterPort: Send + Sync {
    async fn insert_book(
        &self,
        book_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> anyhow::Result<EntityThumbnailRecord>;

    async fn select_book(&self, thumbnail_id: &str) -> anyhow::Result<bool>;

    async fn delete_book(&self, thumbnail_id: &str) -> anyhow::Result<bool>;

    async fn insert_series(
        &self,
        series_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> anyhow::Result<SeriesThumbnailRecord>;

    async fn select_series(&self, series_id: &str, thumbnail_id: &str) -> anyhow::Result<bool>;

    async fn delete_series(&self, series_id: &str, thumbnail_id: &str) -> anyhow::Result<bool>;

    async fn insert_readlist(
        &self,
        readlist_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> anyhow::Result<ReadlistThumbnailRecord>;

    async fn select_readlist(&self, readlist_id: &str, thumbnail_id: &str) -> anyhow::Result<bool>;

    async fn delete_readlist(&self, readlist_id: &str, thumbnail_id: &str) -> anyhow::Result<bool>;

    async fn insert_collection(
        &self,
        collection_id: &str,
        thumbnail: &[u8],
        media_type: &str,
        width: i64,
        height: i64,
        selected: bool,
    ) -> anyhow::Result<CollectionThumbnailRecord>;

    async fn select_collection(&self, thumbnail_id: &str) -> anyhow::Result<bool>;

    async fn delete_collection(
        &self,
        collection_id: &str,
        thumbnail_id: &str,
    ) -> anyhow::Result<bool>;
}

/// Stateless filesystem I/O for resolving page/resource content from archives and PDFs.
#[async_trait::async_trait]
pub trait ContentResolverPort: Send + Sync {
    async fn resolve_page_bytes(
        &self,
        media: &BookMediaRecord,
        page: &BookPageRecord,
        page_number: u64,
    ) -> anyhow::Result<Option<Vec<u8>>>;

    async fn render_page_thumbnail(
        &self,
        media: &BookMediaRecord,
        page: &BookPageRecord,
        page_number: u64,
        max_edge: u32,
        output_format: ImageOutputFormat,
    ) -> anyhow::Result<Option<RenderedImage>>;

    async fn render_pdf_page(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
        output_format: ImageOutputFormat,
    ) -> anyhow::Result<Option<RenderedImage>>;

    async fn archive_page_row(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> anyhow::Result<Option<BookPageRecord>>;

    async fn archive_page_rows(
        &self,
        media: &BookMediaRecord,
    ) -> anyhow::Result<Option<Vec<BookPageRecord>>>;

    fn pdf_page_row(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> anyhow::Result<Option<BookPageRecord>>;

    fn read_pdf_page_as_single_page_pdf(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> anyhow::Result<Option<Vec<u8>>>;

    fn media_file_exists(&self, path: &Path) -> anyhow::Result<bool> {
        path.try_exists()
            .with_context(|| format!("check media file existence '{}'", path.display()))
    }

    async fn read_media_file_bytes(&self, path: &Path) -> anyhow::Result<Option<Vec<u8>>>;

    async fn read_epub_publication_bytes(
        &self,
        media: &BookMediaRecord,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        self.read_media_file_bytes(&media.file_path).await
    }

    async fn read_media_file_size(&self, path: &Path) -> anyhow::Result<Option<i64>>;

    async fn read_media_image_dimensions(
        &self,
        _path: &Path,
    ) -> anyhow::Result<Option<MediaImageDimensions>> {
        Ok(None)
    }

    fn convert_image_bytes(
        &self,
        bytes: &[u8],
        source_content_type: &str,
        target_content_type: &str,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        if source_content_type.eq_ignore_ascii_case(target_content_type) {
            return Ok(Some(bytes.to_vec()));
        }
        Ok(None)
    }

    async fn read_epub_resource_bytes(
        &self,
        epub_path: &Path,
        resource_name: &str,
    ) -> anyhow::Result<Option<Vec<u8>>>;

    fn decode_epub_navigation_extension(
        &self,
        blob: &[u8],
    ) -> anyhow::Result<EpubNavigationExtension>;

    async fn epub_cover_bytes(
        &self,
        media: &BookMediaRecord,
    ) -> anyhow::Result<Option<EpubCoverImage>>;

    async fn epub_package_document(
        &self,
        media: &BookMediaRecord,
    ) -> anyhow::Result<Option<Vec<u8>>>;

    fn epub_fixed_layout(&self, package_document: &[u8]) -> bool;

    fn normalize_epub_resource_href(&self, rootfile_path: &str, href: &str) -> String;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EpubExtensionBlob {
    pub extension_class: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EpubCoverImage {
    pub bytes: Vec<u8>,
    pub media_type: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaImageDimensions {
    pub width: i64,
    pub height: i64,
}

/// Read access to book media metadata.
#[async_trait::async_trait]
pub trait BookMediaPort: Send + Sync {
    async fn book_media(&self, book_id: &str) -> anyhow::Result<Option<BookMediaRecord>>;
    async fn book_media_files(&self, book_id: &str) -> anyhow::Result<Vec<String>>;
    async fn media_file_records(
        &self,
        book_id: &str,
    ) -> anyhow::Result<Vec<PersistedMediaFileRecord>>;
    async fn book_media_is_ready(&self, book_id: &str) -> anyhow::Result<bool>;
    async fn book_pages(&self, book_id: &str) -> anyhow::Result<Vec<BookPageRecord>>;
    async fn book_page(
        &self,
        book_id: &str,
        page_number: u64,
    ) -> anyhow::Result<Option<BookPageRecord>>;
    async fn epub_extension_blob(&self, book_id: &str)
    -> anyhow::Result<Option<EpubExtensionBlob>>;
}

/// Read access to series/book relationship data.
#[derive(Clone, Debug, PartialEq)]
pub struct SeriesBookNumberSort {
    pub book_id: String,
    pub number_sort: f64,
}

#[async_trait::async_trait]
pub trait SeriesRelationPort: Send + Sync {
    async fn series_book_ids(&self, series_id: &str) -> anyhow::Result<Vec<String>>;
    async fn series_book_number_sorts(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Vec<SeriesBookNumberSort>>;
    async fn series_oneshot(&self, series_id: &str) -> anyhow::Result<Option<bool>>;
}

/// Existence checks for entities.
#[async_trait::async_trait]
pub trait EntityExistencePort: Send + Sync {
    async fn book_exists(&self, book_id: &str) -> anyhow::Result<bool>;
    async fn series_exists(&self, series_id: &str) -> anyhow::Result<bool>;
    async fn readlist_exists(&self, readlist_id: &str) -> anyhow::Result<bool>;
    async fn collection_exists(&self, collection_id: &str) -> anyhow::Result<bool>;
}

/// Access control and content manifest queries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookAccessRestrictions {
    pub age_rating: Option<u32>,
    pub labels: Vec<String>,
}

#[async_trait::async_trait]
pub trait ContentAccessPort: Send + Sync {
    async fn book_restrictions(
        &self,
        book_id: &str,
    ) -> anyhow::Result<Option<BookAccessRestrictions>>;
    async fn series_archive_entries(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Option<SeriesArchiveEntries>>;
    async fn manifest_book(&self, book_id: &str) -> anyhow::Result<Option<ManifestBookRecord>>;
    async fn readlist_name(&self, readlist_id: &str) -> anyhow::Result<Option<String>>;
}

/// Read access to thumbnails across all entity types.
#[async_trait::async_trait]
pub trait ThumbnailReadPort: Send + Sync {
    async fn selected_book_thumbnail(
        &self,
        book_id: &str,
    ) -> anyhow::Result<Option<EntityThumbnailBinary>>;
    async fn book_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> anyhow::Result<Option<EntityThumbnailBinary>>;
    async fn book_thumbnails(&self, book_id: &str) -> anyhow::Result<Vec<EntityThumbnailRecord>>;
    async fn selected_series_thumbnail(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Option<EntityThumbnailBinary>>;
    async fn series_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> anyhow::Result<Option<EntityThumbnailBinary>>;
    async fn series_thumbnails(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Vec<SeriesThumbnailRecord>>;
    async fn readlist_thumbnails(
        &self,
        readlist_id: &str,
    ) -> anyhow::Result<Vec<ReadlistThumbnailRecord>>;
    async fn readlist_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> anyhow::Result<Option<ReadlistThumbnailRecord>>;
    async fn collection_thumbnails(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Vec<CollectionThumbnailRecord>>;
    async fn collection_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> anyhow::Result<Option<CollectionThumbnailRecord>>;
}

/// Read access to reading progress data.
#[async_trait::async_trait]
pub trait ReadProgressReadPort: Send + Sync {
    async fn book_progression(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> anyhow::Result<Option<BookProgressionRecord>>;
    async fn book_read_progress_completed(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> anyhow::Result<Option<bool>>;
    async fn series_tachiyomi_progress_books(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> anyhow::Result<Vec<SeriesTachiyomiProgressBook>>;
    async fn read_progress_completed_by_book_ids(
        &self,
        ordered_book_ids: &[String],
        user_id: &str,
    ) -> anyhow::Result<Vec<Option<bool>>>;
    async fn book_page_count(&self, book_id: &str) -> anyhow::Result<Option<u64>>;
}

/// Read access needed by read-progress orchestration.
#[async_trait::async_trait]
pub trait ReadProgressSurfacePort: Send + Sync {
    async fn series_book_ids(&self, series_id: &str) -> anyhow::Result<Vec<String>>;
    async fn series_book_number_sorts(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Vec<SeriesBookNumberSort>>;
    async fn book_read_progress_completed(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> anyhow::Result<Option<bool>>;
    async fn series_tachiyomi_progress_books(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> anyhow::Result<Vec<SeriesTachiyomiProgressBook>>;
    async fn read_progress_completed_by_book_ids(
        &self,
        ordered_book_ids: &[String],
        user_id: &str,
    ) -> anyhow::Result<Vec<Option<bool>>>;
    async fn book_page_count(&self, book_id: &str) -> anyhow::Result<Option<u64>>;
}

/// Read access needed by direct read-progress routes.
#[async_trait::async_trait]
pub trait ReadProgressReaderPort: Send + Sync {
    async fn book_exists(&self, book_id: &str) -> anyhow::Result<bool>;
    async fn series_exists(&self, series_id: &str) -> anyhow::Result<bool>;
    async fn book_page_count(&self, book_id: &str) -> anyhow::Result<Option<u64>>;
}

#[async_trait::async_trait]
impl<T> ReadProgressReaderPort for T
where
    T: EntityExistencePort + ReadProgressReadPort + Send + Sync,
{
    async fn book_exists(&self, book_id: &str) -> anyhow::Result<bool> {
        EntityExistencePort::book_exists(self, book_id).await
    }

    async fn series_exists(&self, series_id: &str) -> anyhow::Result<bool> {
        EntityExistencePort::series_exists(self, series_id).await
    }

    async fn book_page_count(&self, book_id: &str) -> anyhow::Result<Option<u64>> {
        ReadProgressReadPort::book_page_count(self, book_id).await
    }
}

#[async_trait::async_trait]
impl<T> ReadProgressSurfacePort for T
where
    T: ReadProgressReadPort + SeriesRelationPort + Send + Sync,
{
    async fn series_book_ids(&self, series_id: &str) -> anyhow::Result<Vec<String>> {
        SeriesRelationPort::series_book_ids(self, series_id).await
    }

    async fn series_book_number_sorts(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Vec<SeriesBookNumberSort>> {
        SeriesRelationPort::series_book_number_sorts(self, series_id).await
    }

    async fn book_read_progress_completed(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> anyhow::Result<Option<bool>> {
        ReadProgressReadPort::book_read_progress_completed(self, book_id, user_id).await
    }

    async fn read_progress_completed_by_book_ids(
        &self,
        ordered_book_ids: &[String],
        user_id: &str,
    ) -> anyhow::Result<Vec<Option<bool>>> {
        ReadProgressReadPort::read_progress_completed_by_book_ids(self, ordered_book_ids, user_id)
            .await
    }

    async fn series_tachiyomi_progress_books(
        &self,
        series_id: &str,
        user_id: &str,
    ) -> anyhow::Result<Vec<SeriesTachiyomiProgressBook>> {
        ReadProgressReadPort::series_tachiyomi_progress_books(self, series_id, user_id).await
    }

    async fn book_page_count(&self, book_id: &str) -> anyhow::Result<Option<u64>> {
        ReadProgressReadPort::book_page_count(self, book_id).await
    }
}
