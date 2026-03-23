use super::*;

#[test]
pub(super) fn phase7_series_oneshot_exact_route_shape_is_frozen() {
    let expected = BTreeSet::from([
        "GET /api/v1/series/{seriesId}?oneshot=true (newly-owned Phase 7 exact route)",
        "GET /api/v1/series/{seriesId} (reused pre-owned source-truth dependency)",
    ]);

    assert_eq!(expected, frozen_oneshot_direct_route_shapes());

    let config = HarnessConfig::load_default().expect("default compat cases should load");
    for id in [
        "P3-DETAIL-SERIES-DETAIL-OWNED",
        "P7-ONESHOT-SERIES-DETAIL-EXACT-OWNED",
        "P3-DETAIL-EXCLUDED-ONESHOT-ROUTE-CLOSURE",
    ] {
        assert!(
            config.cases.iter().any(|it| it.id == id),
            "missing oneshot direct-route compat case: {id}",
        );
    }

    let exact_owned = config
        .cases
        .iter()
        .find(|it| it.id == "P7-ONESHOT-SERIES-DETAIL-EXACT-OWNED")
        .expect("phase7 exact oneshot compat case should exist");
    assert_eq!(exact_owned.method, "GET");
    assert_eq!(exact_owned.path, "/api/v1/series/series-1?oneshot=true");
    assert_eq!(
        exact_owned
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
        None,
        "exact oneshot=true route must be listed as owned in Phase 7",
    );

    let closure_fallback = config
        .cases
        .iter()
        .find(|it| it.id == "P3-DETAIL-EXCLUDED-ONESHOT-ROUTE-CLOSURE")
        .expect("oneshot closure compat case should exist");
    assert_eq!(
        closure_fallback.path, "/api/v1/series/series-1?oneshot=false",
        "closure fallback should track adjacent non-native oneshot=false shape",
    );
    assert_eq!(
        closure_fallback
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
        Some(&"shadow-java-writer".to_string()),
        "adjacent oneshot closure case must remain explicit non-native",
    );
}

#[test]
pub(super) fn phase7_adjacent_oneshot_query_variants_remain_explicitly_non_native() {
    let expected = BTreeSet::from([
        "oneshot query boundary | BrowseOneshot.vue:779-800 | only exact GET /api/v1/series/{seriesId}?oneshot=true is native in Phase 7; oneshot=false remains explicit non-native",
        "oneshot query boundary | BrowseOneshot.vue:779-800 | duplicate oneshot params remain explicit non-native",
        "oneshot query boundary | BrowseOneshot.vue:779-800 | mixed extra params remain explicit non-native",
        "oneshot query boundary | BrowseOneshot.vue:779-800 | case-variant param names remain explicit non-native",
        "READLIST detail/list-family boundary | BrowseOneshot.vue:785-842 | readlist detail/list/context siblings stay explicit non-native in Phase 7",
        "READLIST listing branch | BrowseOneshot.vue:785-842 | GET /api/v1/readlists stays explicit non-native",
        "READLIST books family boundary | BrowseOneshot.vue:785-842 | Phase 8 promotes paged/filter books queries; bare unpaged=true stays dependency-only and list-family stays explicit non-native",
        "READLIST context siblings | BrowseOneshot.vue:785-842 | /books?unpaged=true + sibling previous/next remain explicit fallback/non-native",
        "oneshot bootstrap widening guards | BrowseOneshot.vue:798-800 | paged/unpaged/read-status/read-date books/list variants stay explicit non-native",
        "media delivery adjacency | BrowseOneshot.vue:118-125 + 497-499 | /pages + /thumbnail + /manifest + /resource/* + /positions stay explicit non-native",
        "reader handoff + download affordances | BrowseOneshot.vue:215-249 + 261-295 | readRouteName/fileUrl visible in page but not newly owned in Phase 6",
        "progress visibility vs route ownership | BrowseOneshot.vue:126-136 + 689-710 | embedded read progress is visible, but read-progress/progression routes stay non-native",
        "collection/readlist removal affordances | BrowseOneshot.vue:417-445 | PATCH/DELETE collection/readlist routes stay explicit non-native",
        "OneshotActionsMenu write/admin affordances | OneshotActionsMenu.vue:10-29 + 85-110 | analyze/refresh/add/remove/mark-read/delete visibility is not a native claim",
        "SSE/live-refresh parity | BrowseOneshot.vue:616-645 + 741-778 | event-driven refresh stays explicit non-native",
    ]);

    assert_eq!(expected, frozen_oneshot_named_exclusion_proofs());

    let config = HarnessConfig::load_default().expect("default compat cases should load");
    for id in [
        "P3-DETAIL-EXCLUDED-ONESHOT-ROUTE-CLOSURE",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-BOOKS",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-PREVIOUS",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-NEXT",
        "P5-ONESHOT-EXCLUDED-BOOKS-LIST-WIDENED-PAGED",
        "P3-DETAIL-EXCLUDED-BOOKS-LIST-READDATE-SORT",
        "P3-DETAIL-EXCLUDED-BOOKS-LIST-READSTATUS-FILTER",
        "P3-DETAIL-EXCLUDED-BOOK-PAGES",
        "P3-DETAIL-EXCLUDED-BOOK-FILE",
        "P3-DETAIL-EXCLUDED-BOOK-THUMBNAIL",
        "P3-DETAIL-EXCLUDED-BOOK-MANIFEST",
        "P3-DETAIL-EXCLUDED-BOOK-RESOURCE",
        "P3-DETAIL-EXCLUDED-BOOK-POSITIONS",
        "P3-DETAIL-EXCLUDED-READ-PROGRESS-PATCH",
        "P3-DETAIL-EXCLUDED-READ-PROGRESS-DELETE",
        "P3-DETAIL-EXCLUDED-PROGRESSION-PUT",
        "P3-DETAIL-EXCLUDED-READLIST-PATCH",
        "P3-DETAIL-EXCLUDED-READLIST-DELETE",
        "P3-DETAIL-EXCLUDED-COLLECTION-PATCH",
        "P3-DETAIL-EXCLUDED-COLLECTION-DELETE",
        "P5-ONESHOT-EXCLUDED-SSE-LIVE-REFRESH",
    ] {
        let case = config
            .cases
            .iter()
            .find(|it| it.id == id)
            .unwrap_or_else(|| panic!("missing oneshot excluded compat case: {id}"));
        assert_eq!(
            case.headers
                .as_ref()
                .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
            Some(&"shadow-java-writer".to_string()),
            "oneshot excluded case must carry explicit non-native marker: {id}",
        );
    }
}
