use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::{
    ArchiveBuilderPort, ArchiveContentPort, ArchiveDelivery, ArchiveDeliveryService, ArchiveEntry,
    ArchiveFileEntry, ArchiveReaderPort, BookMediaRecord, SeriesArchiveEntries,
};

#[derive(Default)]
struct TestArchiveReader {
    readlist_names: HashMap<String, String>,
    media_by_book: HashMap<String, BookMediaRecord>,
    series_by_id: HashMap<String, SeriesArchiveEntries>,
}

#[async_trait::async_trait]
impl ArchiveReaderPort for TestArchiveReader {
    async fn readlist_name(&self, readlist_id: &str) -> Result<Option<String>, String> {
        Ok(self.readlist_names.get(readlist_id).cloned())
    }

    async fn book_media(&self, book_id: &str) -> Result<Option<BookMediaRecord>, String> {
        Ok(self.media_by_book.get(book_id).cloned())
    }

    async fn series_archive_entries(
        &self,
        series_id: &str,
    ) -> Result<Option<SeriesArchiveEntries>, String> {
        Ok(self.series_by_id.get(series_id).cloned())
    }
}

#[derive(Default)]
struct TestArchiveContent {
    bytes_by_path: HashMap<PathBuf, Result<Option<Vec<u8>>, String>>,
}

#[async_trait::async_trait]
impl ArchiveContentPort for TestArchiveContent {
    async fn read_media_file_bytes(&self, path: &Path) -> Result<Option<Vec<u8>>, String> {
        self.bytes_by_path.get(path).cloned().unwrap_or(Ok(None))
    }
}

struct TestArchiveBuilder {
    result: Result<Vec<u8>, String>,
    entries: Mutex<Vec<Vec<ArchiveFileEntry>>>,
}

impl Default for TestArchiveBuilder {
    fn default() -> Self {
        Self {
            result: Ok(b"archive-bytes".to_vec()),
            entries: Mutex::new(Vec::new()),
        }
    }
}

impl TestArchiveBuilder {
    fn failing(error: &str) -> Self {
        Self {
            result: Err(error.to_string()),
            entries: Mutex::new(Vec::new()),
        }
    }

    fn recorded_entries(&self) -> Vec<Vec<ArchiveFileEntry>> {
        self.entries.lock().expect("entries lock").clone()
    }
}

impl ArchiveBuilderPort for TestArchiveBuilder {
    fn build_archive(&self, entries: Vec<ArchiveFileEntry>) -> Result<Vec<u8>, String> {
        self.entries.lock().expect("entries lock").push(entries);
        self.result.clone()
    }
}

#[tokio::test]
async fn readlist_archive_names_visible_books_and_skips_missing_content() {
    let mut reader = TestArchiveReader::default();
    reader
        .readlist_names
        .insert("readlist-1".to_string(), "Readlist One".to_string());
    reader.media_by_book.insert(
        "book-1".to_string(),
        media_record("books/book-1.epub", "/library/books/book-1.epub"),
    );
    reader.media_by_book.insert(
        "book-3".to_string(),
        media_record("books/book-3.epub", "/library/books/book-3.epub"),
    );

    let mut content = TestArchiveContent::default();
    content.bytes_by_path.insert(
        PathBuf::from("/library/books/book-1.epub"),
        Ok(Some(b"book-one".to_vec())),
    );
    content.bytes_by_path.insert(
        PathBuf::from("/library/books/book-3.epub"),
        Ok(Some(b"book-three".to_vec())),
    );

    let builder = TestArchiveBuilder::default();
    let service = ArchiveDeliveryService::new(&reader, &content, &builder);
    let delivery = service
        .readlist_archive(
            "readlist-1",
            vec![
                "book-1".to_string(),
                "missing-book".to_string(),
                "book-3".to_string(),
            ],
        )
        .await;

    let ArchiveDelivery::Asset(asset) = delivery else {
        panic!("readlist archive should resolve");
    };
    assert_eq!(asset.file_name, "Readlist One.zip");
    assert_eq!(
        asset.bytes, b"archive-bytes",
        "delivery should expose builder output"
    );
    assert_eq!(
        builder.recorded_entries(),
        vec![vec![
            ArchiveFileEntry {
                file_name: "1 - book-1.epub".to_string(),
                bytes: b"book-one".to_vec(),
            },
            ArchiveFileEntry {
                file_name: "3 - book-3.epub".to_string(),
                bytes: b"book-three".to_vec(),
            },
        ]]
    );
}

