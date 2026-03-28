use std::collections::BTreeMap;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use komga_rust::app::build_router_with_config;
use komga_rust::config::{RuntimeCli, RuntimeConfig};
use komga_rust::persistence::sqlite::connect_pool;
use serde_json::{Value, json};
use sqlx::Row;
use tower::util::ServiceExt;

#[path = "support/persistence_contract_fixture.rs"]
mod persistence_contract_fixture;

#[tokio::test]
async fn router_discovery_books_list_accepts_webui_shape_with_auth_token() {
    let paths = new_router_fixture("router-discovery-books-list").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "seriesId": {
                                "operator": "is",
                                "value": "series-1"
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("books/list request should build"),
        )
        .await
        .expect("books/list request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("books/list payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_rejects_unsupported_operator_in_native_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-operator").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "MediaStatus",
                            "operator": "beginsWith",
                            "value": "READY"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list request should build"),
        )
        .await
        .expect("strict books/list request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    let error = payload
        .get("error")
        .and_then(Value::as_str)
        .expect("strict invalid request should include error message");
    assert!(error.contains("invalid native books request"));
    assert!(error.contains("MediaStatus"));

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_media_status_is_in_native_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-media-status").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "MediaStatus",
                            "operator": "is",
                            "value": "READY"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list media-status request should build"),
        )
        .await
        .expect("strict books/list media-status request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("books/list media-status payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_applies_default_sort_for_unknown_sort_mode_in_native_owned_mode()
 {
    let paths = new_router_fixture("router-discovery-books-list-strict-sort-modes").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for sort in [
        "metadata.title,asc",
        "series,metadata.numberSort,asc",
        "createdDate,desc",
        "lastModifiedDate,desc",
        "metadata.releaseDate,desc",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/books/list?page=0&size=20&sort={sort}"))
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                    .expect("strict books/list supported sort request should build"),
            )
            .await
            .expect("strict books/list supported sort request should complete");
        assert_eq!(response.status(), StatusCode::OK);
    }

    let unsupported_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20&sort=unsupported.sort,asc")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list unsupported sort request should build"),
        )
        .await
        .expect("strict books/list unsupported sort request should complete");
    assert_eq!(unsupported_response.status(), StatusCode::OK);
    let unsupported_payload = response_json(unsupported_response).await;
    let unsupported_content = unsupported_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books unsupported sort fallback payload should expose content array");
    assert_eq!(unsupported_content.len(), 1);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_media_status_is_not_in_native_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-media-status-is-not").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let excluded_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "MediaStatus",
                            "operator": "isNot",
                            "value": "READY"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list media-status isNot excluded request should build"),
        )
        .await
        .expect("strict books/list media-status isNot excluded request should complete");
    assert_eq!(excluded_response.status(), StatusCode::OK);
    let excluded_payload = response_json(excluded_response).await;
    let excluded_content = excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict media-status isNot excluded payload should expose content array");
    assert_eq!(excluded_content.len(), 0);

    let kept_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "MediaStatus",
                            "operator": "isNot",
                            "value": "UNKNOWN"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list media-status isNot kept request should build"),
        )
        .await
        .expect("strict books/list media-status isNot kept request should complete");
    assert_eq!(kept_response.status(), StatusCode::OK);
    let kept_payload = response_json(kept_response).await;
    let kept_content = kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict media-status isNot kept payload should expose content array");
    assert_eq!(kept_content.len(), 1);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_rejects_unknown_condition_type_in_native_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-filter-combo").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Genre",
                            "operator": "is",
                            "value": "Sci-Fi"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list unsupported condition request should build"),
        )
        .await
        .expect("strict books/list unsupported condition request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    let error = payload
        .get("error")
        .and_then(Value::as_str)
        .expect("strict unknown condition payload should expose error");
    assert!(error.contains("invalid native books request"));
    assert!(error.contains("unsupported native books condition type"));

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_series_id_with_query_is_not_silent_empty_in_native_owned_mode()
{
    let paths = new_router_fixture("router-discovery-books-list-strict-seriesid-query").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "SeriesId",
                            "operator": "is",
                            "value": "series-1"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list seriesId request should build"),
        )
        .await
        .expect("strict books/list seriesId request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict seriesId request should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    let excluded_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "SeriesId",
                            "operator": "isNot",
                            "value": "series-1"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list seriesId isNot request should build"),
        )
        .await
        .expect("strict books/list seriesId isNot request should complete");

    assert_eq!(excluded_response.status(), StatusCode::OK);
    let excluded_payload = response_json(excluded_response).await;
    let excluded_content = excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict seriesId isNot request should expose content array");
    assert_eq!(excluded_content.len(), 0);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_anyof_and_allof_in_native_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-anyof-allof").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let all_of_match_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AllOfBook",
                            "conditions": [
                                {"type": "LibraryId", "operator": "is", "value": "library-1"},
                                {"type": "MediaStatus", "operator": "is", "value": "READY"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list allOf match request should build"),
        )
        .await
        .expect("strict books/list allOf match request should complete");
    assert_eq!(all_of_match_response.status(), StatusCode::OK);
    let all_of_match_payload = response_json(all_of_match_response).await;
    let all_of_match_content = all_of_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books allOf match payload should expose content array");
    assert_eq!(all_of_match_content.len(), 1);

    let all_of_miss_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AllOfBook",
                            "conditions": [
                                {"type": "LibraryId", "operator": "is", "value": "library-1"},
                                {"type": "MediaStatus", "operator": "is", "value": "UNKNOWN"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list allOf miss request should build"),
        )
        .await
        .expect("strict books/list allOf miss request should complete");
    assert_eq!(all_of_miss_response.status(), StatusCode::OK);
    let all_of_miss_payload = response_json(all_of_miss_response).await;
    let all_of_miss_content = all_of_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books allOf miss payload should expose content array");
    assert_eq!(all_of_miss_content.len(), 0);

    let any_of_match_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AnyOfBook",
                            "conditions": [
                                {"type": "MediaStatus", "operator": "is", "value": "UNKNOWN"},
                                {"type": "MediaStatus", "operator": "is", "value": "READY"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list anyOf match request should build"),
        )
        .await
        .expect("strict books/list anyOf match request should complete");
    assert_eq!(any_of_match_response.status(), StatusCode::OK);
    let any_of_match_payload = response_json(any_of_match_response).await;
    let any_of_match_content = any_of_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books anyOf match payload should expose content array");
    assert_eq!(any_of_match_content.len(), 1);

    let any_of_miss_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AnyOfBook",
                            "conditions": [
                                {"type": "MediaStatus", "operator": "is", "value": "UNKNOWN"},
                                {"type": "MediaStatus", "operator": "is", "value": "UNKNOWN"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list anyOf miss request should build"),
        )
        .await
        .expect("strict books/list anyOf miss request should complete");
    assert_eq!(any_of_miss_response.status(), StatusCode::OK);
    let any_of_miss_payload = response_json(any_of_miss_response).await;
    let any_of_miss_content = any_of_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books anyOf miss payload should expose content array");
    assert_eq!(any_of_miss_content.len(), 0);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_read_status_is_and_is_not_in_native_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-read-status").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, true).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let unread_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list unread request should build"),
        )
        .await
        .expect("strict books/list unread request should complete");
    assert_eq!(unread_response.status(), StatusCode::OK);
    let unread_payload = response_json(unread_response).await;
    let unread_content = unread_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict unread payload should expose content array");
    assert_eq!(unread_content.len(), 0);

    let read_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list read request should build"),
        )
        .await
        .expect("strict books/list read request should complete");
    assert_eq!(read_response.status(), StatusCode::OK);
    let read_payload = response_json(read_response).await;
    let read_content = read_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict read payload should expose content array");
    assert_eq!(read_content.len(), 1);
    assert_eq!(
        read_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    let excluded_read_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list read isNot excluded request should build"),
        )
        .await
        .expect("strict books/list read isNot excluded request should complete");
    assert_eq!(excluded_read_response.status(), StatusCode::OK);
    let excluded_read_payload = response_json(excluded_read_response).await;
    let excluded_read_content = excluded_read_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict read isNot excluded payload should expose content array");
    assert_eq!(excluded_read_content.len(), 0);

    let kept_not_unread_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list read isNot kept request should build"),
        )
        .await
        .expect("strict books/list read isNot kept request should complete");
    assert_eq!(kept_not_unread_response.status(), StatusCode::OK);
    let kept_not_unread_payload = response_json(kept_not_unread_response).await;
    let kept_not_unread_content = kept_not_unread_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict read isNot kept payload should expose content array");
    assert_eq!(kept_not_unread_content.len(), 1);
    assert_eq!(
        kept_not_unread_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_library_id_in_native_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-library-id").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let matched_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list library-id match request should build"),
        )
        .await
        .expect("strict books/list library-id match request should complete");
    assert_eq!(matched_response.status(), StatusCode::OK);
    let matched_payload = response_json(matched_response).await;
    let matched_content = matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books library-id match payload should expose content array");
    assert_eq!(matched_content.len(), 1);

    let missing_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list library-id miss request should build"),
        )
        .await
        .expect("strict books/list library-id miss request should complete");
    assert_eq!(missing_response.status(), StatusCode::OK);
    let missing_payload = response_json(missing_response).await;
    let missing_content = missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books library-id miss payload should expose content array");
    assert_eq!(missing_content.len(), 0);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_deleted_filter_in_native_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-deleted").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let not_deleted_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list deleted isFalse request should build"),
        )
        .await
        .expect("strict books/list deleted isFalse request should complete");
    assert_eq!(not_deleted_response.status(), StatusCode::OK);
    let not_deleted_payload = response_json(not_deleted_response).await;
    let not_deleted_content = not_deleted_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books deleted isFalse payload should expose content array");
    assert_eq!(not_deleted_content.len(), 1);

    let deleted_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list deleted isTrue request should build"),
        )
        .await
        .expect("strict books/list deleted isTrue request should complete");
    assert_eq!(deleted_response.status(), StatusCode::OK);
    let deleted_payload = response_json(deleted_response).await;
    let deleted_content = deleted_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books deleted isTrue payload should expose content array");
    assert_eq!(deleted_content.len(), 0);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_oneshot_filter_in_native_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-oneshot").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let oneshot_true_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list oneshot=true request should build"),
        )
        .await
        .expect("strict books/list oneshot=true request should complete");
    assert_eq!(oneshot_true_response.status(), StatusCode::OK);
    let oneshot_true_payload = response_json(oneshot_true_response).await;
    let oneshot_true_content = oneshot_true_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict oneshot=true payload should expose content array");
    assert_eq!(oneshot_true_content.len(), 0);

    let oneshot_false_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list oneshot=false request should build"),
        )
        .await
        .expect("strict books/list oneshot=false request should complete");
    assert_eq!(oneshot_false_response.status(), StatusCode::OK);
    let oneshot_false_payload = response_json(oneshot_false_response).await;
    let oneshot_false_content = oneshot_false_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict oneshot=false payload should expose content array");
    assert_eq!(oneshot_false_content.len(), 1);
    assert_eq!(
        oneshot_false_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_release_date_is_in_native_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-release-date").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let matched_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date match request should build"),
        )
        .await
        .expect("strict books/list release-date match request should complete");
    assert_eq!(matched_response.status(), StatusCode::OK);
    let matched_payload = response_json(matched_response).await;
    let matched_content = matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict release-date match payload should expose content array");
    assert_eq!(matched_content.len(), 1);

    let missing_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "is",
                            "value": "2025-01-15"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date missing request should build"),
        )
        .await
        .expect("strict books/list release-date missing request should complete");
    assert_eq!(missing_response.status(), StatusCode::OK);
    let missing_payload = response_json(missing_response).await;
    let missing_content = missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict release-date missing payload should expose content array");
    assert_eq!(missing_content.len(), 0);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_release_date_is_not_in_native_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-release-date-is-not").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let excluded_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date isNot excluded request should build"),
        )
        .await
        .expect("strict books/list release-date isNot excluded request should complete");
    assert_eq!(excluded_response.status(), StatusCode::OK);
    let excluded_payload = response_json(excluded_response).await;
    let excluded_content = excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict release-date isNot excluded payload should expose content array");
    assert_eq!(excluded_content.len(), 0);

    let kept_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date isNot kept request should build"),
        )
        .await
        .expect("strict books/list release-date isNot kept request should complete");
    assert_eq!(kept_response.status(), StatusCode::OK);
    let kept_payload = response_json(kept_response).await;
    let kept_content = kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict release-date isNot kept payload should expose content array");
    assert_eq!(kept_content.len(), 1);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_release_date_null_operators_in_native_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-release-date-null").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let is_null_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date isNull request should build"),
        )
        .await
        .expect("strict books/list release-date isNull request should complete");
    assert_eq!(is_null_response.status(), StatusCode::OK);
    let is_null_payload = response_json(is_null_response).await;
    let is_null_content = is_null_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date isNull payload should expose content array");
    assert_eq!(is_null_content.len(), 0);

    let is_not_null_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date isNotNull request should build"),
        )
        .await
        .expect("strict books/list release-date isNotNull request should complete");
    assert_eq!(is_not_null_response.status(), StatusCode::OK);
    let is_not_null_payload = response_json(is_not_null_response).await;
    let is_not_null_content = is_not_null_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date isNotNull payload should expose content array");
    assert_eq!(is_not_null_content.len(), 1);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_release_date_greater_than_and_less_than_in_native_owned_mode()
 {
    let paths = new_router_fixture("router-discovery-books-list-strict-release-date-range").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let gt_matched_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date greaterThan match request should build"),
        )
        .await
        .expect("strict books/list release-date greaterThan match request should complete");
    assert_eq!(gt_matched_response.status(), StatusCode::OK);
    let gt_matched_payload = response_json(gt_matched_response).await;
    let gt_matched_content = gt_matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date greaterThan match payload should expose content array");
    assert_eq!(gt_matched_content.len(), 1);

    let gt_missing_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date greaterThan missing request should build"),
        )
        .await
        .expect("strict books/list release-date greaterThan missing request should complete");
    assert_eq!(gt_missing_response.status(), StatusCode::OK);
    let gt_missing_payload = response_json(gt_missing_response).await;
    let gt_missing_content = gt_missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date greaterThan missing payload should expose content array",
        );
    assert_eq!(gt_missing_content.len(), 0);

    let lt_matched_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date lessThan match request should build"),
        )
        .await
        .expect("strict books/list release-date lessThan match request should complete");
    assert_eq!(lt_matched_response.status(), StatusCode::OK);
    let lt_matched_payload = response_json(lt_matched_response).await;
    let lt_matched_content = lt_matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date lessThan match payload should expose content array");
    assert_eq!(lt_matched_content.len(), 1);

    let lt_missing_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date lessThan missing request should build"),
        )
        .await
        .expect("strict books/list release-date lessThan missing request should complete");
    assert_eq!(lt_missing_response.status(), StatusCode::OK);
    let lt_missing_payload = response_json(lt_missing_response).await;
    let lt_missing_content = lt_missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date lessThan missing payload should expose content array");
    assert_eq!(lt_missing_content.len(), 0);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_release_date_date_style_ops_in_native_owned_mode() {
    let paths =
        new_router_fixture("router-discovery-books-list-strict-release-date-date-style").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let after_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date after match request should build"),
        )
        .await
        .expect("strict books/list release-date after match request should complete");
    assert_eq!(after_match.status(), StatusCode::OK);
    let after_match_payload = response_json(after_match).await;
    let after_match_content = after_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date after match payload should expose content array");
    assert_eq!(after_match_content.len(), 1);

    let after_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date after miss request should build"),
        )
        .await
        .expect("strict books/list release-date after miss request should complete");
    assert_eq!(after_miss.status(), StatusCode::OK);
    let after_miss_payload = response_json(after_miss).await;
    let after_miss_content = after_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date after miss payload should expose content array");
    assert_eq!(after_miss_content.len(), 0);

    let before_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date before match request should build"),
        )
        .await
        .expect("strict books/list release-date before match request should complete");
    assert_eq!(before_match.status(), StatusCode::OK);
    let before_match_payload = response_json(before_match).await;
    let before_match_content = before_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date before match payload should expose content array");
    assert_eq!(before_match_content.len(), 1);

    let before_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date before miss request should build"),
        )
        .await
        .expect("strict books/list release-date before miss request should complete");
    assert_eq!(before_miss.status(), StatusCode::OK);
    let before_miss_payload = response_json(before_miss).await;
    let before_miss_content = before_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date before miss payload should expose content array");
    assert_eq!(before_miss_content.len(), 0);

    let is_in_the_last_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date isInTheLast match request should build"),
        )
        .await
        .expect("strict books/list release-date isInTheLast match request should complete");
    assert_eq!(is_in_the_last_match.status(), StatusCode::OK);
    let is_in_the_last_match_payload = response_json(is_in_the_last_match).await;
    let is_in_the_last_match_content = is_in_the_last_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date isInTheLast match payload should expose content array");
    assert_eq!(is_in_the_last_match_content.len(), 1);

    let is_in_the_last_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date isInTheLast miss request should build"),
        )
        .await
        .expect("strict books/list release-date isInTheLast miss request should complete");
    assert_eq!(is_in_the_last_miss.status(), StatusCode::OK);
    let is_in_the_last_miss_payload = response_json(is_in_the_last_miss).await;
    let is_in_the_last_miss_content = is_in_the_last_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date isInTheLast miss payload should expose content array");
    assert_eq!(is_in_the_last_miss_content.len(), 0);

    let is_not_in_the_last_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date isNotInTheLast match request should build"),
        )
        .await
        .expect("strict books/list release-date isNotInTheLast match request should complete");
    assert_eq!(is_not_in_the_last_match.status(), StatusCode::OK);
    let is_not_in_the_last_match_payload = response_json(is_not_in_the_last_match).await;
    let is_not_in_the_last_match_content = is_not_in_the_last_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date isNotInTheLast match payload should expose content array",
        );
    assert_eq!(is_not_in_the_last_match_content.len(), 1);

    let is_not_in_the_last_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date isNotInTheLast miss request should build"),
        )
        .await
        .expect("strict books/list release-date isNotInTheLast miss request should complete");
    assert_eq!(is_not_in_the_last_miss.status(), StatusCode::OK);
    let is_not_in_the_last_miss_payload = response_json(is_not_in_the_last_miss).await;
    let is_not_in_the_last_miss_content = is_not_in_the_last_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date isNotInTheLast miss payload should expose content array",
        );
    assert_eq!(is_not_in_the_last_miss_content.len(), 0);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_number_sort_ops_in_native_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-number-sort").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let number_sort_is_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "is", "value": 1.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort is match request should build"),
        )
        .await
        .expect("strict books/list number-sort is match request should complete");
    assert_eq!(number_sort_is_match.status(), StatusCode::OK);
    let number_sort_is_match_payload = response_json(number_sort_is_match).await;
    let number_sort_is_match_content = number_sort_is_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort is match payload should expose content array");
    assert_eq!(number_sort_is_match_content.len(), 1);

    let number_sort_is_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "is", "value": 2.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort is miss request should build"),
        )
        .await
        .expect("strict books/list number-sort is miss request should complete");
    assert_eq!(number_sort_is_miss.status(), StatusCode::OK);
    let number_sort_is_miss_payload = response_json(number_sort_is_miss).await;
    let number_sort_is_miss_content = number_sort_is_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort is miss payload should expose content array");
    assert_eq!(number_sort_is_miss_content.len(), 0);

    let number_sort_is_not_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "isNot", "value": 2.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort isNot match request should build"),
        )
        .await
        .expect("strict books/list number-sort isNot match request should complete");
    assert_eq!(number_sort_is_not_match.status(), StatusCode::OK);
    let number_sort_is_not_match_payload = response_json(number_sort_is_not_match).await;
    let number_sort_is_not_match_content = number_sort_is_not_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort isNot match payload should expose content array");
    assert_eq!(number_sort_is_not_match_content.len(), 1);

    let number_sort_is_not_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "isNot", "value": 1.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort isNot miss request should build"),
        )
        .await
        .expect("strict books/list number-sort isNot miss request should complete");
    assert_eq!(number_sort_is_not_miss.status(), StatusCode::OK);
    let number_sort_is_not_miss_payload = response_json(number_sort_is_not_miss).await;
    let number_sort_is_not_miss_content = number_sort_is_not_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort isNot miss payload should expose content array");
    assert_eq!(number_sort_is_not_miss_content.len(), 0);

    let number_sort_gt_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "greaterThan", "value": 0.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort greaterThan match request should build"),
        )
        .await
        .expect("strict books/list number-sort greaterThan match request should complete");
    assert_eq!(number_sort_gt_match.status(), StatusCode::OK);
    let number_sort_gt_match_payload = response_json(number_sort_gt_match).await;
    let number_sort_gt_match_content = number_sort_gt_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort greaterThan match payload should expose content array");
    assert_eq!(number_sort_gt_match_content.len(), 1);

    let number_sort_gt_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "greaterThan", "value": 1.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort greaterThan miss request should build"),
        )
        .await
        .expect("strict books/list number-sort greaterThan miss request should complete");
    assert_eq!(number_sort_gt_miss.status(), StatusCode::OK);
    let number_sort_gt_miss_payload = response_json(number_sort_gt_miss).await;
    let number_sort_gt_miss_content = number_sort_gt_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort greaterThan miss payload should expose content array");
    assert_eq!(number_sort_gt_miss_content.len(), 0);

    let number_sort_lt_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "lessThan", "value": 2.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort lessThan match request should build"),
        )
        .await
        .expect("strict books/list number-sort lessThan match request should complete");
    assert_eq!(number_sort_lt_match.status(), StatusCode::OK);
    let number_sort_lt_match_payload = response_json(number_sort_lt_match).await;
    let number_sort_lt_match_content = number_sort_lt_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort lessThan match payload should expose content array");
    assert_eq!(number_sort_lt_match_content.len(), 1);

    let number_sort_lt_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "lessThan", "value": 1.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort lessThan miss request should build"),
        )
        .await
        .expect("strict books/list number-sort lessThan miss request should complete");
    assert_eq!(number_sort_lt_miss.status(), StatusCode::OK);
    let number_sort_lt_miss_payload = response_json(number_sort_lt_miss).await;
    let number_sort_lt_miss_content = number_sort_lt_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort lessThan miss payload should expose content array");
    assert_eq!(number_sort_lt_miss_content.len(), 0);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_read_list_id_ops_in_native_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-read-list-id").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let read_list_is_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "ReadListId", "operator": "is", "value": "readlist-1"}})
                        .to_string(),
                ))
                .expect("strict books/list read-list is match request should build"),
        )
        .await
        .expect("strict books/list read-list is match request should complete");
    assert_eq!(read_list_is_match.status(), StatusCode::OK);
    let read_list_is_match_payload = response_json(read_list_is_match).await;
    let read_list_is_match_content = read_list_is_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books read-list is match payload should expose content array");
    assert_eq!(read_list_is_match_content.len(), 1);

    let read_list_is_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "ReadListId", "operator": "is", "value": "missing-readlist"}})
                        .to_string(),
                ))
                .expect("strict books/list read-list is miss request should build"),
        )
        .await
        .expect("strict books/list read-list is miss request should complete");
    assert_eq!(read_list_is_miss.status(), StatusCode::OK);
    let read_list_is_miss_payload = response_json(read_list_is_miss).await;
    let read_list_is_miss_content = read_list_is_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books read-list is miss payload should expose content array");
    assert_eq!(read_list_is_miss_content.len(), 0);

    let read_list_is_not_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "ReadListId", "operator": "isNot", "value": "missing-readlist"}})
                        .to_string(),
                ))
                .expect("strict books/list read-list isNot match request should build"),
        )
        .await
        .expect("strict books/list read-list isNot match request should complete");
    assert_eq!(read_list_is_not_match.status(), StatusCode::OK);
    let read_list_is_not_match_payload = response_json(read_list_is_not_match).await;
    let read_list_is_not_match_content = read_list_is_not_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books read-list isNot match payload should expose content array");
    assert_eq!(read_list_is_not_match_content.len(), 1);

    let read_list_is_not_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "ReadListId", "operator": "isNot", "value": "readlist-1"}})
                        .to_string(),
                ))
                .expect("strict books/list read-list isNot miss request should build"),
        )
        .await
        .expect("strict books/list read-list isNot miss request should complete");
    assert_eq!(read_list_is_not_miss.status(), StatusCode::OK);
    let read_list_is_not_miss_payload = response_json(read_list_is_not_miss).await;
    let read_list_is_not_miss_content = read_list_is_not_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books read-list isNot miss payload should expose content array");
    assert_eq!(read_list_is_not_miss_content.len(), 0);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_tag_author_media_profile_in_native_owned_mode() {
    let paths =
        new_router_fixture("router-discovery-books-list-strict-tag-author-media-profile").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let tag_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Tag", "operator": "is", "value": "favorite-tag"}})
                        .to_string(),
                ))
                .expect("strict books/list tag match request should build"),
        )
        .await
        .expect("strict books/list tag match request should complete");
    assert_eq!(tag_match.status(), StatusCode::OK);
    let tag_match_payload = response_json(tag_match).await;
    let tag_match_content = tag_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books tag match payload should expose content array");
    assert_eq!(tag_match_content.len(), 1);

    let tag_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Tag", "operator": "is", "value": "missing-tag"}})
                        .to_string(),
                ))
                .expect("strict books/list tag miss request should build"),
        )
        .await
        .expect("strict books/list tag miss request should complete");
    assert_eq!(tag_miss.status(), StatusCode::OK);
    let tag_miss_payload = response_json(tag_miss).await;
    let tag_miss_content = tag_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books tag miss payload should expose content array");
    assert_eq!(tag_miss_content.len(), 0);

    let tag_is_not = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Tag", "operator": "isNot", "value": "favorite-tag"}})
                        .to_string(),
                ))
                .expect("strict books/list tag isNot request should build"),
        )
        .await
        .expect("strict books/list tag isNot request should complete");
    assert_eq!(tag_is_not.status(), StatusCode::OK);
    let tag_is_not_payload = response_json(tag_is_not).await;
    let tag_is_not_content = tag_is_not_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books tag isNot payload should expose content array");
    assert_eq!(tag_is_not_content.len(), 0);

    let tag_is_null = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Tag", "operator": "isNull"}}).to_string(),
                ))
                .expect("strict books/list tag isNull request should build"),
        )
        .await
        .expect("strict books/list tag isNull request should complete");
    assert_eq!(tag_is_null.status(), StatusCode::OK);
    let tag_is_null_payload = response_json(tag_is_null).await;
    let tag_is_null_content = tag_is_null_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books tag isNull payload should expose content array");
    assert_eq!(tag_is_null_content.len(), 0);

    let tag_is_not_null = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Tag", "operator": "isNotNull"}}).to_string(),
                ))
                .expect("strict books/list tag isNotNull request should build"),
        )
        .await
        .expect("strict books/list tag isNotNull request should complete");
    assert_eq!(tag_is_not_null.status(), StatusCode::OK);
    let tag_is_not_null_payload = response_json(tag_is_not_null).await;
    let tag_is_not_null_content = tag_is_not_null_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books tag isNotNull payload should expose content array");
    assert_eq!(tag_is_not_null_content.len(), 1);

    let author_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Author", "operator": "contains", "value": "jane"}})
                        .to_string(),
                ))
                .expect("strict books/list author match request should build"),
        )
        .await
        .expect("strict books/list author match request should complete");
    assert_eq!(author_match.status(), StatusCode::OK);
    let author_match_payload = response_json(author_match).await;
    let author_match_content = author_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books author match payload should expose content array");
    assert_eq!(author_match_content.len(), 1);

    let author_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Author", "operator": "contains", "value": "missing"}})
                        .to_string(),
                ))
                .expect("strict books/list author miss request should build"),
        )
        .await
        .expect("strict books/list author miss request should complete");
    assert_eq!(author_miss.status(), StatusCode::OK);
    let author_miss_payload = response_json(author_miss).await;
    let author_miss_content = author_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books author miss payload should expose content array");
    assert_eq!(author_miss_content.len(), 0);

    let author_role_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Author",
                            "operator": "is",
                            "value": {
                                "name": "Jane Writer",
                                "role": "writer"
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list author role match request should build"),
        )
        .await
        .expect("strict books/list author role match request should complete");
    assert_eq!(author_role_match.status(), StatusCode::OK);
    let author_role_match_payload = response_json(author_role_match).await;
    let author_role_match_content = author_role_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books author role match payload should expose content array");
    assert_eq!(author_role_match_content.len(), 1);

    let author_role_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Author",
                            "operator": "is",
                            "value": {
                                "name": "Jane Writer",
                                "role": "editor"
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list author role miss request should build"),
        )
        .await
        .expect("strict books/list author role miss request should complete");
    assert_eq!(author_role_miss.status(), StatusCode::OK);
    let author_role_miss_payload = response_json(author_role_miss).await;
    let author_role_miss_content = author_role_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books author role miss payload should expose content array");
    assert_eq!(author_role_miss_content.len(), 0);

    let poster_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Poster",
                            "operator": "is",
                            "value": {
                                "type": "USER_UPLOADED",
                                "selected": true
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list poster match request should build"),
        )
        .await
        .expect("strict books/list poster match request should complete");
    assert_eq!(poster_match.status(), StatusCode::OK);
    let poster_match_payload = response_json(poster_match).await;
    let poster_match_content = poster_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books poster match payload should expose content array");
    assert_eq!(poster_match_content.len(), 1);

    let poster_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Poster",
                            "operator": "isNot",
                            "value": {
                                "type": "USER_UPLOADED",
                                "selected": true
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list poster excluded request should build"),
        )
        .await
        .expect("strict books/list poster excluded request should complete");
    assert_eq!(poster_excluded.status(), StatusCode::OK);
    let poster_excluded_payload = response_json(poster_excluded).await;
    let poster_excluded_content = poster_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books poster excluded payload should expose content array");
    assert_eq!(poster_excluded_content.len(), 0);

    let media_profile_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "MediaProfile", "operator": "is", "value": "epub"}})
                        .to_string(),
                ))
                .expect("strict books/list media profile match request should build"),
        )
        .await
        .expect("strict books/list media profile match request should complete");
    assert_eq!(media_profile_match.status(), StatusCode::OK);
    let media_profile_match_payload = response_json(media_profile_match).await;
    let media_profile_match_content = media_profile_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books media profile match payload should expose content array");
    assert_eq!(media_profile_match_content.len(), 1);

    let media_profile_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "MediaProfile", "operator": "is", "value": "pdf"}})
                        .to_string(),
                ))
                .expect("strict books/list media profile miss request should build"),
        )
        .await
        .expect("strict books/list media profile miss request should complete");
    assert_eq!(media_profile_miss.status(), StatusCode::OK);
    let media_profile_miss_payload = response_json(media_profile_miss).await;
    let media_profile_miss_content = media_profile_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books media profile miss payload should expose content array");
    assert_eq!(media_profile_miss_content.len(), 0);

    let media_profile_is_not_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "MediaProfile", "operator": "isNot", "value": "epub"}})
                        .to_string(),
                ))
                .expect("strict books/list media profile isNot excluded request should build"),
        )
        .await
        .expect("strict books/list media profile isNot excluded request should complete");
    assert_eq!(media_profile_is_not_excluded.status(), StatusCode::OK);
    let media_profile_is_not_excluded_payload = response_json(media_profile_is_not_excluded).await;
    let media_profile_is_not_excluded_content = media_profile_is_not_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books media profile isNot excluded payload should expose content array");
    assert_eq!(media_profile_is_not_excluded_content.len(), 0);

    let media_profile_is_not_kept = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "MediaProfile", "operator": "isNot", "value": "pdf"}})
                        .to_string(),
                ))
                .expect("strict books/list media profile isNot kept request should build"),
        )
        .await
        .expect("strict books/list media profile isNot kept request should complete");
    assert_eq!(media_profile_is_not_kept.status(), StatusCode::OK);
    let media_profile_is_not_kept_payload = response_json(media_profile_is_not_kept).await;
    let media_profile_is_not_kept_content = media_profile_is_not_kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books media profile isNot kept payload should expose content array");
    assert_eq!(media_profile_is_not_kept_content.len(), 1);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_tag_nullable_operators_with_null_rows_in_native_owned_mode()
 {
    let paths =
        new_router_fixture("router-discovery-books-list-strict-tag-nullable-positive").await;
    seed_router_contract_data(&paths).await;
    seed_router_contract_nullable_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for (operator, expected_id) in [
        ("is", "book-1"),
        ("isNot", "book-2"),
        ("isNull", "book-2"),
        ("isNotNull", "book-1"),
    ] {
        let body = if operator == "is" || operator == "isNot" {
            json!({
                "condition": {
                    "type": "Tag",
                    "operator": operator,
                    "value": "favorite-tag",
                }
            })
            .to_string()
        } else {
            json!({
                "condition": {
                    "type": "Tag",
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
                    .uri("/api/v1/books/list?page=0&size=20")
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-compat-search-ownership", "native-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .expect("strict books/list nullable tag request should build"),
            )
            .await
            .expect("strict books/list nullable tag request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("strict books nullable tag payload should expose content array");
        assert_eq!(
            content.len(),
            1,
            "unexpected books nullable tag count for operator={operator}",
        );
        assert_eq!(
            content[0].get("id"),
            Some(&Value::String(expected_id.to_string())),
            "unexpected books nullable tag id for operator={operator}",
        );
    }

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_title_string_ops_in_native_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-title-string-ops").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for (operator, value, expected_count) in [
        ("is", "book 1", 1_usize),
        ("is", "book 2", 0_usize),
        ("isNot", "book 1", 0_usize),
        ("isNot", "book 2", 1_usize),
        ("contains", "book", 1_usize),
        ("contains", "missing", 0_usize),
        ("doesNotContain", "book", 0_usize),
        ("doesNotContain", "missing", 1_usize),
        ("beginsWith", "book", 1_usize),
        ("beginsWith", "1", 0_usize),
        ("doesNotBeginWith", "book", 0_usize),
        ("doesNotBeginWith", "x", 1_usize),
        ("endsWith", "1", 1_usize),
        ("endsWith", "book", 0_usize),
        ("doesNotEndWith", "1", 0_usize),
        ("doesNotEndWith", "book", 1_usize),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/books/list?page=0&size=20")
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-compat-search-ownership", "native-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "condition": {
                                "type": "Title",
                                "operator": operator,
                                "value": value,
                            }
                        })
                        .to_string(),
                    ))
                    .expect("strict books/list title matrix request should build"),
            )
            .await
            .expect("strict books/list title matrix request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("strict books title matrix payload should expose content array");
        assert_eq!(
            content.len(),
            expected_count,
            "unexpected books title result for operator={operator}, value={value}",
        );
    }

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_release_date_string_ops_in_native_owned_mode() {
    let paths =
        new_router_fixture("router-discovery-books-list-strict-release-date-string-ops").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let begins_with_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date beginsWith match request should build"),
        )
        .await
        .expect("strict books/list release-date beginsWith match request should complete");
    assert_eq!(begins_with_match.status(), StatusCode::OK);
    let begins_with_match_payload = response_json(begins_with_match).await;
    let begins_with_match_content = begins_with_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date beginsWith match payload should expose content array");
    assert_eq!(begins_with_match_content.len(), 1);

    let begins_with_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date beginsWith miss request should build"),
        )
        .await
        .expect("strict books/list release-date beginsWith miss request should complete");
    assert_eq!(begins_with_miss.status(), StatusCode::OK);
    let begins_with_miss_payload = response_json(begins_with_miss).await;
    let begins_with_miss_content = begins_with_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date beginsWith miss payload should expose content array");
    assert_eq!(begins_with_miss_content.len(), 0);

    let ends_with_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date endsWith match request should build"),
        )
        .await
        .expect("strict books/list release-date endsWith match request should complete");
    assert_eq!(ends_with_match.status(), StatusCode::OK);
    let ends_with_match_payload = response_json(ends_with_match).await;
    let ends_with_match_content = ends_with_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date endsWith match payload should expose content array");
    assert_eq!(ends_with_match_content.len(), 1);

    let ends_with_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date endsWith miss request should build"),
        )
        .await
        .expect("strict books/list release-date endsWith miss request should complete");
    assert_eq!(ends_with_miss.status(), StatusCode::OK);
    let ends_with_miss_payload = response_json(ends_with_miss).await;
    let ends_with_miss_content = ends_with_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date endsWith miss payload should expose content array");
    assert_eq!(ends_with_miss_content.len(), 0);

    let does_not_contain_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date doesNotContain keep request should build"),
        )
        .await
        .expect("strict books/list release-date doesNotContain keep request should complete");
    assert_eq!(does_not_contain_match.status(), StatusCode::OK);
    let does_not_contain_match_payload = response_json(does_not_contain_match).await;
    let does_not_contain_match_content = does_not_contain_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotContain keep payload should expose content array",
        );
    assert_eq!(does_not_contain_match_content.len(), 1);

    let does_not_contain_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                    "strict books/list release-date doesNotContain excluded request should build",
                ),
        )
        .await
        .expect("strict books/list release-date doesNotContain excluded request should complete");
    assert_eq!(does_not_contain_excluded.status(), StatusCode::OK);
    let does_not_contain_excluded_payload = response_json(does_not_contain_excluded).await;
    let does_not_contain_excluded_content = does_not_contain_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotContain excluded payload should expose content array",
        );
    assert_eq!(does_not_contain_excluded_content.len(), 0);

    let does_not_begin_with_keep = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                    "strict books/list release-date doesNotBeginWith keep request should build",
                ),
        )
        .await
        .expect("strict books/list release-date doesNotBeginWith keep request should complete");
    assert_eq!(does_not_begin_with_keep.status(), StatusCode::OK);
    let does_not_begin_with_keep_payload = response_json(does_not_begin_with_keep).await;
    let does_not_begin_with_keep_content = does_not_begin_with_keep_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotBeginWith keep payload should expose content array",
        );
    assert_eq!(does_not_begin_with_keep_content.len(), 1);

    let does_not_begin_with_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                    "strict books/list release-date doesNotBeginWith excluded request should build",
                ),
        )
        .await
        .expect("strict books/list release-date doesNotBeginWith excluded request should complete");
    assert_eq!(does_not_begin_with_excluded.status(), StatusCode::OK);
    let does_not_begin_with_excluded_payload = response_json(does_not_begin_with_excluded).await;
    let does_not_begin_with_excluded_content = does_not_begin_with_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotBeginWith excluded payload should expose content array",
        );
    assert_eq!(does_not_begin_with_excluded_content.len(), 0);

    let does_not_end_with_keep = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .expect("strict books/list release-date doesNotEndWith keep request should build"),
        )
        .await
        .expect("strict books/list release-date doesNotEndWith keep request should complete");
    assert_eq!(does_not_end_with_keep.status(), StatusCode::OK);
    let does_not_end_with_keep_payload = response_json(does_not_end_with_keep).await;
    let does_not_end_with_keep_content = does_not_end_with_keep_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotEndWith keep payload should expose content array",
        );
    assert_eq!(does_not_end_with_keep_content.len(), 1);

    let does_not_end_with_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                    "strict books/list release-date doesNotEndWith excluded request should build",
                ),
        )
        .await
        .expect("strict books/list release-date doesNotEndWith excluded request should complete");
    assert_eq!(does_not_end_with_excluded.status(), StatusCode::OK);
    let does_not_end_with_excluded_payload = response_json(does_not_end_with_excluded).await;
    let does_not_end_with_excluded_content = does_not_end_with_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotEndWith excluded payload should expose content array",
        );
    assert_eq!(does_not_end_with_excluded_content.len(), 0);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_deleted_filter_in_native_owned_mode() {
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_oneshot_filter_in_native_owned_mode() {
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_series_status_is_and_is_not_in_native_owned_mode() {
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_library_id_in_native_owned_mode() {
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_applies_default_sort_for_unknown_sort_mode_in_native_owned_mode()
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
                    .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
        .expect("strict series unsupported sort fallback payload should expose content array");
    assert_eq!(unsupported_content.len(), 1);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_rejects_unknown_condition_type_in_native_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-unknown-condition").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "UnknownSeriesFilter",
                            "operator": "is",
                            "value": "x"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list unknown condition request should build"),
        )
        .await
        .expect("strict series/list unknown condition request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    let error = payload
        .get("error")
        .and_then(Value::as_str)
        .expect("strict series unknown condition payload should expose error");
    assert!(error.contains("invalid native series request"));
    assert!(error.contains("unsupported native series condition type"));

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_tag_filter_in_native_owned_mode() {
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_anyof_and_allof_in_native_owned_mode() {
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_read_status_is_and_is_not_in_native_owned_mode() {
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_complete_in_native_owned_mode() {
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_release_date_is_and_is_not_in_native_owned_mode() {
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_release_date_null_operators_in_native_owned_mode() {
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_release_date_greater_than_and_less_than_in_native_owned_mode()
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_release_date_date_style_ops_in_native_owned_mode() {
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_metadata_and_collection_filters_in_native_owned_mode()
 {
    let paths =
        new_router_fixture("router-discovery-series-list-strict-metadata-and-collection").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let genre_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Genre", "operator": "contains", "value": "Sci"}})
                        .to_string(),
                ))
                .expect("strict series/list genre match request should build"),
        )
        .await
        .expect("strict series/list genre match request should complete");
    assert_eq!(genre_match.status(), StatusCode::OK);
    let genre_match_payload = response_json(genre_match).await;
    let genre_match_content = genre_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series genre match payload should expose content array");
    assert_eq!(genre_match_content.len(), 1);

    let genre_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Genre", "operator": "contains", "value": "Drama"}})
                        .to_string(),
                ))
                .expect("strict series/list genre miss request should build"),
        )
        .await
        .expect("strict series/list genre miss request should complete");
    assert_eq!(genre_miss.status(), StatusCode::OK);
    let genre_miss_payload = response_json(genre_miss).await;
    let genre_miss_content = genre_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series genre miss payload should expose content array");
    assert_eq!(genre_miss_content.len(), 0);

    let collection_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "CollectionId", "operator": "is", "value": "collection-1"}})
                        .to_string(),
                ))
                .expect("strict series/list collection match request should build"),
        )
        .await
        .expect("strict series/list collection match request should complete");
    assert_eq!(collection_match.status(), StatusCode::OK);
    let collection_match_payload = response_json(collection_match).await;
    let collection_match_content = collection_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series collection match payload should expose content array");
    assert_eq!(collection_match_content.len(), 1);

    let collection_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "CollectionId", "operator": "is", "value": "collection-missing"}})
                        .to_string(),
                ))
                .expect("strict series/list collection miss request should build"),
        )
        .await
        .expect("strict series/list collection miss request should complete");
    assert_eq!(collection_miss.status(), StatusCode::OK);
    let collection_miss_payload = response_json(collection_miss).await;
    let collection_miss_content = collection_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series collection miss payload should expose content array");
    assert_eq!(collection_miss_content.len(), 0);

    let language_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Language", "operator": "is", "value": "en"}})
                        .to_string(),
                ))
                .expect("strict series/list language match request should build"),
        )
        .await
        .expect("strict series/list language match request should complete");
    assert_eq!(language_match.status(), StatusCode::OK);
    let language_match_payload = response_json(language_match).await;
    let language_match_content = language_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series language match payload should expose content array");
    assert_eq!(language_match_content.len(), 1);

    let language_is_not_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Language", "operator": "isNot", "value": "en"}})
                        .to_string(),
                ))
                .expect("strict series/list language isNot excluded request should build"),
        )
        .await
        .expect("strict series/list language isNot excluded request should complete");
    assert_eq!(language_is_not_excluded.status(), StatusCode::OK);
    let language_is_not_excluded_payload = response_json(language_is_not_excluded).await;
    let language_is_not_excluded_content = language_is_not_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series language isNot excluded payload should expose content array");
    assert_eq!(language_is_not_excluded_content.len(), 0);

    let language_is_not_kept = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Language", "operator": "isNot", "value": "fr"}})
                        .to_string(),
                ))
                .expect("strict series/list language isNot kept request should build"),
        )
        .await
        .expect("strict series/list language isNot kept request should complete");
    assert_eq!(language_is_not_kept.status(), StatusCode::OK);
    let language_is_not_kept_payload = response_json(language_is_not_kept).await;
    let language_is_not_kept_content = language_is_not_kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series language isNot kept payload should expose content array");
    assert_eq!(language_is_not_kept_content.len(), 1);

    let publisher_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Publisher", "operator": "is", "value": "PubHouse"}})
                        .to_string(),
                ))
                .expect("strict series/list publisher match request should build"),
        )
        .await
        .expect("strict series/list publisher match request should complete");
    assert_eq!(publisher_match.status(), StatusCode::OK);
    let publisher_match_payload = response_json(publisher_match).await;
    let publisher_match_content = publisher_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series publisher match payload should expose content array");
    assert_eq!(publisher_match_content.len(), 1);

    let publisher_is_not_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Publisher", "operator": "isNot", "value": "PubHouse"}})
                        .to_string(),
                ))
                .expect("strict series/list publisher isNot excluded request should build"),
        )
        .await
        .expect("strict series/list publisher isNot excluded request should complete");
    assert_eq!(publisher_is_not_excluded.status(), StatusCode::OK);
    let publisher_is_not_excluded_payload = response_json(publisher_is_not_excluded).await;
    let publisher_is_not_excluded_content = publisher_is_not_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series publisher isNot excluded payload should expose content array");
    assert_eq!(publisher_is_not_excluded_content.len(), 0);

    let publisher_is_not_kept = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Publisher", "operator": "isNot", "value": "OtherPub"}})
                        .to_string(),
                ))
                .expect("strict series/list publisher isNot kept request should build"),
        )
        .await
        .expect("strict series/list publisher isNot kept request should complete");
    assert_eq!(publisher_is_not_kept.status(), StatusCode::OK);
    let publisher_is_not_kept_payload = response_json(publisher_is_not_kept).await;
    let publisher_is_not_kept_content = publisher_is_not_kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series publisher isNot kept payload should expose content array");
    assert_eq!(publisher_is_not_kept_content.len(), 1);

    let age_rating_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "AgeRating", "operator": "is", "value": 16}})
                        .to_string(),
                ))
                .expect("strict series/list age-rating match request should build"),
        )
        .await
        .expect("strict series/list age-rating match request should complete");
    assert_eq!(age_rating_match.status(), StatusCode::OK);
    let age_rating_match_payload = response_json(age_rating_match).await;
    let age_rating_match_content = age_rating_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series age-rating match payload should expose content array");
    assert_eq!(age_rating_match_content.len(), 1);

    for (operator, body, expected_count) in [
        (
            "isNot",
            json!({"condition": {"type": "AgeRating", "operator": "isNot", "value": 16}}),
            0_usize,
        ),
        (
            "isNot",
            json!({"condition": {"type": "AgeRating", "operator": "isNot", "value": 18}}),
            1_usize,
        ),
        (
            "greaterThan",
            json!({"condition": {"type": "AgeRating", "operator": "greaterThan", "value": 15}}),
            1_usize,
        ),
        (
            "greaterThan",
            json!({"condition": {"type": "AgeRating", "operator": "greaterThan", "value": 16}}),
            0_usize,
        ),
        (
            "lessThan",
            json!({"condition": {"type": "AgeRating", "operator": "lessThan", "value": 17}}),
            1_usize,
        ),
        (
            "lessThan",
            json!({"condition": {"type": "AgeRating", "operator": "lessThan", "value": 16}}),
            0_usize,
        ),
        (
            "isNull",
            json!({"condition": {"type": "AgeRating", "operator": "isNull"}}),
            0_usize,
        ),
        (
            "isNotNull",
            json!({"condition": {"type": "AgeRating", "operator": "isNotNull"}}),
            1_usize,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/series/list?page=0&size=20")
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-compat-search-ownership", "native-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("strict series/list age-rating operator request should build"),
            )
            .await
            .expect("strict series/list age-rating operator request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("strict series age-rating operator payload should expose content array");
        assert_eq!(
            content.len(),
            expected_count,
            "unexpected strict series age-rating count for operator={operator}",
        );
    }

    let sharing_label_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "SharingLabel", "operator": "contains", "value": "fam"}})
                        .to_string(),
                ))
                .expect("strict series/list sharing-label match request should build"),
        )
        .await
        .expect("strict series/list sharing-label match request should complete");
    assert_eq!(sharing_label_match.status(), StatusCode::OK);
    let sharing_label_match_payload = response_json(sharing_label_match).await;
    let sharing_label_match_content = sharing_label_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series sharing-label match payload should expose content array");
    assert_eq!(sharing_label_match_content.len(), 1);

    for (condition_type, operator, expected_count) in [
        ("Tag", "isNot", 0_usize),
        ("Tag", "isNull", 0_usize),
        ("Tag", "isNotNull", 1_usize),
        ("Genre", "isNot", 0_usize),
        ("Genre", "isNull", 0_usize),
        ("Genre", "isNotNull", 1_usize),
        ("SharingLabel", "isNot", 0_usize),
        ("SharingLabel", "isNull", 0_usize),
        ("SharingLabel", "isNotNull", 1_usize),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/series/list?page=0&size=20")
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-compat-search-ownership", "native-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(if operator == "isNot" {
                        json!({
                            "condition": {
                                "type": condition_type,
                                "operator": operator,
                                "value": match condition_type {
                                    "Tag" => "Favorite",
                                    "Genre" => "SciFi",
                                    _ => "Family",
                                }
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
                    }))
                    .expect("strict series/list nullable string-op request should build"),
            )
            .await
            .expect("strict series/list nullable string-op request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("strict series nullable string-op payload should expose content array");
        assert_eq!(
            content.len(),
            expected_count,
            "unexpected series nullable result for type={condition_type}, operator={operator}",
        );
    }

    let author_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Author", "operator": "contains", "value": "john"}})
                        .to_string(),
                ))
                .expect("strict series/list author match request should build"),
        )
        .await
        .expect("strict series/list author match request should complete");
    assert_eq!(author_match.status(), StatusCode::OK);
    let author_match_payload = response_json(author_match).await;
    let author_match_content = author_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series author match payload should expose content array");
    assert_eq!(author_match_content.len(), 1);

    let author_role_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Author",
                            "operator": "is",
                            "value": {
                                "name": "John Doe",
                                "role": "writer"
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list author role match request should build"),
        )
        .await
        .expect("strict series/list author role match request should complete");
    assert_eq!(author_role_match.status(), StatusCode::OK);
    let author_role_match_payload = response_json(author_role_match).await;
    let author_role_match_content = author_role_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series author role match payload should expose content array");
    assert_eq!(author_role_match_content.len(), 1);

    let author_role_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-compat-search-ownership", "native-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Author",
                            "operator": "is",
                            "value": {
                                "name": "John Doe",
                                "role": "editor"
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list author role miss request should build"),
        )
        .await
        .expect("strict series/list author role miss request should complete");
    assert_eq!(author_role_miss.status(), StatusCode::OK);
    let author_role_miss_payload = response_json(author_role_miss).await;
    let author_role_miss_content = author_role_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series author role miss payload should expose content array");
    assert_eq!(author_role_miss_content.len(), 0);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_nullable_metadata_operators_with_null_rows_in_native_owned_mode()
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
                    .header("x-komga-compat-search-ownership", "native-rust-owned")
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

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_title_and_title_sort_string_ops_in_native_owned_mode()
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
                    .header("x-komga-compat-search-ownership", "native-rust-owned")
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

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_release_date_string_ops_in_native_owned_mode() {
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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
                .header("x-komga-compat-search-ownership", "native-rust-owned")
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

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_kobo_state_update_roundtrip_persists_progress() {
    let paths = new_router_fixture("router-kobo-state-roundtrip").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/kobo/any-token/v1/library/book-1/state")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "ReadingStates": [{
                            "EntitlementId": "book-1",
                            "LastModified": "2026-03-27T10:00:00Z",
                            "Statistics": {
                                "LastModified": "2026-03-27T10:00:00Z"
                            },
                            "StatusInfo": {
                                "LastModified": "2026-03-27T10:00:00Z",
                                "Status": "Reading"
                            },
                            "CurrentBookmark": {
                                "LastModified": "2026-03-27T10:00:00Z",
                                "ProgressPercent": 47.0,
                                "ContentSourceProgressPercent": 23.0,
                                "Location": {
                                    "Source": "/book-1/manifest#position=5",
                                    "Value": "kobo.5.1"
                                }
                            }
                        }]
                    })
                    .to_string(),
                ))
                .expect("kobo state update request should build"),
        )
        .await
        .expect("kobo state update request should complete");
    assert_eq!(put_response.status(), StatusCode::OK);

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/state")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo state get request should build"),
        )
        .await
        .expect("kobo state get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);

    let payload = response_json(get_response).await;
    let state = payload
        .as_array()
        .and_then(|values| values.first())
        .expect("kobo state response should contain one reading state object");
    assert_eq!(
        state
            .get("StatusInfo")
            .and_then(|value| value.get("Status")),
        Some(&Value::String("Reading".to_string())),
    );
    assert_eq!(
        state
            .get("CurrentBookmark")
            .and_then(|value| value.get("ProgressPercent")),
        Some(&json!(47.0)),
    );
    assert_eq!(
        state
            .get("CurrentBookmark")
            .and_then(|value| value.get("ContentSourceProgressPercent")),
        Some(&json!(23.0)),
    );
    assert_eq!(
        state
            .get("CurrentBookmark")
            .and_then(|value| value.get("Location"))
            .and_then(|value| value.get("Source")),
        Some(&Value::String("/book-1/manifest#position=5".to_string())),
    );
    assert_eq!(
        state
            .get("CurrentBookmark")
            .and_then(|value| value.get("Location"))
            .and_then(|value| value.get("Value")),
        Some(&Value::String("kobo.5.1".to_string())),
    );

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_kobo_library_sync_returns_nested_dto_shape_and_sync_token() {
    let paths = new_router_fixture("router-kobo-library-sync-shape").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/sync")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo library sync request should build"),
        )
        .await
        .expect("kobo library sync request should complete");
    assert_eq!(first_response.status(), StatusCode::OK);

    let sync_token_header = first_response
        .headers()
        .get("x-kobo-synctoken")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("kobo sync response should include x-kobo-synctoken header");
    assert!(sync_token_header.starts_with("KOMGA."));

    let first_payload = response_json(first_response).await;
    let first_events = first_payload
        .as_array()
        .expect("kobo sync response should be a JSON array");
    assert!(!first_events.is_empty());

    let entitlement = first_events
        .iter()
        .find_map(|event| event.get("NewEntitlement"))
        .expect("kobo sync payload should contain a NewEntitlement event");
    assert!(entitlement.get("BookEntitlement").is_some());
    assert!(entitlement.get("BookMetadata").is_some());
    assert!(entitlement.get("ReadingState").is_some());

    let second_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/sync")
                .header("x-auth-token", &auth_token)
                .header("x-kobo-synctoken", sync_token_header)
                .body(Body::empty())
                .expect("kobo library sync continuation request should build"),
        )
        .await
        .expect("kobo library sync continuation request should complete");
    assert_eq!(second_response.status(), StatusCode::OK);

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_kobo_initialization_returns_scoped_api_token_header() {
    let paths = new_router_fixture("router-kobo-initialization-api-token").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/initialization")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo initialization request should build"),
        )
        .await
        .expect("kobo initialization request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let api_token = response
        .headers()
        .get("x-kobo-apitoken")
        .and_then(|value| value.to_str().ok())
        .expect("kobo initialization response should include x-kobo-apitoken");
    assert!(api_token.starts_with("KOMGA."));
    assert_ne!(api_token, "e30=");

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_book_file_wildcard_routes_match_api_v1_and_opds_v2() {
    let paths = new_router_fixture("router-book-file-wildcard-routes").await;
    seed_router_contract_data(&paths).await;
    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for file route test");
    let expected_body = b"router-book-file-content";
    std::fs::write(books_dir.join("book-1.epub"), expected_body)
        .expect("book fixture file should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for route in [
        "/api/v1/books/book-1/file/book-1.epub",
        "/opds/v2/books/book-1/file/book-1.epub",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("book file wildcard request should build"),
            )
            .await
            .expect("book file wildcard request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("book file wildcard response body should be readable");
        assert_eq!(body.as_ref(), expected_body);
    }

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_koreader_progress_put_then_get_roundtrip() {
    let paths = new_router_fixture("router-koreader-progress-roundtrip").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/koreader/syncs/progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "document": "hash-book-1",
                        "percentage": 0.33,
                        "progress": "7",
                        "device": "KOReader",
                        "device_id": "reader-1"
                    })
                    .to_string(),
                ))
                .expect("koreader progress put request should build"),
        )
        .await
        .expect("koreader progress put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/koreader/syncs/progress/hash-book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("koreader progress get request should build"),
        )
        .await
        .expect("koreader progress get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);

    let payload = response_json(get_response).await;
    assert_eq!(
        payload.get("document"),
        Some(&Value::String("hash-book-1".to_string())),
    );
    assert_eq!(
        payload.get("progress"),
        Some(&Value::String("7".to_string()))
    );
    assert_eq!(payload.get("percentage"), Some(&json!(0.33)));

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_books_import_enqueues_individual_tasks_in_tasks_db() {
    let paths = new_router_fixture("router-books-import-individual-tasks").await;
    seed_router_contract_data(&paths).await;

    let source_a = paths.config_dir.join("incoming-a.cbz");
    let source_b = paths.config_dir.join("incoming-b.cbz");
    std::fs::write(&source_a, b"import-fixture-a").expect("source_a should be writable");
    std::fs::write(&source_b, b"import-fixture-b").expect("source_b should be writable");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/import")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "copyMode": "COPY",
                        "books": [
                            {
                                "sourceFile": source_a.to_string_lossy(),
                                "seriesId": "series-1"
                            },
                            {
                                "sourceFile": source_b.to_string_lossy(),
                                "seriesId": "series-1"
                            }
                        ]
                    })
                    .to_string(),
                ))
                .expect("books/import request should build"),
        )
        .await
        .expect("books/import request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open");
    let rows = sqlx::query(
        "SELECT SIMPLE_TYPE, GROUP_ID, PAYLOAD \
         FROM TASK \
         WHERE SIMPLE_TYPE = 'IMPORT_BOOK' \
         ORDER BY ID ASC",
    )
    .fetch_all(&tasks_pool)
    .await
    .expect("import task rows should be queryable");
    tasks_pool.close().await;

    assert_eq!(rows.len(), 2);
    for row in rows {
        assert_eq!(row.get::<String, _>("SIMPLE_TYPE"), "IMPORT_BOOK");
        assert_eq!(
            row.get::<Option<String>, _>("GROUP_ID"),
            Some("series-1".to_string())
        );
        let payload = row.get::<String, _>("PAYLOAD");
        let payload = serde_json::from_str::<Value>(&payload)
            .expect("import task payload should be valid JSON");
        assert!(payload.get("book").is_some());
        assert!(payload.get("books").is_none());
    }

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_actuator_root_exposed_by_default_without_beans_link() {
    let paths = new_router_fixture("router-actuator-root-omits-beans-link").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/actuator")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("actuator root request should build"),
        )
        .await
        .expect("actuator root request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let links = payload
        .get("_links")
        .and_then(Value::as_object)
        .expect("actuator root should include links object");
    assert!(links.get("beans").is_none());

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_actuator_shutdown_requires_admin_authentication() {
    let paths = new_router_fixture("router-actuator-shutdown-auth").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/actuator/shutdown")
                .body(Body::empty())
                .expect("actuator shutdown request should build"),
        )
        .await
        .expect("actuator shutdown request should complete");

    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("actuator shutdown response body should be readable");
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "unexpected actuator shutdown status={status}, body={}",
        String::from_utf8_lossy(&body),
    );

    persistence_contract_fixture::cleanup(paths);
}

