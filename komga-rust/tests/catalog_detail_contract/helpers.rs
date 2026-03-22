use std::collections::BTreeSet;

pub(super) fn frozen_in_scope_direct_browse_shapes() -> BTreeSet<&'static str> {
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

pub(super) fn frozen_non_native_detail_shapes() -> BTreeSet<&'static str> {
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

pub(super) fn frozen_named_exclusion_proofs() -> BTreeSet<&'static str> {
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

pub(super) fn frozen_oneshot_direct_route_shapes() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "GET /api/v1/series/{seriesId}?oneshot=true (newly-owned Phase 7 exact route)",
        "GET /api/v1/series/{seriesId} (reused pre-owned source-truth dependency)",
    ])
}

pub(super) fn frozen_oneshot_explicit_non_native_shapes() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "GET /api/v1/series/{seriesId}?oneshot=false",
        "GET /api/v1/series/{seriesId}?oneshot=true&oneshot=true",
        "GET /api/v1/series/{seriesId}?oneshot=true&foo=bar",
        "GET /api/v1/series/{seriesId}?oneShot=true",
        "GET /api/v1/series/{seriesId}?ONESHOT=true",
        "GET /api/v1/readlists/{readListId}",
        "GET /api/v1/readlists/{readListId}/books?unpaged=true",
        "GET /api/v1/readlists/{readListId}/books/{bookId}/previous",
        "GET /api/v1/readlists/{readListId}/books/{bookId}/next",
        "POST /api/v1/books/list?page={page}&size={size}&sort=metadata.numberSort,asc body=AllOfBook([SeriesId])",
        "POST /api/v1/books/list?unpaged=true&sort=metadata.numberSort,asc body=SeriesId",
        "POST /api/v1/books/list sort=readProgress.readDate,desc",
        "POST /api/v1/books/list condition=ReadStatus",
        "GET /api/v1/books/{bookId}/pages",
        "GET /api/v1/books/{bookId}/file",
        "GET /api/v1/books/{bookId}/thumbnail",
        "GET /api/v1/books/{bookId}/manifest",
        "GET /api/v1/books/{bookId}/resource/*",
        "GET /api/v1/books/{bookId}/positions",
        "PATCH /api/v1/books/{bookId}/read-progress",
        "DELETE /api/v1/books/{bookId}/read-progress",
        "PUT /api/v1/books/{bookId}/progression",
        "PATCH /api/v1/readlists/{readListId}",
        "DELETE /api/v1/readlists/{readListId}",
        "PATCH /api/v1/collections/{collectionId}",
        "DELETE /api/v1/collections/{collectionId}",
        "SSE/live-refresh parity",
    ])
}

pub(super) fn frozen_oneshot_named_exclusion_proofs() -> BTreeSet<&'static str> {
    BTreeSet::from([
        "oneshot query boundary | BrowseOneshot.vue:779-800 | only exact GET /api/v1/series/{seriesId}?oneshot=true is native in Phase 7; oneshot=false remains explicit non-native",
        "oneshot query boundary | BrowseOneshot.vue:779-800 | duplicate oneshot params remain explicit non-native",
        "oneshot query boundary | BrowseOneshot.vue:779-800 | mixed extra params remain explicit non-native",
        "oneshot query boundary | BrowseOneshot.vue:779-800 | case-variant param names remain explicit non-native",
        "READLIST detail/list-family boundary | BrowseOneshot.vue:785-842 | readlist detail/list/context siblings stay explicit non-native in Phase 7",
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
    ])
}
