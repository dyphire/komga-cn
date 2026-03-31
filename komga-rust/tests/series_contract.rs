use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use tokio::time::{Duration, sleep};
use tower::util::ServiceExt;

#[path = "support/runtime_router_contract_support.rs"]
mod runtime_router_contract_support;

use runtime_router_contract_support::*;

#[test]
fn series_contract_target_is_registered() {
    assert_required_target_declared("series", "series_contract");
}

#[tokio::test]
async fn router_discovery_series_list_supports_deleted_filter_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-deleted").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let not_deleted_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Deleted",
                            "operator": "isFalse"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list deleted=false request should build"),
        )
        .await
        .expect("strict series/list deleted=false request should complete");
    assert_eq!(not_deleted_response.status(), StatusCode::OK);
    let not_deleted_payload = response_json(not_deleted_response).await;
    let not_deleted_content = not_deleted_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series deleted=false payload should expose content array");
    assert_eq!(not_deleted_content.len(), 1);

    let deleted_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Deleted",
                            "operator": "isTrue"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list deleted=true request should build"),
        )
        .await
        .expect("strict series/list deleted=true request should complete");
    assert_eq!(deleted_response.status(), StatusCode::OK);
    let deleted_payload = response_json(deleted_response).await;
    let deleted_content = deleted_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series deleted=true payload should expose content array");
    assert_eq!(deleted_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_oneshot_filter_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-oneshot").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let not_oneshot_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "OneShot",
                            "operator": "isFalse"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list oneshot=false request should build"),
        )
        .await
        .expect("strict series/list oneshot=false request should complete");
    assert_eq!(not_oneshot_response.status(), StatusCode::OK);
    let not_oneshot_payload = response_json(not_oneshot_response).await;
    let not_oneshot_content = not_oneshot_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series oneshot=false payload should expose content array");
    assert_eq!(not_oneshot_content.len(), 1);

    let oneshot_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "OneShot",
                            "operator": "isTrue"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list oneshot=true request should build"),
        )
        .await
        .expect("strict series/list oneshot=true request should complete");
    assert_eq!(oneshot_response.status(), StatusCode::OK);
    let oneshot_payload = response_json(oneshot_response).await;
    let oneshot_content = oneshot_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series oneshot=true payload should expose content array");
    assert_eq!(oneshot_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_series_status_is_and_is_not_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-series-status").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let matched_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "SeriesStatus",
                            "operator": "is",
                            "value": "ONGOING"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list status=is request should build"),
        )
        .await
        .expect("strict series/list status=is request should complete");
    assert_eq!(matched_response.status(), StatusCode::OK);
    let matched_payload = response_json(matched_response).await;
    let matched_content = matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series status=is payload should expose content array");
    assert_eq!(matched_content.len(), 1);

    let excluded_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "SeriesStatus",
                            "operator": "isNot",
                            "value": "ONGOING"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list status=isNot excluded request should build"),
        )
        .await
        .expect("strict series/list status=isNot excluded request should complete");
    assert_eq!(excluded_response.status(), StatusCode::OK);
    let excluded_payload = response_json(excluded_response).await;
    let excluded_content = excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series status=isNot excluded payload should expose content array");
    assert_eq!(excluded_content.len(), 0);

    let kept_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "SeriesStatus",
                            "operator": "isNot",
                            "value": "ENDED"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list status=isNot kept request should build"),
        )
        .await
        .expect("strict series/list status=isNot kept request should complete");
    assert_eq!(kept_response.status(), StatusCode::OK);
    let kept_payload = response_json(kept_response).await;
    let kept_content = kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series status=isNot kept payload should expose content array");
    assert_eq!(kept_content.len(), 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_library_id_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-library-id").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let matched_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "LibraryId",
                            "operator": "is",
                            "value": "library-1"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list library-id match request should build"),
        )
        .await
        .expect("strict series/list library-id match request should complete");
    assert_eq!(matched_response.status(), StatusCode::OK);
    let matched_payload = response_json(matched_response).await;
    let matched_content = matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series library-id match payload should expose content array");
    assert_eq!(matched_content.len(), 1);

    let missing_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "LibraryId",
                            "operator": "is",
                            "value": "library-missing"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list library-id miss request should build"),
        )
        .await
        .expect("strict series/list library-id miss request should complete");
    assert_eq!(missing_response.status(), StatusCode::OK);
    let missing_payload = response_json(missing_response).await;
    let missing_content = missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series library-id miss payload should expose content array");
    assert_eq!(missing_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_applies_default_sort_for_unknown_sort_mode_in_runtime_owned_mode()
 {
    let paths = new_router_fixture("router-discovery-series-list-strict-sort-modes").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for sort in [
        "metadata.titleSort,asc",
        "createdDate,desc",
        "lastModifiedDate,desc",
        "booksMetadata.releaseDate,desc",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/series/list?page=0&size=20&sort={sort}"))
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "condition": {
                                "type": "LibraryId",
                                "operator": "is",
                                "value": "library-1"
                            }
                        })
                        .to_string(),
                    ))
                    .expect("strict series/list supported sort request should build"),
            )
            .await
            .expect("strict series/list supported sort request should complete");
        assert_eq!(response.status(), StatusCode::OK);
    }

    let unsupported_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20&sort=unsupported.sort,asc")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "LibraryId",
                            "operator": "is",
                            "value": "library-1"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list unsupported sort request should build"),
        )
        .await
        .expect("strict series/list unsupported sort request should complete");
    assert_eq!(unsupported_response.status(), StatusCode::OK);
    let unsupported_payload = response_json(unsupported_response).await;
    let unsupported_content = unsupported_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series unsupported sort payload should expose content array");
    assert_eq!(unsupported_content.len(), 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_tag_filter_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-tag").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let matched_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Tag",
                            "operator": "is",
                            "value": "favorite"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list tag match request should build"),
        )
        .await
        .expect("strict series/list tag match request should complete");
    assert_eq!(matched_response.status(), StatusCode::OK);
    let matched_payload = response_json(matched_response).await;
    let matched_content = matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series tag match payload should expose content array");
    assert_eq!(matched_content.len(), 1);

    let missing_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Tag",
                            "operator": "is",
                            "value": "missing-tag"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list tag miss request should build"),
        )
        .await
        .expect("strict series/list tag miss request should complete");
    assert_eq!(missing_response.status(), StatusCode::OK);
    let missing_payload = response_json(missing_response).await;
    let missing_content = missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series tag miss payload should expose content array");
    assert_eq!(missing_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_anyof_and_allof_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-anyof-allof").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let all_of_match_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AllOfSeries",
                            "conditions": [
                                {"type": "LibraryId", "operator": "is", "value": "library-1"},
                                {"type": "SeriesStatus", "operator": "is", "value": "ONGOING"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list allOf match request should build"),
        )
        .await
        .expect("strict series/list allOf match request should complete");
    assert_eq!(all_of_match_response.status(), StatusCode::OK);
    let all_of_match_payload = response_json(all_of_match_response).await;
    let all_of_match_content = all_of_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series allOf match payload should expose content array");
    assert_eq!(all_of_match_content.len(), 1);

    let all_of_miss_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AllOfSeries",
                            "conditions": [
                                {"type": "LibraryId", "operator": "is", "value": "library-1"},
                                {"type": "SeriesStatus", "operator": "is", "value": "ENDED"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list allOf miss request should build"),
        )
        .await
        .expect("strict series/list allOf miss request should complete");
    assert_eq!(all_of_miss_response.status(), StatusCode::OK);
    let all_of_miss_payload = response_json(all_of_miss_response).await;
    let all_of_miss_content = all_of_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series allOf miss payload should expose content array");
    assert_eq!(all_of_miss_content.len(), 0);

    let any_of_match_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AnyOfSeries",
                            "conditions": [
                                {"type": "SeriesStatus", "operator": "is", "value": "ENDED"},
                                {"type": "SeriesStatus", "operator": "is", "value": "ONGOING"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list anyOf match request should build"),
        )
        .await
        .expect("strict series/list anyOf match request should complete");
    assert_eq!(any_of_match_response.status(), StatusCode::OK);
    let any_of_match_payload = response_json(any_of_match_response).await;
    let any_of_match_content = any_of_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series anyOf match payload should expose content array");
    assert_eq!(any_of_match_content.len(), 1);

    let any_of_miss_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AnyOfSeries",
                            "conditions": [
                                {"type": "SeriesStatus", "operator": "is", "value": "ENDED"},
                                {"type": "SeriesStatus", "operator": "is", "value": "ENDED"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list anyOf miss request should build"),
        )
        .await
        .expect("strict series/list anyOf miss request should complete");
    assert_eq!(any_of_miss_response.status(), StatusCode::OK);
    let any_of_miss_payload = response_json(any_of_miss_response).await;
    let any_of_miss_content = any_of_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series anyOf miss payload should expose content array");
    assert_eq!(any_of_miss_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_read_status_is_and_is_not_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-read-status").await;
    seed_router_contract_data(&paths).await;
    seed_router_series_counts(&paths, 1, Some(1)).await;
    seed_router_series_read_progress(&paths, 1, 0).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let matched_read_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReadStatus",
                            "operator": "is",
                            "value": "READ"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list read-status is=READ request should build"),
        )
        .await
        .expect("strict series/list read-status is=READ request should complete");
    assert_eq!(matched_read_response.status(), StatusCode::OK);
    let matched_read_payload = response_json(matched_read_response).await;
    let matched_read_content = matched_read_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series read-status is=READ payload should expose content array");
    assert_eq!(matched_read_content.len(), 1);

    let unmatched_unread_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReadStatus",
                            "operator": "is",
                            "value": "UNREAD"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list read-status is=UNREAD request should build"),
        )
        .await
        .expect("strict series/list read-status is=UNREAD request should complete");
    assert_eq!(unmatched_unread_response.status(), StatusCode::OK);
    let unmatched_unread_payload = response_json(unmatched_unread_response).await;
    let unmatched_unread_content = unmatched_unread_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series read-status is=UNREAD payload should expose content array");
    assert_eq!(unmatched_unread_content.len(), 0);

    let excluded_read_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReadStatus",
                            "operator": "isNot",
                            "value": "READ"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list read-status isNot=READ request should build"),
        )
        .await
        .expect("strict series/list read-status isNot=READ request should complete");
    assert_eq!(excluded_read_response.status(), StatusCode::OK);
    let excluded_read_payload = response_json(excluded_read_response).await;
    let excluded_read_content = excluded_read_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series read-status isNot=READ payload should expose content array");
    assert_eq!(excluded_read_content.len(), 0);

    let kept_not_unread_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReadStatus",
                            "operator": "isNot",
                            "value": "UNREAD"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list read-status isNot=UNREAD request should build"),
        )
        .await
        .expect("strict series/list read-status isNot=UNREAD request should complete");
    assert_eq!(kept_not_unread_response.status(), StatusCode::OK);
    let kept_not_unread_payload = response_json(kept_not_unread_response).await;
    let kept_not_unread_content = kept_not_unread_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series read-status isNot=UNREAD payload should expose content array");
    assert_eq!(kept_not_unread_content.len(), 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_complete_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-complete").await;
    seed_router_contract_data(&paths).await;
    seed_router_series_counts(&paths, 1, Some(1)).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let complete_true_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Complete",
                            "operator": "isTrue"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list complete isTrue request should build"),
        )
        .await
        .expect("strict series/list complete isTrue request should complete");
    assert_eq!(complete_true_response.status(), StatusCode::OK);
    let complete_true_payload = response_json(complete_true_response).await;
    let complete_true_content = complete_true_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series complete isTrue payload should expose content array");
    assert_eq!(complete_true_content.len(), 1);

    let complete_false_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Complete",
                            "operator": "isFalse"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list complete isFalse request should build"),
        )
        .await
        .expect("strict series/list complete isFalse request should complete");
    assert_eq!(complete_false_response.status(), StatusCode::OK);
    let complete_false_payload = response_json(complete_false_response).await;
    let complete_false_content = complete_false_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series complete isFalse payload should expose content array");
    assert_eq!(complete_false_content.len(), 0);

    seed_router_series_counts(&paths, 1, Some(2)).await;

    let incomplete_false_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Complete",
                            "operator": "isFalse"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list complete isFalse (incomplete) request should build"),
        )
        .await
        .expect("strict series/list complete isFalse (incomplete) request should complete");
    assert_eq!(incomplete_false_response.status(), StatusCode::OK);
    let incomplete_false_payload = response_json(incomplete_false_response).await;
    let incomplete_false_content = incomplete_false_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series complete isFalse (incomplete) payload should expose content array");
    assert_eq!(incomplete_false_content.len(), 1);

    seed_router_series_counts(&paths, 1, None).await;

    let null_total_false_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Complete",
                            "operator": "isFalse"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list complete isFalse (null total) request should build"),
        )
        .await
        .expect("strict series/list complete isFalse (null total) request should complete");
    assert_eq!(null_total_false_response.status(), StatusCode::OK);
    let null_total_false_payload = response_json(null_total_false_response).await;
    let null_total_false_content = null_total_false_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series complete isFalse (null total) payload should expose content array");
    assert_eq!(null_total_false_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_release_date_is_and_is_not_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-release-date").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let matched_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "is",
                            "value": "2024-01-15"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date is request should build"),
        )
        .await
        .expect("strict series/list release-date is request should complete");
    assert_eq!(matched_response.status(), StatusCode::OK);
    let matched_payload = response_json(matched_response).await;
    let matched_content = matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date is payload should expose content array");
    assert_eq!(matched_content.len(), 1);

    let excluded_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "isNot",
                            "value": "2024-01-15"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date isNot excluded request should build"),
        )
        .await
        .expect("strict series/list release-date isNot excluded request should complete");
    assert_eq!(excluded_response.status(), StatusCode::OK);
    let excluded_payload = response_json(excluded_response).await;
    let excluded_content = excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date isNot excluded payload should expose content array");
    assert_eq!(excluded_content.len(), 0);

    let kept_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "isNot",
                            "value": "2025-01-15"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date isNot kept request should build"),
        )
        .await
        .expect("strict series/list release-date isNot kept request should complete");
    assert_eq!(kept_response.status(), StatusCode::OK);
    let kept_payload = response_json(kept_response).await;
    let kept_content = kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date isNot kept payload should expose content array");
    assert_eq!(kept_content.len(), 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_release_date_null_operators_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-release-date-null").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let is_null_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "isNull"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date isNull request should build"),
        )
        .await
        .expect("strict series/list release-date isNull request should complete");
    assert_eq!(is_null_response.status(), StatusCode::OK);
    let is_null_payload = response_json(is_null_response).await;
    let is_null_content = is_null_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date isNull payload should expose content array");
    assert_eq!(is_null_content.len(), 0);

    let is_not_null_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "isNotNull"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date isNotNull request should build"),
        )
        .await
        .expect("strict series/list release-date isNotNull request should complete");
    assert_eq!(is_not_null_response.status(), StatusCode::OK);
    let is_not_null_payload = response_json(is_not_null_response).await;
    let is_not_null_content = is_not_null_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date isNotNull payload should expose content array");
    assert_eq!(is_not_null_content.len(), 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_release_date_greater_than_and_less_than_in_runtime_owned_mode()
 {
    let paths = new_router_fixture("router-discovery-series-list-strict-release-date-range").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let gt_matched_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "greaterThan",
                            "value": "2024-01-01"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date greaterThan match request should build"),
        )
        .await
        .expect("strict series/list release-date greaterThan match request should complete");
    assert_eq!(gt_matched_response.status(), StatusCode::OK);
    let gt_matched_payload = response_json(gt_matched_response).await;
    let gt_matched_content = gt_matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date greaterThan match payload should expose content array");
    assert_eq!(gt_matched_content.len(), 1);

    let gt_missing_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "greaterThan",
                            "value": "2024-12-01"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date greaterThan missing request should build"),
        )
        .await
        .expect("strict series/list release-date greaterThan missing request should complete");
    assert_eq!(gt_missing_response.status(), StatusCode::OK);
    let gt_missing_payload = response_json(gt_missing_response).await;
    let gt_missing_content = gt_missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date greaterThan missing payload should expose content array",
        );
    assert_eq!(gt_missing_content.len(), 0);

    let lt_matched_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "lessThan",
                            "value": "2024-12-01"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date lessThan match request should build"),
        )
        .await
        .expect("strict series/list release-date lessThan match request should complete");
    assert_eq!(lt_matched_response.status(), StatusCode::OK);
    let lt_matched_payload = response_json(lt_matched_response).await;
    let lt_matched_content = lt_matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date lessThan match payload should expose content array");
    assert_eq!(lt_matched_content.len(), 1);

    let lt_missing_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "lessThan",
                            "value": "2024-01-01"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date lessThan missing request should build"),
        )
        .await
        .expect("strict series/list release-date lessThan missing request should complete");
    assert_eq!(lt_missing_response.status(), StatusCode::OK);
    let lt_missing_payload = response_json(lt_missing_response).await;
    let lt_missing_content = lt_missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date lessThan missing payload should expose content array");
    assert_eq!(lt_missing_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_release_date_date_style_ops_in_runtime_owned_mode() {
    let paths =
        new_router_fixture("router-discovery-series-list-strict-release-date-date-style").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let after_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "after",
                            "dateTime": "2024-01-01T00:00:00Z"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date after match request should build"),
        )
        .await
        .expect("strict series/list release-date after match request should complete");
    assert_eq!(after_match.status(), StatusCode::OK);
    let after_match_payload = response_json(after_match).await;
    let after_match_content = after_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date after match payload should expose content array");
    assert_eq!(after_match_content.len(), 1);

    let after_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "after",
                            "dateTime": "2024-02-01T00:00:00Z"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date after miss request should build"),
        )
        .await
        .expect("strict series/list release-date after miss request should complete");
    assert_eq!(after_miss.status(), StatusCode::OK);
    let after_miss_payload = response_json(after_miss).await;
    let after_miss_content = after_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date after miss payload should expose content array");
    assert_eq!(after_miss_content.len(), 0);

    let before_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "before",
                            "dateTime": "2024-02-01T00:00:00Z"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date before match request should build"),
        )
        .await
        .expect("strict series/list release-date before match request should complete");
    assert_eq!(before_match.status(), StatusCode::OK);
    let before_match_payload = response_json(before_match).await;
    let before_match_content = before_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date before match payload should expose content array");
    assert_eq!(before_match_content.len(), 1);

    let before_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "before",
                            "dateTime": "2024-01-01T00:00:00Z"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date before miss request should build"),
        )
        .await
        .expect("strict series/list release-date before miss request should complete");
    assert_eq!(before_miss.status(), StatusCode::OK);
    let before_miss_payload = response_json(before_miss).await;
    let before_miss_content = before_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date before miss payload should expose content array");
    assert_eq!(before_miss_content.len(), 0);

    let is_in_the_last_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "isInTheLast",
                            "duration": "P10000D"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date isInTheLast match request should build"),
        )
        .await
        .expect("strict series/list release-date isInTheLast match request should complete");
    assert_eq!(is_in_the_last_match.status(), StatusCode::OK);
    let is_in_the_last_match_payload = response_json(is_in_the_last_match).await;
    let is_in_the_last_match_content = is_in_the_last_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date isInTheLast match payload should expose content array");
    assert_eq!(is_in_the_last_match_content.len(), 1);

    let is_in_the_last_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "isInTheLast",
                            "duration": "P1D"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date isInTheLast miss request should build"),
        )
        .await
        .expect("strict series/list release-date isInTheLast miss request should complete");
    assert_eq!(is_in_the_last_miss.status(), StatusCode::OK);
    let is_in_the_last_miss_payload = response_json(is_in_the_last_miss).await;
    let is_in_the_last_miss_content = is_in_the_last_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date isInTheLast miss payload should expose content array");
    assert_eq!(is_in_the_last_miss_content.len(), 0);

    let is_not_in_the_last_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "isNotInTheLast",
                            "duration": "P1D"
                        }
                    })
                    .to_string(),
                ))
                .expect(
                    "strict series/list release-date isNotInTheLast match request should build",
                ),
        )
        .await
        .expect("strict series/list release-date isNotInTheLast match request should complete");
    assert_eq!(is_not_in_the_last_match.status(), StatusCode::OK);
    let is_not_in_the_last_match_payload = response_json(is_not_in_the_last_match).await;
    let is_not_in_the_last_match_content = is_not_in_the_last_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date isNotInTheLast match payload should expose content array",
        );
    assert_eq!(is_not_in_the_last_match_content.len(), 1);

    let is_not_in_the_last_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "isNotInTheLast",
                            "duration": "P10000D"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date isNotInTheLast miss request should build"),
        )
        .await
        .expect("strict series/list release-date isNotInTheLast miss request should complete");
    assert_eq!(is_not_in_the_last_miss.status(), StatusCode::OK);
    let is_not_in_the_last_miss_payload = response_json(is_not_in_the_last_miss).await;
    let is_not_in_the_last_miss_content = is_not_in_the_last_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date isNotInTheLast miss payload should expose content array",
        );
    assert_eq!(is_not_in_the_last_miss_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_nullable_metadata_operators_with_null_rows_in_runtime_owned_mode()
 {
    let paths =
        new_router_fixture("router-discovery-series-list-strict-nullable-metadata-positive").await;
    seed_router_contract_data(&paths).await;
    seed_router_contract_nullable_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for (condition_type, operator, expected_id) in [
        ("Tag", "is", "series-1"),
        ("Tag", "isNot", "series-2"),
        ("Tag", "isNull", "series-2"),
        ("Tag", "isNotNull", "series-1"),
        ("Genre", "is", "series-1"),
        ("Genre", "isNot", "series-2"),
        ("Genre", "isNull", "series-2"),
        ("Genre", "isNotNull", "series-1"),
        ("SharingLabel", "is", "series-1"),
        ("SharingLabel", "isNot", "series-2"),
        ("SharingLabel", "isNull", "series-2"),
        ("SharingLabel", "isNotNull", "series-1"),
    ] {
        let value = match condition_type {
            "Tag" => "Favorite",
            "Genre" => "SciFi",
            _ => "Family",
        };
        let body = if operator == "is" || operator == "isNot" {
            json!({
                "condition": {
                    "type": condition_type,
                    "operator": operator,
                    "value": value,
                }
            })
            .to_string()
        } else {
            json!({
                "condition": {
                    "type": condition_type,
                    "operator": operator,
                }
            })
            .to_string()
        };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/series/list?page=0&size=20")
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .expect("strict series/list nullable metadata request should build"),
            )
            .await
            .expect("strict series/list nullable metadata request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("strict series nullable metadata payload should expose content array");
        assert_eq!(
            content.len(),
            1,
            "unexpected series nullable metadata count for type={condition_type}, operator={operator}",
        );
        assert_eq!(
            content[0].get("id"),
            Some(&Value::String(expected_id.to_string())),
            "unexpected series nullable metadata id for type={condition_type}, operator={operator}",
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_title_and_title_sort_string_ops_in_runtime_owned_mode()
 {
    let paths = new_router_fixture("router-discovery-series-list-strict-title-string-ops").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for (condition_type, operator, value, expected_count) in [
        ("Title", "is", "series 1", 1_usize),
        ("Title", "is", "series 2", 0_usize),
        ("Title", "isNot", "series 1", 0_usize),
        ("Title", "isNot", "series 2", 1_usize),
        ("Title", "contains", "series", 1_usize),
        ("Title", "contains", "missing", 0_usize),
        ("Title", "doesNotContain", "series", 0_usize),
        ("Title", "doesNotContain", "missing", 1_usize),
        ("Title", "beginsWith", "series", 1_usize),
        ("Title", "beginsWith", "1", 0_usize),
        ("Title", "doesNotBeginWith", "series", 0_usize),
        ("Title", "doesNotBeginWith", "x", 1_usize),
        ("Title", "endsWith", "1", 1_usize),
        ("Title", "endsWith", "series", 0_usize),
        ("Title", "doesNotEndWith", "1", 0_usize),
        ("Title", "doesNotEndWith", "series", 1_usize),
        ("TitleSort", "is", "series 1", 1_usize),
        ("TitleSort", "is", "series 2", 0_usize),
        ("TitleSort", "isNot", "series 1", 0_usize),
        ("TitleSort", "isNot", "series 2", 1_usize),
        ("TitleSort", "contains", "series", 1_usize),
        ("TitleSort", "contains", "missing", 0_usize),
        ("TitleSort", "doesNotContain", "series", 0_usize),
        ("TitleSort", "doesNotContain", "missing", 1_usize),
        ("TitleSort", "beginsWith", "series", 1_usize),
        ("TitleSort", "beginsWith", "1", 0_usize),
        ("TitleSort", "doesNotBeginWith", "series", 0_usize),
        ("TitleSort", "doesNotBeginWith", "x", 1_usize),
        ("TitleSort", "endsWith", "1", 1_usize),
        ("TitleSort", "endsWith", "series", 0_usize),
        ("TitleSort", "doesNotEndWith", "1", 0_usize),
        ("TitleSort", "doesNotEndWith", "series", 1_usize),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/series/list?page=0&size=20")
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "condition": {
                                "type": condition_type,
                                "operator": operator,
                                "value": value,
                            }
                        })
                        .to_string(),
                    ))
                    .expect("strict series/list title matrix request should build"),
            )
            .await
            .expect("strict series/list title matrix request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("strict series title matrix payload should expose content array");
        assert_eq!(
            content.len(),
            expected_count,
            "unexpected series title result for type={condition_type}, operator={operator}, value={value}",
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_release_date_string_ops_in_runtime_owned_mode() {
    let paths =
        new_router_fixture("router-discovery-series-list-strict-release-date-string-ops").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let begins_with_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "beginsWith",
                            "value": "2024-01"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date beginsWith match request should build"),
        )
        .await
        .expect("strict series/list release-date beginsWith match request should complete");
    assert_eq!(begins_with_match.status(), StatusCode::OK);
    let begins_with_match_payload = response_json(begins_with_match).await;
    let begins_with_match_content = begins_with_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date beginsWith match payload should expose content array");
    assert_eq!(begins_with_match_content.len(), 1);

    let begins_with_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "beginsWith",
                            "value": "2025"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date beginsWith miss request should build"),
        )
        .await
        .expect("strict series/list release-date beginsWith miss request should complete");
    assert_eq!(begins_with_miss.status(), StatusCode::OK);
    let begins_with_miss_payload = response_json(begins_with_miss).await;
    let begins_with_miss_content = begins_with_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date beginsWith miss payload should expose content array");
    assert_eq!(begins_with_miss_content.len(), 0);

    let ends_with_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "endsWith",
                            "value": "-15"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date endsWith match request should build"),
        )
        .await
        .expect("strict series/list release-date endsWith match request should complete");
    assert_eq!(ends_with_match.status(), StatusCode::OK);
    let ends_with_match_payload = response_json(ends_with_match).await;
    let ends_with_match_content = ends_with_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date endsWith match payload should expose content array");
    assert_eq!(ends_with_match_content.len(), 1);

    let ends_with_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "endsWith",
                            "value": "-99"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date endsWith miss request should build"),
        )
        .await
        .expect("strict series/list release-date endsWith miss request should complete");
    assert_eq!(ends_with_miss.status(), StatusCode::OK);
    let ends_with_miss_payload = response_json(ends_with_miss).await;
    let ends_with_miss_content = ends_with_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series release-date endsWith miss payload should expose content array");
    assert_eq!(ends_with_miss_content.len(), 0);

    let does_not_contain_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "doesNotContain",
                            "value": "2025"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date doesNotContain keep request should build"),
        )
        .await
        .expect("strict series/list release-date doesNotContain keep request should complete");
    assert_eq!(does_not_contain_match.status(), StatusCode::OK);
    let does_not_contain_match_payload = response_json(does_not_contain_match).await;
    let does_not_contain_match_content = does_not_contain_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date doesNotContain keep payload should expose content array",
        );
    assert_eq!(does_not_contain_match_content.len(), 1);

    let does_not_contain_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "doesNotContain",
                            "value": "2024"
                        }
                    })
                    .to_string(),
                ))
                .expect(
                    "strict series/list release-date doesNotContain excluded request should build",
                ),
        )
        .await
        .expect("strict series/list release-date doesNotContain excluded request should complete");
    assert_eq!(does_not_contain_excluded.status(), StatusCode::OK);
    let does_not_contain_excluded_payload = response_json(does_not_contain_excluded).await;
    let does_not_contain_excluded_content = does_not_contain_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date doesNotContain excluded payload should expose content array",
        );
    assert_eq!(does_not_contain_excluded_content.len(), 0);

    let does_not_begin_with_keep = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "doesNotBeginWith",
                            "value": "2025"
                        }
                    })
                    .to_string(),
                ))
                .expect(
                    "strict series/list release-date doesNotBeginWith keep request should build",
                ),
        )
        .await
        .expect("strict series/list release-date doesNotBeginWith keep request should complete");
    assert_eq!(does_not_begin_with_keep.status(), StatusCode::OK);
    let does_not_begin_with_keep_payload = response_json(does_not_begin_with_keep).await;
    let does_not_begin_with_keep_content = does_not_begin_with_keep_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date doesNotBeginWith keep payload should expose content array",
        );
    assert_eq!(does_not_begin_with_keep_content.len(), 1);

    let does_not_begin_with_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "doesNotBeginWith",
                            "value": "2024"
                        }
                    })
                    .to_string(),
                ))
                .expect(
                    "strict series/list release-date doesNotBeginWith excluded request should build",
                ),
        )
        .await
        .expect(
            "strict series/list release-date doesNotBeginWith excluded request should complete",
        );
    assert_eq!(does_not_begin_with_excluded.status(), StatusCode::OK);
    let does_not_begin_with_excluded_payload = response_json(does_not_begin_with_excluded).await;
    let does_not_begin_with_excluded_content = does_not_begin_with_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date doesNotBeginWith excluded payload should expose content array",
        );
    assert_eq!(does_not_begin_with_excluded_content.len(), 0);

    let does_not_end_with_keep = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "doesNotEndWith",
                            "value": "-99"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list release-date doesNotEndWith keep request should build"),
        )
        .await
        .expect("strict series/list release-date doesNotEndWith keep request should complete");
    assert_eq!(does_not_end_with_keep.status(), StatusCode::OK);
    let does_not_end_with_keep_payload = response_json(does_not_end_with_keep).await;
    let does_not_end_with_keep_content = does_not_end_with_keep_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date doesNotEndWith keep payload should expose content array",
        );
    assert_eq!(does_not_end_with_keep_content.len(), 1);

    let does_not_end_with_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "doesNotEndWith",
                            "value": "-15"
                        }
                    })
                    .to_string(),
                ))
                .expect(
                    "strict series/list release-date doesNotEndWith excluded request should build",
                ),
        )
        .await
        .expect("strict series/list release-date doesNotEndWith excluded request should complete");
    assert_eq!(does_not_end_with_excluded.status(), StatusCode::OK);
    let does_not_end_with_excluded_payload = response_json(does_not_end_with_excluded).await;
    let does_not_end_with_excluded_content = does_not_end_with_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict series release-date doesNotEndWith excluded payload should expose content array",
        );
    assert_eq!(does_not_end_with_excluded_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_detail_uses_persisted_title_sort_value() {
    let paths = new_router_fixture("router-discovery-series-detail-title-sort").await;
    seed_router_contract_data(&paths).await;
    seed_router_series_title_sort(&paths, "series-1", "Series Sort 1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail request should build"),
        )
        .await
        .expect("series detail request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("titleSort")),
        Some(&Value::String("Series Sort 1".to_string())),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_detail_includes_persisted_metadata_and_aggregates() {
    let paths =
        new_router_fixture("router-discovery-series-detail-persisted-metadata-aggregates").await;
    seed_router_contract_data(&paths).await;
    seed_router_series_counts(&paths, 1, Some(5)).await;
    seed_router_series_read_progress(&paths, 1, 0).await;
    seed_router_series_aggregated_tag(&paths, "series-1", "aggregated-tag").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail persisted metadata request should build"),
        )
        .await
        .expect("series detail persisted metadata request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("genres")),
        Some(&json!(["SciFi"])),
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("tags")),
        Some(&json!(["Favorite"])),
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("totalBookCount")),
        Some(&Value::Number(5.into())),
    );
    assert_eq!(
        payload
            .get("booksMetadata")
            .and_then(|metadata| metadata.get("releaseDate")),
        Some(&Value::String("2024-01-15".to_string())),
    );
    assert_eq!(
        payload
            .get("booksMetadata")
            .and_then(|metadata| metadata.get("tags")),
        Some(&json!(["aggregated-tag"])),
    );
    assert_eq!(
        payload.get("booksReadCount"),
        Some(&Value::Number(1.into()))
    );
    assert_eq!(
        payload.get("booksInProgressCount"),
        Some(&Value::Number(0.into()))
    );
    assert_eq!(
        payload.get("booksUnreadCount"),
        Some(&Value::Number(0.into()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_detail_id_bridge_preserves_real_library_id() {
    let paths = new_router_fixture("router-discovery-series-detail-id-bridge-library-id").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "custom-series-2", "Series 2", "library-1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail id-bridge request should build"),
        )
        .await
        .expect("series detail id-bridge request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload.get("id"),
        Some(&Value::String("custom-series-2".to_string())),
    );
    assert_eq!(
        payload.get("libraryId"),
        Some(&Value::String("library-1".to_string())),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_detail_accepts_oneshot_true_with_extra_query_parameters() {
    let paths = new_router_fixture("router-discovery-series-detail-oneshot-query-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1?oneshot=true&extra=ignored")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail oneshot query request should build"),
        )
        .await
        .expect("series detail oneshot query request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("x-komga-runtime-search-ownership")
            .is_none(),
        "accepted oneshot=true detail requests should not be marked persisted-owned",
    );

    let payload = response_json(response).await;
    assert!(
        payload.get("_diagnostics").is_none(),
        "accepted oneshot=true detail requests should not emit unsupported diagnostics",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_detail_ignores_unrelated_query_parameters_without_diagnostics() {
    let paths = new_router_fixture("router-discovery-series-detail-ignores-unrelated-query").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1?foo=bar")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail unrelated-query request should build"),
        )
        .await
        .expect("series detail unrelated-query request should complete");
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        response
            .headers()
            .get("x-komga-runtime-search-ownership")
            .is_none(),
        "unrelated query parameters should not force persisted-owned fallback",
    );

    let payload = response_json(response).await;
    assert!(
        payload.get("_diagnostics").is_none(),
        "unrelated query parameters should not emit unsupported diagnostics",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_metadata_update_refreshes_series_last_modified() {
    let paths = new_router_fixture("router-discovery-series-metadata-refresh").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let before_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail before metadata update request should build"),
        )
        .await
        .expect("series detail before metadata update request should complete");
    assert_eq!(before_response.status(), StatusCode::OK);
    let before_payload = response_json(before_response).await;
    let before_last_modified = before_payload
        .get("lastModified")
        .and_then(Value::as_str)
        .expect("series detail payload should expose lastModified")
        .to_string();

    sleep(Duration::from_millis(1100)).await;

    let patch_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/series/series-1/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "summary": "Updated summary from series contract"
                    })
                    .to_string(),
                ))
                .expect("series metadata patch request should build"),
        )
        .await
        .expect("series metadata patch request should complete");
    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);

    let after_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail after metadata update request should build"),
        )
        .await
        .expect("series detail after metadata update request should complete");
    assert_eq!(after_response.status(), StatusCode::OK);
    let after_payload = response_json(after_response).await;
    let after_last_modified = after_payload
        .get("lastModified")
        .and_then(Value::as_str)
        .expect("series detail payload should expose lastModified after metadata update");
    assert_ne!(after_last_modified, before_last_modified);
    assert_eq!(
        after_payload
            .get("metadata")
            .and_then(|metadata| metadata.get("summary")),
        Some(&Value::String(
            "Updated summary from series contract".to_string()
        )),
    );

    cleanup_router_fixture(paths);
}
