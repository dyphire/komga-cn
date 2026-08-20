use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::discovery::PersistedBookIdResolverPort;
use crate::identity_access::{AuthUser, AuthUserRole};

use super::{
    BookAccessRestrictions, BookMediaContentPort, BookMediaDelivery, BookMediaDeliveryAsset,
    BookMediaDeliveryDisposition, BookMediaDeliveryService, BookMediaPageRequest,
    BookMediaReaderPort, BookPageRecord, EpubCoverImage, ImageOutputFormat, MediaImageDimensions,
    RenderedImage,
};

fn clone_result<T: Clone>(result: &Result<T, String>) -> anyhow::Result<T> {
    result.clone().map_err(anyhow::Error::msg)
}

fn clone_rendered_result(
    result: &Result<Option<Vec<u8>>, String>,
    output_format: ImageOutputFormat,
) -> anyhow::Result<Option<RenderedImage>> {
    clone_result(result).map(|bytes| {
        bytes.map(|bytes| RenderedImage {
            bytes,
            format: output_format,
        })
    })
}

#[derive(Default)]
struct TestBookMediaReader {
    media_by_book: HashMap<String, super::BookMediaRecord>,
    page_by_book: HashMap<String, BookPageRecord>,
    restriction_error: Option<String>,
    media_ready_error: Option<String>,
    book_page_error: Option<String>,
    selected_thumbnail_error: Option<String>,
}

#[async_trait::async_trait]
impl BookMediaReaderPort for TestBookMediaReader {
    async fn book_media(&self, book_id: &str) -> anyhow::Result<Option<super::BookMediaRecord>> {
        Ok(self.media_by_book.get(book_id).cloned())
    }

    async fn book_media_is_ready(&self, _book_id: &str) -> anyhow::Result<bool> {
        if let Some(error) = self.media_ready_error.clone() {
            return Err(anyhow::anyhow!(error));
        }
        Ok(true)
    }

    async fn book_pages(&self, _book_id: &str) -> anyhow::Result<Vec<BookPageRecord>> {
        Ok(Vec::new())
    }

    async fn book_page(
        &self,
        book_id: &str,
        _page_number: u64,
    ) -> anyhow::Result<Option<BookPageRecord>> {
        if let Some(error) = self.book_page_error.clone() {
            return Err(anyhow::anyhow!(error));
        }
        Ok(self.page_by_book.get(book_id).cloned())
    }

    async fn book_restrictions(
        &self,
        _book_id: &str,
    ) -> anyhow::Result<Option<BookAccessRestrictions>> {
        if let Some(error) = self.restriction_error.clone() {
            return Err(anyhow::anyhow!(error));
        }
        Ok(None)
    }

    async fn selected_book_thumbnail(
        &self,
        _book_id: &str,
    ) -> anyhow::Result<Option<super::EntityThumbnailBinary>> {
        if let Some(error) = self.selected_thumbnail_error.clone() {
            return Err(anyhow::anyhow!(error));
        }
        Ok(None)
    }
}

struct TestBookMediaContent {
    archive_page_row: Result<Option<BookPageRecord>, String>,
    page_bytes: Result<Option<Vec<u8>>, String>,
    thumbnail_bytes: Result<Option<Vec<u8>>, String>,
    pdf_page_bytes: Result<Option<Vec<u8>>, String>,
    pdf_page_count: Result<Option<u64>, String>,
    media_file_bytes: Result<Option<Vec<u8>>, String>,
    media_file_exists: Result<bool, String>,
    media_file_size: Result<Option<i64>, String>,
    media_image_dimensions: Result<Option<MediaImageDimensions>, String>,
    converted_image_bytes: Result<Option<Vec<u8>>, String>,
    epub_cover: Result<Option<EpubCoverImage>, String>,
}

impl Default for TestBookMediaContent {
    fn default() -> Self {
        Self {
            archive_page_row: Ok(None),
            page_bytes: Ok(None),
            thumbnail_bytes: Ok(None),
            pdf_page_bytes: Ok(None),
            pdf_page_count: Ok(None),
            media_file_bytes: Ok(None),
            media_file_exists: Ok(true),
            media_file_size: Ok(None),
            media_image_dimensions: Ok(None),
            converted_image_bytes: Ok(None),
            epub_cover: Ok(None),
        }
    }
}

#[async_trait::async_trait]
impl BookMediaContentPort for TestBookMediaContent {
    async fn resolve_page_bytes(
        &self,
        _media: &super::BookMediaRecord,
        _page: &BookPageRecord,
        _page_number: u64,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        clone_result(&self.page_bytes)
    }

