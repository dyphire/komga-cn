use super::*;

#[test]
fn excluded_media_context_and_write_shapes_remain_non_native() {
    let expected = BTreeSet::from([
        "GET /api/v1/books/{bookId}/pages",
        "GET /api/v1/books/{bookId}/file",
        "GET /api/v1/books/{bookId}/thumbnail",
        "GET /api/v1/books/{bookId}/manifest",
        "GET /api/v1/books/{bookId}/resource/*",
        "GET /api/v1/books/{bookId}/positions",
        "GET /api/v1/series/{seriesId}/thumbnail*",
        "GET /api/v1/series/{seriesId}/file",
        "GET /api/v1/readlists/{readListId}/books?unpaged=true",
        "GET /api/v1/readlists/{readListId}/books/{bookId}/previous",
        "GET /api/v1/readlists/{readListId}/books/{bookId}/next",
        "PATCH /api/v1/books/{bookId}/read-progress",
        "DELETE /api/v1/books/{bookId}/read-progress",
        "PUT /api/v1/books/{bookId}/progression",
        "PATCH /api/v1/readlists/{readListId}",
        "DELETE /api/v1/readlists/{readListId}",
        "PATCH /api/v1/collections/{collectionId}",
        "DELETE /api/v1/collections/{collectionId}",
        "POST /api/v1/books/list sort=readProgress.readDate,desc",
        "POST /api/v1/books/list condition=ReadStatus",
    ]);

    assert_eq!(expected, frozen_non_native_detail_shapes());
    assert!(
        !frozen_non_native_detail_shapes().contains("GET /api/v1/series/{seriesId}?oneshot=true"),
        "exact Phase 7 oneshot=true route must not be listed in generic non-native ledger",
    );

    let config = HarnessConfig::load_default().expect("default compat cases should load");
    for id in [
        "P3-DETAIL-EXCLUDED-BOOK-PAGES",
        "P3-DETAIL-EXCLUDED-BOOK-FILE",
        "P3-DETAIL-EXCLUDED-BOOK-THUMBNAIL",
        "P3-DETAIL-EXCLUDED-BOOK-MANIFEST",
        "P3-DETAIL-EXCLUDED-BOOK-RESOURCE",
        "P3-DETAIL-EXCLUDED-BOOK-POSITIONS",
        "P3-DETAIL-EXCLUDED-SERIES-THUMBNAIL",
        "P3-DETAIL-EXCLUDED-SERIES-FILE",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-BOOKS",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-PREVIOUS",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-NEXT",
        "P3-DETAIL-EXCLUDED-ONESHOT-ROUTE-CLOSURE",
        "P3-DETAIL-EXCLUDED-READ-PROGRESS-PATCH",
        "P3-DETAIL-EXCLUDED-READ-PROGRESS-DELETE",
        "P3-DETAIL-EXCLUDED-PROGRESSION-PUT",
        "P3-DETAIL-EXCLUDED-READLIST-PATCH",
        "P3-DETAIL-EXCLUDED-READLIST-DELETE",
        "P3-DETAIL-EXCLUDED-COLLECTION-PATCH",
        "P3-DETAIL-EXCLUDED-COLLECTION-DELETE",
        "P3-DETAIL-EXCLUDED-BOOKS-LIST-READDATE-SORT",
        "P3-DETAIL-EXCLUDED-BOOKS-LIST-READSTATUS-FILTER",
    ] {
        let case = config
            .cases
            .iter()
            .find(|it| it.id == id)
            .unwrap_or_else(|| panic!("missing non-native detail compat case: {id}"));
        assert_eq!(
            case.headers
                .as_ref()
                .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
            Some(&"shadow-java-writer".to_string()),
            "excluded detail case must carry explicit non-native marker: {id}",
        );
    }
}

#[test]
fn contextual_media_and_write_branches_are_explicitly_non_native() {
    let expected = BTreeSet::from([
        "browse-oneshot closure | BrowseSeries.vue:996-1004 + BrowseBook.vue:630-645 | visible redirect only, not direct-detail native ownership",
        "READLIST context routing | BrowseBook.vue:650-680 -> /api/v1/readlists/{readListId}/books?unpaged=true + sibling previous/next | explicit non-native",
        "media delivery | /api/v1/books/{bookId}/pages + /pages/{pageNumber} + /pages/{pageNumber}/thumbnail + /thumbnail | explicit non-native",
        "reader handoff + download URLs | BrowseBook.vue:199-222 + 531-550 and BrowseSeries.vue:195-203 + 745-749 -> fileUrl/readRouteName adjacent branches stay explicit non-native",
        "read-progress mutation + progression | /api/v1/books/{bookId}/read-progress (PATCH/DELETE) + /api/v1/books/{bookId}/progression | explicit non-native",
        "collection/readlist removal actions | BrowseSeries.vue:448-457 + BrowseBook.vue:347-356 -> PATCH/DELETE collection/readlist routes stay non-native",
        "admin edit/delete affordances | BrowseSeries.vue:30-32 + BrowseBook.vue:22-24 | visible UI is not a PATCH/DELETE ownership claim",
        "SSE/live-refresh parity | BrowseBook.vue:457-514 + /sse/v1/events | explicit non-native",
    ]);

    assert_eq!(expected, frozen_named_exclusion_proofs());

    let config = HarnessConfig::load_default().expect("default compat cases should load");
    for id in [
        "P3-DETAIL-EXCLUDED-BOOK-PAGES",
        "P3-DETAIL-EXCLUDED-BOOK-FILE",
        "P3-DETAIL-EXCLUDED-BOOK-THUMBNAIL",
        "P3-DETAIL-EXCLUDED-BOOK-MANIFEST",
        "P3-DETAIL-EXCLUDED-BOOK-RESOURCE",
        "P3-DETAIL-EXCLUDED-BOOK-POSITIONS",
        "P3-DETAIL-EXCLUDED-SERIES-THUMBNAIL",
        "P3-DETAIL-EXCLUDED-SERIES-FILE",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-BOOKS",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-PREVIOUS",
        "P3-DETAIL-EXCLUDED-READLIST-CONTEXT-NEXT",
        "P3-DETAIL-EXCLUDED-ONESHOT-ROUTE-CLOSURE",
        "P3-DETAIL-EXCLUDED-READ-PROGRESS-PATCH",
        "P3-DETAIL-EXCLUDED-READ-PROGRESS-DELETE",
        "P3-DETAIL-EXCLUDED-PROGRESSION-PUT",
        "P3-DETAIL-EXCLUDED-READLIST-PATCH",
        "P3-DETAIL-EXCLUDED-READLIST-DELETE",
        "P3-DETAIL-EXCLUDED-COLLECTION-PATCH",
        "P3-DETAIL-EXCLUDED-COLLECTION-DELETE",
    ] {
        let case = config
            .cases
            .iter()
            .find(|it| it.id == id)
            .unwrap_or_else(|| panic!("missing explicit exclusion compat case: {id}"));
        assert_eq!(
            case.headers
                .as_ref()
                .and_then(|headers| headers.get("X-Komga-Compat-Search-Ownership")),
            Some(&"shadow-java-writer".to_string()),
            "named exclusion must keep explicit non-native marker: {id}",
        );
    }
}
