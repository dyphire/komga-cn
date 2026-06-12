use std::path::PathBuf;
use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::{Value, json};

use crate::identity_access::{AuthUser, AuthUserRole};

use super::{
    BookAccessRestrictions, BookMediaRecord, BookProgressionInput, BookProgressionLocator,
    BookProgressionOutcome, BookProgressionReaderPort, BookProgressionRecord,
    BookProgressionService, BookProgressionUpdate, BookProgressionWriterPort, EpubExtensionBlob,
    EpubNavigationContentPort, EpubNavigationExtension, EpubNavigationExtensionReaderPort,
    EpubNavigationPosition, EpubNavigationReaderPort,
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
        epub_extension_blob: Some(EpubExtensionBlob {
            extension_class: "org.gotson.komga.domain.model.MediaExtensionEpub".to_string(),
            bytes: Vec::new(),
        }),
        ..TestProgressionReader::default()
    };
    let content = TestContentResolver {
        positions_extension: EpubNavigationExtension {
            positions: vec![epub_position(json!({
                "href": "chapter-1.xhtml",
                "type": "application/xhtml+xml",
                "koboSpan": "kobo.1.1",
                "locations": {
                    "progression": 0.5,
                    "totalProgression": 0.21,
                    "position": 2
                }
            }))],
            is_fixed_layout: false,
            ..EpubNavigationExtension::default()
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
                locator: Some(BookProgressionLocator::new(
                    json!({
                        "href": "/chapter-1.xhtml#frag",
                        "locations": { "progression": 0.5 }
                    }),
                    Some("/chapter-1.xhtml#frag".to_string()),
                    Some(0.5),
                    None,
                    None,
                )),
            },
        )
        .await;

    assert_eq!(outcome, BookProgressionOutcome::Updated);
    let persisted = progress.persisted.lock().unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].book_id, "book-1");
    assert_eq!(persisted[0].user_id, "admin");
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
        persisted[0]
            .locator
            .as_ref()
            .and_then(|locator| locator.get("koboSpan")),
        Some(&json!("kobo.1.1"))
    );
}

#[tokio::test]
async fn book_progression_update_validates_epub_locator_before_extension_lookup() {
    let reader = TestProgressionReader {
        media: Some(BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "book.epub".to_string(),
            file_path: PathBuf::from("/library/book.epub"),
            media_type: "application/epub+zip".to_string(),
            page_count: 10,
        }),
        ..TestProgressionReader::default()
    };
    let content = TestContentResolver {
        positions_extension: EpubNavigationExtension {
            positions: Vec::new(),
            is_fixed_layout: false,
            ..EpubNavigationExtension::default()
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
                locator: Some(BookProgressionLocator::new(
                    json!({
                        "href": "chapter-1.xhtml",
                        "locations": { "position": 15 }
                    }),
                    Some("chapter-1.xhtml".to_string()),
                    None,
                    Some(15),
                    None,
                )),
            },
        )
        .await;

    assert_eq!(
        outcome,
        BookProgressionOutcome::BadRequest("location.progression is required".to_string())
    );
    assert!(progress.persisted.lock().unwrap().is_empty());
}

#[tokio::test]
async fn book_progression_update_propagates_restriction_load_errors() {
    let reader = TestProgressionReader {
        media: Some(BookMediaRecord {
            library_id: "library-1".to_string(),
            file_name: "book.epub".to_string(),
            file_path: PathBuf::from("/library/book.epub"),
            media_type: "application/epub+zip".to_string(),
            page_count: 10,
        }),
        restriction_error: Some("restriction lookup failed".to_string()),
        ..TestProgressionReader::default()
    };
    let content = TestContentResolver {
        positions_extension: EpubNavigationExtension::default(),
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
                locator: None,
            },
        )
        .await;

    assert_eq!(
        outcome,
        BookProgressionOutcome::Internal("restriction lookup failed".to_string())
    );
    assert!(progress.persisted.lock().unwrap().is_empty());
}

#[derive(Default)]
struct TestProgressionReader {
    media: Option<BookMediaRecord>,
    media_files: Vec<String>,
    epub_extension_blob: Option<EpubExtensionBlob>,
    book_progression: Option<BookProgressionRecord>,
    restriction_error: Option<String>,
}

#[async_trait]
impl BookProgressionReaderPort for TestProgressionReader {
    async fn book_media(&self, _book_id: &str) -> Result<Option<BookMediaRecord>, String> {
        Ok(self.media.clone())
    }

    async fn book_restrictions(
        &self,
        _book_id: &str,
    ) -> Result<Option<BookAccessRestrictions>, String> {
        if let Some(error) = self.restriction_error.clone() {
            return Err(error);
        }
        Ok(None)
    }

    async fn book_progression(
        &self,
        _book_id: &str,
        _user_id: &str,
    ) -> Result<Option<BookProgressionRecord>, String> {
        Ok(self.book_progression.clone())
    }
}

#[async_trait]
impl EpubNavigationExtensionReaderPort for TestProgressionReader {
    async fn epub_extension_blob(
        &self,
        _book_id: &str,
    ) -> Result<Option<EpubExtensionBlob>, String> {
        Ok(self.epub_extension_blob.clone())
    }
}

#[async_trait]
impl EpubNavigationReaderPort for TestProgressionReader {
    async fn book_media_files(&self, _book_id: &str) -> Result<Vec<String>, String> {
        Ok(self.media_files.clone())
    }
}

struct TestContentResolver {
    positions_extension: EpubNavigationExtension,
}

impl EpubNavigationContentPort for TestContentResolver {
    fn decode_epub_navigation_extension(
        &self,
        _blob: &[u8],
    ) -> Result<EpubNavigationExtension, String> {
        Ok(self.positions_extension.clone())
    }
}

#[derive(Default)]
struct TestProgressWriter {
    persisted: Mutex<Vec<BookProgressionInput>>,
}

#[async_trait]
impl BookProgressionWriterPort for TestProgressWriter {
    async fn persist_book_progression(&self, input: BookProgressionInput) -> Result<(), String> {
        self.persisted.lock().unwrap().push(input);
        Ok(())
    }
}

fn admin_user() -> AuthUser {
    AuthUser {
        id: "admin".to_string(),
        email: "admin@example.org".to_string(),
        password: "password".to_string(),
        roles: vec![AuthUserRole::Admin],
        shared_all_libraries: true,
        shared_library_ids: Vec::new(),
        labels_allow: Vec::new(),
        labels_exclude: Vec::new(),
        age_restriction: None,
    }
}

fn epub_position(raw: Value) -> EpubNavigationPosition {
    EpubNavigationPosition::from_raw(raw)
}
