use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::identity_access::AuthUser;

use super::{
    BookMediaRecord, BookPageRecord, BookProgressionInput, BookProgressionOutcome,
    BookProgressionReaderPort, BookProgressionService, BookProgressionUpdate, ContentResolverPort,
    EpubPositionsExtension, ProgressWriterPort,
};

#[tokio::test]
async fn book_progression_update_normalizes_epub_locator_and_persists_progression() {
    let reader = TestProgressionReader {
        media: Some(BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "book.epub".to_string(),
            file_path: PathBuf::from("/library/book.epub"),
            media_type: "application/epub+zip".to_string(),
            page_count: 10,
        }),
        media_files: vec!["/chapter-1.xhtml".to_string()],
        epub_extension_blob: Some(("EPUB".to_string(), Vec::new())),
        ..TestProgressionReader::default()
    };
    let content = TestContentResolver {
        positions_extension: EpubPositionsExtension {
            positions: vec![json!({
                "href": "chapter-1.xhtml",
                "type": "application/xhtml+xml",
                "koboSpan": "kobo.1.1",
                "locations": {
                    "progression": 0.5,
                    "totalProgression": 0.21,
                    "position": 2
                }
            })],
            is_fixed_layout: false,
        },
    };
    let progress = TestProgressWriter::default();
    let service = BookProgressionService::new(&reader, &content, &progress);

    let outcome = service
        .update_progression(
            &admin_user(),
            "book-1",
            BookProgressionUpdate {
                modified: "2026-03-27T10:00:00Z".to_string(),
                device_id: "device-1".to_string(),
                device_name: "Readium".to_string(),
                locator: Some(json!({
                    "href": "/chapter-1.xhtml#frag",
                    "locations": { "progression": 0.5 }
                })),
            },
        )
        .await;

    assert_eq!(outcome, BookProgressionOutcome::Updated);
    let persisted = progress.persisted.lock().unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].book_id, "book-1");
    assert_eq!(persisted[0].user_id, "admin");
    assert_eq!(persisted[0].progression, 0.5);
    assert!(!persisted[0].use_locator_position_for_page);
    assert_eq!(
        persisted[0]
            .locator
            .as_ref()
            .and_then(|locator| locator.pointer("/locations/totalProgression")),
        Some(&json!(0.21))
    );
    assert_eq!(
        persisted[0]
            .locator
            .as_ref()
            .and_then(|locator| locator.get("koboSpan")),
        Some(&json!("kobo.1.1"))
    );
}

#[derive(Default)]
struct TestProgressionReader {
    media: Option<BookMediaRecord>,
    media_files: Vec<String>,
    epub_extension_blob: Option<(String, Vec<u8>)>,
    book_progression: Option<Value>,
}

#[async_trait]
impl BookProgressionReaderPort for TestProgressionReader {
    async fn book_media(&self, _book_id: &str) -> Result<Option<BookMediaRecord>, String> {
        Ok(self.media.clone())
    }

    async fn book_restrictions(
        &self,
        _book_id: &str,
    ) -> Result<Option<(Option<u16>, Vec<String>)>, String> {
        Ok(None)
    }

    async fn book_progression(
        &self,
        _book_id: &str,
        _user_id: &str,
    ) -> Result<Option<Value>, String> {
        Ok(self.book_progression.clone())
    }

    async fn book_media_files(&self, _book_id: &str) -> Result<Vec<String>, String> {
        Ok(self.media_files.clone())
    }

    async fn epub_extension_blob(
        &self,
        _book_id: &str,
    ) -> Result<Option<(String, Vec<u8>)>, String> {
        Ok(self.epub_extension_blob.clone())
    }
}

struct TestContentResolver {
    positions_extension: EpubPositionsExtension,
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
        Ok(self.positions_extension.clone())
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

#[derive(Default)]
struct TestProgressWriter {
    persisted: Mutex<Vec<BookProgressionInput>>,
}

#[async_trait]
impl ProgressWriterPort for TestProgressWriter {
    async fn persist_read_progress(
        &self,
        _book_id: &str,
        _user_id: &str,
        _page: u64,
        _completed: bool,
        _locator: Option<Value>,
    ) -> Result<(), String> {
        unreachable!("page progress writes are not part of this test")
    }

    async fn persist_book_progression(&self, input: BookProgressionInput) -> Result<(), String> {
        self.persisted.lock().unwrap().push(input);
        Ok(())
    }

    async fn delete_read_progress(&self, _book_id: &str, _user_id: &str) -> Result<(), String> {
        unreachable!("delete progress is not part of this test")
    }

    async fn persist_readlist_tachiyomi_progress(
        &self,
        _ordered_book_ids: &[String],
        _user_id: &str,
        _last_book_read: usize,
    ) -> Result<Option<()>, String> {
        unreachable!("readlist progress is not part of this test")
    }

    async fn refresh_series_read_progress(
        &self,
        _series_id: &str,
        _user_id: &str,
    ) -> Result<(), String> {
        unreachable!("series progress is not part of this test")
    }

    async fn delete_series_read_progress(
        &self,
        _series_id: &str,
        _user_id: &str,
    ) -> Result<(), String> {
        unreachable!("series progress is not part of this test")
    }
}

fn admin_user() -> AuthUser {
    AuthUser {
        id: "admin".to_string(),
        email: "admin@example.org".to_string(),
        password: "password".to_string(),
        roles: vec!["ADMIN".to_string()],
        shared_all_libraries: true,
        shared_library_ids: Vec::new(),
        labels_allow: Vec::new(),
        labels_exclude: Vec::new(),
        age_restriction: None,
    }
}
