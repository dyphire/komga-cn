use std::path::Path;

use async_trait::async_trait;
use serde_json::{Value, json};

use super::{
    BookMediaRecord, BookPageRecord, ContentResolverPort, EpubNavigationReaderPort,
    EpubPositionsExtension, load_book_epub_navigation,
};

#[tokio::test]
async fn epub_navigation_normalizes_locator_and_maps_koreader_fragments() {
    let reader = TestEpubNavigationReader {
        media_files: vec!["/OEBPS/chapter-2.xhtml".to_string()],
        extension_blob: Some((
            "org.gotson.komga.domain.model.MediaExtensionEpub".to_string(),
            Vec::new(),
        )),
    };
    let content = TestContentResolver {
        extension: EpubPositionsExtension {
            positions: vec![
                json!({
                    "href": "OEBPS/chapter-1.xhtml",
                    "type": "application/xhtml+xml",
                    "locations": {
                        "progression": 0.0,
                        "totalProgression": 0.1,
                        "position": 1
                    }
                }),
                json!({
                    "href": "OEBPS/chapter-2.xhtml",
                    "type": "application/xhtml+xml",
                    "koboSpan": "kobo.2.1",
                    "locations": {
                        "progression": 0.5,
                        "totalProgression": 0.42,
                        "position": 2
                    }
                }),
            ],
            is_fixed_layout: false,
        },
    };

    let navigation = load_book_epub_navigation(&reader, &content, "book-1")
        .await
        .expect("epub navigation should load");

    let normalized = navigation
        .normalize_locator(&json!({
            "href": "/OEBPS/chapter-2.xhtml#frag",
            "locations": { "progression": 0.5 }
        }))
        .expect("locator should normalize against EPUB positions");
    assert_eq!(
        normalized.pointer("/locations/totalProgression"),
        Some(&json!(0.42))
    );
    assert_eq!(normalized.get("koboSpan"), Some(&json!("kobo.2.1")));

    let locator = navigation
        .koreader_locator_for_progress("/body/DocFragment[2]/body/div/p[1]/text().0")
        .expect("KOReader DocFragment should resolve to matching EPUB locator");
    assert_eq!(locator.get("href"), Some(&json!("OEBPS/chapter-2.xhtml")));
    assert_eq!(
        locator.pointer("/locations/totalProgression"),
        Some(&json!(0.42))
    );

    assert_eq!(
        navigation.koreader_progress_for_locator(&locator),
        Some("/body/DocFragment[2].0".to_string())
    );
}

struct TestEpubNavigationReader {
    media_files: Vec<String>,
    extension_blob: Option<(String, Vec<u8>)>,
}

#[async_trait]
impl EpubNavigationReaderPort for TestEpubNavigationReader {
    async fn book_media_files(&self, _book_id: &str) -> Result<Vec<String>, String> {
        Ok(self.media_files.clone())
    }

    async fn epub_extension_blob(
        &self,
        _book_id: &str,
    ) -> Result<Option<(String, Vec<u8>)>, String> {
        Ok(self.extension_blob.clone())
    }
}

struct TestContentResolver {
    extension: EpubPositionsExtension,
}

#[async_trait]
impl ContentResolverPort for TestContentResolver {
    async fn resolve_page_bytes(
        &self,
        _media: &BookMediaRecord,
        _page: &BookPageRecord,
        _page_number: u64,
    ) -> Option<Vec<u8>> {
        None
    }

    async fn render_page_thumbnail(
        &self,
        _media: &BookMediaRecord,
        _page: &BookPageRecord,
        _page_number: u64,
        _max_edge: u32,
    ) -> Option<Vec<u8>> {
        None
    }

    async fn archive_page_row(
        &self,
        _media: &BookMediaRecord,
        _page_number: u64,
    ) -> Option<BookPageRecord> {
        None
    }

    async fn archive_page_rows(&self, _media: &BookMediaRecord) -> Option<Vec<BookPageRecord>> {
        None
    }

    fn pdf_page_row(&self, _media: &BookMediaRecord, _page_number: u64) -> Option<BookPageRecord> {
        None
    }

    fn generated_pdf_page_rows(&self, _media: &BookMediaRecord) -> Vec<BookPageRecord> {
        Vec::new()
    }

    fn read_pdf_page_as_single_page_pdf(
        &self,
        _media: &BookMediaRecord,
        _page_number: u64,
    ) -> Option<Vec<u8>> {
        None
    }

    fn detect_pdf_page_count(&self, _media: &BookMediaRecord) -> Option<u64> {
        None
    }

    async fn read_media_file_bytes(&self, _path: &Path) -> Option<Vec<u8>> {
        None
    }

    async fn read_media_file_size(&self, _path: &Path) -> Option<i64> {
        None
    }

    fn is_font_resource(&self, _resource_name: &str) -> bool {
        false
    }

    async fn read_epub_resource_bytes(
        &self,
        _epub_path: &Path,
        _resource_name: &str,
    ) -> Option<Vec<u8>> {
        None
    }

    fn decode_epub_positions_extension(
        &self,
        _blob: &[u8],
    ) -> Result<EpubPositionsExtension, String> {
        Ok(self.extension.clone())
    }

    async fn epub_archive_positions(&self, _media: &BookMediaRecord) -> Option<Vec<Value>> {
        None
    }

    async fn epub_cover_bytes(&self, _media: &BookMediaRecord) -> Option<(Vec<u8>, String)> {
        None
    }

    async fn epub_package_document(&self, _media: &BookMediaRecord) -> Option<Vec<u8>> {
        None
    }

    fn epub_fixed_layout(&self, _package_document: &[u8]) -> bool {
        false
    }

    fn epub_kobo_spans(&self, _resource_bytes: &[u8]) -> Vec<(String, f64)> {
        Vec::new()
    }

    fn normalize_epub_resource_href(&self, _rootfile_path: &str, href: &str) -> String {
        href.to_string()
    }
}