    async fn render_page_thumbnail(
        &self,
        _media: &super::BookMediaRecord,
        _page: &BookPageRecord,
        _page_number: u64,
        _max_edge: u32,
        output_format: ImageOutputFormat,
    ) -> anyhow::Result<Option<RenderedImage>> {
        clone_rendered_result(&self.thumbnail_bytes, output_format)
    }

    async fn render_pdf_page(
        &self,
        _media: &super::BookMediaRecord,
        _page_number: u64,
        output_format: ImageOutputFormat,
    ) -> anyhow::Result<Option<RenderedImage>> {
        clone_rendered_result(&self.pdf_page_bytes, output_format)
    }

    async fn archive_page_row(
        &self,
        _media: &super::BookMediaRecord,
        _page_number: u64,
    ) -> anyhow::Result<Option<BookPageRecord>> {
        clone_result(&self.archive_page_row)
    }

    async fn archive_page_rows(
        &self,
        _media: &super::BookMediaRecord,
    ) -> anyhow::Result<Option<Vec<BookPageRecord>>> {
        Ok(None)
    }

    fn pdf_page_row(
        &self,
        _media: &super::BookMediaRecord,
        _page_number: u64,
    ) -> anyhow::Result<Option<BookPageRecord>> {
        Ok(None)
    }

    fn generated_pdf_page_rows(
        &self,
        _media: &super::BookMediaRecord,
    ) -> anyhow::Result<Vec<BookPageRecord>> {
        Ok(Vec::new())
    }

    fn read_pdf_page_as_single_page_pdf(
        &self,
        _media: &super::BookMediaRecord,
        _page_number: u64,
    ) -> anyhow::Result<Option<Vec<u8>>> {
        clone_result(&self.pdf_page_bytes)
    }

    fn detect_pdf_page_count(
        &self,
        _media: &super::BookMediaRecord,
    ) -> anyhow::Result<Option<u64>> {
        clone_result(&self.pdf_page_count)
    }

    fn media_file_exists(&self, _path: &Path) -> anyhow::Result<bool> {
        clone_result(&self.media_file_exists)
    }

    async fn read_media_file_bytes(&self, _path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
        clone_result(&self.media_file_bytes)
    }

    async fn read_media_file_size(&self, _path: &Path) -> anyhow::Result<Option<i64>> {
        clone_result(&self.media_file_size)
    }

    async fn read_media_image_dimensions(
        &self,
        _path: &Path,
    ) -> anyhow::Result<Option<MediaImageDimensions>> {
        clone_result(&self.media_image_dimensions)
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
        clone_result(&self.converted_image_bytes)
    }

    async fn epub_cover_bytes(
        &self,
        _media: &super::BookMediaRecord,
    ) -> anyhow::Result<Option<EpubCoverImage>> {
        clone_result(&self.epub_cover)
    }
}

struct IdentityBookIdResolver;

#[async_trait::async_trait]
impl PersistedBookIdResolverPort for IdentityBookIdResolver {
    async fn persisted_book_resource_exists(&self, _book_id: &str) -> anyhow::Result<bool> {
        Ok(true)
    }

    async fn load_book_id_by_sorted_position(
        &self,
        _index: usize,
    ) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
}

#[tokio::test]
async fn book_file_propagates_restriction_load_errors() {
    let mut reader = TestBookMediaReader {
        restriction_error: Some("restriction lookup failed".to_string()),
        ..Default::default()
    };
    reader.media_by_book.insert(
        "book-1".to_string(),
        super::BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: PathBuf::from("/library/book.cbz"),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 1,
        },
    );
    let content = TestBookMediaContent::default();
    let service = BookMediaDeliveryService::new(&reader, &content, &IdentityBookIdResolver);

    let delivery = service.book_file(&admin_user(), "book-1").await;

    assert_eq!(
        delivery,
        BookMediaDelivery::Internal(anyhow::anyhow!("restriction lookup failed"))
    );
}

#[tokio::test]
async fn book_page_propagates_media_ready_load_errors() {
    let mut reader = TestBookMediaReader {
        media_ready_error: Some("media ready lookup failed".to_string()),
        ..Default::default()
    };
    reader.media_by_book.insert(
        "book-1".to_string(),
        super::BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: PathBuf::from("/library/book.cbz"),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 1,
        },
    );
    let content = TestBookMediaContent::default();
    let service = BookMediaDeliveryService::new(&reader, &content, &IdentityBookIdResolver);

    let delivery = service
        .book_page(&admin_user(), "book-1", 1, BookMediaPageRequest::default())
        .await;

    assert_eq!(
        delivery,
        BookMediaDelivery::Internal(anyhow::anyhow!("media ready lookup failed"))
    );
}

