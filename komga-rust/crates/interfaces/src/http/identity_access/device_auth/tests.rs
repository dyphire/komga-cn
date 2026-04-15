use super::*;
use axum::extract::{Extension, Path};
use axum::http::{HeaderMap, StatusCode};
use base64::engine::general_purpose::STANDARD;
use komga_application::identity_access::{
    KoboSyncBookSnapshot, KoboSyncReadListSnapshot, KoboSyncReadProgressSnapshot,
    KoboSyncSnapshot, build_kobo_sync_events, generated_kobo_api_token, sanitize_identifier,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path as FsPath;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::KoreaderBookTarget;
use crate::runtime_identity_access::test_support::seed_koreader_book_target;

fn unique_temp_path(prefix: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_millis();
    std::env::temp_dir().join(format!("{prefix}-{millis}.sqlite"))
}

#[test]
fn sanitize_identifier_normalizes_and_replaces_non_alnum() {
    assert_eq!(sanitize_identifier("Ab C_1?"), "ab-c-1-");
}

#[test]
fn generated_kobo_api_token_is_non_hardcoded_and_identity_scoped() {
    let token = generated_kobo_api_token("auth-token-a", "user-a");
    assert_ne!(token, "e30=");
    assert!(token.starts_with("KOMGA."));

    let changed_auth_token = generated_kobo_api_token("auth-token-b", "user-a");
    let changed_user_token = generated_kobo_api_token("auth-token-a", "user-b");
    assert_ne!(token, changed_auth_token);
    assert_ne!(token, changed_user_token);
}

#[tokio::test]
async fn resolved_kobo_user_returns_none_when_not_authenticated() {
    let headers = HeaderMap::new();
    assert!(
        resolved_kobo_user(
            "",
            &headers,
            None,
            FsPath::new("/tmp/komga-kobo-user-none.sqlite")
        )
        .await
        .is_none()
    );
}

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
fn decode_or_passthrough_sync_token_extracts_calibre_web_raw_token() {
    let calibre_payload = json!({
        "data": {
            "raw_kobo_store_token": "store.token.segment"
        }
    })
    .to_string();
    let encoded = STANDARD.encode(calibre_payload.as_bytes());

    let decoded = decode_or_passthrough_sync_token(encoded.as_str());
    assert_eq!(decoded, Some("store.token.segment".to_string()));
}

#[test]
fn decode_or_passthrough_sync_token_keeps_komga_payload_json() {
    let payload = json!({
        "version": 1,
        "rawKoboSyncToken": "store.token.segment",
        "ongoingSyncPointId": "sync-1",
        "lastSuccessfulSyncPointId": null,
    })
    .to_string();
    let encoded = format!("KOMGA.{}", STANDARD_NO_PAD.encode(payload.as_bytes()));

    let decoded = decode_or_passthrough_sync_token(encoded.as_str());
    assert_eq!(decoded, Some(payload));
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

#[test]
fn build_kobo_sync_events_initial_sync_uses_nested_dto_shape() {
    let mut books = HashMap::new();
    books.insert(
        "book-1".to_string(),
        KoboSyncBookSnapshot {
            id: "book-1".to_string(),
            title: "Book One".to_string(),
            summary: String::new(),
            release_date: None,
            language: "EN".to_string(),
            file_size: 123,
            page_count: 10,
            created: "2026-01-01T00:00:00Z".to_string(),
            last_modified: "2026-01-02T00:00:00Z".to_string(),
            contributor_names: vec!["Jane Writer".to_string()],
            isbn: Some("9781234567890".to_string()),
            publisher_name: Some("PubHouse".to_string()),
            cover_image_id: Some("thumb-book-1".to_string()),
            series_id: Some("series-1".to_string()),
            series_name: Some("Series 1".to_string()),
            series_number: Some("1".to_string()),
            series_number_float: Some(1.0),
            oneshot: false,
        },
    );

    let mut progress = HashMap::new();
    progress.insert(
        "book-1".to_string(),
        KoboSyncReadProgressSnapshot {
            page: 4,
            completed: false,
            created: "2026-01-01T00:00:00Z".to_string(),
            last_modified: "2026-01-03T00:00:00Z".to_string(),
            locator: Some(
                json!({
                    "href": "/chapter-1.xhtml",
                    "koboSpan": "kobo.1.1",
                    "locations": {
                        "progression": 0.2,
                        "totalProgression": 0.4,
                    }
                })
                .to_string()
                .into_bytes(),
            ),
        },
    );

    let mut readlists = HashMap::new();
    readlists.insert(
        "list-1".to_string(),
        KoboSyncReadListSnapshot {
            id: "list-1".to_string(),
            name: "On Deck".to_string(),
            created: "2026-01-01T00:00:00Z".to_string(),
            last_modified: "2026-01-03T00:00:00Z".to_string(),
            items: vec!["book-1".to_string()],
        },
    );

    let to = KoboSyncSnapshot {
        books,
        progress,
        readlists,
    };

    let events = build_kobo_sync_events(None, &to, "http://localhost:8080", "token-1");
    assert_eq!(events.len(), 2);

    let entitlement = events[0]
        .get("NewEntitlement")
        .expect("new entitlement expected");
    assert_eq!(
        entitlement
            .get("BookEntitlement")
            .and_then(|value| value.get("Id")),
        Some(&Value::String("book-1".to_string()))
    );
    assert_eq!(
        entitlement
            .get("BookMetadata")
            .and_then(|value| value.get("Description")),
        Some(&Value::String(" ".to_string()))
    );
    assert_eq!(
        entitlement
            .get("BookMetadata")
            .and_then(|value| value.get("Language")),
        Some(&Value::String("en".to_string()))
    );
    assert_eq!(
        entitlement
            .get("BookMetadata")
            .and_then(|value| value.get("CoverImageId")),
        Some(&Value::String("thumb-book-1".to_string()))
    );
    assert_eq!(
        entitlement
            .get("BookMetadata")
            .and_then(|value| value.get("ISBN")),
        Some(&Value::String("9781234567890".to_string()))
    );
    assert_eq!(
        entitlement
            .get("BookMetadata")
            .and_then(|value| value.get("Publisher"))
            .and_then(|value| value.get("Name")),
        Some(&Value::String("PubHouse".to_string()))
    );
    assert_eq!(
        entitlement
            .get("BookMetadata")
            .and_then(|value| value.get("Series"))
            .and_then(|value| value.get("Id")),
        Some(&Value::String("series-1".to_string()))
    );
    assert_eq!(
        entitlement
            .get("BookMetadata")
            .and_then(|value| value.get("Series"))
            .and_then(|value| value.get("NumberFloat")),
        Some(&json!(1.0))
    );
    assert_eq!(
        entitlement
            .get("BookMetadata")
            .and_then(|value| value.get("Contributors")),
        Some(&json!(["Jane Writer"]))
    );
    assert_eq!(
        entitlement
            .get("BookMetadata")
            .and_then(|value| value.get("ContributorRoles"))
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("Name")),
        Some(&Value::String("Jane Writer".to_string()))
    );
    assert_eq!(
        entitlement
            .get("BookMetadata")
            .and_then(|value| value.get("PublicationDate")),
        Some(&Value::String("2026-01-01T00:00:00Z".to_string()))
    );
    assert_eq!(
        entitlement
            .get("BookMetadata")
            .and_then(|value| value.get("DownloadUrls"))
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("Url")),
        Some(&Value::String(
            "http://localhost:8080/kobo/token-1/v1/books/book-1/file/epub".to_string()
        ))
    );
    assert_eq!(
        entitlement
            .get("ReadingState")
            .and_then(|value| value.get("CurrentBookmark"))
            .and_then(|value| value.get("Location"))
            .and_then(|value| value.get("Source")),
        Some(&Value::String("/chapter-1.xhtml".to_string()))
    );

    let tag = events[1].get("NewTag").expect("new tag expected");
    assert_eq!(
        tag.get("Tag")
            .and_then(|value| value.get("Id"))
            .and_then(Value::as_str),
        Some("list-1")
    );
    assert_eq!(
        tag.get("Tag")
            .and_then(|value| value.get("Items"))
            .and_then(Value::as_array)
            .and_then(|items| items.first())
            .and_then(|item| item.get("RevisionId"))
            .and_then(Value::as_str),
        Some("book-1")
    );
}

