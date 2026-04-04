use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::Value;
use tower::util::ServiceExt;

#[path = "support/runtime_router_contract_support.rs"]
mod runtime_router_contract_support;

use runtime_router_contract_support::*;

#[test]
fn opds_contract_target_is_registered() {
    assert_required_target_declared("OPDS", "opds_contract");
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_libraries_unauthorized_includes_basic_challenge() {
    let paths = new_router_fixture("router-opds-v1-libraries-basic-challenge").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/libraries")
                .body(Body::empty())
                .expect("opds v1 libraries unauthorized request should build"),
        )
        .await
        .expect("opds v1 libraries unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_libraries_preserves_kotlin_dao_iteration_order() {
    let paths = new_router_fixture("router-opds-v1-libraries-dao-order").await;
    seed_router_contract_data(&paths).await;
    update_router_library_name(&paths, "library-1", "Z Library").await;
    seed_router_library(&paths, "library-2", "A Library").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/libraries")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 libraries request should build"),
        )
        .await
        .expect("opds v1 libraries request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    let library_1_pos = body
        .find("/opds/v1.2/libraries/library-1")
        .expect("library-1 entry should be present");
    let library_2_pos = body
        .find("/opds/v1.2/libraries/library-2")
        .expect("library-2 entry should be present");
    assert!(
        library_1_pos < library_2_pos,
        "OPDS v1 libraries should keep Kotlin DAO iteration order instead of name-sorting, body={body}"
    );

    cleanup_router_fixture(paths);
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

    cleanup_router_fixture(paths);
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

    cleanup_router_fixture(paths);
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

    cleanup_router_fixture(paths);
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

    cleanup_router_fixture(paths);
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_publishers_unauthorized_includes_basic_challenge() {
    let paths = new_router_fixture("router-opds-v1-publishers-basic-challenge").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/publishers")
                .body(Body::empty())
                .expect("opds v1 publishers unauthorized request should build"),
        )
        .await
        .expect("opds v1 publishers unauthorized request should complete");

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response
            .headers()
            .get(header::WWW_AUTHENTICATE)
            .and_then(|value| value.to_str().ok()),
        Some("Basic realm=\"Realm\"")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_library_detail_orders_series_by_title_sort() {
    let paths = new_router_fixture("router-opds-v1-library-detail-title-sort").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-2", "Alpha Display", "library-1").await;
    update_router_series_metadata_titles(&paths, "series-1", "Zeta Display", "Alpha Sort").await;
    update_router_series_metadata_titles(&paths, "series-2", "Alpha Display", "Zeta Sort").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 library detail request should build"),
        )
        .await
        .expect("opds v1 library detail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    let series_1_pos = body
        .find("/opds/v1.2/series/series-1")
        .expect("series-1 entry should be present");
    let series_2_pos = body
        .find("/opds/v1.2/series/series-2")
        .expect("series-2 entry should be present");
    assert!(
        series_1_pos < series_2_pos,
        "OPDS v1 library detail should order by Kotlin titleSort semantics, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_library_detail_hides_age_restricted_series() {
    let paths = new_router_fixture("router-opds-v1-library-detail-age-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-0", "Visible Series", "library-1").await;
    update_router_series_age_rating(&paths, "series-0", 0).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/libraries/library-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 restricted library detail request should build"),
        )
        .await
        .expect("opds v1 restricted library detail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(!body.contains("/opds/v1.2/series/series-1"));
    assert!(body.contains("/opds/v1.2/series/series-0"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_library_detail_paginates_after_restrictions_filtering() {
    let paths = new_router_fixture("router-opds-v1-library-detail-filtered-pagination").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-0", "Visible Series", "library-1").await;
    seed_router_custom_series(&paths, "series-2", "Restricted Series 2", "library-1").await;
    update_router_series_metadata_titles(&paths, "series-2", "Restricted Series 2", "Alpha Sort")
        .await;
    update_router_series_metadata_titles(&paths, "series-1", "Restricted Series 1", "Beta Sort")
        .await;
    update_router_series_metadata_titles(&paths, "series-0", "Visible Series", "Gamma Sort").await;
    update_router_series_age_rating(&paths, "series-2", 18).await;
    update_router_series_age_rating(&paths, "series-1", 18).await;
    update_router_series_age_rating(&paths, "series-0", 0).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/libraries/library-1?page=0&size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 restricted library detail paged request should build"),
        )
        .await
        .expect("opds v1 restricted library detail paged request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("/opds/v1.2/series/series-0"));
    assert!(!body.contains("/opds/v1.2/series/series-1"));
    assert!(!body.contains("/opds/v1.2/series/series-2"));
    assert!(
        !body.contains("rel=\"next\""),
        "OPDS v1 library detail must paginate after restrictions filtering, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_publishers_preserves_unicode_collation_order() {
    let paths = new_router_fixture("router-opds-v1-publishers-unicode-order").await;
    seed_router_contract_data(&paths).await;
    update_router_series_publisher(&paths, "series-1", "Zulu House").await;
    seed_router_custom_series(&paths, "series-ang", "Series Å", "library-1").await;
    update_router_series_publisher(&paths, "series-ang", "Ångström Press").await;

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
    let body = response_text(response).await;
    let angstrom_pos = body
        .find("publisher:%C3%85ngstr%C3%B6m%20Press")
        .expect("Ångström publisher entry should be present");
    let zulu_pos = body
        .find("publisher:Zulu%20House")
        .expect("Zulu publisher entry should be present");
    assert!(
        angstrom_pos < zulu_pos,
        "OPDS v1 publishers should keep Kotlin Unicode collation order, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_manifest_sets_private_cache_and_supports_if_none_match() {
    let paths = new_router_fixture("router-opds-v2-manifest-cache-headers").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/books/book-1/manifest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 manifest conditional request should build"),
        )
        .await
        .expect("opds v2 manifest conditional request should complete");

    assert_eq!(first_response.status(), StatusCode::OK);
    assert_eq!(
        first_response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("max-age=0, must-revalidate, private")
    );

    let etag = first_response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("opds v2 manifest response should include etag");

    let second_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/books/book-1/manifest")
                .header("x-auth-token", &auth_token)
                .header(header::IF_NONE_MATCH, etag)
                .body(Body::empty())
                .expect("opds v2 manifest conditional follow-up request should build"),
        )
        .await
        .expect("opds v2 manifest conditional follow-up request should complete");

    assert_eq!(second_response.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        second_response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("max-age=0, must-revalidate, private")
    );
    assert!(second_response.headers().contains_key(header::ETAG));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_latest_books_feed_hides_books_for_age_exclude_restricted_user() {
    let paths = new_router_fixture("router-opds-v2-latest-books-age-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/libraries/library-1/books/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 latest-books request should build"),
        )
        .await
        .expect("opds v2 latest-books request should complete");

    let status = response.status();
    let payload = response_json(response).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "unexpected OPDS latest-books response payload: {payload}",
    );
    assert_eq!(
        payload
            .get("publications")
            .and_then(Value::as_array)
            .map(Vec::len),
        Some(0),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_search_feed_uses_search_title_for_non_blank_search() {
    let paths = new_router_fixture("router-opds-v1-series-search-title").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?search=Series")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 series search request should build"),
        )
        .await
        .expect("opds v1 series search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("Series search for: Series"),
        "OPDS v1 non-blank search must expose Kotlin-compatible feed title, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_blank_search_behaves_as_unfiltered_series_feed() {
    let paths = new_router_fixture("router-opds-v1-series-blank-search").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-alpha", "Alpha Series", "library-1").await;
    seed_router_custom_series(&paths, "series-zeta", "Zeta Series", "library-1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?search=%20%20%20")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 blank-search request should build"),
        )
        .await
        .expect("opds v1 blank-search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("<title>All series</title>"),
        "OPDS v1 blank search must fall back to unfiltered All series feed, body={body}",
    );
    assert!(
        body.contains("/opds/v1.2/series/series-alpha")
            && body.contains("/opds/v1.2/series/series-zeta"),
        "OPDS v1 blank search must not filter out matching libraries, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_search_hides_unauthorized_library_series() {
    let paths = new_router_fixture("router-opds-v1-series-library-visibility").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-restricted-user",
        "library.restricted@example.org",
        "router-contract-library-restricted-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library.restricted@example.org",
        "router-contract-library-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?search=Series")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 restricted search request should build"),
        )
        .await
        .expect("opds v1 restricted search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        !body.contains("/opds/v1.2/series/series-3"),
        "OPDS v1 search must hide series from unauthorized libraries, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_latest_series_feed_hides_series_for_age_exclude_restricted_user() {
    let paths = new_router_fixture("router-opds-v1-latest-series-age-restricted").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 latest-series request should build"),
        )
        .await
        .expect("opds v1 latest-series request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        !body.contains("/opds/v1.2/series/series-1"),
        "OPDS v1 latest-series feed must hide restricted series, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_latest_series_feed_paginates_after_restriction_filtering() {
    let paths = new_router_fixture("router-opds-v1-latest-series-restricted-pagination").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-2", "Series 2", "library-1").await;
    seed_router_custom_series(&paths, "series-0", "Series 0", "library-1").await;
    seed_router_age_exclude_user(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds latest-series pagination db should open");
    sqlx::query("UPDATE SERIES_METADATA SET AGE_RATING = ? WHERE SERIES_ID = ?")
        .bind(0_i64)
        .bind("series-0")
        .execute(&pool)
        .await
        .expect("visible latest series age rating should update");
    for (series_id, last_modified) in [
        ("series-2", "2024-03-03T00:00:00"),
        ("series-1", "2024-03-02T00:00:00"),
        ("series-0", "2024-03-01T00:00:00"),
    ] {
        sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
            .bind(last_modified)
            .bind(last_modified)
            .bind(series_id)
            .execute(&pool)
            .await
            .expect("series latest ordering should update");
    }
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/latest?page=0&size=1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 latest-series paged request should build"),
        )
        .await
        .expect("opds v1 latest-series paged request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("/opds/v1.2/series/series-0"));
    assert!(!body.contains("/opds/v1.2/series/series-2"));
    assert!(!body.contains("/opds/v1.2/series/series-1"));
    assert!(
        !body.contains("rel=\"next\""),
        "OPDS v1 latest-series must compute pagination after restrictions filtering, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_latest_series_feed_normalizes_entry_updated_to_utc_z() {
    let paths = new_router_fixture("router-opds-v1-latest-series-updated-format").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds latest-series updated db should open");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ?, CREATED_DATE = ? WHERE ID = ?")
        .bind("2024-03-03 00:00:00")
        .bind("2024-03-03 00:00:00")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("latest series updated timestamp should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series/latest")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 latest-series updated request should build"),
        )
        .await
        .expect("opds v1 latest-series updated request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("<updated>2024-03-03T00:00:00Z</updated>"),
        "OPDS v1 latest-series entry updated must be normalized to UTC/Z, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_series_search_supports_fielded_query_candidate_lookup() {
    let paths = new_router_fixture("router-opds-v1-series-fielded-query").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?search=publisher:AltPub")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 fielded search request should build"),
        )
        .await
        .expect("opds v1 fielded search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(
        body.contains("Series search for: publisher:AltPub"),
        "OPDS v1 fielded search should preserve feed title semantics, body={body}",
    );
    assert!(
        body.contains("/opds/v1.2/series/series-3"),
        "OPDS v1 fielded search should surface unified-search candidate matches, body={body}",
    );
    assert!(
        !body.contains("/opds/v1.2/series/series-1")
            && !body.contains("/opds/v1.2/series/series-2"),
        "OPDS v1 fielded search should keep result set narrowed to matching candidates, body={body}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_search_query_contract_covers_group_presence_and_order() {
    let paths = new_router_fixture("router-opds-v2-search-group-contract").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let expectations = [
        (
            "/opds/v2/search?query=Series%201",
            vec!["Series"],
            "single-group search should only retain non-empty groups",
        ),
        (
            "/opds/v2/search?query=1",
            vec!["Series", "Books", "Read Lists"],
            "multi-group search should preserve Kotlin group ordering",
        ),
    ];

    for (uri, expected_group_titles, context) in expectations {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(uri)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("opds v2 search request should build"),
            )
            .await
            .expect("opds v2 search request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let groups = payload
            .get("groups")
            .and_then(Value::as_array)
            .expect("opds v2 search payload should expose groups array");
        let group_titles = groups
            .iter()
            .filter_map(|group| {
                group
                    .get("metadata")
                    .and_then(|value| value.get("title"))
                    .and_then(Value::as_str)
            })
            .collect::<Vec<_>>();

        assert_eq!(group_titles, expected_group_titles, "{context}: {payload}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_search_supports_fielded_query_candidate_lookup() {
    let paths = new_router_fixture("router-opds-v2-search-fielded-query").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/search?query=title:1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 fielded search request should build"),
        )
        .await
        .expect("opds v2 fielded search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let groups = payload
        .get("groups")
        .and_then(Value::as_array)
        .expect("opds v2 fielded search payload should expose groups array");
    let group_titles = groups
        .iter()
        .filter_map(|group| {
            group
                .get("metadata")
                .and_then(|value| value.get("title"))
                .and_then(Value::as_str)
        })
        .collect::<Vec<_>>();

    assert_eq!(
        group_titles,
        vec!["Series", "Books", "Read Lists"],
        "{payload}"
    );

    let rendered = payload.to_string();
    assert!(
        rendered.contains("/opds/v2/series/series-1")
            && rendered.contains("book-1/manifest")
            && rendered.contains("/opds/v2/readlists/readlist-1"),
        "OPDS v2 fielded search should include unified-search candidate matches: {payload}",
    );
    assert!(
        !rendered.contains("/opds/v2/series/series-2")
            && !rendered.contains("/opds/v2/series/series-3")
            && !rendered.contains("book-2/manifest")
            && !rendered.contains("book-3/manifest"),
        "OPDS v2 fielded search should keep non-matching entities out of groups: {payload}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v2_search_hides_unauthorized_library_results() {
    let paths = new_router_fixture("router-opds-v2-search-library-visibility").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-restricted-user-v2",
        "library.restricted.v2@example.org",
        "router-contract-library-restricted-v2-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library.restricted.v2@example.org",
        "router-contract-library-restricted-v2-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/search?query=Series%203")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 restricted search request should build"),
        )
        .await
        .expect("opds v2 restricted search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let groups = payload
        .get("groups")
        .and_then(Value::as_array)
        .expect("opds v2 restricted search payload should expose groups array");
    assert!(
        groups.is_empty(),
        "OPDS v2 search must omit unauthorized-only results instead of returning empty groups: {payload}",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_search_supports_accent_folded_and_cjk_series_queries() {
    let paths = new_router_fixture("router-opds-search-accent-cjk-recall").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-cafe", "Café 東京 Series", "library-1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let v1_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/series?search=Cafe%20東京")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 accent+cjk search request should build"),
        )
        .await
        .expect("opds v1 accent+cjk search request should complete");
    assert_eq!(v1_response.status(), StatusCode::OK);
    let v1_body = response_text(v1_response).await;
    assert!(
        v1_body.contains("/opds/v1.2/series/series-cafe"),
        "OPDS v1 search should retain accent-folded mixed CJK recall: {v1_body}",
    );

    let v2_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/search?query=Cafe%20東京")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 accent+cjk search request should build"),
        )
        .await
        .expect("opds v2 accent+cjk search request should complete");
    assert_eq!(v2_response.status(), StatusCode::OK);
    let v2_payload = response_json(v2_response).await;
    let rendered = v2_payload.to_string();
    assert!(
        rendered.contains("/opds/v2/series/series-cafe"),
        "OPDS v2 search should retain accent-folded mixed CJK recall: {v2_payload}",
    );

    cleanup_router_fixture(paths);
}

async fn response_text(response: axum::response::Response) -> String {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    String::from_utf8(body.to_vec()).expect("response body should be valid utf-8")
}

async fn update_router_series_publisher(paths: &RuntimeDbPaths, series_id: &str, publisher: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds publisher update db should open");

    sqlx::query("UPDATE SERIES_METADATA SET PUBLISHER = ? WHERE SERIES_ID = ?")
        .bind(publisher)
        .bind(series_id)
        .execute(&pool)
        .await
        .expect("series publisher should be updated");

    pool.close().await;
}

async fn update_router_series_metadata_titles(
    paths: &RuntimeDbPaths,
    series_id: &str,
    title: &str,
    title_sort: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds series metadata update db should open");

    sqlx::query("UPDATE SERIES_METADATA SET TITLE = ?, TITLE_SORT = ? WHERE SERIES_ID = ?")
        .bind(title)
        .bind(title_sort)
        .bind(series_id)
        .execute(&pool)
        .await
        .expect("series metadata titles should be updated");

    pool.close().await;
}

async fn update_router_series_age_rating(paths: &RuntimeDbPaths, series_id: &str, age_rating: i64) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds series age rating update db should open");

    sqlx::query("UPDATE SERIES_METADATA SET AGE_RATING = ? WHERE SERIES_ID = ?")
        .bind(age_rating)
        .bind(series_id)
        .execute(&pool)
        .await
        .expect("series metadata age rating should be updated");

    pool.close().await;
}

async fn update_router_library_name(paths: &RuntimeDbPaths, library_id: &str, name: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds library update db should open");

    sqlx::query("UPDATE LIBRARY SET NAME = ? WHERE ID = ?")
        .bind(name)
        .bind(library_id)
        .execute(&pool)
        .await
        .expect("library name should be updated");

    pool.close().await;
}

async fn seed_router_library(paths: &RuntimeDbPaths, library_id: &str, name: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds library seed db should open");

    sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
        .bind(library_id)
        .bind(name)
        .bind(paths.config_dir.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .expect("library row should be inserted");

    pool.close().await;
}
