use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::Value;
use serde_json::json;

use crate::identity_access::{
    DeviceProgressService, DeviceSyncPort, KoboReadingStateUpdate, KoreaderBookLookupError,
    KoreaderBookTarget, PersistedReadProgressRecord, ReadProgressWithLocatorInput,
};
use crate::media_assets::{
    BookMediaRecord, BookPageRecord, BookProgressionInput, CollectionThumbnailRecord,
    ContentAccessPort, ContentResolverPort, EntityExistencePort, EntityThumbnailBinary,
    EntityThumbnailRecord, EpubPositionsExtension, PersistedMediaFileRecord, ProgressWriterPort,
    ReadProgressReadPort, ReadlistThumbnailRecord, SeriesRelationPort, SeriesThumbnailRecord,
    ThumbnailReadPort,
};

#[tokio::test]
async fn koreader_visual_progress_update_persists_typed_book_progression() {
    let device_sync = Arc::new(TestDeviceSync {
        koreader_target: Some(KoreaderBookTarget {
            id: "book-1".to_string(),
            page_count: 10,
            media_type: "application/vnd.comicbook+zip".to_string(),
        }),
        ..TestDeviceSync::default()
    });
    let progress = Arc::new(TestProgressWriter::default());
    let reader = NoopMediaReader::default();
    let content = NoopContentResolver::default();
    let service =
        DeviceProgressService::new(device_sync.as_ref(), &reader, &content, progress.as_ref());

    service
        .update_koreader_progress(
            "user-1",
            crate::identity_access::KoreaderProgressUpdate {
                document: "hash-book-1".to_string(),
                percentage: 0.7,
                progress: "7".to_string(),
                device: "KOReader".to_string(),
                device_id: "device-1".to_string(),
                modified: "2026-06-07T12:00:00Z".to_string(),
            },
        )
        .await
        .expect("visual KOReader progress should persist");

    let persisted = progress.persisted.lock().unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].book_id, "book-1");
    assert_eq!(persisted[0].user_id, "user-1");
    assert_eq!(persisted[0].page, 7);
    assert!(!persisted[0].completed);
    assert_eq!(
        persisted[0].locator,
        Some(json!({
            "koreaderProgress": "7",
            "locations": {
                "position": 7,
                "totalProgression": 0.7,
            },
        }))
    );
}

#[tokio::test]
async fn koreader_progress_maps_epub_locator_href_to_doc_fragment() {
    let device_sync = TestDeviceSync {
        koreader_target: Some(KoreaderBookTarget {
            id: "book-1".to_string(),
            page_count: 10,
            media_type: "application/epub+zip".to_string(),
        }),
        read_progress: Some(PersistedReadProgressRecord {
            page: 3,
            completed: false,
            created: "2026-01-01T00:00:00Z".to_string(),
            last_modified: "2026-01-02T00:00:00Z".to_string(),
            device_id: "device-a".to_string(),
            device_name: "KOReader".to_string(),
            locator: Some(
                serde_json::to_vec(&json!({
                    "href": "chapter-2.xhtml",
                    "locations": {
                        "totalProgression": 0.5,
                    }
                }))
                .expect("locator should serialize"),
            ),
        }),
    };
    let reader = NoopMediaReader {
        epub_extension_blob: Some(("EPUB".to_string(), Vec::new())),
        ..NoopMediaReader::default()
    };
    let content = NoopContentResolver {
        positions_extension: EpubPositionsExtension {
            positions: vec![
                json!({"href": "chapter-1.xhtml"}),
                json!({"href": "chapter-2.xhtml"}),
            ],
            is_fixed_layout: false,
        },
    };
    let progress = TestProgressWriter::default();
    let service = DeviceProgressService::new(&device_sync, &reader, &content, &progress);

    let snapshot = service
        .koreader_progress("hash-book-1", "user-1")
        .await
        .expect("KOReader progress should load");

    assert_eq!(snapshot.percentage, 0.5);
    assert_eq!(snapshot.progress, "/body/DocFragment[2].0");
    assert_eq!(snapshot.device, "KOReader");
    assert_eq!(snapshot.device_id, "device-a");
}

