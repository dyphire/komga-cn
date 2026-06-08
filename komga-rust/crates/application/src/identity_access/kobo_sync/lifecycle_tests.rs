use base64::{Engine as _, engine::general_purpose::STANDARD_NO_PAD};
use serde_json::json;

use super::{
    KOBO_SYNC_ITEM_LIMIT, KoboLibrarySyncRequest, KoboSyncPage, parse_komga_sync_token_payload,
};
use crate::identity_access::AuthUser;

use super::lifecycle::KoboSyncLifecycle;

#[test]
fn syncpoint_lifecycle_carries_current_point_while_store_sync_continues() {
    let sync_token = encoded_komga_sync_token(
        "store.raw.token",
        Some("sync-current"),
        Some("sync-previous"),
    );
    let lifecycle = KoboSyncLifecycle::from_sync_token(Some(sync_token.as_str()));

    let page_request = lifecycle.page_request(&request(Some(sync_token)));
    assert_eq!(
        page_request.ongoing_sync_point_id.as_deref(),
        Some("sync-current"),
    );
    assert_eq!(
        page_request.current_api_key_id.as_deref(),
        Some("api-key-1")
    );
    assert_eq!(
        page_request.last_successful_sync_point_id.as_deref(),
        Some("sync-previous"),
    );
    assert_eq!(page_request.limit, KOBO_SYNC_ITEM_LIMIT);

    let page = sync_page("sync-current", Some("sync-previous"), false);
    assert_eq!(lifecycle.sync_point_to_remove(&page, true), None);

    let outgoing = parse_komga_sync_token_payload(
        lifecycle
            .outgoing_sync_token_payload(&page, Some("store.next.token".to_string()), true)
            .as_str(),
    )
    .expect("outgoing sync token should remain a Komga payload");

    assert_eq!(outgoing.raw_kobo_sync_token, "store.next.token");
    assert_eq!(
        outgoing.ongoing_sync_point_id.as_deref(),
        Some("sync-current"),
    );
    assert_eq!(
        outgoing.last_successful_sync_point_id.as_deref(),
        Some("sync-previous"),
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
        forwarded_headers: Vec::new(),
        query: None,
        base_url: "http://localhost:8080".to_string(),
        auth_token: "kobo-token".to_string(),
        limit: KOBO_SYNC_ITEM_LIMIT,
    }
}

fn sync_page(
    to_sync_point_id: &str,
    from_sync_point_id: Option<&str>,
    should_continue: bool,
) -> KoboSyncPage {
    KoboSyncPage {
        to_sync_point_id: to_sync_point_id.to_string(),
        from_sync_point_id: from_sync_point_id.map(str::to_string),
        books_added: Vec::new(),
        books_changed: Vec::new(),
        books_removed: Vec::new(),
        books_read_progress_changed: Vec::new(),
        readlists_added: Vec::new(),
        readlists_changed: Vec::new(),
        readlists_removed: Vec::new(),
        should_continue,
    }
}

fn encoded_komga_sync_token(
    raw_kobo_sync_token: &str,
    ongoing_sync_point_id: Option<&str>,
    last_successful_sync_point_id: Option<&str>,
) -> String {
    let payload = json!({
        "version": 1,
        "rawKoboSyncToken": raw_kobo_sync_token,
        "ongoingSyncPointId": ongoing_sync_point_id,
        "lastSuccessfulSyncPointId": last_successful_sync_point_id,
    })
    .to_string();

    format!("KOMGA.{}", STANDARD_NO_PAD.encode(payload.as_bytes()))
}