#[tokio::test]
async fn readlist_archive_propagates_content_read_errors() {
    let mut reader = TestArchiveReader::default();
    reader
        .readlist_names
        .insert("readlist-1".to_string(), "Readlist One".to_string());
    reader.media_by_book.insert(
        "book-1".to_string(),
        media_record("books/book-1.epub", "/library/books/book-1.epub"),
    );

    let mut content = TestArchiveContent::default();
    content.bytes_by_path.insert(
        PathBuf::from("/library/books/book-1.epub"),
        Err("read book file failed".to_string()),
    );

    let builder = TestArchiveBuilder::default();
    let service = ArchiveDeliveryService::new(&reader, &content, &builder);
    let delivery = service
        .readlist_archive("readlist-1", vec!["book-1".to_string()])
        .await;

    assert_eq!(
        delivery,
        ArchiveDelivery::Internal("read book file failed".to_string())
    );
}

#[tokio::test]
async fn series_archive_preserves_entry_names_and_skips_missing_content() {
    let mut reader = TestArchiveReader::default();
    reader.series_by_id.insert(
        "series-1".to_string(),
        SeriesArchiveEntries {
            series_title: "Series One".to_string(),
            entries: vec![
                ArchiveEntry {
                    file_name: "Volume 01.cbz".to_string(),
                    file_path: PathBuf::from("/library/series/volume-1.cbz"),
                },
                ArchiveEntry {
                    file_name: "Volume 02.cbz".to_string(),
                    file_path: PathBuf::from("/library/series/volume-2.cbz"),
                },
            ],
        },
    );

    let mut content = TestArchiveContent::default();
    content.bytes_by_path.insert(
        PathBuf::from("/library/series/volume-2.cbz"),
        Ok(Some(b"volume-two".to_vec())),
    );

    let builder = TestArchiveBuilder::default();
    let service = ArchiveDeliveryService::new(&reader, &content, &builder);
    let delivery = service.series_archive("series-1").await;

    let ArchiveDelivery::Asset(asset) = delivery else {
        panic!("series archive should resolve");
    };
    assert_eq!(asset.file_name, "Series One.zip");
    assert_eq!(
        builder.recorded_entries(),
        vec![vec![ArchiveFileEntry {
            file_name: "Volume 02.cbz".to_string(),
            bytes: b"volume-two".to_vec(),
        }]]
    );
}

#[tokio::test]
async fn archive_delivery_returns_builder_errors() {
    let mut reader = TestArchiveReader::default();
    reader
        .readlist_names
        .insert("readlist-1".to_string(), "Readlist One".to_string());
    reader.media_by_book.insert(
        "book-1".to_string(),
        media_record("book-1.epub", "/library/book-1.epub"),
    );

    let mut content = TestArchiveContent::default();
    content.bytes_by_path.insert(
        PathBuf::from("/library/book-1.epub"),
        Ok(Some(b"book-one".to_vec())),
    );

    let builder = TestArchiveBuilder::failing("zip builder failed");
    let service = ArchiveDeliveryService::new(&reader, &content, &builder);
    let delivery = service
        .readlist_archive("readlist-1", vec!["book-1".to_string()])
        .await;

    assert_eq!(
        delivery,
        ArchiveDelivery::Internal("zip builder failed".to_string())
    );
}

fn media_record(file_name: &str, file_path: &str) -> BookMediaRecord {
    BookMediaRecord {
        library_id: "library-1".to_string(),
        file_name: file_name.to_string(),
        file_path: PathBuf::from(file_path),
        media_type: "application/epub+zip".to_string(),
        page_count: 0,
    }
}