#[tokio::test]
async fn page_delivery_requires_page_streaming_role_even_for_admins() {
    let mut reader = TestBookMediaReader::default();
    reader.media_by_book.insert(
        "book-1".to_string(),
        super::BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: PathBuf::from("/library/book.cbz"),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 1,
        },
    );
    let content = TestBookMediaContent::default();
    let service = BookMediaDeliveryService::new(&reader, &content, &IdentityBookIdResolver);
    let user = admin_without_page_streaming_user();

    let page_delivery = service
        .book_page(&user, "book-1", 1, BookMediaPageRequest::default())
        .await;
    let pages_delivery = service.book_pages(&user, "book-1").await;
    let thumbnail_delivery = service
        .book_page_thumbnail(&user, "book-1", 1, ImageOutputFormat::Jpeg)
        .await;

    assert_eq!(page_delivery, BookMediaDelivery::Forbidden);
    assert_eq!(pages_delivery, BookMediaDelivery::Forbidden);
    assert_eq!(thumbnail_delivery, BookMediaDelivery::Forbidden);
}

#[tokio::test]
async fn book_page_renders_pdf_using_requested_image_format() {
    let mut reader = TestBookMediaReader::default();
    reader.media_by_book.insert(
        "book-pdf".to_string(),
        super::BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "book.pdf".to_string(),
            file_path: PathBuf::from("/library/book.pdf"),
            media_type: "application/pdf".to_string(),
            page_count: 1,
        },
    );
    reader.page_by_book.insert(
        "book-pdf".to_string(),
        BookPageRecord {
            number: 1,
            file_name: "1".to_string(),
            media_type: "image/jpeg".to_string(),
            width: Some(3_200),
            height: Some(4_528),
            file_size: -1,
        },
    );
    let content = TestBookMediaContent {
        pdf_page_bytes: Ok(Some(b"small-avif-fixture".to_vec())),
        ..Default::default()
    };
    let service = BookMediaDeliveryService::new(&reader, &content, &IdentityBookIdResolver);

    let delivery = service
        .book_page(
            &admin_user(),
            "book-pdf",
            1,
            BookMediaPageRequest {
                image_format: Some(ImageOutputFormat::Avif),
                ..Default::default()
            },
        )
        .await;

    assert_eq!(
        delivery,
        BookMediaDelivery::Asset(BookMediaDeliveryAsset {
            bytes: b"small-avif-fixture".to_vec(),
            content_type: "image/avif".to_string(),
            file_name: Some("book.pdf-1.avif".to_string()),
            source_file: Some(PathBuf::from("/library/book.pdf")),
            disposition: BookMediaDeliveryDisposition::Inline,
        })
    );
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
        archive_page_row: Ok(Some(BookPageRecord {
            number: 1,
            file_name: "001.png".to_string(),
            media_type: "image/png".to_string(),
            width: Some(640),
            height: Some(900),
            file_size: 12,
        })),
        page_bytes: Ok(Some(b"page-bytes".to_vec())),
        ..Default::default()
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
async fn book_page_propagates_page_byte_load_errors() {
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
        archive_page_row: Ok(Some(BookPageRecord {
            number: 1,
            file_name: "001.png".to_string(),
            media_type: "image/png".to_string(),
            width: Some(640),
            height: Some(900),
            file_size: 12,
        })),
        page_bytes: Err("page bytes failed".to_string()),
        ..Default::default()
    };
    let service = BookMediaDeliveryService::new(&reader, &content, &IdentityBookIdResolver);

    let delivery = service
        .book_page(&admin_user(), "book-1", 1, BookMediaPageRequest::default())
        .await;

    assert_eq!(
        delivery,
        BookMediaDelivery::Internal(anyhow::anyhow!("page bytes failed"))
    );
}

#[tokio::test]
async fn book_page_propagates_page_conversion_errors() {
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
        archive_page_row: Ok(Some(BookPageRecord {
            number: 1,
            file_name: "001.png".to_string(),
            media_type: "image/png".to_string(),
            width: Some(640),
            height: Some(900),
            file_size: 12,
        })),
        page_bytes: Ok(Some(b"not-an-image".to_vec())),
        converted_image_bytes: Err("page conversion failed".to_string()),
        ..Default::default()
    };
    let service = BookMediaDeliveryService::new(&reader, &content, &IdentityBookIdResolver);

    let delivery = service
        .book_page(
            &admin_user(),
            "book-1",
            1,
            BookMediaPageRequest {
                convert: Some("jpeg".to_string()),
                ..Default::default()
            },
        )
        .await;

    assert_eq!(
        delivery,
        BookMediaDelivery::Internal(anyhow::anyhow!("page conversion failed"))
    );
}

