use super::*;

fn assert_bad_request_message(payload: &Value, message: &str, path: &str) {
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Bad Request".to_string()))
    );
    assert_eq!(
        payload.get("message"),
        Some(&Value::String(message.to_string()))
    );
    assert_eq!(payload.get("status"), Some(&Value::from(400)));
    assert_eq!(payload.get("path"), Some(&Value::String(path.to_string())));
    assert!(
        payload.get("timestamp").and_then(Value::as_u64).is_some(),
        "expected numeric timestamp in error payload: {payload:?}"
    );
}

mod collection_detail_create_patch;
mod collection_series;
mod collections_list_search;
mod readlist_patch_and_books_list_filters;
mod readlists_list_search;
mod referential_facets;
mod tachiyomi_progress;
