use std::sync::Mutex;

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use serde_json::json;

use crate::identity_access::{
    AuthUser, AuthUserRole, KoboLibrarySyncRequest, KoboLibrarySyncService, KoboProxyHeader,
    KoboStoreSyncMergeResult, KoboStoreSyncPort, KoboSyncBookSnapshot, KoboSyncBookState,
    KoboSyncEvent, KoboSyncPage, KoboSyncPageRequest, KoboSyncPointBook,
    KoboSyncReadProgressSnapshot, KoboSyncStatePort, parse_komga_sync_token_payload,
};

#[tokio::test]
async fn library_sync_pipeline_returns_typed_local_events() {
    let state = TestSyncState::new(KoboSyncPage {
        to_sync_point_id: "sync-1".to_string(),
        from_sync_point_id: None,
        books_added: vec![book_sync_point("book-1")],
        books_changed: Vec::new(),
        books_removed: Vec::new(),
        books_read_progress_changed: Vec::new(),
        readlists_added: Vec::new(),
        readlists_changed: Vec::new(),
        readlists_removed: Vec::new(),
        should_continue: true,
    })
    .with_book_state(sample_book_state("book-1", None));
    let store = TestStoreSync::new(KoboStoreSyncMergeResult {
        events: Vec::new(),
        raw_sync_token: None,
        should_continue: false,
    });
    let service = KoboLibrarySyncService::new(&state, &store);

    let response = service
        .sync_library(request(None))
        .await
        .expect("sync should complete");

    assert_eq!(response.events.len(), 1);
    let KoboSyncEvent::NewEntitlement { book, progress } = &response.events[0] else {
        panic!("local added book should produce a typed NewEntitlement event");
    };
    assert_eq!(book.id, "book-1");
    assert_eq!(book.title, "Book One");
    assert!(progress.is_none());
}

