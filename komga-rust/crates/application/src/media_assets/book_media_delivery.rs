use std::path::{Path, PathBuf};

use crate::discovery::{PersistedBookIdResolverPort, resolve_persisted_book_id};
use crate::identity_access::{AuthUser, AuthUserRole, user_has_role};

use super::book_access::BookAccessContext;
use super::{
    BookAccessRestrictions, BookMediaPort, BookMediaRecord, BookPageRecord, ContentAccessPort,
    ContentResolverPort, EntityThumbnailBinary, EpubCoverImage, MediaImageDimensions,
    ThumbnailReadPort, ThumbnailType, book_media_is_epub, book_media_is_pdf,
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

#[async_trait::async_trait]
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
    ) -> Result<Option<BookAccessRestrictions>, String>;

    async fn selected_book_thumbnail(
        &self,
        book_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String>;
}

#[async_trait::async_trait]
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
    ) -> Result<Option<BookAccessRestrictions>, String> {
        ContentAccessPort::book_restrictions(self, book_id).await
    }

    async fn selected_book_thumbnail(
        &self,
        book_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String> {
        ThumbnailReadPort::selected_book_thumbnail(self, book_id).await
    }
}

#[async_trait::async_trait]
pub trait BookMediaContentPort: Send + Sync {
    async fn resolve_page_bytes(
        &self,
        media: &BookMediaRecord,
        page: &BookPageRecord,
        page_number: u64,
    ) -> Result<Option<Vec<u8>>, String>;

    async fn render_page_thumbnail(
        &self,
        media: &BookMediaRecord,
        page: &BookPageRecord,
        page_number: u64,
        max_edge: u32,
    ) -> Result<Option<Vec<u8>>, String>;