async fn new_router_fixture(case_id: &str) -> persistence_contract_fixture::LegacyDbPaths {
    let paths = persistence_contract_fixture::new_legacy_db_paths(case_id)
        .expect("router contract fixture paths should be created");
    persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
        .await
        .expect("main db flyway fixture should be created");
    persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
        .await
        .expect("tasks db flyway fixture should be created");
    paths
}

fn runtime_config_for_paths(paths: &persistence_contract_fixture::LegacyDbPaths) -> RuntimeConfig {
    let mut env = BTreeMap::new();
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        paths.config_dir.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_DATABASE_FILE".to_string(),
        paths.main_db.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_TASKS_DB_FILE".to_string(),
        paths.tasks_db.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_RUST_COMPAT_PROFILE".to_string(),
        "snapshot-aligned".to_string(),
    );

    RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &env)
        .expect("runtime config should resolve fixture paths")
}

async fn seed_router_contract_data(paths: &persistence_contract_fixture::LegacyDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open");

    for sql in [
        "ALTER TABLE READ_PROGRESS ADD COLUMN DEVICE_ID varchar NOT NULL DEFAULT ''",
        "ALTER TABLE READ_PROGRESS ADD COLUMN DEVICE_NAME varchar NOT NULL DEFAULT ''",
        "ALTER TABLE READ_PROGRESS ADD COLUMN LOCATOR blob",
    ] {
        let _ = sqlx::query(sql).execute(&pool).await;
    }

    sqlx::query(
        "INSERT INTO LIBRARY (ID, NAME, ROOT) \
                 VALUES (?, ?, ?)",
    )
    .bind("library-1")
    .bind("Library 1")
    .bind(paths.config_dir.to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("library row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-1")
    .bind(0_i64)
    .bind("Series 1")
    .bind("series/series-1")
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, \
           SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series 1")
    .bind("Series 1")
    .bind("PubHouse")
    .bind("EN")
    .bind(16_i64)
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) \
                 VALUES (?, ?)",
    )
    .bind("series-1")
    .bind("SciFi")
    .execute(&pool)
    .await
    .expect("series metadata genre row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA_TAG (SERIES_ID, TAG) \
                 VALUES (?, ?)",
    )
    .bind("series-1")
    .bind("Favorite")
    .execute(&pool)
    .await
    .expect("series metadata tag row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA_SHARING (SERIES_ID, LABEL) \
                 VALUES (?, ?)",
    )
    .bind("series-1")
    .bind("Family")
    .execute(&pool)
    .await
    .expect("series metadata sharing row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION_AUTHOR (SERIES_ID, NAME, ROLE) \
         VALUES (?, ?, ?)",
    )
    .bind("series-1")
    .bind("John Doe")
    .bind("writer")
    .execute(&pool)
    .await
    .expect("book metadata aggregation author row should be inserted");

    sqlx::query(
        "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("collection-1")
    .bind("Collection 1")
    .bind(false)
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("collection row should be inserted");

    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) \
         VALUES (?, ?, ?)",
    )
    .bind("collection-1")
    .bind("series-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("collection series row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, \
           LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind(0_i64)
    .bind("book-1.epub")
    .bind("books/book-1.epub")
    .bind("series-1")
    .bind(1_024_i64)
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book row should be inserted");

    sqlx::query(
        "UPDATE BOOK \
                 SET FILE_HASH_KOREADER = ? \
                 WHERE ID = ?",
    )
    .bind("hash-book-1")
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("book koreader hash should be set");

    sqlx::query(
        "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("application/epub+zip")
    .bind("READY")
    .bind("book-1")
    .bind(10_i64)
    .execute(&pool)
    .await
    .expect("media row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("1")
    .bind(1.0_f64)
    .bind("Book 1")
    .bind("2024-01-15")
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("book metadata row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) \
                 VALUES (?, ?)",
    )
    .bind("book-1")
    .bind("favorite-tag")
    .execute(&pool)
    .await
    .expect("book metadata tag row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) \
                 VALUES (?, ?, ?)",
    )
    .bind("book-1")
    .bind("Jane Writer")
    .bind("writer")
    .execute(&pool)
    .await
    .expect("book metadata author row should be inserted");

    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, SELECTED) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("thumb-book-1")
    .bind("book-1")
    .bind("USER_UPLOADED")
    .bind(true)
    .execute(&pool)
    .await
    .expect("book thumbnail row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION (RELEASE_DATE, SUMMARY, SUMMARY_NUMBER, SERIES_ID) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("2024-01-15")
    .bind("")
    .bind("")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("book metadata aggregation row should be inserted");

    sqlx::query(
        "INSERT INTO READLIST (ID, NAME, BOOK_COUNT) \
                 VALUES (?, ?, ?)",
    )
    .bind("readlist-1")
    .bind("ReadList 1")
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("readlist row should be inserted");

    sqlx::query(
        "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) \
                 VALUES (?, ?, ?)",
    )
    .bind("readlist-1")
    .bind("book-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("readlist book row should be inserted");

    let hashed_password = hash_bcrypt_password("router-contract-admin-123", DEFAULT_COST)
        .expect("bcrypt hash should be computed");
    sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("admin-user")
    .bind("admin@example.org")
    .bind(hashed_password)
    .bind(true)
    .execute(&pool)
    .await
    .expect("admin user should be inserted");

    for role in ["USER", "ADMIN", "FILE_DOWNLOAD", "PAGE_STREAMING"] {
        sqlx::query(
            "INSERT INTO USER_ROLE (USER_ID, ROLE) \
                     VALUES (?, ?)",
        )
        .bind("admin-user")
        .bind(role)
        .execute(&pool)
        .await
        .expect("admin role should be inserted");
    }

    pool.close().await;
}

#[tokio::test]
async fn router_opds_v1_catalog_route_returns_atom_feed() {
    let paths = new_router_fixture("router-opds-v1-catalog-route").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/catalog")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 catalog request should build"),
        )
        .await
        .expect("opds v1 catalog request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(content_type.contains("application/atom+xml"));

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_opds_v2_library_browse_route_returns_navigation_feed() {
    let paths = new_router_fixture("router-opds-v2-library-browse-route").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-1/browse")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 browse request should build"),
        )
        .await
        .expect("opds v2 browse request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert!(
        payload
            .get("navigation")
            .and_then(Value::as_array)
            .is_some_and(|entries| !entries.is_empty())
    );

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_opds_v2_catalog_returns_feed_when_authenticated() {
    let paths = new_router_fixture("router-opds-v2-catalog-authenticated").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/catalog")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 catalog request should build"),
        )
        .await
        .expect("opds v2 catalog request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert!(payload.get("metadata").is_some());
    assert!(
        payload
            .get("metadata")
            .and_then(|value| value.get("modified"))
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty())
    );

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_opds_v2_library_recommended_includes_modified_metadata() {
    let paths = new_router_fixture("router-opds-v2-library-recommended-modified").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 library recommended request should build"),
        )
        .await
        .expect("opds v2 library recommended request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert!(
        payload
            .get("metadata")
            .and_then(|value| value.get("modified"))
            .and_then(Value::as_str)
            .is_some_and(|value| !value.is_empty())
    );

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_opds_v1_ondeck_returns_atom_feed() {
    let paths = new_router_fixture("router-opds-v1-ondeck-feed").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, true).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/ondeck")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 ondeck request should build"),
        )
        .await
        .expect("opds v1 ondeck request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(content_type.contains("application/atom+xml"));

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_opds_v1_publishers_returns_atom_feed() {
    let paths = new_router_fixture("router-opds-v1-publishers-feed").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/publishers")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 publishers request should build"),
        )
        .await
        .expect("opds v1 publishers request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(content_type.contains("application/atom+xml"));

    persistence_contract_fixture::cleanup(paths);
}

