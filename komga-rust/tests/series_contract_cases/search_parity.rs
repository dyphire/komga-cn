use super::*;

#[tokio::test]
async fn router_discovery_series_list_locks_main_search_parity_for_retained_inputs() {
    let ctx = TestFixture::builder("router-discovery-series-list-main-search-parity")
        .with_search_index()
        .with_seed(|paths| async move {
            seed_router_authors_scope_variants(&paths).await;
            update_series_search_fixture_title(&paths, "series-2", "Café 東京 Series Series 2")
                .await;
            seed_router_series_title_sort(&paths, "series-3", "Zeta Filing Title").await;
            seed_router_series_alternate_title(&paths, "series-1", "alt-1", "Hidden Alias").await;
        })
        .build()
        .await;
    let admin_token = ctx.login_admin().await;

    let blank_ids =
        series_list_ids(ctx.app(), &admin_token, Some("relevance,desc"), Some("   ")).await;
    assert_eq!(blank_ids, vec!["series-1", "series-2", "series-3"]);

    let relevance_desc_ids = series_list_ids(
        ctx.app(),
        &admin_token,
        Some("relevance,desc"),
        Some("series"),
    )
    .await;
    assert_eq!(relevance_desc_ids, vec!["series-3", "series-1", "series-2"]);

    let relevance_asc_ids = series_list_ids(
        ctx.app(),
        &admin_token,
        Some("relevance,asc"),
        Some("series"),
    )
    .await;
    assert_eq!(relevance_asc_ids, vec!["series-2", "series-1", "series-3"]);

    let fielded_ids = series_list_ids(
        ctx.app(),
        &admin_token,
        Some("relevance,desc"),
        Some("title:series"),
    )
    .await;
    assert_eq!(fielded_ids, vec!["series-3", "series-1", "series-2"]);

    let title_sort_ids = series_list_ids(
        ctx.app(),
        &admin_token,
        Some("relevance,desc"),
        Some("title:Zeta"),
    )
    .await;
    assert_eq!(title_sort_ids, vec!["series-3"]);

    let alternate_title_ids = series_list_ids(
        ctx.app(),
        &admin_token,
        Some("relevance,desc"),
        Some("title:Hidden"),
    )
    .await;
    assert_eq!(alternate_title_ids, vec!["series-1"]);

    let invalid_query_ids = series_list_ids(
        ctx.app(),
        &admin_token,
        Some("relevance,desc"),
        Some("title:("),
    )
    .await;
    assert!(invalid_query_ids.is_empty());

    let accent_cjk_ids = series_list_ids(
        ctx.app(),
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
        ctx.paths(),
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        16,
        &["USER", "PAGE_STREAMING"],
    )
    .await;
    let restricted_token = ctx
        .login_with_credentials("restricted@example.org", "router-contract-restricted-123")
        .await;
    let visible_ids = series_list_ids(
        ctx.app(),
        &restricted_token,
        Some("relevance,desc"),
        Some("series"),
    )
    .await;
    assert_eq!(visible_ids, vec!["series-3"]);
}

#[tokio::test]
async fn router_discovery_series_list_defaults_to_relevance_sort_when_full_text_search_is_present()
{
    let ctx = TestFixture::builder("router-discovery-series-list-default-relevance-sort")
        .with_search_index()
        .with_seed(|paths| async move {
            seed_router_authors_scope_variants(&paths).await;
            update_series_search_fixture_title(&paths, "series-2", "Café 東京 Series Series 2")
                .await;
            seed_router_series_title_sort(&paths, "series-3", "Zeta Filing Title").await;
        })
        .build()
        .await;
    let admin_token = ctx.login_admin().await;

    let explicit_relevance_asc_ids = series_list_ids(
        ctx.app(),
        &admin_token,
        Some("relevance,asc"),
        Some("series"),
    )
    .await;
    let default_relevance_ids =
        series_list_ids(ctx.app(), &admin_token, None, Some("series")).await;
    assert_eq!(
        default_relevance_ids,
        // Kotlin uses Sort.by("relevance"), which is ascending unless the request overrides it.
        explicit_relevance_asc_ids
    );
}
