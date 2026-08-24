use std::path::Path;

use crate::media::content::epub_resources as epub;
use crate::media::content::page_rendering as page_content;
use komga_epub::{normalize_epub_resource_href, parse_epub_fixed_layout};

use komga_application::media_assets::{
    BookMediaRecord, BookPageRecord, ContentResolverPort, EpubCoverImage, EpubNavigationExtension,
    ImageOutputFormat, MediaImageDimensions, RenderedImage,
};

/// Stateless filesystem I/O for resolving page/resource content from archives and PDFs.
#[derive(Clone, Default)]
pub struct ContentResolver;

#[async_trait::async_trait]
impl ContentResolverPort for ContentResolver {
    // --- Page content ---

    async fn resolve_page_bytes(
        &self,
        media: &BookMediaRecord,
        page: &BookPageRecord,
        page_number: u64,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        page_content::resolve_book_page_bytes(media, page, page_number).await
    }

    async fn render_page_thumbnail(
        &self,
        media: &BookMediaRecord,
        page: &BookPageRecord,
        page_number: u64,
        max_edge: u32,
        output_format: ImageOutputFormat,
    ) -> anyhow::Result<Option<RenderedImage>> {
        page_content::render_book_page_thumbnail(media, page, page_number, max_edge, output_format)
            .await
    }

    async fn render_pdf_page(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
        output_format: ImageOutputFormat,
    ) -> anyhow::Result<Option<RenderedImage>> {
        page_content::render_pdf_page(media, page_number, output_format).await
    }

    async fn archive_page_row(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> anyhow::Result<Option<BookPageRecord>> {
        page_content::load_archive_page_row(media, page_number).await
    }

    async fn archive_page_rows(
        &self,
        media: &BookMediaRecord,
    ) -> anyhow::Result<Option<Vec<BookPageRecord>>> {
        page_content::load_archive_page_rows(media).await
    }

    fn pdf_page_row(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> anyhow::Result<Option<BookPageRecord>> {
        page_content::load_pdf_page_row(media, page_number)
    }

    fn read_pdf_page_as_single_page_pdf(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        page_content::read_pdf_page_as_single_page_pdf(media, page_number)
    }

    async fn read_media_file_bytes(&self, path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
        page_content::read_media_file_bytes(path).await
    }

    async fn read_epub_publication_bytes(
        &self,
        media: &BookMediaRecord,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        epub::read_epub_publication_bytes(media).await
    }

    async fn read_media_file_size(&self, path: &Path) -> anyhow::Result<Option<i64>> {
        page_content::read_media_file_size(path).await
    }

    async fn read_media_image_dimensions(
        &self,
        path: &Path,
    ) -> anyhow::Result<Option<MediaImageDimensions>> {
        page_content::read_media_image_dimensions(path).await
    }

    fn convert_image_bytes(
        &self,
        bytes: &[u8],
        source_content_type: &str,
        target_content_type: &str,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        page_content::convert_image_bytes(bytes, source_content_type, target_content_type)
    }

    // --- EPUB ---

    async fn read_epub_resource_bytes(
        &self,
        epub_path: &Path,
        resource_name: &str,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        epub::read_epub_resource_bytes(epub_path, resource_name).await
    }

    fn decode_epub_navigation_extension(
        &self,
        blob: &[u8],
    ) -> anyhow::Result<EpubNavigationExtension> {
        epub::decode_epub_navigation_extension(blob)
    }

    async fn epub_cover_bytes(
        &self,
        media: &BookMediaRecord,
    ) -> anyhow::Result<Option<EpubCoverImage>> {
        epub::load_epub_cover_bytes(media).await
    }

    async fn epub_package_document(
        &self,
        media: &BookMediaRecord,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        epub::load_epub_package_document(media).await
    }

    fn epub_fixed_layout(&self, package_document: &[u8]) -> bool {
        parse_epub_fixed_layout(package_document).unwrap_or(false)
    }

    fn normalize_epub_resource_href(&self, rootfile_path: &str, href: &str) -> String {
        normalize_epub_resource_href(rootfile_path, href)
    }
}
