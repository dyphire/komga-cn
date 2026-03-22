use super::*;

#[test]
pub(super) fn phase6_readlist_detail_route_shape_is_frozen() {
    let expected = BTreeSet::from([
        "GET /api/v1/readlists/{readListId} (newly-owned Phase 6 route)",
        "GET /api/v1/series/{seriesId} (reused pre-owned dependency)",
        "GET /api/v1/series/{seriesId}/collections (reused pre-owned dependency)",
        "POST /api/v1/books/list body=SeriesId(seriesId) only (reused pre-owned dependency)",
        "GET /api/v1/books/{bookId}/readlists (reused pre-owned dependency)",
    ]);

    assert_eq!(expected, frozen_oneshot_direct_route_shapes());

    let config = HarnessConfig::load_default().expect("default compat cases should load");
    for id in [
        "P6-ONESHOT-READLIST-DETAIL-OWNED",
        "P3-DETAIL-SERIES-DETAIL-OWNED",
        "P3-DETAIL-SERIES-COLLECTIONS-OWNED",
        "P5-ONESHOT-BOOKS-LIST-SERIESID-ONLY-OWNED",
        "P3-DETAIL-BOOK-READLISTS-OWNED",
    ] {
        assert!(
            config.cases.iter().any(|it| it.id == id),
            "missing oneshot direct-route compat case: {id}",
        );
    }

    let phase6_owned = config
        .cases
        .iter()
        .find(|it| it.id == "P6-ONESHOT-READLIST-DETAIL-OWNED")
        .expect("phase6 readlist detail owned compat case should exist");
    assert_eq!(
        phase6_owned
            .headers
            .as_ref()
            .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
        None,
        "phase6 newly-owned route must not carry shadow marker",
    );
}

#[test]
pub(super) fn phase6_adjacent_branches_remain_explicitly_non_native() {
    let expected = BTreeSet::from([
        "oneshot query closure | BrowseOneshot.vue:779-800 | GET /api/v1/series/{seriesId}?oneshot=true stays explicit non-native",
        "READLIST detail ownership boundary | BrowseOneshot.vue:785-842 | only GET /api/v1/readlists/{readListId} is newly owned in Phase 6; list + context siblings stay explicit non-native",
        "READLIST listing branch | BrowseOneshot.vue:785-842 | GET /api/v1/readlists stays explicit non-native",
        "READLIST books pagination/library filtering | BrowseOneshot.vue:785-842 | paged readlist books and library_id variants stay explicit non-native",
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
        "P2-DISCOVERY-UNSUPPORTED-READLISTS",
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