#[tokio::test]
async fn kobo_reading_state_uses_locator_progress_values() {
    let device_sync = TestDeviceSync {
        read_progress: Some(PersistedReadProgressRecord {
            page: 3,
            completed: false,
            created: "2026-01-01T00:00:00Z".to_string(),
            last_modified: "2026-01-02T00:00:00Z".to_string(),
            device_id: "device-a".to_string(),
            device_name: "KOReader".to_string(),
            locator: Some(
                serde_json::to_vec(&json!({
                    "href": "/chapter-2.xhtml",
                    "koboSpan": "span-2",
                    "locations": {
                        "progression": 0.25,
                        "totalProgression": 0.5,
                    }
                }))
                .expect("locator should serialize"),
            ),
        }),
        ..TestDeviceSync::default()
    };
    let progress = TestProgressWriter::default();
    let reader = NoopMediaReader::default();
    let content = NoopContentResolver::default();
    let service = DeviceProgressService::new(&device_sync, &reader, &content, &progress);

    let payload = service
        .kobo_reading_state("book-1", "user-1", "2026-01-01T00:00:00Z")
        .await
        .expect("kobo reading state should build");

    assert_eq!(
        payload
            .get("CurrentBookmark")
            .and_then(|value| value.get("ProgressPercent")),
        Some(&json!(50.0))
    );
    assert_eq!(
        payload
            .get("CurrentBookmark")
            .and_then(|value| value.get("ContentSourceProgressPercent")),
        Some(&json!(25.0))
    );
    assert_eq!(
        payload
            .get("CurrentBookmark")
            .and_then(|value| value.get("Location"))
            .and_then(|value| value.get("Source")),
        Some(&Value::String("/chapter-2.xhtml".to_string()))
    );
}

