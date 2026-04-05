use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::json;
use tower::util::ServiceExt;

#[path = "support/runtime_router_contract_support.rs"]
mod runtime_router_contract_support;

use runtime_router_contract_support::*;

#[test]
fn libraries_contract_target_is_registered() {
    assert_required_target_declared("libraries", "libraries_contract");
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_catch_all_returns_empty_json_when_proxy_disabled() {
    let paths = new_router_fixture("router-kobo-catch-all-disabled").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/unimplemented-resource")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo catch-all request should build"),
        )
        .await
        .expect("kobo catch-all request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload, serde_json::json!({}));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_libraries_route_matches_kotlin_etag_without_extra_cache_headers() {
    let paths = new_router_fixture("router-api-libraries-kotlin-cache-headers").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("libraries cache request should build"),
        )
        .await
        .expect("libraries cache request should complete");

    assert_eq!(first_response.status(), StatusCode::OK);
    assert!(
        first_response
            .headers()
            .get(header::CACHE_CONTROL)
            .is_none(),
        "Kotlin libraries list does not emit Cache-Control on 200"
    );

    let etag = first_response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("libraries response should include etag");

    let second_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .header(header::IF_NONE_MATCH, etag)
                .body(Body::empty())
                .expect("conditional libraries request should build"),
        )
        .await
        .expect("conditional libraries request should complete");

    assert_eq!(second_response.status(), StatusCode::NOT_MODIFIED);
    assert!(
        second_response
            .headers()
            .get(header::CACHE_CONTROL)
            .is_none(),
        "Kotlin conditional libraries list does not emit Cache-Control on 304"
    );
    assert!(second_response.headers().contains_key(header::ETAG));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_patch_accepts_null_scan_directory_exclusions_as_clear() {
    let paths = new_router_fixture("router-api-library-patch-null-exclusions").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for library exclusions seed");
    sqlx::query("INSERT INTO LIBRARY_EXCLUSIONS (LIBRARY_ID, EXCLUSION) VALUES (?, ?), (?, ?)")
        .bind("library-1")
        .bind("folder-a")
        .bind("library-1")
        .bind("folder-b")
        .execute(&pool)
        .await
        .expect("library exclusions should be seeded");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "scanDirectoryExclusions": null }).to_string(),
                ))
                .expect("library patch request should build"),
        )
        .await
        .expect("library patch request should complete");
    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("library detail request should build"),
        )
        .await
        .expect("library detail request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);
    let payload = response_json(get_response).await;
    assert_eq!(
        payload.get("scanDirectoryExclusions"),
        Some(&json!([])),
        "PATCH null scanDirectoryExclusions should clear exclusions like Kotlin"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_patch_rejects_blank_name() {
    let paths = new_router_fixture("router-api-library-patch-blank-name").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch_response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "name": "   " }).to_string()))
                .expect("library patch blank-name request should build"),
        )
        .await
        .expect("library patch blank-name request should complete");

    assert_eq!(patch_response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(patch_response).await;
    assert_eq!(
        payload,
        json!({
            "violations": [
                {
                    "fieldName": "name",
                    "message": "must not be blank"
                }
            ]
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_patch_rejects_blank_root() {
    let paths = new_router_fixture("router-api-library-patch-blank-root").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch_response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "root": "   " }).to_string()))
                .expect("library patch blank-root request should build"),
        )
        .await
        .expect("library patch blank-root request should complete");

    assert_eq!(patch_response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(patch_response).await;
    assert_eq!(
        payload,
        json!({
            "violations": [
                {
                    "fieldName": "root",
                    "message": "must not be blank"
                }
            ]
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_library_patch_rejects_multiple_blank_fields_with_kotlin_validation_payload() {
    let paths = new_router_fixture("router-api-library-patch-multiple-blank-fields").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch_response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "name": "   ", "root": "   " }).to_string(),
                ))
                .expect("library patch multiple-blank-fields request should build"),
        )
        .await
        .expect("library patch multiple-blank-fields request should complete");

    assert_eq!(patch_response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(patch_response).await;
    assert_eq!(
        payload,
        json!({
            "violations": [
                {
                    "fieldName": "root",
                    "message": "must not be blank"
                },
                {
                    "fieldName": "name",
                    "message": "must not be blank"
                }
            ]
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_metadata_route_sets_etag_and_supports_if_none_match() {
    let paths = new_router_fixture("router-kobo-book-metadata-cache-headers").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo metadata request should build"),
        )
        .await
        .expect("kobo metadata request should complete");

    assert_eq!(first_response.status(), StatusCode::OK);
    let etag = first_response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("kobo metadata response should include etag");

    let second_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::IF_NONE_MATCH, etag)
                .body(Body::empty())
                .expect("conditional kobo metadata request should build"),
        )
        .await
        .expect("conditional kobo metadata request should complete");

    assert_eq!(second_response.status(), StatusCode::NOT_MODIFIED);
    assert!(second_response.headers().contains_key(header::ETAG));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_metadata_uses_persisted_fields_instead_of_placeholders() {
    let paths = new_router_fixture("router-kobo-book-metadata-parity").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for kobo metadata parity");
    sqlx::query("UPDATE BOOK_METADATA SET ISBN = ? WHERE BOOK_ID = ?")
        .bind("9781234567890")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book metadata isbn should be updated");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo metadata parity request should build"),
        )
        .await
        .expect("kobo metadata parity request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let metadata = payload
        .as_array()
        .and_then(|items| items.first())
        .expect("kobo metadata response should contain one item");

    assert_eq!(metadata.get("Description"), Some(&json!(" ")));
    assert_eq!(metadata.get("Language"), Some(&json!("en")));
    assert_eq!(metadata.get("CoverImageId"), Some(&json!("thumb-book-1")));
    assert_eq!(metadata.get("ISBN"), Some(&json!("9781234567890")));
    assert_eq!(
        metadata.pointer("/Publisher/Name"),
        Some(&json!("PubHouse"))
    );
    assert_eq!(metadata.pointer("/Publisher/Imprint"), Some(&json!("")));
    assert_eq!(metadata.pointer("/Series/Id"), Some(&json!("series-1")));
    assert_eq!(metadata.pointer("/Series/Name"), Some(&json!("Series 1")));
    assert_eq!(metadata.pointer("/Series/Number"), Some(&json!("1")));
    assert_eq!(metadata.pointer("/Series/NumberFloat"), Some(&json!(1.0)));
    assert_eq!(metadata.get("Contributors"), Some(&json!(["Jane Writer"])));
    assert_eq!(
        metadata.pointer("/ContributorRoles/0/Name"),
        Some(&json!("Jane Writer"))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_api_libraries_head_reuses_get_etag_for_conditional_requests() {
    let paths = new_router_fixture("router-api-libraries-head-etag").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("libraries get request should build"),
        )
        .await
        .expect("libraries get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);
    let etag = get_response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("libraries get response should include etag");

    let head_response = app
        .oneshot(
            Request::builder()
                .method("HEAD")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .header(header::IF_NONE_MATCH, etag)
                .body(Body::empty())
                .expect("libraries head request should build"),
        )
        .await
        .expect("libraries head request should complete");

    assert_eq!(head_response.status(), StatusCode::NOT_MODIFIED);

    cleanup_router_fixture(paths);
}
