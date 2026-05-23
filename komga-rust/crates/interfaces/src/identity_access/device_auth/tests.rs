use super::*;
use axum::http::{HeaderMap, StatusCode};

#[test]
fn parse_koreader_progress_page_supports_numeric_and_ratio_modes_for_non_epub() {
    assert_eq!(parse_koreader_progress_page("7", 10, 0.0), Some(7));
    assert_eq!(parse_koreader_progress_page("chapter_3", 10, 0.0), None);
}

#[test]
fn parse_koreader_epub_resource_index_accepts_only_kotlin_formats() {
    assert_eq!(
        parse_koreader_epub_resource_index("/body/DocFragment[10]/body/div/p[1]/text().0"),
        Some(9)
    );
    assert_eq!(
        parse_koreader_epub_resource_index("#_doc_fragment_44_ c37"),
        Some(44)
    );
    assert_eq!(parse_koreader_epub_resource_index("7"), None);
}

#[test]
fn content_type_from_filename_maps_supported_extensions() {
    assert_eq!(
        content_type_from_filename("volume.cbz", "application/octet-stream"),
        "application/vnd.comicbook+zip"
    );
    assert_eq!(
        content_type_from_filename("volume.cbr", "application/octet-stream"),
        "application/vnd.comicbook-rar"
    );
    assert_eq!(
        content_type_from_filename("book.epub", "application/octet-stream"),
        "application/epub+zip"
    );
    assert_eq!(
        content_type_from_filename("cover.webp", "application/octet-stream"),
        "image/webp"
    );
}

#[test]
fn parse_locator_payload_returns_object_and_handles_invalid_json() {
    let valid = parse_locator_payload(Some(
        br#"{"href":"/chapter-1","locations":{"progression":0.2}}"#,
    ));
    assert_eq!(
        valid.get("href"),
        Some(&Value::String("/chapter-1".to_string()))
    );

    let invalid = parse_locator_payload(Some(br#"{not-json}"#));
    assert_eq!(invalid, json!({}));
}

#[test]
fn kobo_empty_reading_state_payload_uses_ready_defaults() {
    let payload = kobo_empty_reading_state_payload("book-1", "2026-01-01T00:00:00Z");
    assert_eq!(
        payload.get("EntitlementId"),
        Some(&Value::String("book-1".to_string()))
    );
    assert_eq!(
        payload.get("Created"),
        Some(&Value::String("2026-01-01T00:00:00Z".to_string()))
    );
    assert_eq!(
        payload.get("LastModified"),
        Some(&Value::String("2026-01-01T00:00:00Z".to_string()))
    );
    assert_eq!(
        payload
            .get("StatusInfo")
            .and_then(|value| value.get("Status")),
        Some(&Value::String("ReadyToRead".to_string()))
    );
}

#[test]
fn kobo_reading_state_payload_prefers_locator_progress_values() {
    let progress = PersistedReadProgressRecord {
        page: 3,
        completed: false,
        created: "2026-01-01T00:00:00Z".to_string(),
        last_modified: "2026-01-02T00:00:00Z".to_string(),
        device_id: "device-a".to_string(),
        device_name: "KOReader".to_string(),
        locator: None,
    };
    let locator = json!({
        "href": "/chapter-2.xhtml",
        "koboSpan": "span-2",
        "locations": {
            "progression": 0.25,
            "totalProgression": 0.5,
        }
    });

    let payload = kobo_reading_state_payload("book-1", &progress, locator);
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
    assert_eq!(
        payload
            .get("CurrentBookmark")
            .and_then(|value| value.get("Location"))
            .and_then(|value| value.get("Value")),
        Some(&Value::String("span-2".to_string()))
    );
    assert_eq!(
        payload
            .get("StatusInfo")
            .and_then(|value| value.get("Status")),
        Some(&Value::String("Reading".to_string()))
    );
}

#[tokio::test]
async fn kobo_ping_rejects_requests_without_valid_auth() {
    let identity = crate::state::tests::test_identity_state().await;
    let response = kobo_ping_for_tests(
        &identity,
        "invalid-token",
        RequestConnectionInfo::default(),
        HeaderMap::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
