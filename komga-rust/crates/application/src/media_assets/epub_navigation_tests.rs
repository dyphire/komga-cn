use serde_json::{Value, json};

use super::{
    EpubExtensionBlob, EpubNavigationContentPort, EpubNavigationExtension,
    EpubNavigationExtensionReaderPort, EpubNavigationPosition, EpubNavigationReaderPort,
    load_book_epub_navigation, load_book_epub_positions,
};

#[tokio::test]
async fn epub_navigation_normalizes_locator_and_maps_koreader_fragments() {
    let reader = TestEpubNavigationReader {
        media_files: vec!["/OEBPS/chapter-2.xhtml".to_string()],
        extension_blob: Some(EpubExtensionBlob {
            extension_class: "org.gotson.komga.domain.model.MediaExtensionEpub".to_string(),
            bytes: Vec::new(),
        }),
    };
    let content = TestContentResolver {
        extension: EpubNavigationExtension {
            positions: vec![
                epub_position(json!({
                    "href": "OEBPS/chapter-1.xhtml",
                    "type": "application/xhtml+xml",
                    "locations": {
                        "progression": 0.0,
                        "totalProgression": 0.1,
                        "position": 1
                    }
                })),
                epub_position(json!({
                    "href": "OEBPS/chapter-2.xhtml",
                    "type": "application/xhtml+xml",
                    "koboSpan": "kobo.2.1",
                    "locations": {
                        "progression": 0.5,
                        "totalProgression": 0.42,
                        "position": 2
                    }
                })),
            ],
            is_fixed_layout: false,
            ..EpubNavigationExtension::default()
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
        normalized.raw().pointer("/locations/totalProgression"),
        Some(&json!(0.42))
    );
    assert_eq!(normalized.raw().get("koboSpan"), Some(&json!("kobo.2.1")));

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

#[tokio::test]
async fn epub_navigation_loads_existing_blob_without_extension_class_gate() {
    let reader = TestEpubNavigationReader {
        media_files: Vec::new(),
        extension_blob: Some(EpubExtensionBlob {
            extension_class: "legacy.extension.Class".to_string(),
            bytes: Vec::new(),
        }),
    };
    let content = TestContentResolver {
        extension: EpubNavigationExtension {
            positions: vec![epub_position(json!({
                "href": "chapter-1.xhtml",
                "locations": { "progression": 0.0 }
            }))],
            is_fixed_layout: false,
            ..EpubNavigationExtension::default()
        },
    };

    let navigation = load_book_epub_navigation(&reader, &content, "book-1")
        .await
        .expect("existing EPUB positions blob should be enough to load navigation");

    assert_eq!(navigation.positions().len(), 1);
}

#[tokio::test]
async fn epub_positions_ignore_non_epub_extension_class() {
    let reader = TestEpubNavigationReader {
        media_files: Vec::new(),
        extension_blob: Some(EpubExtensionBlob {
            extension_class: "legacy.extension.Class".to_string(),
            bytes: Vec::new(),
        }),
    };
    let content = TestContentResolver {
        extension: EpubNavigationExtension {
            positions: vec![epub_position(json!({
                "href": "chapter-1.xhtml",
                "locations": { "progression": 0.0 }
            }))],
            is_fixed_layout: false,
            ..EpubNavigationExtension::default()
        },
    };

    let positions = load_book_epub_positions(&reader, &content, "book-1")
        .await
        .expect("non-EPUB extension class should not fail the positions boundary");

    assert_eq!(positions, None);
}

#[tokio::test]
async fn epub_navigation_uses_typed_positions_while_preserving_raw_payload() {
    let raw_position = json!({
        "href": "chapter-1.xhtml",
        "type": "application/xhtml+xml",
        "koboSpan": "span-1",
        "locations": {
            "position": 1,
            "progression": 0.25,
            "totalProgression": 0.5
        }
    });
    let reader = TestEpubNavigationReader {
        media_files: Vec::new(),
        extension_blob: Some(EpubExtensionBlob {
            extension_class: "org.gotson.komga.domain.model.MediaExtensionEpub".to_string(),
            bytes: Vec::new(),
        }),
    };
    let content = TestContentResolver {
        extension: EpubNavigationExtension {
            positions: vec![EpubNavigationPosition::from_raw(raw_position.clone())],
            is_fixed_layout: false,
            ..EpubNavigationExtension::default()
        },
    };

    let navigation = load_book_epub_navigation(&reader, &content, "book-1")
        .await
        .expect("epub navigation should load");
    let position = navigation
        .positions()
        .first()
        .expect("typed position should be exposed");

    assert_eq!(position.href(), Some("chapter-1.xhtml"));
    assert_eq!(position.progression(), Some(0.25));
    assert_eq!(position.total_progression(), Some(0.5));
    assert_eq!(position.raw(), &raw_position);
}

struct TestEpubNavigationReader {
    media_files: Vec<String>,
    extension_blob: Option<EpubExtensionBlob>,
}

#[async_trait::async_trait]
impl EpubNavigationExtensionReaderPort for TestEpubNavigationReader {
    async fn epub_extension_blob(
        &self,
        _book_id: &str,
    ) -> anyhow::Result<Option<EpubExtensionBlob>> {
        Ok(self.extension_blob.clone())
    }
}

#[async_trait::async_trait]
impl EpubNavigationReaderPort for TestEpubNavigationReader {
    async fn book_media_files(&self, _book_id: &str) -> anyhow::Result<Vec<String>> {
        Ok(self.media_files.clone())
    }
}

struct TestContentResolver {
    extension: EpubNavigationExtension,
}

impl EpubNavigationContentPort for TestContentResolver {
    fn decode_epub_navigation_extension(
        &self,
        _blob: &[u8],
    ) -> anyhow::Result<EpubNavigationExtension> {
        Ok(self.extension.clone())
    }
}

fn epub_position(raw: Value) -> EpubNavigationPosition {
    EpubNavigationPosition::from_raw(raw)
}