#[tokio::test]
async fn book_page_propagates_single_image_dimension_load_errors() {
    let mut reader = TestBookMediaReader::default();
    reader.media_by_book.insert(
        "book-1".to_string(),
        super::BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "cover.jpg".to_string(),
            file_path: PathBuf::from("/library/cover.jpg"),
            media_type: "image/jpeg".to_string(),
            page_count: 1,
        },
    );
    let content = TestBookMediaContent {
        media_image_dimensions: Err("image dimensions failed".to_string()),
        ..Default::default()
    };
    let service = BookMediaDeliveryService::new(&reader, &content, &IdentityBookIdResolver);

    let delivery = service
        .book_page(&admin_user(), "book-1", 1, BookMediaPageRequest::default())
        .await;

    assert_eq!(
        delivery,
        BookMediaDelivery::Internal(anyhow::anyhow!("image dimensions failed"))
    );
}

#[tokio::test]
async fn book_page_propagates_single_image_file_size_load_errors() {
    let mut reader = TestBookMediaReader::default();
    reader.media_by_book.insert(
        "book-1".to_string(),
        super::BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "cover.jpg".to_string(),
            file_path: PathBuf::from("/library/cover.jpg"),
            media_type: "image/jpeg".to_string(),
            page_count: 1,
        },
    );
    let content = TestBookMediaContent {
        media_file_size: Err("file size failed".to_string()),
        ..Default::default()
    };
    let service = BookMediaDeliveryService::new(&reader, &content, &IdentityBookIdResolver);

    let delivery = service
        .book_page(&admin_user(), "book-1", 1, BookMediaPageRequest::default())
        .await;

    assert_eq!(
        delivery,
        BookMediaDelivery::Internal(anyhow::anyhow!("file size failed"))
    );
}

#[tokio::test]
async fn book_pages_do_not_synthesize_single_image_row_for_missing_file() {
    let mut reader = TestBookMediaReader::default();
    reader.media_by_book.insert(
        "book-1".to_string(),
        super::BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "cover.jpg".to_string(),
            file_path: PathBuf::from("/library/cover.jpg"),
            media_type: "image/jpeg".to_string(),
            page_count: 1,
        },
    );
    let content = TestBookMediaContent {
        media_file_exists: Ok(false),
        ..Default::default()
    };
    let service = BookMediaDeliveryService::new(&reader, &content, &IdentityBookIdResolver);

    let delivery = service.book_pages(&admin_user(), "book-1").await;

    assert_eq!(delivery, BookMediaDelivery::NotFound);
}

#[tokio::test]
async fn book_pages_propagate_single_image_file_probe_errors() {
    let mut reader = TestBookMediaReader::default();
    reader.media_by_book.insert(
        "book-1".to_string(),
        super::BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "cover.jpg".to_string(),
            file_path: PathBuf::from("/library/cover.jpg"),
            media_type: "image/jpeg".to_string(),
            page_count: 1,
        },
    );
    let content = TestBookMediaContent {
        media_file_exists: Err("file probe failed".to_string()),
        ..Default::default()
    };
    let service = BookMediaDeliveryService::new(&reader, &content, &IdentityBookIdResolver);

    let delivery = service.book_pages(&admin_user(), "book-1").await;

    assert_eq!(
        delivery,
        BookMediaDelivery::Internal(anyhow::anyhow!("file probe failed"))
    );
}

#[tokio::test]
async fn book_pages_propagate_single_image_file_size_missing_after_probe() {
    let mut reader = TestBookMediaReader::default();
    reader.media_by_book.insert(
        "book-1".to_string(),
        super::BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "cover.jpg".to_string(),
            file_path: PathBuf::from("/library/cover.jpg"),
            media_type: "image/jpeg".to_string(),
            page_count: 1,
        },
    );
    let content = TestBookMediaContent {
        media_file_exists: Ok(true),
        media_file_size: Ok(None),
        ..Default::default()
    };
    let service = BookMediaDeliveryService::new(&reader, &content, &IdentityBookIdResolver);

    let delivery = service.book_pages(&admin_user(), "book-1").await;

    assert_eq!(
        delivery,
        BookMediaDelivery::Internal(anyhow::anyhow!(
            "single image media file missing: /library/cover.jpg"
        ))
    );
}

