use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::discovery::BookDetailPort;
use crate::identity_access::{AuthUser, user_has_role, user_is_admin};

use super::book_access::BookAccessContext;
use super::{
    BookMediaPort, BookMediaRecord, BookPageRecord, ContentAccessPort, ContentResolverPort,
    EntityThumbnailBinary, ThumbnailReadPort, book_media_is_epub, book_media_is_pdf,
    book_media_is_single_image, book_media_supports_page_api, content_type_from_filename,
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BookMediaPageRequest {
    pub convert: Option<String>,
    pub zero_based: bool,
    pub prefer_pdf: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BookMediaDelivery {
    Asset(BookMediaDeliveryAsset),
    Pages(Vec<BookPageRecord>),
    NotFound,
    Forbidden,
    MediaAnalysisFailed,
    MissingFile,
    BadRequest(Option<String>),
    Internal(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookMediaDeliveryAsset {
    pub bytes: Vec<u8>,
    pub content_type: String,
    pub file_name: Option<String>,
    pub source_file: Option<PathBuf>,
    pub disposition: BookMediaDeliveryDisposition,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookMediaDeliveryDisposition {
    Attachment,
    Inline,
    None,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BookThumbnailDelivery {
    Thumbnail(BookThumbnailAsset),
    NotFound,
    Forbidden,
    Internal(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookThumbnailAsset {
    pub bytes: Vec<u8>,
    pub media_type: String,
    pub generated: bool,
}

pub struct BookMediaDeliveryService<'a, R, C, B>
where
    R: BookMediaReaderPort + ?Sized,
    C: BookMediaContentPort + ?Sized,
    B: PersistedBookIdResolverPort + ?Sized,
{
    reader: &'a R,
    content: &'a C,
    book_ids: &'a B,
}

#[async_trait]
pub trait BookMediaReaderPort: Send + Sync {
    async fn book_media(&self, book_id: &str) -> Result<Option<BookMediaRecord>, String>;

    async fn book_media_is_ready(&self, book_id: &str) -> Result<bool, String>;

    async fn book_pages(&self, book_id: &str) -> Result<Vec<BookPageRecord>, String>;

    async fn book_page(
        &self,
        book_id: &str,
        page_number: u64,
    ) -> Result<Option<BookPageRecord>, String>;

    async fn book_restrictions(
        &self,
        book_id: &str,
    ) -> Result<Option<(Option<u16>, Vec<String>)>, String>;

    async fn selected_book_thumbnail(
        &self,
        book_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String>;
}

#[async_trait]
impl<T> BookMediaReaderPort for T
where
    T: BookMediaPort + ContentAccessPort + ThumbnailReadPort + Send + Sync + ?Sized,
{
    async fn book_media(&self, book_id: &str) -> Result<Option<BookMediaRecord>, String> {
        BookMediaPort::book_media(self, book_id).await
    }

    async fn book_media_is_ready(&self, book_id: &str) -> Result<bool, String> {
        BookMediaPort::book_media_is_ready(self, book_id).await
    }

    async fn book_pages(&self, book_id: &str) -> Result<Vec<BookPageRecord>, String> {
        BookMediaPort::book_pages(self, book_id).await
    }

    async fn book_page(
        &self,
        book_id: &str,
        page_number: u64,
    ) -> Result<Option<BookPageRecord>, String> {
        BookMediaPort::book_page(self, book_id, page_number).await
    }

    async fn book_restrictions(
        &self,
        book_id: &str,
    ) -> Result<Option<(Option<u16>, Vec<String>)>, String> {
        ContentAccessPort::book_restrictions(self, book_id).await
    }

    async fn selected_book_thumbnail(
        &self,
        book_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String> {
        ThumbnailReadPort::selected_book_thumbnail(self, book_id).await
    }
}

#[async_trait]
pub trait BookMediaContentPort: Send + Sync {
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

    fn media_file_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    async fn read_media_file_bytes(&self, path: &Path) -> Option<Vec<u8>>;

    async fn read_media_file_size(&self, path: &Path) -> Option<i64>;

    async fn read_media_image_dimensions(&self, path: &Path) -> Option<(i64, i64)>;

    fn convert_image_bytes(
        &self,
        bytes: &[u8],
        source_content_type: &str,
        target_content_type: &str,
    ) -> Option<Vec<u8>>;

    async fn epub_cover_bytes(&self, media: &BookMediaRecord) -> Option<(Vec<u8>, String)>;
}

#[async_trait]
impl<T> BookMediaContentPort for T
where
    T: ContentResolverPort + Send + Sync + ?Sized,
{
    async fn resolve_page_bytes(
        &self,
        media: &BookMediaRecord,
        page: &BookPageRecord,
        page_number: u64,
    ) -> Option<Vec<u8>> {
        ContentResolverPort::resolve_page_bytes(self, media, page, page_number).await
    }

    async fn render_page_thumbnail(
        &self,
        media: &BookMediaRecord,
        page: &BookPageRecord,
        page_number: u64,
        max_edge: u32,
    ) -> Option<Vec<u8>> {
        ContentResolverPort::render_page_thumbnail(self, media, page, page_number, max_edge).await
    }

    async fn archive_page_row(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> Option<BookPageRecord> {
        ContentResolverPort::archive_page_row(self, media, page_number).await
    }

    async fn archive_page_rows(&self, media: &BookMediaRecord) -> Option<Vec<BookPageRecord>> {
        ContentResolverPort::archive_page_rows(self, media).await
    }

    fn pdf_page_row(&self, media: &BookMediaRecord, page_number: u64) -> Option<BookPageRecord> {
        ContentResolverPort::pdf_page_row(self, media, page_number)
    }

    fn generated_pdf_page_rows(&self, media: &BookMediaRecord) -> Vec<BookPageRecord> {
        ContentResolverPort::generated_pdf_page_rows(self, media)
    }

    fn read_pdf_page_as_single_page_pdf(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> Option<Vec<u8>> {
        ContentResolverPort::read_pdf_page_as_single_page_pdf(self, media, page_number)
    }

    fn detect_pdf_page_count(&self, media: &BookMediaRecord) -> Option<u64> {
        ContentResolverPort::detect_pdf_page_count(self, media)
    }

    async fn read_media_file_bytes(&self, path: &Path) -> Option<Vec<u8>> {
        ContentResolverPort::read_media_file_bytes(self, path).await
    }

    async fn read_media_file_size(&self, path: &Path) -> Option<i64> {
        ContentResolverPort::read_media_file_size(self, path).await
    }

    async fn read_media_image_dimensions(&self, path: &Path) -> Option<(i64, i64)> {
        ContentResolverPort::read_media_image_dimensions(self, path).await
    }

    fn convert_image_bytes(
        &self,
        bytes: &[u8],
        source_content_type: &str,
        target_content_type: &str,
    ) -> Option<Vec<u8>> {
        ContentResolverPort::convert_image_bytes(
            self,
            bytes,
            source_content_type,
            target_content_type,
        )
    }

    async fn epub_cover_bytes(&self, media: &BookMediaRecord) -> Option<(Vec<u8>, String)> {
        ContentResolverPort::epub_cover_bytes(self, media).await
    }
}

#[async_trait]
pub trait PersistedBookIdResolverPort: Send + Sync {
    async fn persisted_book_resource_exists(&self, book_id: &str) -> Result<bool, String>;

    async fn load_book_id_by_sorted_position(&self, index: usize)
    -> Result<Option<String>, String>;
}

#[async_trait]
impl<T> PersistedBookIdResolverPort for T
where
    T: BookDetailPort + Send + Sync + ?Sized,
{
    async fn persisted_book_resource_exists(&self, book_id: &str) -> Result<bool, String> {
        self.load_persisted_book_resource(book_id)
            .await
            .map(|record| record.is_some())
    }

    async fn load_book_id_by_sorted_position(
        &self,
        index: usize,
    ) -> Result<Option<String>, String> {
        BookDetailPort::load_book_id_by_sorted_position(self, index).await
    }
}

impl<'a, R, C, B> BookMediaDeliveryService<'a, R, C, B>
where
    R: BookMediaReaderPort + ?Sized,
    C: BookMediaContentPort + ?Sized,
    B: PersistedBookIdResolverPort + ?Sized,
{
    pub fn new(reader: &'a R, content: &'a C, book_ids: &'a B) -> Self {
        Self {
            reader,
            content,
            book_ids,
        }
    }

    pub async fn book_file(&self, user: &AuthUser, book_id: &str) -> BookMediaDelivery {
        let media = match self.reader.book_media(book_id).await {
            Ok(Some(media)) => media,
            Ok(None) => return BookMediaDelivery::NotFound,
            Err(error) => return BookMediaDelivery::Internal(error),
        };
        if !self.user_can_access_book(book_id, user, &media).await {
            return BookMediaDelivery::Forbidden;
        }

        let Some(bytes) = self.content.read_media_file_bytes(&media.file_path).await else {
            return BookMediaDelivery::MissingFile;
        };

        BookMediaDelivery::Asset(BookMediaDeliveryAsset {
            bytes,
            content_type: media.media_type,
            file_name: Some(media.file_name),
            source_file: Some(media.file_path),
            disposition: BookMediaDeliveryDisposition::Attachment,
        })
    }

    pub async fn book_page(
        &self,
        user: &AuthUser,
        book_id: &str,
        page_number: u32,
        request: BookMediaPageRequest,
    ) -> BookMediaDelivery {
        let resolved_book_id = self.resolve_book_id(book_id).await;
        let requested_page_number = if request.zero_based {
            page_number.saturating_add(1)
        } else {
            page_number
        };
        if requested_page_number == 0 {
            return page_number_does_not_exist();
        }

        let requested_convert = match validated_convert(request.convert.as_deref()) {
            Ok(convert) => convert,
            Err(()) => return BookMediaDelivery::BadRequest(None),
        };

        let media = match self.reader.book_media(&resolved_book_id).await {
            Ok(Some(media)) => media,
            Ok(None) => return BookMediaDelivery::NotFound,
            Err(error) => return BookMediaDelivery::Internal(error),
        };
        if !self
            .reader
            .book_media_is_ready(&resolved_book_id)
            .await
            .unwrap_or(false)
        {
            return BookMediaDelivery::MediaAnalysisFailed;
        }
        if !can_stream_pages(user) {
            return BookMediaDelivery::Forbidden;
        }
        if !self
            .user_can_access_book(&resolved_book_id, user, &media)
            .await
        {
            return BookMediaDelivery::Forbidden;
        }
        if !book_media_supports_page_api(&media) {
            return BookMediaDelivery::NotFound;
        }

        if request.prefer_pdf && book_media_is_pdf(&media) {
            return self
                .pdf_page_asset(&media, requested_page_number as u64)
                .await;
        }

        let page_row = match self
            .load_book_page_row(
                &resolved_book_id,
                &media,
                requested_page_number as u64,
                true,
            )
            .await
        {
            Ok(Some(row)) => row,
            Ok(None) => return page_number_does_not_exist(),
            Err(error) => return BookMediaDelivery::Internal(error),
        };

        let Some(bytes) = self
            .content
            .resolve_page_bytes(&media, &page_row, requested_page_number as u64)
            .await
        else {
            return BookMediaDelivery::NotFound;
        };
        let content_type = page_row_media_type(&page_row, &media);
        let (bytes, content_type) = match requested_convert {
            Some(convert) => {
                let target_content_type = convert.content_type();
                let Some(converted) =
                    self.content
                        .convert_image_bytes(&bytes, &content_type, target_content_type)
                else {
                    return BookMediaDelivery::NotFound;
                };
                (converted, target_content_type.to_string())
            }
            None => (bytes, content_type),
        };

        BookMediaDelivery::Asset(page_asset(
            &media,
            requested_page_number,
            content_type,
            bytes,
        ))
    }

    pub async fn book_page_raw(
        &self,
        user: &AuthUser,
        book_id: &str,
        page_number: i32,
    ) -> BookMediaDelivery {
        if page_number <= 0 {
            return page_number_does_not_exist();
        }
        let page_number = page_number as u32;
        let resolved_book_id = self.resolve_book_id(book_id).await;
        let media = match self.reader.book_media(&resolved_book_id).await {
            Ok(Some(media)) => media,
            Ok(None) => return BookMediaDelivery::NotFound,
            Err(error) => return BookMediaDelivery::Internal(error),
        };
        if !user_has_role(user, "PAGE_STREAMING") {
            return BookMediaDelivery::Forbidden;
        }
        if !self
            .user_can_access_book(&resolved_book_id, user, &media)
            .await
        {
            return BookMediaDelivery::Forbidden;
        }
        if !book_media_is_pdf(&media) {
            return BookMediaDelivery::BadRequest(Some(
                "Extractor does not support raw extraction of pages".to_string(),
            ));
        }
        if !self
            .reader
            .book_media_is_ready(&resolved_book_id)
            .await
            .unwrap_or(false)
        {
            return BookMediaDelivery::MediaAnalysisFailed;
        }
        if !self.content.media_file_exists(&media.file_path) {
            return BookMediaDelivery::MissingFile;
        }

        self.pdf_page_asset(&media, page_number as u64).await
    }

    pub async fn book_page_thumbnail(
        &self,
        user: &AuthUser,
        book_id: &str,
        page_number: u32,
    ) -> BookMediaDelivery {
        if page_number == 0 {
            return page_number_does_not_exist();
        }
        let resolved_book_id = self.resolve_book_id(book_id).await;
        let media = match self.reader.book_media(&resolved_book_id).await {
            Ok(Some(media)) => media,
            Ok(None) => return BookMediaDelivery::NotFound,
            Err(error) => return BookMediaDelivery::Internal(error),
        };
        if !self
            .user_can_access_book(&resolved_book_id, user, &media)
            .await
        {
            return BookMediaDelivery::Forbidden;
        }
        if !book_media_supports_page_api(&media) {
            return BookMediaDelivery::NotFound;
        }

        let page_row = match self
            .load_book_page_row(&resolved_book_id, &media, page_number as u64, true)
            .await
        {
            Ok(Some(row)) => row,
            Ok(None) => return page_number_does_not_exist(),
            Err(error) => return BookMediaDelivery::Internal(error),
        };
        let Some(bytes) = self
            .content
            .render_page_thumbnail(&media, &page_row, page_number as u64, 300)
            .await
        else {
            return BookMediaDelivery::NotFound;
        };

        BookMediaDelivery::Asset(BookMediaDeliveryAsset {
            bytes,
            content_type: "image/jpeg".to_string(),
            file_name: None,
            source_file: Some(media.file_path),
            disposition: BookMediaDeliveryDisposition::None,
        })
    }

    pub async fn book_pages(&self, user: &AuthUser, book_id: &str) -> BookMediaDelivery {
        let resolved_book_id = self.resolve_book_id(book_id).await;
        let media = match self.reader.book_media(&resolved_book_id).await {
            Ok(Some(media)) => media,
            Ok(None) => return BookMediaDelivery::NotFound,
            Err(error) => return BookMediaDelivery::Internal(error),
        };
        if !self
            .user_can_access_book(&resolved_book_id, user, &media)
            .await
        {
            return BookMediaDelivery::Forbidden;
        }
        if !self
            .reader
            .book_media_is_ready(&resolved_book_id)
            .await
            .unwrap_or(false)
        {
            return BookMediaDelivery::NotFound;
        }
        if !book_media_supports_page_api(&media) {
            return BookMediaDelivery::NotFound;
        }

        match self.list_book_page_rows(&resolved_book_id, &media).await {
            Ok(Some(page_rows)) => BookMediaDelivery::Pages(page_rows),
            Ok(None) => BookMediaDelivery::NotFound,
            Err(error) => BookMediaDelivery::Internal(error),
        }
    }

    pub async fn book_thumbnail_source(
        &self,
        user: &AuthUser,
        book_id: &str,
    ) -> BookThumbnailDelivery {
        let media = match self.reader.book_media(book_id).await {
            Ok(Some(media)) => media,
            Ok(None) => return BookThumbnailDelivery::NotFound,
            Err(error) => return BookThumbnailDelivery::Internal(error),
        };
        if !self.user_can_access_book(book_id, user, &media).await {
            return BookThumbnailDelivery::Forbidden;
        }

        match self.load_book_thumbnail_source(book_id, &media).await {
            Some(thumbnail) => BookThumbnailDelivery::Thumbnail(thumbnail),
            None => BookThumbnailDelivery::NotFound,
        }
    }

    pub async fn selected_book_thumbnail(
        &self,
        user: &AuthUser,
        book_id: &str,
    ) -> BookThumbnailDelivery {
        let media = match self.reader.book_media(book_id).await {
            Ok(Some(media)) => media,
            Ok(None) => return BookThumbnailDelivery::NotFound,
            Err(error) => return BookThumbnailDelivery::Internal(error),
        };
        if !self.user_can_access_book(book_id, user, &media).await {
            return BookThumbnailDelivery::Forbidden;
        }

        match self.reader.selected_book_thumbnail(book_id).await {
            Ok(Some(thumbnail)) => BookThumbnailDelivery::Thumbnail(BookThumbnailAsset {
                bytes: thumbnail.thumbnail,
                media_type: thumbnail.media_type,
                generated: thumbnail.thumbnail_type == "GENERATED",
            }),
            Ok(None) => BookThumbnailDelivery::NotFound,
            Err(error) => BookThumbnailDelivery::Internal(error),
        }
    }

    async fn resolve_book_id(&self, requested_book_id: &str) -> String {
        let Some(index) = requested_book_id
            .strip_prefix("book-")
            .and_then(|value| value.parse::<usize>().ok())
        else {
            return requested_book_id.to_string();
        };

        if index == 0 {
            return requested_book_id.to_string();
        }

        if matches!(
            self.book_ids
                .persisted_book_resource_exists(requested_book_id)
                .await,
            Ok(true)
        ) {
            return requested_book_id.to_string();
        }

        match self.book_ids.load_book_id_by_sorted_position(index).await {
            Ok(Some(book_id)) => book_id,
            _ => requested_book_id.to_string(),
        }
    }

    async fn user_can_access_book(
        &self,
        book_id: &str,
        user: &AuthUser,
        media: &BookMediaRecord,
    ) -> bool {
        let context = BookAccessContext::from_auth_user(user);
        if !context.can_access_library(&media.library_id) {
            return false;
        }

        let Ok(Some((age_rating, labels))) = self.reader.book_restrictions(book_id).await else {
            return true;
        };

        context.content_allowed(age_rating, &labels)
    }

    async fn pdf_page_asset(&self, media: &BookMediaRecord, page_number: u64) -> BookMediaDelivery {
        let page_count = self
            .content
            .detect_pdf_page_count(media)
            .unwrap_or(media.page_count);
        if page_number > page_count {
            return page_number_does_not_exist();
        }
        let Some(bytes) = self
            .content
            .read_pdf_page_as_single_page_pdf(media, page_number)
        else {
            return page_number_does_not_exist();
        };

        BookMediaDelivery::Asset(page_asset(
            media,
            page_number as u32,
            "application/pdf".to_string(),
            bytes,
        ))
    }

    async fn load_book_page_row(
        &self,
        book_id: &str,
        media: &BookMediaRecord,
        page_number: u64,
        allow_pdf_fallback: bool,
    ) -> Result<Option<BookPageRecord>, String> {
        match self.reader.book_page(book_id, page_number).await {
            Ok(Some(row)) => Ok(Some(row)),
            Ok(None) if book_media_is_single_image(media) && page_number == 1 => {
                Ok(Some(self.single_image_page_row(media, page_number).await))
            }
            Ok(None) => {
                if let Some(row) = self.content.archive_page_row(media, page_number).await {
                    return Ok(Some(row));
                }
                if allow_pdf_fallback {
                    return Ok(self.content.pdf_page_row(media, page_number));
                }
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    async fn list_book_page_rows(
        &self,
        book_id: &str,
        media: &BookMediaRecord,
    ) -> Result<Option<Vec<BookPageRecord>>, String> {
        let page_rows = self.reader.book_pages(book_id).await?;

        if !page_rows.is_empty() {
            let page_rows = if book_media_is_pdf(media) {
                map_kotlin_pdf_pages(page_rows)
            } else {
                page_rows
            };
            return Ok(Some(page_rows));
        }

        if let Some(archive_rows) = self.content.archive_page_rows(media).await
            && !archive_rows.is_empty()
        {
            return Ok(Some(archive_rows));
        }

        let generated_pdf_rows = self.content.generated_pdf_page_rows(media);
        if !generated_pdf_rows.is_empty() {
            return Ok(Some(generated_pdf_rows));
        }

        if !book_media_is_single_image(media) {
            return Ok(None);
        }

        Ok(Some(vec![self.single_image_page_row(media, 1).await]))
    }

    async fn single_image_page_row(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> BookPageRecord {
        let (width, height) = self
            .content
            .read_media_image_dimensions(media.file_path.as_path())
            .await
            .map(|(width, height)| (Some(width), Some(height)))
            .unwrap_or((None, None));
        BookPageRecord {
            number: page_number,
            file_name: media.file_name.clone(),
            media_type: content_type_from_filename(&media.file_name, &media.media_type),
            width,
            height,
            file_size: self
                .content
                .read_media_file_size(&media.file_path)
                .await
                .unwrap_or(0),
        }
    }

    async fn load_book_thumbnail_source(
        &self,
        book_id: &str,
        media: &BookMediaRecord,
    ) -> Option<BookThumbnailAsset> {
        if let Ok(Some(thumbnail)) = self.reader.selected_book_thumbnail(book_id).await
            && thumbnail.thumbnail_type != "GENERATED"
        {
            return Some(BookThumbnailAsset {
                bytes: thumbnail.thumbnail,
                media_type: thumbnail.media_type,
                generated: false,
            });
        }

        if book_media_is_epub(media)
            && let Some((bytes, media_type)) = self.content.epub_cover_bytes(media).await
        {
            return Some(BookThumbnailAsset {
                bytes,
                media_type,
                generated: false,
            });
        }

        self.load_book_thumbnail_page_source(media, book_id)
            .await
            .map(|bytes| BookThumbnailAsset {
                bytes,
                media_type: "image/jpeg".to_string(),
                generated: false,
            })
    }

    async fn load_book_thumbnail_page_source(
        &self,
        media: &BookMediaRecord,
        book_id: &str,
    ) -> Option<Vec<u8>> {
        if book_media_is_single_image(media) {
            return self.content.read_media_file_bytes(&media.file_path).await;
        }

        if book_media_is_pdf(media) {
            let page_row = self
                .reader
                .book_page(book_id, 1)
                .await
                .ok()
                .flatten()
                .or_else(|| self.content.pdf_page_row(media, 1))?;
            return self
                .content
                .render_page_thumbnail(media, &page_row, 1, 300)
                .await;
        }

        let page_row =
            if let Some(page_row) = self.reader.book_page(book_id, 1).await.ok().flatten() {
                page_row
            } else {
                self.content.archive_page_row(media, 1).await?
            };
        let media_type = page_row_media_type(&page_row, media);
        if !media_type.to_ascii_lowercase().starts_with("image/") {
            return None;
        }

        self.content.resolve_page_bytes(media, &page_row, 1).await
    }
}

#[derive(Clone, Copy)]
enum PageImageConversion {
    Jpeg,
    Png,
}

impl PageImageConversion {
    fn content_type(self) -> &'static str {
        match self {
            PageImageConversion::Jpeg => "image/jpeg",
            PageImageConversion::Png => "image/png",
        }
    }
}

fn validated_convert(value: Option<&str>) -> Result<Option<PageImageConversion>, ()> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    match value {
        "jpeg" => Ok(Some(PageImageConversion::Jpeg)),
        "png" => Ok(Some(PageImageConversion::Png)),
        _ => Err(()),
    }
}

fn can_stream_pages(user: &AuthUser) -> bool {
    user_is_admin(user) || user_has_role(user, "PAGE_STREAMING")
}

fn page_number_does_not_exist() -> BookMediaDelivery {
    BookMediaDelivery::BadRequest(Some("Page number does not exist".to_string()))
}

fn page_asset(
    media: &BookMediaRecord,
    page_number: u32,
    content_type: String,
    bytes: Vec<u8>,
) -> BookMediaDeliveryAsset {
    BookMediaDeliveryAsset {
        bytes,
        content_type: content_type.clone(),
        file_name: Some(page_response_file_name(
            &media.file_name,
            page_number,
            &content_type,
        )),
        source_file: Some(media.file_path.clone()),
        disposition: BookMediaDeliveryDisposition::Inline,
    }
}

fn page_response_file_name(book_display_name: &str, page_number: u32, media_type: &str) -> String {
    let extension = page_response_extension(media_type);
    format!("{book_display_name}-{page_number}.{extension}")
}

fn page_response_extension(media_type: &str) -> &'static str {
    match media_type {
        "application/pdf" => "pdf",
        "image/avif" => "avif",
        "image/gif" => "gif",
        "image/jpeg" => "jpeg",
        "image/png" => "png",
        "image/webp" => "webp",
        _ => "bin",
    }
}

fn page_row_media_type(page_row: &BookPageRecord, media: &BookMediaRecord) -> String {
    if page_row.media_type.is_empty() {
        content_type_from_filename(&page_row.file_name, &media.media_type)
    } else {
        page_row.media_type.clone()
    }
}

fn map_kotlin_pdf_pages(page_rows: Vec<BookPageRecord>) -> Vec<BookPageRecord> {
    page_rows
        .into_iter()
        .map(|page| {
            let (width, height) = scale_pdf_dimensions(page.width, page.height);
            BookPageRecord {
                media_type: "image/jpeg".to_string(),
                width,
                height,
                ..page
            }
        })
        .collect()
}

fn scale_pdf_dimensions(width: Option<i64>, height: Option<i64>) -> (Option<i64>, Option<i64>) {
    const PDF_RESOLUTION: f64 = 3200.0;

    let (Some(width), Some(height)) = (width, height) else {
        return (None, None);
    };
    let min_edge = width.min(height);
    if min_edge <= 0 {
        return (Some(width), Some(height));
    }

    let scale = PDF_RESOLUTION / min_edge as f64;
    let scaled_width = (width as f64 * scale).round().max(1.0) as i64;
    let scaled_height = (height as f64 * scale).round().max(1.0) as i64;
    (Some(scaled_width), Some(scaled_height))
}
