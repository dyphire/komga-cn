use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use tower::util::ServiceExt;

#[path = "support/runtime_router_contract_support.rs"]
mod runtime_router_contract_support;

use runtime_router_contract_support::*;

#[test]
fn readlists_collections_contract_target_is_registered() {
    assert_required_target_declared("readlists/collections", "readlists_collections_contract");
}

#[tokio::test]
async fn router_discovery_books_list_supports_read_list_id_ops_in_runtime_owned_mode() {
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_metadata_and_collection_filters_in_runtime_owned_mode()
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
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

    cleanup_router_fixture(paths);
}