    async fn archive_page_row(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> Result<Option<BookPageRecord>, String>;

    async fn archive_page_rows(
        &self,
        media: &BookMediaRecord,
    ) -> Result<Option<Vec<BookPageRecord>>, String>;

    fn pdf_page_row(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> Result<Option<BookPageRecord>, String>;

    fn generated_pdf_page_rows(
        &self,
        media: &BookMediaRecord,
    ) -> Result<Vec<BookPageRecord>, String>;

    fn read_pdf_page_as_single_page_pdf(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> Result<Option<Vec<u8>>, String>;

    fn detect_pdf_page_count(&self, media: &BookMediaRecord) -> Result<Option<u64>, String>;

    fn media_file_exists(&self, path: &Path) -> Result<bool, String> {
        path.try_exists()
            .map_err(|error| format!("check media file existence '{}': {error}", path.display()))
    }

    async fn read_media_file_bytes(&self, path: &Path) -> Result<Option<Vec<u8>>, String>;

    async fn read_media_file_size(&self, path: &Path) -> Result<Option<i64>, String>;

    async fn read_media_image_dimensions(
        &self,
        path: &Path,
    ) -> Result<Option<MediaImageDimensions>, String>;

    fn convert_image_bytes(
        &self,
        bytes: &[u8],
        source_content_type: &str,
        target_content_type: &str,
    ) -> Result<Option<Vec<u8>>, String>;

    async fn epub_cover_bytes(
        &self,
        media: &BookMediaRecord,
    ) -> Result<Option<EpubCoverImage>, String>;
}

#[async_trait::async_trait]
impl<T> BookMediaContentPort for T
where
    T: ContentResolverPort + Send + Sync + ?Sized,
{
    async fn resolve_page_bytes(
        &self,
        media: &BookMediaRecord,
        page: &BookPageRecord,
        page_number: u64,
    ) -> Result<Option<Vec<u8>>, String> {
        ContentResolverPort::resolve_page_bytes(self, media, page, page_number).await
    }

    async fn render_page_thumbnail(
        &self,
        media: &BookMediaRecord,
        page: &BookPageRecord,
        page_number: u64,
        max_edge: u32,
    ) -> Result<Option<Vec<u8>>, String> {
        ContentResolverPort::render_page_thumbnail(self, media, page, page_number, max_edge).await
    }

    async fn archive_page_row(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> Result<Option<BookPageRecord>, String> {
        ContentResolverPort::archive_page_row(self, media, page_number).await
    }

    async fn archive_page_rows(
        &self,
        media: &BookMediaRecord,
    ) -> Result<Option<Vec<BookPageRecord>>, String> {
        ContentResolverPort::archive_page_rows(self, media).await
    }

    fn pdf_page_row(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> Result<Option<BookPageRecord>, String> {
        ContentResolverPort::pdf_page_row(self, media, page_number)
    }

    fn generated_pdf_page_rows(
        &self,
        media: &BookMediaRecord,
    ) -> Result<Vec<BookPageRecord>, String> {
        ContentResolverPort::generated_pdf_page_rows(self, media)
    }

    fn read_pdf_page_as_single_page_pdf(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> Result<Option<Vec<u8>>, String> {
        ContentResolverPort::read_pdf_page_as_single_page_pdf(self, media, page_number)
    }

    fn detect_pdf_page_count(&self, media: &BookMediaRecord) -> Result<Option<u64>, String> {
        ContentResolverPort::detect_pdf_page_count(self, media)
    }

    async fn read_media_file_bytes(&self, path: &Path) -> Result<Option<Vec<u8>>, String> {
        ContentResolverPort::read_media_file_bytes(self, path).await
    }

    async fn read_media_file_size(&self, path: &Path) -> Result<Option<i64>, String> {
        ContentResolverPort::read_media_file_size(self, path).await
    }

    async fn read_media_image_dimensions(
        &self,
        path: &Path,
    ) -> Result<Option<MediaImageDimensions>, String> {
        ContentResolverPort::read_media_image_dimensions(self, path).await
    }

    fn convert_image_bytes(
        &self,
        bytes: &[u8],
        source_content_type: &str,
        target_content_type: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        ContentResolverPort::convert_image_bytes(
            self,
            bytes,
            source_content_type,
            target_content_type,
        )
    }

    async fn epub_cover_bytes(
        &self,
        media: &BookMediaRecord,
    ) -> Result<Option<EpubCoverImage>, String> {
        ContentResolverPort::epub_cover_bytes(self, media).await
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
        match self.user_can_access_book(book_id, user, &media).await {
            Ok(true) => {}
            Ok(false) => return BookMediaDelivery::Forbidden,
            Err(error) => return BookMediaDelivery::Internal(error),
        }

        let bytes = match self.content.read_media_file_bytes(&media.file_path).await {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return BookMediaDelivery::MissingFile,
            Err(error) => return BookMediaDelivery::Internal(error),
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
        match self.reader.book_media_is_ready(&resolved_book_id).await {
            Ok(true) => {}
            Ok(false) => return BookMediaDelivery::MediaAnalysisFailed,
            Err(error) => return BookMediaDelivery::Internal(error),
        }
        if !can_stream_pages(user) {
            return BookMediaDelivery::Forbidden;
        }
        match self
            .user_can_access_book(&resolved_book_id, user, &media)
            .await
        {
            Ok(true) => {}
            Ok(false) => return BookMediaDelivery::Forbidden,
            Err(error) => return BookMediaDelivery::Internal(error),
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

        let bytes = match self
            .content
            .resolve_page_bytes(&media, &page_row, requested_page_number as u64)
            .await
        {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return BookMediaDelivery::NotFound,
            Err(error) => return BookMediaDelivery::Internal(error),
        };
        let content_type = page_row_media_type(&page_row, &media);
        let content = match requested_convert {
            Some(convert) => {
                let target_content_type = convert.content_type();
                let converted = match self.content.convert_image_bytes(
                    &bytes,
                    &content_type,
                    target_content_type,
                ) {
                    Ok(Some(converted)) => converted,
                    Ok(None) => return BookMediaDelivery::NotFound,
                    Err(error) => return BookMediaDelivery::Internal(error),
                };
                ResolvedPageContent {
                    bytes: converted,
                    content_type: target_content_type.to_string(),
                }
            }
            None => ResolvedPageContent {
                bytes,
                content_type,
            },
        };

        BookMediaDelivery::Asset(page_asset(
            &media,
            requested_page_number,
            content.content_type,
            content.bytes,
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
        if !user_has_role(user, AuthUserRole::PageStreaming) {
            return BookMediaDelivery::Forbidden;
        }
        match self
            .user_can_access_book(&resolved_book_id, user, &media)
            .await
        {
            Ok(true) => {}
            Ok(false) => return BookMediaDelivery::Forbidden,
            Err(error) => return BookMediaDelivery::Internal(error),
        }
        if !book_media_is_pdf(&media) {
            return BookMediaDelivery::BadRequest(Some(
                "Extractor does not support raw extraction of pages".to_string(),
            ));
        }
        match self.reader.book_media_is_ready(&resolved_book_id).await {
            Ok(true) => {}
            Ok(false) => return BookMediaDelivery::MediaAnalysisFailed,
            Err(error) => return BookMediaDelivery::Internal(error),
        }
        match self.content.media_file_exists(&media.file_path) {
            Ok(true) => {}
            Ok(false) => return BookMediaDelivery::MissingFile,
            Err(error) => return BookMediaDelivery::Internal(error),
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
        if !can_stream_pages(user) {
            return BookMediaDelivery::Forbidden;
        }
        match self
            .user_can_access_book(&resolved_book_id, user, &media)
            .await
        {
            Ok(true) => {}
            Ok(false) => return BookMediaDelivery::Forbidden,
            Err(error) => return BookMediaDelivery::Internal(error),
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
        let bytes = match self
            .content
            .render_page_thumbnail(&media, &page_row, page_number as u64, 300)
            .await
        {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return BookMediaDelivery::NotFound,
            Err(error) => return BookMediaDelivery::Internal(error),
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
        if !can_stream_pages(user) {
            return BookMediaDelivery::Forbidden;
        }
        match self
            .user_can_access_book(&resolved_book_id, user, &media)
            .await
        {
            Ok(true) => {}
            Ok(false) => return BookMediaDelivery::Forbidden,
            Err(error) => return BookMediaDelivery::Internal(error),
        }
        match self.reader.book_media_is_ready(&resolved_book_id).await {
            Ok(true) => {}
            Ok(false) => return BookMediaDelivery::NotFound,
            Err(error) => return BookMediaDelivery::Internal(error),
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
        match self.user_can_access_book(book_id, user, &media).await {
            Ok(true) => {}
            Ok(false) => return BookThumbnailDelivery::Forbidden,
            Err(error) => return BookThumbnailDelivery::Internal(error),
        }

        match self.load_book_thumbnail_source(book_id, &media).await {
            Ok(Some(thumbnail)) => BookThumbnailDelivery::Thumbnail(thumbnail),
            Ok(None) => BookThumbnailDelivery::NotFound,
            Err(error) => BookThumbnailDelivery::Internal(error),
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
        match self.user_can_access_book(book_id, user, &media).await {
            Ok(true) => {}
            Ok(false) => return BookThumbnailDelivery::Forbidden,
            Err(error) => return BookThumbnailDelivery::Internal(error),
        }

        match self.reader.selected_book_thumbnail(book_id).await {
            Ok(Some(thumbnail)) => BookThumbnailDelivery::Thumbnail(BookThumbnailAsset {
                bytes: thumbnail.thumbnail,
                media_type: thumbnail.media_type,
                generated: thumbnail.thumbnail_type == ThumbnailType::Generated,
            }),
            Ok(None) => BookThumbnailDelivery::NotFound,
            Err(error) => BookThumbnailDelivery::Internal(error),
        }
    }

    async fn resolve_book_id(&self, requested_book_id: &str) -> String {
        resolve_persisted_book_id(self.book_ids, requested_book_id).await
    }

    async fn user_can_access_book(
        &self,
        book_id: &str,
        user: &AuthUser,
        media: &BookMediaRecord,
    ) -> Result<bool, String> {
        let context = BookAccessContext::from_auth_user(user);
        if !context.can_access_library(&media.library_id) {
            return Ok(false);
        }

        let Some(restrictions) = self.reader.book_restrictions(book_id).await? else {
            return Ok(true);
        };

        Ok(context.content_allowed(restrictions.age_rating, &restrictions.labels))
    }

    async fn pdf_page_asset(&self, media: &BookMediaRecord, page_number: u64) -> BookMediaDelivery {
        let page_count = match self.content.detect_pdf_page_count(media) {
            Ok(Some(count)) => count,
            Ok(None) => media.page_count,
            Err(error) => return BookMediaDelivery::Internal(error),
        };
        if page_number > page_count {
            return page_number_does_not_exist();
        }
        let bytes = match self
            .content
            .read_pdf_page_as_single_page_pdf(media, page_number)
        {
            Ok(Some(bytes)) => bytes,
            Ok(None) => return page_number_does_not_exist(),
            Err(error) => return BookMediaDelivery::Internal(error),
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
                Ok(Some(self.single_image_page_row(media, page_number).await?))
            }
            Ok(None) => {
                if let Some(row) = self.content.archive_page_row(media, page_number).await? {
                    return Ok(Some(row));
                }
                if allow_pdf_fallback {
                    return self.content.pdf_page_row(media, page_number);
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

        if let Some(archive_rows) = self.content.archive_page_rows(media).await?
            && !archive_rows.is_empty()
        {
            return Ok(Some(archive_rows));
        }

        let generated_pdf_rows = self.content.generated_pdf_page_rows(media)?;
        if !generated_pdf_rows.is_empty() {
            return Ok(Some(generated_pdf_rows));
        }

        if !book_media_is_single_image(media) {
            return Ok(None);
        }
        if !self.content.media_file_exists(&media.file_path)? {
            return Ok(None);
        }

        Ok(Some(vec![self.single_image_page_row(media, 1).await?]))
    }

    async fn single_image_page_row(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> Result<BookPageRecord, String> {
        let dimensions = self
            .content
            .read_media_image_dimensions(media.file_path.as_path())
            .await?;
        Ok(BookPageRecord {
            number: page_number,
            file_name: media.file_name.clone(),
            media_type: content_type_from_filename(&media.file_name, &media.media_type),
            width: dimensions.as_ref().map(|dimensions| dimensions.width),
            height: dimensions.as_ref().map(|dimensions| dimensions.height),
            file_size: self
                .content
                .read_media_file_size(&media.file_path)
                .await?
                .ok_or_else(|| {
                    format!(
                        "single image media file missing: {}",
                        media.file_path.display()
                    )
                })?,
        })
    }

    async fn load_book_thumbnail_source(
        &self,
        book_id: &str,
        media: &BookMediaRecord,
    ) -> Result<Option<BookThumbnailAsset>, String> {
        match self.reader.selected_book_thumbnail(book_id).await? {
            Some(thumbnail) if thumbnail.thumbnail_type != ThumbnailType::Generated => {
                return Ok(Some(BookThumbnailAsset {
                    bytes: thumbnail.thumbnail,
                    media_type: thumbnail.media_type,
                    generated: false,
                }));
            }
            Some(_) | None => {}
        }

        if book_media_is_epub(media)
            && let Some(cover) = self.content.epub_cover_bytes(media).await?
        {
            return Ok(Some(BookThumbnailAsset {
                bytes: cover.bytes,
                media_type: cover.media_type,
                generated: false,
            }));
        }

        Ok(self
            .load_book_thumbnail_page_source(media, book_id)
            .await?
            .map(|bytes| BookThumbnailAsset {
                bytes,
                media_type: "image/jpeg".to_string(),
                generated: false,
            }))
    }

    async fn load_book_thumbnail_page_source(
        &self,
        media: &BookMediaRecord,
        book_id: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        if book_media_is_single_image(media) {
            return self.content.read_media_file_bytes(&media.file_path).await;
        }

        if book_media_is_pdf(media) {
            let page_row = self
                .reader
                .book_page(book_id, 1)
                .await?
                .map_or_else(|| self.content.pdf_page_row(media, 1), |row| Ok(Some(row)))?;
            let Some(page_row) = page_row else {
                return Ok(None);
            };
            return self
                .content
                .render_page_thumbnail(media, &page_row, 1, 300)
                .await;
        }

        let page_row = match self.reader.book_page(book_id, 1).await? {
            Some(page_row) => page_row,
            None => {
                let Some(page_row) = self.content.archive_page_row(media, 1).await? else {
                    return Ok(None);
                };
                page_row
            }
        };
        let media_type = page_row_media_type(&page_row, media);
        if !media_type.to_ascii_lowercase().starts_with("image/") {
            return Ok(None);
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
    user_has_role(user, AuthUserRole::PageStreaming)
}

fn page_number_does_not_exist() -> BookMediaDelivery {
    BookMediaDelivery::BadRequest(Some("Page number does not exist".to_string()))
}

struct ResolvedPageContent {
    bytes: Vec<u8>,
    content_type: String,
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
            let dimensions = scale_pdf_dimensions(page.width, page.height);
            BookPageRecord {
                media_type: "image/jpeg".to_string(),
                width: dimensions.width,
                height: dimensions.height,
                ..page
            }
        })
        .collect()
}

struct PageDimensions {
    width: Option<i64>,
    height: Option<i64>,
}

fn scale_pdf_dimensions(width: Option<i64>, height: Option<i64>) -> PageDimensions {
    const PDF_RESOLUTION: f64 = 3200.0;

    let (Some(width), Some(height)) = (width, height) else {
        return PageDimensions {
            width: None,
            height: None,
        };
    };
    let min_edge = width.min(height);
    if min_edge <= 0 {
        return PageDimensions {
            width: Some(width),
            height: Some(height),
        };
    }

    let scale = PDF_RESOLUTION / min_edge as f64;
    let scaled_width = (width as f64 * scale).round().max(1.0) as i64;
    let scaled_height = (height as f64 * scale).round().max(1.0) as i64;
    PageDimensions {
        width: Some(scaled_width),
        height: Some(scaled_height),
    }
}