#[test]
fn build_kobo_sync_events_incremental_sync_emits_changed_and_removed_shapes() {
    let from = KoboSyncSnapshot {
        books: HashMap::from([(
            "book-1".to_string(),
            KoboSyncBookSnapshot {
                id: "book-1".to_string(),
                title: "Old".to_string(),
                summary: String::new(),
                release_date: None,
                language: "en".to_string(),
                file_size: 1,
                page_count: 10,
                created: "2026-01-01T00:00:00Z".to_string(),
                last_modified: "2026-01-01T00:00:00Z".to_string(),
                contributor_names: vec![],
                isbn: None,
                publisher_name: None,
                cover_image_id: None,
                series_id: None,
                series_name: None,
                series_number: None,
                series_number_float: None,
                oneshot: false,
            },
        )]),
        progress: HashMap::new(),
        readlists: HashMap::from([(
            "list-1".to_string(),
            KoboSyncReadListSnapshot {
                id: "list-1".to_string(),
                name: "List One".to_string(),
                created: "2026-01-01T00:00:00Z".to_string(),
                last_modified: "2026-01-01T00:00:00Z".to_string(),
                items: vec!["book-1".to_string()],
            },
        )]),
    };
    let to = KoboSyncSnapshot {
        books: HashMap::from([(
            "book-2".to_string(),
            KoboSyncBookSnapshot {
                id: "book-2".to_string(),
                title: "New".to_string(),
                summary: String::new(),
                release_date: None,
                language: "en".to_string(),
                file_size: 1,
                page_count: 10,
                created: "2026-01-02T00:00:00Z".to_string(),
                last_modified: "2026-01-02T00:00:00Z".to_string(),
                contributor_names: vec![],
                isbn: None,
                publisher_name: None,
                cover_image_id: None,
                series_id: None,
                series_name: None,
                series_number: None,
                series_number_float: None,
                oneshot: false,
            },
        )]),
        progress: HashMap::from([(
            "book-2".to_string(),
            KoboSyncReadProgressSnapshot {
                page: 5,
                completed: false,
                created: "2026-01-02T00:00:00Z".to_string(),
                last_modified: "2026-01-03T00:00:00Z".to_string(),
                locator: None,
            },
        )]),
        readlists: HashMap::new(),
    };

    let events = build_kobo_sync_events(Some(&from), &to, "http://localhost:8080", "token-1");
    assert!(
        events
            .iter()
            .any(|event| event.get("NewEntitlement").is_some())
    );
    assert!(
        events
            .iter()
            .any(|event| event.get("ChangedEntitlement").is_some())
    );
    assert!(
        events
            .iter()
            .any(|event| event.get("ChangedReadingState").is_some())
    );
    assert!(events.iter().any(|event| event.get("DeletedTag").is_some()));

    let removed = events
        .iter()
        .find_map(|event| event.get("ChangedEntitlement"))
        .expect("removed entitlement expected");
    assert_eq!(
        removed
            .get("BookEntitlement")
            .and_then(|value| value.get("IsRemoved")),
        Some(&Value::Bool(true))
    );
}