#[tokio::test]
async fn router_transient_books_scan_and_analyze_returns_non_placeholder_payload() {
    let paths = new_router_fixture("router-transient-books-scan-analyze").await;
    seed_router_contract_data(&paths).await;

    let transient_dir = paths.config_dir.join("transient-import");
    std::fs::create_dir_all(&transient_dir).expect("transient import directory should be created");
    let candidate_file = transient_dir.join("candidate.jpg");
    let candidate_bytes = b"transient-image-bytes";
    std::fs::write(&candidate_file, candidate_bytes)
        .expect("transient candidate file should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let scan_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/transient-books")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "path": transient_dir.to_string_lossy().to_string(),
                    })
                    .to_string(),
                ))
                .expect("transient scan request should build"),
        )
        .await
        .expect("transient scan request should complete");
    assert_eq!(scan_response.status(), StatusCode::OK);

    let scanned_payload = response_json(scan_response).await;
    let scanned_books = scanned_payload
        .as_array()
        .expect("transient scan payload should be an array");
    assert_eq!(scanned_books.len(), 1);
    let scanned_book = &scanned_books[0];
    assert!(scanned_book.get("path").is_none());
    assert_eq!(
        scanned_book.get("url"),
        Some(&Value::String(candidate_file.to_string_lossy().to_string())),
    );
    assert!(
        scanned_book
            .get("size")
            .and_then(Value::as_str)
            .is_some_and(|size| !size.is_empty())
    );

    let transient_id = scanned_book
        .get("id")
        .and_then(Value::as_str)
        .expect("transient scan payload should include id")
        .to_string();

    let analyze_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/transient-books/{transient_id}/analyze"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("transient analyze request should build"),
        )
        .await
        .expect("transient analyze request should complete");
    assert_eq!(analyze_response.status(), StatusCode::OK);

    let analyzed_payload = response_json(analyze_response).await;
    assert_eq!(
        analyzed_payload.get("status"),
        Some(&Value::String("READY".to_string())),
    );
    let analyzed_pages = analyzed_payload
        .get("pages")
        .and_then(Value::as_array)
        .expect("transient analyze payload should include pages");
    assert_eq!(analyzed_pages.len(), 1);
    assert_eq!(
        analyzed_pages[0].get("fileName"),
        Some(&Value::String("candidate.jpg".to_string())),
    );

    let page_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/transient-books/{transient_id}/pages/1"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("transient page request should build"),
        )
        .await
        .expect("transient page request should complete");
    assert_eq!(page_response.status(), StatusCode::OK);
    let page_bytes = to_bytes(page_response.into_body(), usize::MAX)
        .await
        .expect("transient page response body should be readable");
    assert_eq!(page_bytes.as_ref(), candidate_bytes);

    let invalid_page_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/transient-books/{transient_id}/pages/2"))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("transient invalid page request should build"),
        )
        .await
        .expect("transient invalid page request should complete");
    assert_eq!(invalid_page_response.status(), StatusCode::BAD_REQUEST);

    persistence_contract_fixture::cleanup(paths);
}