#[tokio::test]
async fn book_thumbnail_source_propagates_selected_thumbnail_load_errors() {
    let mut reader = TestBookMediaReader {
        selected_thumbnail_error: Some("selected thumbnail lookup failed".to_string()),
        ..Default::default()
    };
    reader.media_by_book.insert(
        "book-1".to_string(),
        super::BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: PathBuf::from("/library/book.cbz"),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 1,
        },
    );
    let content = TestBookMediaContent::default();
    let service = BookMediaDeliveryService::new(&reader, &content, &IdentityBookIdResolver);

    let delivery = service.book_thumbnail_source(&admin_user(), "book-1").await;

    assert_eq!(
        delivery,
        super::BookThumbnailDelivery::Internal(anyhow::anyhow!("selected thumbnail lookup failed"))
    );
}

#[tokio::test]
async fn book_thumbnail_source_propagates_page_lookup_errors() {
    let mut reader = TestBookMediaReader {
        book_page_error: Some("thumbnail page lookup failed".to_string()),
        ..Default::default()
    };
    reader.media_by_book.insert(
        "book-1".to_string(),
        super::BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: PathBuf::from("/library/book.cbz"),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 1,
        },
    );
    let content = TestBookMediaContent {
        archive_page_row: Ok(Some(BookPageRecord {
            number: 1,
            file_name: "001.png".to_string(),
            media_type: "image/png".to_string(),
            width: Some(640),
            height: Some(900),
            file_size: 12,
        })),
        page_bytes: Ok(Some(b"page-bytes".to_vec())),
        ..Default::default()
    };
    let service = BookMediaDeliveryService::new(&reader, &content, &IdentityBookIdResolver);

    let delivery = service.book_thumbnail_source(&admin_user(), "book-1").await;

    assert_eq!(
        delivery,
        super::BookThumbnailDelivery::Internal(anyhow::anyhow!("thumbnail page lookup failed"))
    );
}

#[tokio::test]
async fn book_thumbnail_source_propagates_epub_cover_errors() {
    let mut reader = TestBookMediaReader::default();
    reader.media_by_book.insert(
        "book-1".to_string(),
        super::BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "book.epub".to_string(),
            file_path: PathBuf::from("/library/book.epub"),
            media_type: "application/epub+zip".to_string(),
            page_count: 1,
        },
    );
    let content = TestBookMediaContent {
        epub_cover: Err("epub cover read failed".to_string()),
        ..Default::default()
    };
    let service = BookMediaDeliveryService::new(&reader, &content, &IdentityBookIdResolver);

    let delivery = service.book_thumbnail_source(&admin_user(), "book-1").await;

    assert_eq!(
        delivery,
        super::BookThumbnailDelivery::Internal(anyhow::anyhow!("epub cover read failed"))
    );
}

#[tokio::test]
async fn book_page_thumbnail_propagates_render_errors() {
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
        archive_page_row: Ok(Some(BookPageRecord {
            number: 1,
            file_name: "001.png".to_string(),
            media_type: "image/png".to_string(),
            width: Some(640),
            height: Some(900),
            file_size: 12,
        })),
        thumbnail_bytes: Err("page thumbnail render failed".to_string()),
        ..Default::default()
    };
    let service = BookMediaDeliveryService::new(&reader, &content, &IdentityBookIdResolver);

    let delivery = service
        .book_page_thumbnail(&admin_user(), "book-1", 1, ImageOutputFormat::Jpeg)
        .await;

    assert_eq!(
        delivery,
        BookMediaDelivery::Internal(anyhow::anyhow!("page thumbnail render failed"))
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
        archive_page_row: Ok(Some(BookPageRecord {
            number: 1,
            file_name: "001.webp".to_string(),
            media_type: "image/webp".to_string(),
            width: Some(640),
            height: Some(900),
            file_size: 12,
        })),
        page_bytes: Ok(Some(b"webp-bytes".to_vec())),
        ..Default::default()
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
        roles: vec![AuthUserRole::Admin, AuthUserRole::PageStreaming],
        shared_all_libraries: true,
        shared_library_ids: Vec::new(),
        labels_allow: Vec::new(),
        labels_exclude: Vec::new(),
        age_restriction: None,
    }
}

fn admin_without_page_streaming_user() -> AuthUser {
    AuthUser {
        id: "admin-without-page-streaming".to_string(),
        email: "admin-without-page-streaming@example.org".to_string(),
        password: "password".to_string(),
        roles: vec![AuthUserRole::Admin],
        shared_all_libraries: true,
        shared_library_ids: Vec::new(),
        labels_allow: Vec::new(),
        labels_exclude: Vec::new(),
        age_restriction: None,
    }
}
