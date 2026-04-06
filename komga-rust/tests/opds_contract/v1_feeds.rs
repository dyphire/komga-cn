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
async fn router_opds_v1_collection_detail_unauthorized_includes_basic_challenge() {
    let paths = new_router_fixture("router-opds-v1-collection-detail-basic-challenge").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections/collection-1")
                .body(Body::empty())
                .expect("opds v1 collection detail unauthorized request should build"),
        )
        .await
        .expect("opds v1 collection detail unauthorized request should complete");

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
async fn router_opds_v1_collection_detail_returns_empty_feed_when_visible_page_is_empty() {
    let paths = new_router_fixture("router-opds-v1-collection-detail-empty-feed").await;
    seed_router_contract_data(&paths).await;
    update_router_series_age_rating(&paths, "series-1", 21).await;
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
                .uri("/opds/v1.2/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collection detail empty-feed request should build"),
        )
        .await
        .expect("opds v1 collection detail empty-feed request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<id>collection-1</id>"));
    assert!(body.contains("<title>Collection 1</title>"));
    assert!(body.contains("rel=\"self\""));
    assert!(body.contains("rel=\"start\""));
    assert!(!body.contains("/opds/v1.2/series/series-1"));
    assert!(!body.contains("<entry>"));
    assert!(!body.contains("rel=\"previous\""));
    assert!(!body.contains("rel=\"next\""));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collection_detail_uses_collection_and_series_last_modified_timestamps() {
    let paths = new_router_fixture("router-opds-v1-collection-detail-updated").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collection detail updated db should open");
    sqlx::query("UPDATE COLLECTION SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind("2024-01-20T01:02:03Z")
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("collection last modified should update");
    sqlx::query("UPDATE SERIES SET LAST_MODIFIED_DATE = ? WHERE ID = ?")
        .bind("2024-01-21T02:03:04Z")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series last modified should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collection detail updated request should build"),
        )
        .await
        .expect("opds v1 collection detail updated request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    assert!(body.contains("<updated>2024-01-20T01:02:03Z</updated>"));
    assert!(body.contains("<updated>2024-01-21T02:03:04Z</updated>"));
    assert!(body.contains("/opds/v1.2/series/series-1"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collection_detail_orders_unordered_entries_by_title_sort() {
    let paths = new_router_fixture("router-opds-v1-collection-detail-unordered-title-sort").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-2", "Alpha Display", "library-1").await;
    update_router_series_metadata_titles(&paths, "series-1", "Zeta Display", "Zulu Sort").await;
    update_router_series_metadata_titles(&paths, "series-2", "Alpha Display", "Alpha Sort").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collection detail order db should open");
    sqlx::query("UPDATE COLLECTION SET ORDERED = ? WHERE ID = ?")
        .bind(false)
        .bind("collection-1")
        .execute(&pool)
        .await
        .expect("collection ordered flag should update");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-1")
    .bind("series-2")
    .bind(99_i64)
    .execute(&pool)
    .await
    .expect("second series should be attached to collection");
    sqlx::query(
        "UPDATE COLLECTION_SERIES SET NUMBER = ? WHERE COLLECTION_ID = ? AND SERIES_ID = ?",
    )
    .bind(0_i64)
    .bind("collection-1")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("first series collection number should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collection detail unordered request should build"),
        )
        .await
        .expect("opds v1 collection detail unordered request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let body = response_text(response).await;
    let alpha_pos = body
        .find("/opds/v1.2/series/series-2")
        .expect("series-2 entry should be present");
    let zeta_pos = body
        .find("/opds/v1.2/series/series-1")
        .expect("series-1 entry should be present");
    assert!(
        alpha_pos < zeta_pos,
        "unordered OPDS v1 collection detail must order by Kotlin titleSort semantics, body={body}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_opds_v1_collection_detail_returns_not_found_when_collection_is_outside_shared_libraries()
 {
    let paths =
        new_router_fixture("router-opds-v1-collection-detail-library-scope-not-found").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-user",
        "library-user@example.org",
        "router-contract-library-123",
        &["library-1"],
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("opds v1 collection detail library-scope db should open");
    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
        .bind("collection-2")
        .bind("Collection 2")
        .bind(false)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("library-scoped collection should be inserted");
    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-2")
    .bind("series-3")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("library-scoped collection series should be inserted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library-user@example.org",
        "router-contract-library-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v1.2/collections/collection-2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v1 collection detail library-scope request should build"),
        )
        .await
        .expect("opds v1 collection detail library-scope request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

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
