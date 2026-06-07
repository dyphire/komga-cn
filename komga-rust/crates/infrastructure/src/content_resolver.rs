use std::path::Path;

use async_trait::async_trait;
use komga_application::media_assets::{
    BookMediaRecord, BookPageRecord, ContentResolverPort, EpubPositionsExtension,
};
use serde_json::Value;

use crate::filesystem::media_access::epub;
use crate::filesystem::media_access::page_content;

/// Stateless filesystem I/O for resolving page/resource content from archives and PDFs.
#[derive(Clone, Default)]
pub struct ContentResolver;

#[async_trait]
impl ContentResolverPort for ContentResolver {
    // --- Page content ---

    async fn resolve_page_bytes(
        &self,
        media: &BookMediaRecord,
        page: &BookPageRecord,
        page_number: u64,
    ) -> Option<Vec<u8>> {
        page_content::resolve_book_page_bytes(media, page, page_number).await
    }

    async fn render_page_thumbnail(
        &self,
        media: &BookMediaRecord,
        page: &BookPageRecord,
        page_number: u64,
        max_edge: u32,
    ) -> Option<Vec<u8>> {
        page_content::render_book_page_thumbnail(media, page, page_number, max_edge).await
    }

    async fn archive_page_row(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> Option<BookPageRecord> {
        page_content::load_archive_page_row(media, page_number).await
    }

    async fn archive_page_rows(&self, media: &BookMediaRecord) -> Option<Vec<BookPageRecord>> {
        page_content::load_archive_page_rows(media).await
    }

    fn pdf_page_row(&self, media: &BookMediaRecord, page_number: u64) -> Option<BookPageRecord> {
        page_content::load_pdf_page_row(media, page_number)
    }

    fn generated_pdf_page_rows(&self, media: &BookMediaRecord) -> Vec<BookPageRecord> {
        page_content::load_generated_pdf_page_rows(media)
    }

    fn read_pdf_page_as_single_page_pdf(
        &self,
        media: &BookMediaRecord,
        page_number: u64,
    ) -> Option<Vec<u8>> {
        page_content::read_pdf_page_as_single_page_pdf(media, page_number)
    }

    fn detect_pdf_page_count(&self, media: &BookMediaRecord) -> Option<u64> {
        page_content::detect_pdf_page_count(media)
    }

    async fn read_media_file_bytes(&self, path: &Path) -> Option<Vec<u8>> {
        page_content::read_media_file_bytes(path).await
    }

    async fn read_media_file_size(&self, path: &Path) -> Option<i64> {
        page_content::read_media_file_size(path).await
    }

    // --- EPUB ---

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

    fn decode_epub_positions_extension(
        &self,
        blob: &[u8],
    ) -> Result<EpubPositionsExtension, String> {
        epub::decode_epub_positions_extension(blob)
    }

    async fn epub_archive_positions(&self, media: &BookMediaRecord) -> Option<Vec<Value>> {
        epub::load_epub_archive_positions(media).await
    }

    async fn epub_cover_bytes(&self, media: &BookMediaRecord) -> Option<(Vec<u8>, String)> {
        epub::load_epub_cover_bytes(media).await
    }

    async fn epub_package_document(&self, media: &BookMediaRecord) -> Option<Vec<u8>> {
        epub::load_epub_package_document(media).await
    }

    fn epub_fixed_layout(&self, package_document: &[u8]) -> bool {
        epub::parse_epub_fixed_layout(package_document)
    }

    fn epub_kobo_spans(&self, resource_bytes: &[u8]) -> Vec<(String, f64)> {
        epub::parse_epub_kobo_spans(resource_bytes)
    }

    fn normalize_epub_resource_href(&self, rootfile_path: &str, href: &str) -> String {
        epub::normalize_epub_resource_href(rootfile_path, href)
    }
}