async fn seed_router_contract_nullable_samples(
    paths: &persistence_contract_fixture::LegacyDbPaths,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract nullable db should open");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Series 2")
    .bind("series/series-2")
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("nullable series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, \
           SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series 2")
    .bind("Series 2")
    .bind("NullPub")
    .bind("EN")
    .bind(18_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("nullable series metadata row should be inserted");

    sqlx::query(
        "UPDATE SERIES \
                 SET BOOK_COUNT = ? \
                 WHERE ID = ?",
    )
    .bind(1_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("nullable series book count should be updated");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, \
           LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-2")
    .bind(0_i64)
    .bind("book-2.epub")
    .bind("books/book-2.epub")
    .bind("series-2")
    .bind(2_048_i64)
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("nullable book row should be inserted");

    sqlx::query(
        "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("application/epub+zip")
    .bind("READY")
    .bind("book-2")
    .bind(12_i64)
    .execute(&pool)
    .await
    .expect("nullable media row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("2")
    .bind(2.0_f64)
    .bind("Book 2")
    .bind("2024-01-16")
    .bind("book-2")
    .execute(&pool)
    .await
    .expect("nullable book metadata row should be inserted");

    pool.close().await;
}

async fn seed_router_read_progress(
    paths: &persistence_contract_fixture::LegacyDbPaths,
    completed: bool,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract read-progress db should open");

    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(if completed { 10_i64 } else { 1_i64 })
    .bind(completed)
    .execute(&pool)
    .await
    .expect("router contract read-progress row should be inserted");

    pool.close().await;
}

async fn seed_router_series_read_progress(
    paths: &persistence_contract_fixture::LegacyDbPaths,
    read_count: i64,
    in_progress_count: i64,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract series read-progress db should open");

    sqlx::query(
        "INSERT INTO READ_PROGRESS_SERIES (SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT (SERIES_ID, USER_ID) DO UPDATE \
         SET READ_COUNT = excluded.READ_COUNT, IN_PROGRESS_COUNT = excluded.IN_PROGRESS_COUNT",
    )
    .bind("series-1")
    .bind("admin-user")
    .bind(read_count)
    .bind(in_progress_count)
    .execute(&pool)
    .await
    .expect("router contract series read-progress row should be upserted");

    pool.close().await;
}

async fn seed_router_series_counts(
    paths: &persistence_contract_fixture::LegacyDbPaths,
    book_count: i64,
    total_book_count: Option<i64>,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract series counts db should open");

    sqlx::query(
        "UPDATE SERIES \
                 SET BOOK_COUNT = ? \
                 WHERE ID = ?",
    )
    .bind(book_count)
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("router contract series book_count should be updated");

    sqlx::query(
        "UPDATE SERIES_METADATA \
                 SET TOTAL_BOOK_COUNT = ? \
                 WHERE SERIES_ID = ?",
    )
    .bind(total_book_count)
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("router contract series total_book_count should be updated");

    pool.close().await;
}

async fn login_with_basic_and_get_token(app: axum::Router) -> String {
    login_with_basic_credentials_and_get_token(
        app,
        "admin@example.org",
        "router-contract-admin-123",
    )
    .await
}

async fn login_with_basic_credentials_and_get_token(
    app: axum::Router,
    email: &str,
    password: &str,
) -> String {
    let basic_token = STANDARD.encode(format!("{email}:{password}"));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, format!("Basic {basic_token}"))
                .body(Body::empty())
                .expect("users/me request should build"),
        )
        .await
        .expect("users/me request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get("x-auth-token")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("users/me login should return x-auth-token")
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&body).expect("response body should be valid json")
}
