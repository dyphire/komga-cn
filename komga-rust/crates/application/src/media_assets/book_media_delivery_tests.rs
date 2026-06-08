use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::identity_access::AuthUser;

use super::{
    BookMediaContentPort, BookMediaDelivery, BookMediaDeliveryAsset, BookMediaDeliveryDisposition,
    BookMediaDeliveryService, BookMediaPageRequest, BookMediaReaderPort, BookPageRecord,
    PersistedBookIdResolverPort,
};

#[derive(Default)]
struct TestBookMediaReader {
    media_by_book: HashMap<String, super::BookMediaRecord>,
}

#[async_trait]
impl BookMediaReaderPort for TestBookMediaReader {
    async fn book_media(&self, book_id: &str) -> Result<Option<super::BookMediaRecord>, String> {
        Ok(self.media_by_book.get(book_id).cloned())
    }

    async fn book_media_is_ready(&self, _book_id: &str) -> Result<bool, String> {
        Ok(true)
    }

    async fn book_pages(&self, _book_id: &str) -> Result<Vec<BookPageRecord>, String> {
        Ok(Vec::new())
    }

    async fn book_page(
        &self,
        _book_id: &str,
        _page_number: u64,
    ) -> Result<Option<BookPageRecord>, String> {
        Ok(None)
    }

    async fn book_restrictions(
        &self,
        _book_id: &str,
    ) -> Result<Option<(Option<u16>, Vec<String>)>, String> {
        Ok(None)
    }

    async fn selected_book_thumbnail(
        &self,
        _book_id: &str,
    ) -> Result<Option<super::EntityThumbnailBinary>, String> {
        Ok(None)
    }
}

#[derive(Default)]
struct TestBookMediaContent {
    archive_page_row: Option<BookPageRecord>,
    page_bytes: Vec<u8>,
}

#[async_trait]
impl BookMediaContentPort for TestBookMediaContent {
    async fn resolve_page_bytes(
        &self,
        _media: &super::BookMediaRecord,
        _page: &BookPageRecord,
        _page_number: u64,
    ) -> Option<Vec<u8>> {
        Some(self.page_bytes.clone())
    }

    async fn render_page_thumbnail(
        &self,
        _media: &super::BookMediaRecord,
        _page: &BookPageRecord,
        _page_number: u64,
        _max_edge: u32,
    ) -> Option<Vec<u8>> {
        None
    }

    async fn archive_page_row(
        &self,
        _media: &super::BookMediaRecord,
        _page_number: u64,
    ) -> Option<BookPageRecord> {
        self.archive_page_row.clone()
    }

    async fn archive_page_rows(
        &self,
        _media: &super::BookMediaRecord,
    ) -> Option<Vec<BookPageRecord>> {
        None
    }

    fn pdf_page_row(
        &self,
        _media: &super::BookMediaRecord,
        _page_number: u64,
    ) -> Option<BookPageRecord> {
        None
    }

    fn generated_pdf_page_rows(&self, _media: &super::BookMediaRecord) -> Vec<BookPageRecord> {
        Vec::new()
    }

    fn read_pdf_page_as_single_page_pdf(
        &self,
        _media: &super::BookMediaRecord,
        _page_number: u64,
    ) -> Option<Vec<u8>> {
        None
    }

    fn detect_pdf_page_count(&self, _media: &super::BookMediaRecord) -> Option<u64> {
        None
    }

    async fn read_media_file_bytes(&self, _path: &Path) -> Option<Vec<u8>> {
        None
    }

    async fn read_media_file_size(&self, _path: &Path) -> Option<i64> {
        None
    }

    async fn read_media_image_dimensions(&self, _path: &Path) -> Option<(i64, i64)> {
        None
    }

    fn convert_image_bytes(
        &self,
        bytes: &[u8],
        source_content_type: &str,
        target_content_type: &str,
    ) -> Option<Vec<u8>> {
        if source_content_type.eq_ignore_ascii_case(target_content_type) {
            return Some(bytes.to_vec());
        }
        None
    }

    async fn epub_cover_bytes(&self, _media: &super::BookMediaRecord) -> Option<(Vec<u8>, String)> {
        None
    }
}

struct IdentityBookIdResolver;

#[async_trait]
impl PersistedBookIdResolverPort for IdentityBookIdResolver {
    async fn persisted_book_resource_exists(&self, _book_id: &str) -> Result<bool, String> {
        Ok(true)
    }

    async fn load_book_id_by_sorted_position(
        &self,
        _index: usize,
    ) -> Result<Option<String>, String> {
        Ok(None)
    }
}

#[tokio::test]
async fn book_page_uses_archive_page_when_persisted_page_row_is_missing() {
    let mut reader = TestBookMediaReader::default();
    reader.media_by_book.insert(
        "book-1".to_string(),
        super::BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: PathBuf::from("/library/book.cbz"),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 0,
        },
    );
    let content = TestBookMediaContent {
        archive_page_row: Some(BookPageRecord {
            number: 1,
            file_name: "001.png".to_string(),
            media_type: "image/png".to_string(),
            width: Some(640),
            height: Some(900),
            file_size: 12,
        }),
        page_bytes: b"page-bytes".to_vec(),
    };
    let service = BookMediaDeliveryService::new(&reader, &content, &IdentityBookIdResolver);

    let delivery = service
        .book_page(&admin_user(), "book-1", 1, BookMediaPageRequest::default())
        .await;

    assert_eq!(
        delivery,
        BookMediaDelivery::Asset(BookMediaDeliveryAsset {
            bytes: b"page-bytes".to_vec(),
            content_type: "image/png".to_string(),
            file_name: Some("book.cbz-1.png".to_string()),
            source_file: Some(PathBuf::from("/library/book.cbz")),
            disposition: BookMediaDeliveryDisposition::Inline,
        })
    );
}

#[tokio::test]
async fn book_page_preserves_page_media_type_extension_in_file_name() {
    let mut reader = TestBookMediaReader::default();
    reader.media_by_book.insert(
        "book-1".to_string(),
        super::BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: PathBuf::from("/library/book.cbz"),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 0,
        },
    );
    let content = TestBookMediaContent {
        archive_page_row: Some(BookPageRecord {
            number: 1,
            file_name: "001.webp".to_string(),
            media_type: "image/webp".to_string(),
            width: Some(640),
            height: Some(900),
            file_size: 12,
        }),
        page_bytes: b"webp-bytes".to_vec(),
    };
    let service = BookMediaDeliveryService::new(&reader, &content, &IdentityBookIdResolver);

    let delivery = service
        .book_page(&admin_user(), "book-1", 1, BookMediaPageRequest::default())
        .await;

    let BookMediaDelivery::Asset(asset) = delivery else {
        panic!("book page should resolve to an asset");
    };
    assert_eq!(asset.file_name, Some("book.cbz-1.webp".to_string()));
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