#[tokio::test]
async fn library_sync_pipeline_skips_store_proxy_until_local_page_is_final() {
    let state = TestSyncState::new(KoboSyncPage {
        to_sync_point_id: "sync-1".to_string(),
        from_sync_point_id: None,
        books_added: Vec::new(),
        books_changed: Vec::new(),
        books_removed: Vec::new(),
        books_read_progress_changed: Vec::new(),
        readlists_added: Vec::new(),
        readlists_changed: Vec::new(),
        readlists_removed: Vec::new(),
        should_continue: true,
    });
    let store = TestStoreSync::new(KoboStoreSyncMergeResult {
        events: vec![json!({"StoreOnly": true})],
        raw_sync_token: Some("store.next.token".to_string()),
        should_continue: true,
    });
    let service = KoboLibrarySyncService::new(&state, &store);

    let response = service
        .sync_library(request(Some(encoded_komga_sync_token(
            "store.raw.token",
            Some("sync-existing"),
            Some("sync-previous"),
        ))))
        .await
        .expect("sync should complete");

    assert!(response.events.is_empty());
    assert!(response.should_continue);
    assert_eq!(*store.calls.lock().unwrap(), 0);
    assert_eq!(state.requests.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn library_sync_pipeline_merges_store_proxy_after_local_page_is_final() {
    let state = TestSyncState::new(KoboSyncPage {
        to_sync_point_id: "sync-2".to_string(),
        from_sync_point_id: Some("sync-old".to_string()),
        books_added: Vec::new(),
        books_changed: Vec::new(),
        books_removed: Vec::new(),
        books_read_progress_changed: Vec::new(),
        readlists_added: Vec::new(),
        readlists_changed: Vec::new(),
        readlists_removed: Vec::new(),
        should_continue: false,
    });
    let store = TestStoreSync::new(KoboStoreSyncMergeResult {
        events: vec![json!({"StoreOnly": true})],
        raw_sync_token: Some("store.next.token".to_string()),
        should_continue: false,
    });
    let service = KoboLibrarySyncService::new(&state, &store);

    let response = service
        .sync_library(request(Some("store.raw.token".to_string())))
        .await
        .expect("sync should complete");

    assert_eq!(
        response.events,
        vec![KoboSyncEvent::Raw(json!({"StoreOnly": true}))]
    );
    assert!(!response.should_continue);
    assert_eq!(*store.calls.lock().unwrap(), 1);
    assert_eq!(
        state.removed_sync_points.lock().unwrap().as_slice(),
        ["sync-old"]
    );
    let token = parse_komga_sync_token_payload(response.sync_token_payload.as_str())
        .expect("sync token should be valid");
    assert_eq!(token.raw_kobo_sync_token, "store.next.token");
    assert!(token.ongoing_sync_point_id.is_none());
    assert_eq!(
        token.last_successful_sync_point_id.as_deref(),
        Some("sync-2")
    );
}

fn request(sync_token: Option<String>) -> KoboLibrarySyncRequest {
    KoboLibrarySyncRequest {
        user: AuthUser {
            id: "user-1".to_string(),
            email: "user@example.org".to_string(),
            password: String::new(),
            roles: vec![AuthUserRole::KoboSync],
            shared_all_libraries: true,
            shared_library_ids: Vec::new(),
            labels_allow: Vec::new(),
            labels_exclude: Vec::new(),
            age_restriction: None,
        },
        current_api_key_id: Some("api-key-1".to_string()),
        sync_token,
        store_sync_enabled: true,
        forwarded_headers: vec![KoboProxyHeader::new("accept", "application/json")],
        query: Some("limit=50".to_string()),
        limit: 200,
    }
}

fn komga_sync_token_raw(
    raw_kobo_sync_token: &str,
    ongoing_sync_point_id: Option<&str>,
    last_successful_sync_point_id: Option<&str>,
) -> String {
    json!({
        "version": 1,
        "rawKoboSyncToken": raw_kobo_sync_token,
        "ongoingSyncPointId": ongoing_sync_point_id,
        "lastSuccessfulSyncPointId": last_successful_sync_point_id,
    })
    .to_string()
}

fn encoded_komga_sync_token(
    raw_kobo_sync_token: &str,
    ongoing_sync_point_id: Option<&str>,
    last_successful_sync_point_id: Option<&str>,
) -> String {
    format!(
        "KOMGA.{}",
        STANDARD_NO_PAD.encode(komga_sync_token_raw(
            raw_kobo_sync_token,
            ongoing_sync_point_id,
            last_successful_sync_point_id,
        ))
    )
}

struct TestSyncState {
    page: KoboSyncPage,
    book_states: Vec<KoboSyncBookState>,
    requests: Mutex<Vec<KoboSyncPageRequest>>,
    removed_sync_points: Mutex<Vec<String>>,
}

impl TestSyncState {
    fn new(page: KoboSyncPage) -> Self {
        Self {
            page,
            book_states: Vec::new(),
            requests: Mutex::new(Vec::new()),
            removed_sync_points: Mutex::new(Vec::new()),
        }
    }

    fn with_book_state(mut self, state: KoboSyncBookState) -> Self {
        self.book_states.push(state);
        self
    }
}

#[async_trait]
impl KoboSyncStatePort for TestSyncState {
    async fn load_sync_page(&self, request: KoboSyncPageRequest) -> Result<KoboSyncPage, String> {
        self.requests.lock().unwrap().push(request);
        Ok(self.page.clone())
    }

    async fn load_sync_book_states(
        &self,
        _books: &[KoboSyncPointBook],
        _user_id: &str,
    ) -> Result<Vec<KoboSyncBookState>, String> {
        Ok(self.book_states.clone())
    }

    async fn remove_sync_point(&self, sync_point_id: &str) -> Result<(), String> {
        self.removed_sync_points
            .lock()
            .unwrap()
            .push(sync_point_id.to_string());
        Ok(())
    }
}

fn book_sync_point(book_id: &str) -> KoboSyncPointBook {
    KoboSyncPointBook {
        book_id: book_id.to_string(),
        created: "2026-01-01T00:00:00Z".to_string(),
        file_last_modified: "2026-01-02T00:00:00Z".to_string(),
        file_size: 1_024,
        file_hash: format!("hash-{book_id}"),
        metadata_last_modified: "2026-01-03T00:00:00Z".to_string(),
        read_progress_last_modified: None,
        cover_image_id: Some(format!("cover-{book_id}")),
    }
}

fn sample_book_state(
    book_id: &str,
    progress: Option<KoboSyncReadProgressSnapshot>,
) -> KoboSyncBookState {
    KoboSyncBookState {
        book_id: book_id.to_string(),
        book: Some(KoboSyncBookSnapshot {
            id: book_id.to_string(),
            title: "Book One".to_string(),
            summary: "Summary".to_string(),
            release_date: Some("2026-02-03".to_string()),
            language: "en".to_string(),
            file_size: 1_024,
            page_count: 1,
            created: "2026-01-01T00:00:00Z".to_string(),
            last_modified: "2026-01-02T00:00:00Z".to_string(),
            contributor_names: vec!["Jane Writer".to_string()],
            isbn: None,
            publisher_name: None,
            cover_image_id: Some("cover-book-1".to_string()),
            series_id: None,
            series_name: None,
            series_number: None,
            series_number_float: None,
            oneshot: true,
        }),
        progress,
    }
}

struct TestStoreSync {
    response: KoboStoreSyncMergeResult,
    calls: Mutex<usize>,
}

impl TestStoreSync {
    fn new(response: KoboStoreSyncMergeResult) -> Self {
        Self {
            response,
            calls: Mutex::new(0),
        }
    }
}

#[async_trait]
impl KoboStoreSyncPort for TestStoreSync {
    async fn sync_store_library(
        &self,
        _forwarded_headers: &[KoboProxyHeader],
        _query: Option<&str>,
        _raw_sync_token: &str,
    ) -> Result<KoboStoreSyncMergeResult, String> {
        *self.calls.lock().unwrap() += 1;
        Ok(self.response.clone())
    }
}
