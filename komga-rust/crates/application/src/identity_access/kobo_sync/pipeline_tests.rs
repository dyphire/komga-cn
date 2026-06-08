use std::sync::Mutex;

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use serde_json::json;

use crate::identity_access::{
    AuthUser, KoboLibrarySyncRequest, KoboLibrarySyncService, KoboMetadataRecord,
    KoboStoreSyncMergeResult, KoboStoreSyncPort, KoboSyncPage, KoboSyncPageRequest,
    KoboSyncStatePort, PersistedReadProgressRecord, parse_komga_sync_token_payload,
};

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
    let requests = state.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].current_api_key_id.as_deref(), Some("api-key-1"));
    assert_eq!(
        requests[0].ongoing_sync_point_id.as_deref(),
        Some("sync-existing"),
    );
    assert_eq!(
        requests[0].last_successful_sync_point_id.as_deref(),
        Some("sync-previous"),
    );
    assert_eq!(requests[0].limit, 200);
    let token = parse_komga_sync_token_payload(response.sync_token_payload.as_str())
        .expect("sync token should be valid");
    assert_eq!(token.raw_kobo_sync_token, "store.raw.token");
    assert_eq!(token.ongoing_sync_point_id.as_deref(), Some("sync-1"));
    assert_eq!(
        token.last_successful_sync_point_id.as_deref(),
        Some("sync-previous")
    );
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

    assert_eq!(response.events, vec![json!({"StoreOnly": true})]);
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
            roles: vec!["USER".to_string(), "KOBO_SYNC".to_string()],
            shared_all_libraries: true,
            shared_library_ids: Vec::new(),
            labels_allow: Vec::new(),
            labels_exclude: Vec::new(),
            age_restriction: None,
        },
        current_api_key_id: Some("api-key-1".to_string()),
        sync_token,
        store_sync_enabled: true,
        forwarded_headers: vec![("accept".to_string(), "application/json".to_string())],
        query: Some("limit=50".to_string()),
        base_url: "http://localhost:8080".to_string(),
        auth_token: "kobo-token".to_string(),
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
    requests: Mutex<Vec<KoboSyncPageRequest>>,
    removed_sync_points: Mutex<Vec<String>>,
}

impl TestSyncState {
    fn new(page: KoboSyncPage) -> Self {
        Self {
            page,
            requests: Mutex::new(Vec::new()),
            removed_sync_points: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl KoboSyncStatePort for TestSyncState {
    async fn load_sync_page(&self, request: KoboSyncPageRequest) -> Result<KoboSyncPage, String> {
        self.requests.lock().unwrap().push(request);
        Ok(self.page.clone())
    }

    async fn load_kobo_metadata_record(
        &self,
        _book_id: &str,
    ) -> Result<Option<KoboMetadataRecord>, String> {
        Ok(None)
    }

    async fn load_read_progress(
        &self,
        _book_id: &str,
        _user_id: &str,
    ) -> Result<Option<PersistedReadProgressRecord>, String> {
        Ok(None)
    }

    async fn remove_sync_point(&self, sync_point_id: &str) -> Result<(), String> {
        self.removed_sync_points
            .lock()
            .unwrap()
            .push(sync_point_id.to_string());
        Ok(())
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
        _forwarded_headers: &[(String, String)],
        _query: Option<&str>,
        _raw_sync_token: &str,
    ) -> Result<KoboStoreSyncMergeResult, String> {
        *self.calls.lock().unwrap() += 1;
        Ok(self.response.clone())
    }
}