#[tokio::test]
async fn kobo_reading_state_update_normalizes_locator_and_persists_progression() {
    let device_sync = TestDeviceSync::default();
    let reader = NoopMediaReader {
        media_files: vec!["/book-1.xhtml".to_string()],
        epub_extension_blob: Some(("EPUB".to_string(), Vec::new())),
        page_count: Some(10),
        ..NoopMediaReader::default()
    };
    let content = NoopContentResolver {
        positions_extension: EpubPositionsExtension {
            positions: vec![json!({
                "href": "book-1.xhtml",
                "type": "application/xhtml+xml",
                "koboSpan": "kobo.5.1",
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
    let service = DeviceProgressService::new(&device_sync, &reader, &content, &progress);

    service
        .update_kobo_reading_state(
            "book-1",
            "user-1",
            KoboReadingStateUpdate {
                last_modified: "2026-03-27T10:00:00Z".to_string(),
                status: "Reading".to_string(),
                progress_percent: Some(99.0),
                content_source_progress_percent: Some(50.0),
                location_source: "/book-1.xhtml#frag".to_string(),
                location_type: "KoboSpan".to_string(),
                location_value: Some("kobo.5.1".to_string()),
                device_id: "api-key-validkobotoken".to_string(),
                device_name: "kobo sync".to_string(),
            },
        )
        .await
        .expect("kobo reading state update should persist");

    let persisted = progress.persisted.lock().unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].book_id, "book-1");
    assert_eq!(persisted[0].user_id, "user-1");
    assert_eq!(persisted[0].page, 2);
    assert!(!persisted[0].completed);
    assert_eq!(
        persisted[0]
            .locator
            .as_ref()
            .and_then(|locator| locator.pointer("/locations/totalProgression")),
        Some(&json!(0.21))
    );
    assert_eq!(
        persisted[0].device_id.as_deref(),
        Some("api-key-validkobotoken")
    );
    assert_eq!(persisted[0].device_name.as_deref(), Some("kobo sync"));
}

#[tokio::test]
async fn kobo_reading_state_update_accepts_fixed_layout_single_position() {
    let device_sync = TestDeviceSync::default();
    let reader = NoopMediaReader {
        media_files: vec!["/fixed.xhtml".to_string()],
        epub_extension_blob: Some(("EPUB".to_string(), Vec::new())),
        page_count: Some(10),
        ..NoopMediaReader::default()
    };
    let content = NoopContentResolver {
        positions_extension: EpubPositionsExtension {
            positions: vec![json!({
                "href": "fixed.xhtml",
                "type": "application/xhtml+xml",
                "locations": {
                    "progression": 0.0,
                    "totalProgression": 0.73,
                    "position": 4
                }
            })],
            is_fixed_layout: true,
        },
    };
    let progress = TestProgressWriter::default();
    let service = DeviceProgressService::new(&device_sync, &reader, &content, &progress);

    service
        .update_kobo_reading_state(
            "book-1",
            "user-1",
            KoboReadingStateUpdate {
                last_modified: "2026-03-27T10:00:00Z".to_string(),
                status: "Reading".to_string(),
                progress_percent: Some(90.0),
                content_source_progress_percent: Some(90.0),
                location_source: "/fixed.xhtml#frag".to_string(),
                location_type: "KoboSpan".to_string(),
                location_value: Some("kobo.1.1".to_string()),
                device_id: "api-key-validkobotoken".to_string(),
                device_name: "kobo sync".to_string(),
            },
        )
        .await
        .expect("fixed-layout Kobo update should accept the only matching position");

    let persisted = progress.persisted.lock().unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].page, 7);
    assert!(!persisted[0].completed);
    assert_eq!(
        persisted[0]
            .locator
            .as_ref()
            .and_then(|locator| locator.pointer("/locations/totalProgression")),
        Some(&json!(0.73))
    );
}

#[derive(Default)]
struct TestDeviceSync {
    koreader_target: Option<KoreaderBookTarget>,
    read_progress: Option<PersistedReadProgressRecord>,
}

#[async_trait]
impl DeviceSyncPort for TestDeviceSync {
    async fn load_book_created_timestamp(&self, _book_id: &str) -> Result<Option<String>, String> {
        Ok(None)
    }

    async fn load_book_last_epub_position_locator(
        &self,
        _book_id: &str,
    ) -> Result<Option<Value>, String> {
        Ok(None)
    }

    async fn load_kobo_metadata_record(
        &self,
        _book_id: &str,
    ) -> Result<Option<crate::identity_access::KoboMetadataRecord>, String> {
        Ok(None)
    }

    async fn load_koreader_book_target(
        &self,
        _book_hash: &str,
    ) -> Result<Option<KoreaderBookTarget>, KoreaderBookLookupError> {
        Ok(self.koreader_target.clone())
    }

    async fn load_read_progress(
        &self,
        _book_id: &str,
        _user_id: &str,
    ) -> Result<Option<crate::identity_access::PersistedReadProgressRecord>, String> {
        Ok(self.read_progress.clone())
    }

    async fn load_thumbnail_by_id(
        &self,
        _thumbnail_id: &str,
    ) -> Result<Option<(String, Vec<u8>)>, String> {
        Ok(None)
    }

    async fn persisted_book_exists(&self, _book_id: &str) -> Result<bool, String> {
        Ok(false)
    }

    async fn persist_read_progress_with_locator(
        &self,
        _input: ReadProgressWithLocatorInput,
    ) -> Result<(), String> {
        Ok(())
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

#[derive(Default)]
struct NoopContentResolver {
    positions_extension: EpubPositionsExtension,
}

#[async_trait]
impl ContentResolverPort for NoopContentResolver {
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
struct NoopMediaReader {
    media_files: Vec<String>,
    epub_extension_blob: Option<(String, Vec<u8>)>,
    book_progression: Option<Value>,
    page_count: Option<u64>,
}

#[async_trait]
impl crate::media_assets::BookMediaPort for NoopMediaReader {
    async fn book_media(&self, _book_id: &str) -> Result<Option<BookMediaRecord>, String> {
        Ok(None)
    }

    async fn book_media_files(&self, _book_id: &str) -> Result<Vec<String>, String> {
        Ok(self.media_files.clone())
    }

    async fn media_file_records(
        &self,
        _book_id: &str,
    ) -> Result<Vec<PersistedMediaFileRecord>, String> {
        Ok(Vec::new())
    }

    async fn book_media_is_ready(&self, _book_id: &str) -> Result<bool, String> {
        Ok(false)
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

    async fn epub_extension_blob(
        &self,
        _book_id: &str,
    ) -> Result<Option<(String, Vec<u8>)>, String> {
        Ok(self.epub_extension_blob.clone())
    }
}

#[async_trait]
impl SeriesRelationPort for NoopMediaReader {
    async fn series_book_ids(&self, _series_id: &str) -> Result<Vec<String>, String> {
        Ok(Vec::new())
    }

    async fn series_book_number_sorts(
        &self,
        _series_id: &str,
    ) -> Result<Vec<(String, f64)>, String> {
        Ok(Vec::new())
    }

    async fn series_oneshot(&self, _series_id: &str) -> Result<Option<bool>, String> {
        Ok(None)
    }
}

#[async_trait]
impl EntityExistencePort for NoopMediaReader {
    async fn book_exists(&self, _book_id: &str) -> Result<bool, String> {
        Ok(false)
    }

    async fn series_exists(&self, _series_id: &str) -> Result<bool, String> {
        Ok(false)
    }

    async fn readlist_exists(&self, _readlist_id: &str) -> Result<bool, String> {
        Ok(false)
    }

    async fn collection_exists(&self, _collection_id: &str) -> Result<bool, String> {
        Ok(false)
    }
}

#[async_trait]
impl ContentAccessPort for NoopMediaReader {
    async fn book_restrictions(
        &self,
        _book_id: &str,
    ) -> Result<Option<(Option<u16>, Vec<String>)>, String> {
        Ok(None)
    }

    async fn readlist_archive_entries(
        &self,
        _readlist_id: &str,
    ) -> Result<Vec<(String, PathBuf)>, String> {
        Ok(Vec::new())
    }

    async fn series_archive_entries(
        &self,
        _series_id: &str,
    ) -> Result<Option<(String, String, Vec<(String, PathBuf)>)>, String> {
        Ok(None)
    }

    async fn manifest_book(
        &self,
        _book_id: &str,
    ) -> Result<Option<(String, String, String)>, String> {
        Ok(None)
    }

    async fn readlist_name(&self, _readlist_id: &str) -> Result<Option<String>, String> {
        Ok(None)
    }
}

#[async_trait]
impl ThumbnailReadPort for NoopMediaReader {
    async fn selected_book_thumbnail(
        &self,
        _book_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String> {
        Ok(None)
    }

    async fn book_thumbnail_by_id(
        &self,
        _thumbnail_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String> {
        Ok(None)
    }

    async fn book_thumbnails(&self, _book_id: &str) -> Result<Vec<EntityThumbnailRecord>, String> {
        Ok(Vec::new())
    }

    async fn selected_series_thumbnail(
        &self,
        _series_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String> {
        Ok(None)
    }

    async fn series_thumbnail_by_id(
        &self,
        _thumbnail_id: &str,
    ) -> Result<Option<EntityThumbnailBinary>, String> {
        Ok(None)
    }

    async fn series_thumbnails(
        &self,
        _series_id: &str,
    ) -> Result<Vec<SeriesThumbnailRecord>, String> {
        Ok(Vec::new())
    }

    async fn readlist_thumbnails(
        &self,
        _readlist_id: &str,
    ) -> Result<Vec<ReadlistThumbnailRecord>, String> {
        Ok(Vec::new())
    }

    async fn collection_thumbnails(
        &self,
        _collection_id: &str,
    ) -> Result<Vec<CollectionThumbnailRecord>, String> {
        Ok(Vec::new())
    }
}

#[async_trait]
impl ReadProgressReadPort for NoopMediaReader {
    async fn book_progression(
        &self,
        _book_id: &str,
        _user_id: &str,
    ) -> Result<Option<Value>, String> {
        Ok(self.book_progression.clone())
    }

    async fn book_read_progress_completed(
        &self,
        _book_id: &str,
        _user_id: &str,
    ) -> Result<Option<bool>, String> {
        Ok(None)
    }

    async fn series_tachiyomi_progress(
        &self,
        _series_id: &str,
        _user_id: &str,
    ) -> Result<Option<Value>, String> {
        Ok(None)
    }

    async fn readlist_tachiyomi_counters(
        &self,
        _ordered_book_ids: &[String],
        _user_id: &str,
    ) -> Result<(u64, u64, u64, u64, u64), String> {
        Ok((0, 0, 0, 0, 0))
    }

    async fn book_page_count(&self, _book_id: &str) -> Result<Option<u64>, String> {
        Ok(self.page_count)
    }
}
