use super::*;

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