#[tokio::test]
async fn kobo_ping_rejects_requests_without_valid_auth() {
    let auth_db = crate::http::state::AuthDatabaseState {
        database_file: unique_temp_path("komga-device-auth-ping"),
        demo_mode: false,
        session_runtime_key: "test-session".to_string(),
        remember_me_runtime_key: "test-remember-me".to_string(),
    };
    let response = kobo_ping(
        Extension(auth_db),
        Path("invalid-token".to_string()),
        Extension(RequestConnectionInfo::default()),
        HeaderMap::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn koreader_user_auth_rejects_requests_without_auth() {
    let auth_db = crate::http::state::AuthDatabaseState {
        database_file: unique_temp_path("komga-device-auth-koreader-auth"),
        demo_mode: false,
        session_runtime_key: "test-session".to_string(),
        remember_me_runtime_key: "test-remember-me".to_string(),
    };
    let response = koreader_user_auth(
        Extension(auth_db),
        Extension(RequestConnectionInfo::default()),
        HeaderMap::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn koreader_user_create_returns_forbidden() {
    let response = koreader_user_create(HeaderMap::new()).await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn load_koreader_book_target_returns_unique_book_and_page_count() {
    let database_file = unique_temp_path("komga-device-auth-koreader-unique");
    seed_koreader_book_target(
        database_file.as_path(),
        "hash-unique",
        Ok(Some(KoreaderBookTarget {
            id: "book-1".to_string(),
            page_count: 42,
        })),
    );

    let target = load_koreader_book_target(database_file.as_path(), "hash-unique")
        .await
        .expect("unique hash should not fail")
        .expect("unique hash should resolve a book");
    assert_eq!(target.id, "book-1");
    assert_eq!(target.page_count, 42);

    let _ = fs::remove_file(database_file);
}

#[tokio::test]
async fn load_koreader_book_target_reports_conflict_for_duplicate_hash() {
    let database_file = unique_temp_path("komga-device-auth-koreader-conflict");
    seed_koreader_book_target(
        database_file.as_path(),
        "hash-dup",
        Err(KoreaderBookLookupError::Conflict),
    );

    let result = load_koreader_book_target(database_file.as_path(), "hash-dup").await;
    assert!(matches!(result, Err(KoreaderBookLookupError::Conflict)));

    let _ = fs::remove_file(database_file);
}
