use komga_compat_testkit::cases::HarnessConfig;
use std::collections::BTreeSet;

#[test]
fn in_scope_direct_browse_shapes_are_frozen() {
    let expected = BTreeSet::from([
        "GET /api/v1/series/{seriesId}",
        "GET /api/v1/series/{seriesId}/collections",
        "POST /api/v1/books/list?page={page}&size={size}&sort=metadata.numberSort,asc body=AllOfBook([SeriesId])",
        "POST /api/v1/books/list?unpaged=true&sort=metadata.numberSort,asc body=SeriesId",
        "GET /api/v1/books/{bookId}",
        "GET /api/v1/books/{bookId}/previous",
        "GET /api/v1/books/{bookId}/next",
        "GET /api/v1/books/{bookId}/readlists",
    ]);

    assert_eq!(expected, frozen_in_scope_direct_browse_shapes());

    let config = HarnessConfig::load_default().expect("default compat cases should load");
    for id in [
        "P3-DETAIL-SERIES-DETAIL-OWNED",
        "P3-DETAIL-SERIES-COLLECTIONS-OWNED",
        "P3-DETAIL-BOOKS-LIST-PAGED-SERIES-OWNED",
        "P3-DETAIL-BOOKS-LIST-UNPAGED-SIBLINGS-OWNED",
        "P3-DETAIL-BOOK-DETAIL-OWNED",
        "P3-DETAIL-BOOK-PREVIOUS-OWNED",
        "P3-DETAIL-BOOK-NEXT-OWNED",
        "P3-DETAIL-BOOK-READLISTS-OWNED",
    ] {
        assert!(
            config.cases.iter().any(|it| it.id == id),
            "missing detail owned compat case: {id}",
        );
    }
}

#[test]
fn browse_series_books_list_shape_is_frozen() {
    let expected = BTreeSet::from([
        "POST /api/v1/books/list?page={page}&size={size}&sort=metadata.numberSort,asc body=AllOfBook([SeriesId])",
        "POST /api/v1/books/list?unpaged=true&sort=metadata.numberSort,asc body=SeriesId",
    ]);

    let actual = frozen_in_scope_direct_browse_shapes()
        .into_iter()
        .filter(|shape| shape.starts_with("POST /api/v1/books/list"))
        .collect::<BTreeSet<_>>();

    assert_eq!(expected, actual);

    let config = HarnessConfig::load_default().expect("default compat cases should load");
    for id in [
        "P3-DETAIL-BOOKS-LIST-PAGED-SERIES-OWNED",
        "P3-DETAIL-BOOKS-LIST-UNPAGED-SIBLINGS-OWNED",
    ] {
        assert!(
            config.cases.iter().any(|it| it.id == id),
            "missing detail books/list owned compat case: {id}",
        );
    }
}

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
        "GET /api/v1/series/{seriesId}?oneshot=true",
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

fn frozen_in_scope_direct_browse_shapes() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "GET /api/v1/series/{seriesId}",
        "GET /api/v1/series/{seriesId}/collections",
        "POST /api/v1/books/list?page={page}&size={size}&sort=metadata.numberSort,asc body=AllOfBook([SeriesId])",
        "POST /api/v1/books/list?unpaged=true&sort=metadata.numberSort,asc body=SeriesId",
        "GET /api/v1/books/{bookId}",
        "GET /api/v1/books/{bookId}/previous",
        "GET /api/v1/books/{bookId}/next",
        "GET /api/v1/books/{bookId}/readlists",
    ])
}

fn frozen_non_native_detail_shapes() -> BTreeSet<&'static str> {
    BTreeSet::from([
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
        "GET /api/v1/series/{seriesId}?oneshot=true",
        "PATCH /api/v1/books/{bookId}/read-progress",
        "DELETE /api/v1/books/{bookId}/read-progress",
        "PUT /api/v1/books/{bookId}/progression",
        "PATCH /api/v1/readlists/{readListId}",
        "DELETE /api/v1/readlists/{readListId}",
        "PATCH /api/v1/collections/{collectionId}",
        "DELETE /api/v1/collections/{collectionId}",
        "POST /api/v1/books/list sort=readProgress.readDate,desc",
        "POST /api/v1/books/list condition=ReadStatus",
    ])
}

fn frozen_named_exclusion_proofs() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "browse-oneshot closure | BrowseSeries.vue:996-1004 + BrowseBook.vue:630-645 | visible redirect only, not direct-detail native ownership",
        "READLIST context routing | BrowseBook.vue:650-680 -> /api/v1/readlists/{readListId}/books?unpaged=true + sibling previous/next | explicit non-native",
        "media delivery | /api/v1/books/{bookId}/pages + /pages/{pageNumber} + /pages/{pageNumber}/thumbnail + /thumbnail | explicit non-native",
        "reader handoff + download URLs | BrowseBook.vue:199-222 + 531-550 and BrowseSeries.vue:195-203 + 745-749 -> fileUrl/readRouteName adjacent branches stay explicit non-native",
        "read-progress mutation + progression | /api/v1/books/{bookId}/read-progress (PATCH/DELETE) + /api/v1/books/{bookId}/progression | explicit non-native",
        "collection/readlist removal actions | BrowseSeries.vue:448-457 + BrowseBook.vue:347-356 -> PATCH/DELETE collection/readlist routes stay non-native",
        "admin edit/delete affordances | BrowseSeries.vue:30-32 + BrowseBook.vue:22-24 | visible UI is not a PATCH/DELETE ownership claim",
        "SSE/live-refresh parity | BrowseBook.vue:457-514 + /sse/v1/events | explicit non-native",
    ])
}
