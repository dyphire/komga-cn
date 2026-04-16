use super::*;

#[tokio::test]
async fn router_discovery_series_list_locks_main_search_parity_for_retained_inputs() {
    let paths = new_router_fixture("router-discovery-series-list-main-search-parity").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    update_series_search_fixture_title(&paths, "series-2", "Café 東京 Series Series 2").await;
    seed_router_series_title_sort(&paths, "series-3", "Zeta Filing Title").await;
    seed_router_series_alternate_title(&paths, "series-1", "alt-1", "Hidden Alias").await;

    let app = build_router_with_config(&search_ready_runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;

    let blank_ids = series_list_ids(&app, &admin_token, Some("relevance,desc"), Some("   ")).await;
    assert_eq!(blank_ids, vec!["series-1", "series-2", "series-3"]);

    let relevance_desc_ids =
        series_list_ids(&app, &admin_token, Some("relevance,desc"), Some("series")).await;
    assert_eq!(relevance_desc_ids, vec!["series-2", "series-1", "series-3"]);

    let relevance_asc_ids =
        series_list_ids(&app, &admin_token, Some("relevance,asc"), Some("series")).await;
    assert_eq!(relevance_asc_ids, vec!["series-3", "series-1", "series-2"]);

    let fielded_ids = series_list_ids(
        &app,
        &admin_token,
        Some("relevance,desc"),
        Some("title:series"),
    )
    .await;
    assert_eq!(fielded_ids, vec!["series-2", "series-1", "series-3"]);

    let title_sort_ids = series_list_ids(
        &app,
        &admin_token,
        Some("relevance,desc"),
        Some("title:Zeta"),
    )
    .await;
    assert_eq!(title_sort_ids, vec!["series-3"]);

    let alternate_title_ids = series_list_ids(
        &app,
        &admin_token,
        Some("relevance,desc"),
        Some("title:Hidden"),
    )
    .await;
    assert_eq!(alternate_title_ids, vec!["series-1"]);

    let invalid_query_ids =
        series_list_ids(&app, &admin_token, Some("relevance,desc"), Some("title:(")).await;
    assert!(invalid_query_ids.is_empty());

    let accent_cjk_ids = series_list_ids(
        &app,
        &admin_token,
        Some("relevance,desc"),
        Some("cafe 東京"),
    )
    .await;
    assert_eq!(
        accent_cjk_ids,
        vec!["series-2"],
        "series/list should retain accent-folded mixed CJK recall at the route boundary",
    );

    seed_router_age_exclude_user_with_roles(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        16,
        &["USER", "PAGE_STREAMING"],
    )
    .await;
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;
    let visible_ids = series_list_ids(
        &app,
        &restricted_token,
        Some("relevance,desc"),
        Some("series"),
    )
    .await;
    assert_eq!(visible_ids, vec!["series-3"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_defaults_to_relevance_sort_when_full_text_search_is_present()
{
    let paths = new_router_fixture("router-discovery-series-list-default-relevance-sort").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    update_series_search_fixture_title(&paths, "series-2", "Café 東京 Series Series 2").await;
    seed_router_series_title_sort(&paths, "series-3", "Zeta Filing Title").await;

    let app = build_router_with_config(&search_ready_runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;

    let default_relevance_ids = series_list_ids(&app, &admin_token, None, Some("series")).await;
    assert_eq!(
        default_relevance_ids,
        // Intentional exemption: Kotlin's default `Sort.by("relevance")` ordering is a Lucene-
        // specific hit-order quirk. The Rust route keeps the implicit full-text path aligned with
        // explicit `relevance,asc` score semantics instead of reproducing a backend-specific
        // exception only for requests that omit `sort`.
        vec!["series-3", "series-2", "series-1"]
    );

    cleanup_router_fixture(paths);
}
