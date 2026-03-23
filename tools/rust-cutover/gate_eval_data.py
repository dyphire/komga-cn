from pathlib import Path


PHASE2_DISCOVERY_LABEL = "phase2-catalog-discovery"
PHASE3_DETAIL_READ_LABEL = "phase3-detail-read"
PHASE4_READLIST_CONTEXT_READ_LABEL = "phase4-readlist-context-read"
PHASE5_ONESHOT_CLOSURE_LABEL = "phase5-oneshot-closure"
PHASE6_ONESHOT_READLIST_CONTEXT_CLOSURE_LABEL = "phase6-oneshot-readlist-context-closure"
PHASE7_SERIES_ONESHOT_QUERY_CLOSURE_LABEL = "phase7-series-oneshot-query-closure"
PHASE8_READLIST_BOOKS_FAMILY_CLOSURE_LABEL = "phase8-readlist-books-family-closure"
PHASE9_READLISTS_LIST_BROWSE_CLOSURE_LABEL = "phase9-readlists-list-browse-closure"
PHASE10_READLISTS_SEARCH_CLOSURE_LABEL = "phase10-readlists-search-closure"

discovery_supported_scope = [
    "GET /api/v1/libraries",
    "POST /api/v1/series/list",
    "POST /api/v1/books/list",
    "GET /api/v1/books/latest",
]
discovery_out_of_slice = [
    "detail endpoints (/api/v1/series/{id}, /api/v1/books/{id})",
    "pages",
    "binary/file/thumbnail delivery",
    "read-progress",
    "write paths",
]
phase3_detail_supported_scope = [
    "GET /api/v1/series/{seriesId}",
    "GET /api/v1/series/{seriesId}/collections",
    "POST /api/v1/books/list (direct-browse page-scoped families only)",
    "GET /api/v1/books/{bookId}",
    "GET /api/v1/books/{bookId}/previous",
    "GET /api/v1/books/{bookId}/next",
    "GET /api/v1/books/{bookId}/readlists",
]
phase3_detail_out_of_slice = [
    "whole cutover/direct-serving approval",
    "media delivery (/thumbnail, /file, /pages*, /manifest, /resource/*, /positions)",
    "contextual READLIST closure",
    "oneshot closure",
    "read-progress write/progression routes",
    "write-path and mutation claims",
]
phase4_readlist_context_supported_scope = [
    "GET /api/v1/readlists/{readlistId}/books?unpaged=true",
    "GET /api/v1/readlists/{readlistId}/books/{bookId}/previous",
    "GET /api/v1/readlists/{readlistId}/books/{bookId}/next",
]
phase4_readlist_context_out_of_slice = [
    "paged readlist books variants",
    "library_id readlist-context variants",
    "readlist list/detail routes",
    "media delivery (/thumbnail, /file, /pages*, /manifest, /resource/*, /positions)",
    "read-progress write/progression routes",
    "oneshot closure",
    "reader handoff and download branches",
    "SSE/live-refresh parity",
    "collection/readlist removals",
    "admin edit/delete and broader write-path claims",
    "full cutover/direct-serving approval",
]
phase5_oneshot_owned_scope = [
    "POST /api/v1/books/list (oneshot-bootstrap SeriesId-only family for direct /oneshot/:seriesId closure)",
]
phase5_oneshot_pre_owned_dependencies = [
    "GET /api/v1/series/{seriesId}",
    "GET /api/v1/series/{seriesId}/collections",
    "GET /api/v1/books/{bookId}/readlists",
]
phase5_oneshot_out_of_slice = [
    "GET /api/v1/series/{seriesId}?oneshot=true",
    "READLIST-context fallback and readlist detail/list/next/previous branches",
    "generic books/list widening beyond oneshot-bootstrap SeriesId-only",
    "media delivery (/thumbnail, /file, /pages*, /manifest, /resource/*, /positions)",
    "reader handoff and download branches",
    "read-progress write/progression routes",
    "collection/readlist removals",
    "admin edit/delete and broader write-path claims",
    "SSE/live-refresh parity",
    "full cutover/direct-serving approval",
]
phase6_oneshot_readlist_context_owned_scope = [
    "GET /api/v1/readlists/{readListId} (oneshot READLIST-context direct-read closure only)",
]
phase6_oneshot_readlist_context_pre_owned_dependencies = [
    "GET /api/v1/series/{seriesId}",
    "GET /api/v1/series/{seriesId}/collections",
    "POST /api/v1/books/list (exact oneshot-bootstrap SeriesId-only family)",
    "GET /api/v1/books/{bookId}/readlists",
    "GET /api/v1/readlists/{readListId}/books?unpaged=true",
    "GET /api/v1/readlists/{readListId}/books/{bookId}/previous",
    "GET /api/v1/readlists/{readListId}/books/{bookId}/next",
]
phase6_oneshot_readlist_context_out_of_slice = [
    "GET /api/v1/series/{seriesId}?oneshot=true",
    "GET /api/v1/readlists and other readlist list-family routes",
    "paged or library_id readlist books variants",
    "browse-readlist page closure",
    "generic books/list widening beyond the exact oneshot-bootstrap SeriesId-only family",
    "media delivery (/thumbnail, /file, /pages*, /manifest, /resource/*, /positions)",
    "reader handoff and download branches",
    "read-progress write/progression routes",
    "collection/readlist removals",
    "admin edit/delete and broader write-path claims",
    "SSE/live-refresh parity",
    "full cutover/direct-serving approval",
]
phase7_series_oneshot_query_owned_scope = [
    "GET /api/v1/series/{seriesId}?oneshot=true",
]
phase7_series_oneshot_query_pre_owned_dependencies = [
    "GET /api/v1/series/{seriesId}",
    "GET /api/v1/series/{seriesId}/collections",
    "POST /api/v1/books/list (exact oneshot-bootstrap SeriesId-only family)",
    "GET /api/v1/books/{bookId}/readlists",
    "GET /api/v1/readlists/{readListId}",
    "GET /api/v1/readlists/{readListId}/books?unpaged=true",
    "GET /api/v1/readlists/{readListId}/books/{bookId}/previous",
    "GET /api/v1/readlists/{readListId}/books/{bookId}/next",
]
phase7_series_oneshot_query_out_of_slice = [
    "negative/mixed oneshot query variants beyond exact ?oneshot=true",
    "browse-oneshot page closure or browser-owned inventory promotion",
    "GET /api/v1/readlists and other readlist list-family routes",
    "browse-readlist page closure",
    "paged or library_id readlist books variants",
    "generic books/list widening beyond the exact oneshot-bootstrap SeriesId-only family",
    "media delivery (/thumbnail, /file, /pages*, /manifest, /resource/*, /positions)",
    "reader handoff and download branches",
    "read-progress write/progression routes",
    "collection/readlist removals",
    "admin edit/delete and broader write-path claims",
    "SSE/live-refresh parity",
    "full cutover/direct-serving approval",
]
phase8_readlist_books_family_owned_scope = [
    "GET /api/v1/readlists/{readListId}/books (default paged; unpaged omitted)",
    "GET /api/v1/readlists/{readListId}/books?page={page}&size={size}",
    "GET /api/v1/readlists/{readListId}/books?library_id={libraryId}",
    "GET /api/v1/readlists/{readListId}/books?read_status={status}",
    "GET /api/v1/readlists/{readListId}/books?media_status={status}",
    "GET /api/v1/readlists/{readListId}/books?tag={tag} (including repeated tag)",
    "GET /api/v1/readlists/{readListId}/books?author={name,role} (including repeated author)",
    "GET /api/v1/readlists/{readListId}/books?deleted={true|false}",
    "GET /api/v1/readlists/{readListId}/books with supported filter combinations + default paging or explicit page/size",
    "GET /api/v1/readlists/{readListId}/books?unpaged=false",
]
phase8_readlist_books_family_pre_owned_dependencies = [
    "GET /api/v1/readlists/{readListId} (Phase 6, regression-only here)",
    "GET /api/v1/readlists/{readListId}/books?unpaged=true (Phase 4, regression-only here)",
    "GET /api/v1/readlists/{readListId}/books/{bookId}/previous (Phase 4, regression-only here)",
    "GET /api/v1/readlists/{readListId}/books/{bookId}/next (Phase 4, regression-only here)",
    "GET /api/v1/books/{bookId}/readlists (Phase 3, regression-only here)",
]
phase8_readlist_books_family_out_of_slice = [
    "GET /api/v1/readlists and every list-family variant (search/unpaged/paging/library filters)",
    "GET /api/v1/readlists/{readListId}/read-progress/tachiyomi",
    "readlist write/admin routes (POST/PATCH/DELETE, thumbnail mutation, ComicRack match/import, file/download)",
    "media delivery (/thumbnail, /file, /pages*, /manifest, /resource/*, /positions)",
    "reader handoff and download branches",
    "read-progress/progression write routes",
    "collection/readlist removals and admin-write flows",
    "SSE/live-refresh parity",
    "whole cutover/direct-serving approval",
]
phase9_readlists_list_browse_owned_scope = [
    "GET /api/v1/readlists (default browse; search/unpaged/explicit sort omitted)",
    "GET /api/v1/readlists?page={page}&size={size} (explicit browse paging)",
    "GET /api/v1/readlists?page={page}&size=0 (matches JVM exactly)",
    "GET /api/v1/readlists?library_id={libraryId} (single or repeated library_id; default paging)",
    "GET /api/v1/readlists?library_id={libraryId...}&page={page}&size={size} (single or repeated library_id with explicit browse paging)",
]
phase9_readlists_list_browse_pre_owned_dependencies = [
    "GET /api/v1/readlists/{readListId} (Phase 6, regression-only here)",
    "GET /api/v1/readlists/{readListId}/books?unpaged=true (Phase 4, regression-only here)",
    "GET /api/v1/readlists/{readListId}/books/{bookId}/previous (Phase 4, regression-only here)",
    "GET /api/v1/readlists/{readListId}/books/{bookId}/next (Phase 4, regression-only here)",
    "Direct paged/filter GET /api/v1/readlists/{readListId}/books family (Phase 8, regression-only here)",
]
phase9_readlists_list_browse_out_of_slice = [
    "GET /api/v1/readlists?search={term} and every search-bearing variant",
    "GET /api/v1/readlists?unpaged=true",
    "GET /api/v1/readlists?unpaged=false",
    "GET /api/v1/readlists?sort=... and every explicit custom-sort/relevance variant",
    "mixed variants that add search/unpaged/explicit sort onto the Phase 9 browse subset",
    "GET /api/v1/readlists/{readListId}/read-progress/tachiyomi",
    "readlist dialogs/import/edit/delete admin actions",
    "write/media/reader/SSE/whole-cutover claims",
]
phase10_readlists_search_owned_scope = [
    "GET /api/v1/readlists?search={non-blank}",
    "GET /api/v1/readlists?search={non-blank}&page={page}&size={size}",
    "GET /api/v1/readlists?search={non-blank}&library_id={libraryId} (single or repeated library_id)",
    "GET /api/v1/readlists?search={non-blank}&library_id={libraryId...}&page={page}&size={size}",
    "GET /api/v1/readlists?search={non-blank}&size=0 (matches JVM exactly)",
    "GET /api/v1/readlists?search={non-blank}&library_id={libraryId...}&size=0 (matches JVM exactly)",
]
phase10_readlists_search_pre_owned_dependencies = [
    "Phase 2 catalog discovery read slice",
    "Phase 3 detail read slice",
    "Phase 4 readlist-context read",
    "Phase 5 oneshot closure",
    "Phase 6 oneshot readlist-context closure",
    "Phase 7 series oneshot query closure",
    "Phase 8 readlist books family closure",
    "Phase 9 readlists list browse closure (default/page-size/repeated-library/size=0 browse only)",
]
phase10_readlists_search_out_of_slice = [
    "GET /api/v1/readlists?search=",
    "GET /api/v1/readlists?search=%20%20",
    "GET /api/v1/readlists?search={non-blank}&sort=...",
    "GET /api/v1/readlists?search={non-blank}&unpaged=true",
    "GET /api/v1/readlists?search={non-blank}&page=...&page=...",
    "GET /api/v1/readlists?search={non-blank}&size=...&size=...",
    "GET /api/v1/readlists?search={non-blank}&foo=bar",
    "browse-only Phase 9 GET /api/v1/readlists shapes remain governed by Phase 9 rather than promoted into Phase 10",
    "readlist dialogs/import/edit/delete admin actions",
    "write/media/reader/SSE/whole-cutover claims",
]

phase3_skipped_base_checks: dict[str, str] = {
    "auth_api_key": "Skipped for phase3-detail-read: API key parity is outside this direct-browse detail-read runbook.",
    "libraries_visibility": "Skipped for phase3-detail-read: discovery libraries parity is not part of this detail-read slice gate.",
    "opds": "Skipped for phase3-detail-read: OPDS parity is outside this detail-read slice gate.",
    "cache_file_headers": "Skipped for phase3-detail-read: binary metadata/header parity is outside this detail-read slice gate.",
    "read_progress": "Skipped for phase3-detail-read: this runbook does not claim read-progress write/progression ownership.",
    "server_management_browser_smoke": "Skipped for phase3-detail-read: server-management/browser-ops acceptance is outside this direct-browse detail slice.",
    "packaging_tray": "Skipped for phase3-detail-read: packaging/tray startup contract is outside this slice gate.",
    "external_release_credentials": "Skipped for phase3-detail-read: release credentials are not part of direct-browse detail-read readiness.",
}

phase4_skipped_base_checks: dict[str, str] = {
    "auth_api_key": "Skipped for phase4-readlist-context-read: API key parity is outside this readlist-context runbook.",
    "libraries_visibility": "Skipped for phase4-readlist-context-read: discovery libraries parity is not part of this readlist-context slice gate.",
    "opds": "Skipped for phase4-readlist-context-read: OPDS parity is outside this readlist-context slice gate.",
    "cache_file_headers": "Skipped for phase4-readlist-context-read: binary metadata/header parity is outside this readlist-context slice gate.",
    "read_progress": "Skipped for phase4-readlist-context-read: this runbook does not claim read-progress write/progression ownership.",
    "server_management_browser_smoke": "Skipped for phase4-readlist-context-read: server-management/browser-ops acceptance is outside this readlist-context slice.",
    "packaging_tray": "Skipped for phase4-readlist-context-read: packaging/tray startup contract is outside this slice gate.",
    "external_release_credentials": "Skipped for phase4-readlist-context-read: release credentials are not part of readlist-context-read readiness.",
}

phase5_skipped_base_checks: dict[str, str] = {
    "auth_api_key": "Skipped for phase5-oneshot-closure: API key parity is outside this oneshot-closure runbook.",
    "libraries_visibility": "Skipped for phase5-oneshot-closure: discovery libraries parity is not part of this oneshot-closure slice gate.",
    "opds": "Skipped for phase5-oneshot-closure: OPDS parity is outside this oneshot-closure slice gate.",
    "cache_file_headers": "Skipped for phase5-oneshot-closure: binary metadata/header parity is outside this oneshot-closure slice gate.",
    "read_progress": "Skipped for phase5-oneshot-closure: this runbook does not claim read-progress write/progression ownership.",
    "server_management_browser_smoke": "Skipped for phase5-oneshot-closure: server-management/browser-ops acceptance is outside this oneshot-closure slice.",
    "packaging_tray": "Skipped for phase5-oneshot-closure: packaging/tray startup contract is outside this slice gate.",
    "external_release_credentials": "Skipped for phase5-oneshot-closure: release credentials are not part of oneshot-closure readiness.",
}

phase6_skipped_base_checks: dict[str, str] = {
    "auth_api_key": "Skipped for phase6-oneshot-readlist-context-closure: API key parity is outside this oneshot READLIST-context direct-read runbook.",
    "libraries_visibility": "Skipped for phase6-oneshot-readlist-context-closure: discovery libraries parity is not part of this oneshot READLIST-context direct-read slice gate.",
    "opds": "Skipped for phase6-oneshot-readlist-context-closure: OPDS parity is outside this oneshot READLIST-context direct-read slice gate.",
    "cache_file_headers": "Skipped for phase6-oneshot-readlist-context-closure: binary metadata/header parity is outside this oneshot READLIST-context direct-read slice gate.",
    "read_progress": "Skipped for phase6-oneshot-readlist-context-closure: this runbook does not claim read-progress write/progression ownership.",
    "server_management_browser_smoke": "Skipped for phase6-oneshot-readlist-context-closure: server-management/browser-ops acceptance is outside this oneshot READLIST-context direct-read slice.",
    "packaging_tray": "Skipped for phase6-oneshot-readlist-context-closure: packaging/tray startup contract is outside this slice gate.",
    "external_release_credentials": "Skipped for phase6-oneshot-readlist-context-closure: release credentials are not part of oneshot READLIST-context direct-read readiness.",
}
phase7_skipped_base_checks: dict[str, str] = {
    "auth_api_key": "Skipped for phase7-series-oneshot-query-closure: API key parity is outside this exact series-detail query runbook.",
    "libraries_visibility": "Skipped for phase7-series-oneshot-query-closure: discovery libraries parity is not part of this exact series-detail query slice gate.",
    "opds": "Skipped for phase7-series-oneshot-query-closure: OPDS parity is outside this exact series-detail query slice gate.",
    "cache_file_headers": "Skipped for phase7-series-oneshot-query-closure: binary metadata/header parity is outside this exact series-detail query slice gate.",
    "read_progress": "Skipped for phase7-series-oneshot-query-closure: this runbook does not claim read-progress write/progression ownership.",
    "server_management_browser_smoke": "Skipped for phase7-series-oneshot-query-closure: server-management/browser-ops acceptance is outside this exact series-detail query slice.",
    "packaging_tray": "Skipped for phase7-series-oneshot-query-closure: packaging/tray startup contract is outside this slice gate.",
    "external_release_credentials": "Skipped for phase7-series-oneshot-query-closure: release credentials are not part of exact series-detail query readiness.",
}
phase8_skipped_base_checks: dict[str, str] = {
    "auth_api_key": "Skipped for phase8-readlist-books-family-closure: API key parity is outside this readlist-books family runbook.",
    "libraries_visibility": "Skipped for phase8-readlist-books-family-closure: discovery libraries parity is not part of this direct readlist-books family slice gate.",
    "opds": "Skipped for phase8-readlist-books-family-closure: OPDS parity is outside this direct readlist-books family slice gate.",
    "cache_file_headers": "Skipped for phase8-readlist-books-family-closure: binary metadata/header parity is outside this direct readlist-books family slice gate.",
    "read_progress": "Skipped for phase8-readlist-books-family-closure: this runbook does not claim read-progress/progression ownership.",
    "server_management_browser_smoke": "Skipped for phase8-readlist-books-family-closure: server-management/browser-ops acceptance is outside this direct readlist-books family slice.",
    "packaging_tray": "Skipped for phase8-readlist-books-family-closure: packaging/tray startup contract is outside this slice gate.",
    "external_release_credentials": "Skipped for phase8-readlist-books-family-closure: release credentials are not part of direct readlist-books family readiness.",
}
phase9_skipped_base_checks: dict[str, str] = {
    "auth_api_key": "Skipped for phase9-readlists-list-browse-closure: API key parity is outside this readlists browse/list runbook.",
    "libraries_visibility": "Skipped for phase9-readlists-list-browse-closure: discovery libraries parity is not part of this browse/list slice gate.",
    "opds": "Skipped for phase9-readlists-list-browse-closure: OPDS parity is outside this browse/list slice gate.",
    "cache_file_headers": "Skipped for phase9-readlists-list-browse-closure: binary metadata/header parity is outside this browse/list slice gate.",
    "read_progress": "Skipped for phase9-readlists-list-browse-closure: this runbook does not claim Tachiyomi/read-progress ownership.",
    "server_management_browser_smoke": "Skipped for phase9-readlists-list-browse-closure: server-management/browser-ops acceptance is outside this browse/list slice.",
    "packaging_tray": "Skipped for phase9-readlists-list-browse-closure: packaging/tray startup contract is outside this slice gate.",
    "external_release_credentials": "Skipped for phase9-readlists-list-browse-closure: release credentials are not part of browse/list closure readiness.",
}
phase10_skipped_base_checks: dict[str, str] = {
    "auth_api_key": "Skipped for phase10-readlists-search-closure: API key parity is outside this readlists search slice gate.",
    "libraries_visibility": "Skipped for phase10-readlists-search-closure: discovery libraries parity is not part of this readlists search slice gate.",
    "opds": "Skipped for phase10-readlists-search-closure: OPDS parity is outside this readlists search slice gate.",
    "cache_file_headers": "Skipped for phase10-readlists-search-closure: binary metadata/header parity is outside this readlists search slice gate.",
    "read_progress": "Skipped for phase10-readlists-search-closure: this runbook does not claim Tachiyomi/read-progress ownership.",
    "server_management_browser_smoke": "Skipped for phase10-readlists-search-closure: server-management/browser-ops acceptance is outside this readlists search slice.",
    "packaging_tray": "Skipped for phase10-readlists-search-closure: packaging/tray startup contract is outside this slice gate.",
    "external_release_credentials": "Skipped for phase10-readlists-search-closure: release credentials are not part of readlists search closure readiness.",
}


def build_checks(
    run_label: str,
    evidence_root: Path,
) -> tuple[
    list[dict[str, object]],
    list[dict[str, object]],
    list[dict[str, object]],
    list[dict[str, object]],
    list[dict[str, object]],
    list[dict[str, object]],
    list[dict[str, object]],
    list[dict[str, object]],
    list[dict[str, object]],
]:
    is_phase2_discovery = run_label == PHASE2_DISCOVERY_LABEL
    is_phase3_detail_read = run_label == PHASE3_DETAIL_READ_LABEL
    is_phase4_readlist_context_read = run_label == PHASE4_READLIST_CONTEXT_READ_LABEL
    is_phase5_oneshot_closure = run_label == PHASE5_ONESHOT_CLOSURE_LABEL
    is_phase6_oneshot_readlist_context_closure = run_label == PHASE6_ONESHOT_READLIST_CONTEXT_CLOSURE_LABEL
    is_phase7_series_oneshot_query_closure = run_label == PHASE7_SERIES_ONESHOT_QUERY_CLOSURE_LABEL
    is_phase8_readlist_books_family_closure = run_label == PHASE8_READLIST_BOOKS_FAMILY_CLOSURE_LABEL
    is_phase9_readlists_list_browse_closure = run_label == PHASE9_READLISTS_LIST_BROWSE_CLOSURE_LABEL
    is_phase10_readlists_search_closure = run_label == PHASE10_READLISTS_SEARCH_CLOSURE_LABEL

    base_checks = [
        {
            "id": "auth_api_key",
            "category": "compat",
            "refusal_condition": "Auth/API key parity not proven",
            "evidence": [
                evidence_root / "task-9-api-key" / "api-key-diff.txt",
            ],
            "mode": "text",
        },
        {
            "id": "libraries_visibility",
            "category": "compat",
            "refusal_condition": "Libraries visibility parity not proven",
            "evidence": [
                evidence_root / "task-7-libraries" / "library-diff.txt",
            ],
            "mode": "text",
        },
        {
            "id": "opds",
            "category": "compat",
            "refusal_condition": "OPDS parity not proven",
            "evidence": [
                evidence_root / "task-6-opds-v1" / "opds-v1-diff.txt",
            ],
            "mode": "text",
        },
        {
            "id": "cache_file_headers",
            "category": "compat",
            "refusal_condition": "Cache/file-header parity not proven",
            "evidence": [
                evidence_root / "task-8-query-cache" / "binary-metadata-diff.txt",
            ],
            "mode": "text",
        },
        {
            "id": "read_progress",
            "category": "compat",
            "refusal_condition": "Read-progress parity not proven",
            "evidence": [
                evidence_root / "task-10-read-progress" / "read-progress-diff.txt",
            ],
            "mode": "text",
        },
        {
            "id": "search_ownership",
            "category": "ownership",
            "refusal_condition": "Search ownership (Java-only writer in shadow/canary) not proven",
            "evidence": [
                evidence_root / "task-12-search" / "rust-search-parity.txt",
                evidence_root / "task-12-search" / "java-search-lifecycle.txt",
            ],
            "mode": "text",
        },
        {
            "id": "task_ownership",
            "category": "ownership",
            "refusal_condition": "Task queue ownership/scheduler guardrails not proven",
            "evidence": [
                evidence_root / "task-13-tasks" / "task-ownership.txt",
                evidence_root / "task-13-tasks" / "admin-task-queue.json",
            ],
            "mode": "task_ownership",
        },
        {
            "id": "server_management_browser_smoke",
            "category": "browser-ops",
            "refusal_condition": "Server management/browser smoke acceptance slice not passing",
            "evidence": [
                evidence_root / "task-11-browser-smoke" / "summary.json",
                evidence_root / "task-14-ops" / "server-management.json",
            ],
            "mode": "browser_ops",
        },
        {
            "id": "packaging_tray",
            "category": "distribution",
            "refusal_condition": "Packaging/tray startup contract not proven",
            "evidence": [
                evidence_root / "task-15-packaging" / "runtime-startup.txt",
                evidence_root / "task-15-packaging" / "tray-compat.txt",
            ],
            "mode": "packaging",
        },
        {
            "id": "shadow_safety",
            "category": "governance",
            "refusal_condition": "Shadow safety single-writer guardrail not proven",
            "evidence": [
                evidence_root / "task-3-shadow-governance" / "shadow-safety.txt",
                evidence_root / "task-3-shadow-governance" / "config-precedence.txt",
            ],
            "mode": "text",
        },
        {
            "id": "external_release_credentials",
            "category": "distribution",
            "refusal_condition": "External release credentials unavailable for packaging/release",
            "evidence": [],
            "mode": "credential",
            "profile_overrides": {
                PHASE2_DISCOVERY_LABEL: {
                    "status": "skipped",
                    "blocking": False,
                    "details": [
                        "Skipped for the phase2-catalog-discovery shadow runbook: packaging/release credentials are not part of this slice gate.",
                        "This label does not claim direct-serving, release, or whole-cutover readiness.",
                        "External release credentials still have to be proven before any broader cutover/release claim.",
                    ],
                },
            },
        },
    ]

    for check in base_checks:
        check_id = str(check["id"])
        if check_id in phase7_skipped_base_checks:
            profile_overrides = check.setdefault("profile_overrides", {})
            profile_overrides[PHASE7_SERIES_ONESHOT_QUERY_CLOSURE_LABEL] = {
                "status": "skipped",
                "blocking": False,
                "details": [
                    phase7_skipped_base_checks[check_id],
                    "This label proves exact `GET /api/v1/series/{seriesId}?oneshot=true` readiness only and does not approve browse/readlist/media/write/whole-cutover scope.",
                ],
            }
        if check_id in phase8_skipped_base_checks:
            profile_overrides = check.setdefault("profile_overrides", {})
            profile_overrides[PHASE8_READLIST_BOOKS_FAMILY_CLOSURE_LABEL] = {
                "status": "skipped",
                "blocking": False,
                "details": [
                    phase8_skipped_base_checks[check_id],
                    "This label proves direct readlist-books paged/filter closure readiness only and does not approve readlist list-family/Tachiyomi/admin/media/write/whole-cutover scope.",
                ],
            }
        if check_id in phase9_skipped_base_checks:
            profile_overrides = check.setdefault("profile_overrides", {})
            profile_overrides[PHASE9_READLISTS_LIST_BROWSE_CLOSURE_LABEL] = {
                "status": "skipped",
                "blocking": False,
                "details": [
                    phase9_skipped_base_checks[check_id],
                    "This label proves readlists browse/list closure readiness only and does not approve search/dialog/admin/Tachiyomi/media/write/whole-cutover scope.",
                ],
            }
        if check_id in phase10_skipped_base_checks:
            profile_overrides = check.setdefault("profile_overrides", {})
            profile_overrides[PHASE10_READLISTS_SEARCH_CLOSURE_LABEL] = {
                "status": "skipped",
                "blocking": False,
                "details": [
                    phase10_skipped_base_checks[check_id],
                    "This label proves readlists non-blank search closure readiness only and does not approve browse-only Phase 9 promotion, blank-search/sort/unpaged ownership, admin/Tachiyomi/media/write, or whole-cutover scope.",
                ],
            }

    discovery_checks: list[dict[str, object]] = []
    if is_phase2_discovery:
        discovery_inventory = evidence_root / "task-10-discovery-artifacts" / "case-inventory-discovery.txt"
        discovery_parity = evidence_root / "task-10-discovery-artifacts" / "parity-admin-user-limited-restricted.txt"
        discovery_empty = evidence_root / "task-10-discovery-artifacts" / "negative-empty-results-explicit.txt"
        discovery_unsupported = evidence_root / "task-10-discovery-artifacts" / "negative-unsupported-shape-marker.txt"

        discovery_checks = [
            {
                "id": "discovery_libraries_parity",
                "category": "discovery-slice",
                "refusal_condition": "Discovery slice libraries parity not proven",
                "evidence": [discovery_inventory, discovery_parity],
                "mode": "discovery_markers",
                "marker_map": {
                    discovery_inventory: ["P2-DISCOVERY-PARITY-LIBRARIES-ADMIN"],
                    discovery_parity: ["admin/user/limited/restricted", "discovery-owned routes"],
                },
                "success_note": "Libraries parity stays anchored to dedicated discovery inventory plus cross-principal parity evidence.",
            },
            {
                "id": "discovery_series_parity",
                "category": "discovery-slice",
                "refusal_condition": "Discovery slice series/list-search parity not proven",
                "evidence": [discovery_parity, discovery_unsupported],
                "mode": "discovery_markers",
                "marker_map": {
                    discovery_parity: ["admin/user/limited/restricted", "discovery-owned routes"],
                    discovery_unsupported: ["unsupported discovery shapes emit explicit non-native marker"],
                },
                "success_note": "Series/list-search remains shadow-ready only while unsupported series shapes stay explicitly non-native.",
            },
            {
                "id": "discovery_books_list_parity",
                "category": "discovery-slice",
                "refusal_condition": "Discovery slice books/list parity not proven",
                "evidence": [discovery_parity, discovery_empty],
                "mode": "discovery_markers",
                "marker_map": {
                    discovery_parity: ["admin/user/limited/restricted", "discovery-owned routes"],
                    discovery_empty: ["empty-result negative scenarios remain explicit"],
                },
                "success_note": "Books/list parity keeps negative empty-result behavior explicit instead of silently returning mismatched data.",
            },
            {
                "id": "discovery_books_latest_parity",
                "category": "discovery-slice",
                "refusal_condition": "Discovery slice books/latest parity not proven",
                "evidence": [discovery_inventory, discovery_parity],
                "mode": "discovery_markers",
                "marker_map": {
                    discovery_inventory: ["P2-DISCOVERY-PARITY-BOOKS-LATEST-LIMITED"],
                    discovery_parity: ["admin/user/limited/restricted", "discovery-owned routes"],
                },
                "success_note": "Books/latest parity remains explicitly tracked in the dedicated discovery case inventory.",
            },
            {
                "id": "discovery_restricted_user_parity",
                "category": "discovery-slice",
                "refusal_condition": "Discovery slice restricted-user parity not proven",
                "evidence": [discovery_parity, discovery_empty],
                "mode": "discovery_markers",
                "marker_map": {
                    discovery_parity: ["admin/user/limited/restricted", "discovery-owned routes"],
                    discovery_empty: ["empty-result negative scenarios remain explicit"],
                },
                "success_note": "Restricted-user and no-result behaviors remain explicit and traceable inside discovery-scoped evidence.",
            },
            {
                "id": "discovery_unsupported_shape_explicitness",
                "category": "discovery-slice",
                "refusal_condition": "Discovery slice unsupported-shape governance not proven",
                "evidence": [discovery_inventory, discovery_unsupported],
                "mode": "discovery_markers",
                "marker_map": {
                    discovery_inventory: [
                        "P2-DISCOVERY-NEGATIVE-UNSUPPORTED-SERIES-RANDOM-SORT",
                        "P2-DISCOVERY-NEGATIVE-UNSUPPORTED-BOOK-READDATE-SORT",
                    ],
                    discovery_unsupported: ["unsupported discovery shapes emit explicit non-native marker"],
                },
                "success_note": "Unsupported discovery request shapes stay explicit and traceable instead of being omitted from governance.",
            },
        ]

    phase3_checks: list[dict[str, object]] = []
    if is_phase3_detail_read:
        detail_direct_browse_contract = evidence_root / "task-1-contract-matrix" / "direct-browse-contract.txt"
        detail_excluded_branches = evidence_root / "task-1-contract-matrix" / "excluded-branches.txt"
        detail_parity_inventory = evidence_root / "task-9-browser-smoke" / "summary.json"
        detail_browser_series = evidence_root / "task-9-browser-smoke" / "browse-series.json"
        detail_browser_book = evidence_root / "task-9-browser-smoke" / "browse-book.json"
        detail_exclusion_matrix = evidence_root / "task-8-exclusions" / "ui-visible-non-native.txt"
        detail_exclusion_verification = evidence_root / "task-8-exclusions" / "excluded-branch-markers.txt"

        phase3_checks = [
            {
                "id": "phase3_detail_contract",
                "category": "phase3-detail-read",
                "refusal_condition": "Direct-browse detail contract evidence not proven",
                "evidence": [detail_direct_browse_contract, detail_excluded_branches],
                "mode": "discovery_markers",
                "marker_map": {
                    detail_direct_browse_contract: [
                        "in_scope_direct_browse_shapes_are_frozen",
                        "Result: PASS",
                    ],
                    detail_excluded_branches: [
                        "excluded_media_context_and_write_shapes_remain_non_native",
                        "Result: PASS",
                        "shadow-java-writer",
                    ],
                },
                "success_note": "Phase3 detail gate consumes the plan-correct direct-browse contract and excluded-branch evidence without broadening default cutover semantics.",
            },
            {
                "id": "phase3_detail_parity",
                "category": "phase3-detail-read",
                "refusal_condition": "Direct-browse detail parity inventory not proven",
                "evidence": [detail_parity_inventory, detail_browser_series, detail_browser_book],
                "mode": "discovery_markers",
                "marker_map": {
                    detail_parity_inventory: ["browse-series", "browse-book", "expectedOwnedRequests"],
                    detail_browser_series: ["series-detail", "series-collections", "series-books-list"],
                    detail_browser_book: [
                        "book-detail",
                        "book-readlists",
                        "book-siblings-list",
                        "book-sibling-next",
                        "book-sibling-previous",
                    ],
                },
                "success_note": "Detail parity evidence covers series detail/collections/page-scoped books-list and book detail/prev/next/readlists owned request inventory.",
            },
            {
                "id": "phase3_detail_browser",
                "category": "phase3-detail-read",
                "refusal_condition": "Direct-browse detail browser smoke not proven",
                "evidence": [detail_parity_inventory],
                "mode": "phase3_browser_smoke",
            },
            {
                "id": "phase3_detail_exclusions",
                "category": "phase3-detail-read",
                "refusal_condition": "Detail-slice exclusion governance not proven",
                "evidence": [detail_exclusion_matrix, detail_exclusion_verification],
                "mode": "discovery_markers",
                "marker_map": {
                    detail_exclusion_matrix: [
                        "catalog_detail_contract contextual_media_and_write_branches_are_explicitly_non_native",
                        "browse-oneshot closure",
                        "READLIST context routing",
                        "pages / thumbnails / file media delivery",
                        "read-progress mutation / progression",
                        "collection / readlist removal actions",
                        "admin edit/delete affordances",
                        "SSE / live-refresh parity",
                    ],
                    detail_exclusion_verification: [
                        "catalog_detail_shadow excluded_detail_branches_emit_shadow_marker",
                        "Result: PASS",
                        "shadow-java-writer",
                    ],
                },
                "success_note": "Exclusion governance remains explicit: contextual/readlist/oneshot/media/progress-write and broader write-path claims stay non-native.",
            },
        ]

    phase4_checks: list[dict[str, object]] = []
    if is_phase4_readlist_context_read:
        readlist_context_contract = evidence_root / "task-1-contract-matrix" / "readlist-context-contract.txt"
        readlist_context_excluded = evidence_root / "task-1-contract-matrix" / "readlist-context-excluded-branches.txt"
        readlist_books_shadow = evidence_root / "task-4-shadow" / "readlist-books-runtime-ownership.txt"
        readlist_prev_next_shadow = evidence_root / "task-5-shadow" / "readlist-prev-next-runtime-ownership.txt"
        readlist_prev_next_legacy = evidence_root / "task-5-shadow" / "readlist-prev-next-legacy-parity.txt"
        readlist_browser_summary = evidence_root / "task-6-browser-smoke" / "summary.json"
        readlist_browser_readlist = evidence_root / "task-6-browser-smoke" / "browse-readlist.json"
        readlist_browser_book = evidence_root / "task-6-browser-smoke" / "browse-book.json"

        phase4_checks = [
            {
                "id": "phase4_readlist_context_contract",
                "category": "phase4-readlist-context-read",
                "refusal_condition": "Readlist-context contract evidence not proven",
                "evidence": [readlist_context_contract],
                "mode": "discovery_markers",
                "marker_map": {
                    readlist_context_contract: [
                        "in_scope_readlist_context_shapes_are_frozen",
                        "GET /api/v1/readlists/{readlistId}/books?unpaged=true",
                        "GET /api/v1/readlists/{readlistId}/books/{bookId}/previous",
                        "GET /api/v1/readlists/{readlistId}/books/{bookId}/next",
                        "Result: PASS",
                    ],
                },
                "success_note": "Phase4 readlist-context gate remains fail-closed and allows only the frozen unpaged readlist books + previous/next owned routes.",
            },
            {
                "id": "phase4_readlist_context_shadow",
                "category": "phase4-readlist-context-read",
                "refusal_condition": "Readlist-context shadow/runtime ownership evidence not proven",
                "evidence": [readlist_books_shadow, readlist_prev_next_shadow, readlist_prev_next_legacy],
                "mode": "discovery_markers",
                "marker_map": {
                    readlist_books_shadow: [
                        "readlist_books_runtime_ownership_stays_narrow",
                        "books?unpaged=true",
                        "native-rust-owned",
                        "books?page=0&size=20",
                        "books?unpaged=true&library_id=1",
                        "shadow-java-writer",
                        "_compat.discoveryOwnership = non-native",
                        "UnsupportedBookFilter(paged)",
                        "UnsupportedBookFilter(LibraryId)",
                    ],
                    readlist_prev_next_shadow: [
                        "books/book-1/previous",
                        "404 without shadow marker",
                        "books/book-1/next",
                        "Only readlist-context previous/next were newly wired to native ownership.",
                    ],
                    readlist_prev_next_legacy: [
                        "ReadListControllerTest",
                        "boundary, membership, library filtering, and unordered ordering semantics",
                        "BUILD SUCCESSFUL",
                    ],
                },
                "success_note": "Phase4 shadow evidence proves native ownership stays narrow: unpaged readlist books plus readlist previous/next only, while boundary 404s stay native-owned and excluded shapes remain shadow-java-writer.",
            },
            {
                "id": "phase4_readlist_context_browser",
                "category": "phase4-readlist-context-read",
                "refusal_condition": "Readlist-context browser smoke not proven",
                "evidence": [readlist_browser_summary, readlist_browser_readlist, readlist_browser_book],
                "mode": "phase4_browser_smoke",
            },
            {
                "id": "phase4_readlist_context_exclusions",
                "category": "phase4-readlist-context-read",
                "refusal_condition": "Readlist-context exclusion governance not proven",
                "evidence": [readlist_context_excluded],
                "mode": "discovery_markers",
                "marker_map": {
                    readlist_context_excluded: [
                        "excluded_readlist_context_and_write_shapes_remain_non_native",
                        "paged readlist books stay explicit non-native",
                        "readlist list/detail routes stay explicit non-native",
                        "library_id variants stay explicit non-native",
                        "media routes stay explicit non-native",
                        "read-progress/progression stay explicit non-native",
                        "oneshot + reader handoff stay explicit non-native",
                        "SSE stays explicit non-native",
                        "removal/admin write branches stay explicit non-native",
                        "Result: PASS",
                        "shadow-java-writer",
                    ],
                },
                "success_note": "Phase4 exclusions stay explicit: paged/library_id/context/list-detail/media/progress/oneshot/reader/SSE/removal/admin-write branches remain non-native.",
            },
        ]

    phase5_checks: list[dict[str, object]] = []
    if is_phase5_oneshot_closure:
        oneshot_contract = evidence_root / "task-1-contract-matrix" / "oneshot-contract.txt"
        oneshot_excluded_contract = evidence_root / "task-1-contract-matrix" / "oneshot-excluded-branches.txt"
        oneshot_shadow_runtime = evidence_root / "task-4-runtime" / "oneshot-runtime-ownership.txt"
        oneshot_shadow_regression = evidence_root / "task-4-runtime" / "phase3-phase4-regression.txt"
        oneshot_exclusion_visible = evidence_root / "task-5-exclusions" / "oneshot-visible-vs-owned.txt"
        oneshot_exclusion_markers = evidence_root / "task-5-exclusions" / "oneshot-excluded-branch-markers.txt"
        oneshot_browser_summary = evidence_root / "task-6-browser-smoke" / "summary.json"
        oneshot_browser_route = evidence_root / "task-6-browser-smoke" / "browse-oneshot.json"
        oneshot_browser_parity = evidence_root / "task-6-browser-smoke" / "direct-oneshot-parity.txt"

        phase5_checks = [
            {
                "id": "phase5_oneshot_closure_contract",
                "category": "phase5-oneshot-closure",
                "refusal_condition": "Oneshot closure contract evidence not proven",
                "evidence": [oneshot_contract],
                "mode": "discovery_markers",
                "marker_map": {
                    oneshot_contract: [
                        "in_scope_oneshot_closure_shapes_are_frozen",
                        "POST /api/v1/books/list",
                        "SeriesId(seriesId)",
                        "Result: PASS",
                    ],
                },
                "success_note": "Phase5 oneshot gate remains fail-closed and allows only the exact oneshot-bootstrap SeriesId-only books/list family as newly owned surface.",
            },
            {
                "id": "phase5_oneshot_closure_shadow",
                "category": "phase5-oneshot-closure",
                "refusal_condition": "Oneshot closure shadow/runtime ownership evidence not proven",
                "evidence": [oneshot_shadow_runtime, oneshot_shadow_regression],
                "mode": "discovery_markers",
                "marker_map": {
                    oneshot_shadow_runtime: [
                        "browse_oneshot_happy_path_uses_native_bootstrap_shape",
                        "oneshot-bootstrap-books-list",
                        "native-rust-owned",
                        "Result: PASS",
                    ],
                    oneshot_shadow_regression: [
                        "phase3_phase4_owned_routes_do_not_regress_with_oneshot_bootstrap",
                        "Result: PASS",
                    ],
                },
                "success_note": "Phase5 shadow evidence proves direct oneshot closure ownership without regressing phase3/phase4 owned routes.",
            },
            {
                "id": "phase5_oneshot_closure_browser",
                "category": "phase5-oneshot-closure",
                "refusal_condition": "Oneshot closure browser smoke and parity evidence not proven",
                "evidence": [oneshot_browser_summary, oneshot_browser_route, oneshot_browser_parity],
                "mode": "phase5_browser_smoke",
            },
            {
                "id": "phase5_oneshot_closure_exclusions",
                "category": "phase5-oneshot-closure",
                "refusal_condition": "Oneshot closure exclusion governance not proven",
                "evidence": [oneshot_excluded_contract, oneshot_exclusion_visible, oneshot_exclusion_markers],
                "mode": "discovery_markers",
                "marker_map": {
                    oneshot_excluded_contract: [
                        "excluded_oneshot_closure_and_adjacent_branches_remain_non_native",
                        "GET /api/v1/series/{seriesId}?oneshot=true stays explicit non-native",
                        "READLIST-context oneshot fallback stays explicit non-native",
                        "media routes stay explicit non-native",
                        "reader handoff and download branches stay explicit non-native",
                        "read-progress/progression stay explicit non-native",
                        "collection/readlist removals stay explicit non-native",
                        "admin/write branches stay explicit non-native",
                        "SSE/live-refresh parity stays explicit non-native",
                        "generic books/list widening stays explicit non-native",
                        "full cutover/direct-serving approval stays refused",
                        "Result: PASS",
                        "shadow-java-writer",
                    ],
                    oneshot_exclusion_visible: [
                        "oneshot_context_media_reader_and_write_branches_are_explicitly_non_native",
                        "visible in BrowseOneshot does not imply native ownership",
                        "READLIST context fallback remains out of slice",
                    ],
                    oneshot_exclusion_markers: [
                        "excluded_oneshot_branches_emit_shadow_marker",
                        "Result: PASS",
                        "shadow-java-writer",
                    ],
                },
                "success_note": "Phase5 exclusions stay explicit and fail-closed: ?oneshot=true, READLIST-context fallback, media/reader/progress/removal/admin/SSE branches remain refused.",
            },
        ]

    phase6_checks: list[dict[str, object]] = []
    if is_phase6_oneshot_readlist_context_closure:
        phase6_contract = evidence_root / "task-1-contract-matrix" / "phase6_readlist_detail_route_shape_is_frozen.txt"
        phase6_case_inventory = evidence_root / "task-1-contract-matrix" / "phase6_readlist_detail_case_inventory_loads.txt"
        phase6_query_contract = evidence_root / "task-2-query" / "readlist-detail-query-contract.txt"
        phase6_query_semantics = evidence_root / "task-2-query" / "readlist-detail-visible-filtered-not-found.txt"
        phase6_runtime_ownership = evidence_root / "task-3-runtime" / "phase6_readlist_detail_runtime_ownership_is_native.txt"
        phase6_runtime_semantics = evidence_root / "task-3-runtime" / "phase6_readlist_detail_404_and_filtered_semantics_match_contract.txt"
        phase6_browser_summary = evidence_root / "task-4-browser-smoke" / "summary.json"
        phase6_browser_route = evidence_root / "task-4-browser-smoke" / "browse-oneshot.json"
        phase6_browser_selectors = evidence_root / "task-4-browser-smoke" / "browse-oneshot-smoke-selectors.log"
        phase6_browser_gate_check = evidence_root / "task-4-browser-smoke" / "gate-evaluator-check.log"
        phase6_adjacent_contract = evidence_root / "task-1-contract-matrix" / "phase6_adjacent_branches_remain_explicitly_non_native.txt"
        phase6_regression = evidence_root / "task-6-regression" / "phase4-phase5-regression.txt"
        phase6_adjacent_regression = evidence_root / "task-6-regression" / "adjacent-exclusions-stay-shadow.txt"

        phase6_checks = [
            {
                "id": "phase6_oneshot_readlist_context_closure_contract",
                "category": "phase6-oneshot-readlist-context-closure",
                "refusal_condition": "Phase6 readlist-detail contract evidence not proven",
                "evidence": [phase6_contract, phase6_case_inventory],
                "mode": "text",
            },
            {
                "id": "phase6_oneshot_readlist_context_closure_shadow",
                "category": "phase6-oneshot-readlist-context-closure",
                "refusal_condition": "Phase6 readlist-detail query/runtime ownership evidence not proven",
                "evidence": [
                    phase6_query_contract,
                    phase6_query_semantics,
                    phase6_runtime_ownership,
                    phase6_runtime_semantics,
                ],
                "mode": "text",
            },
            {
                "id": "phase6_oneshot_readlist_context_closure_browser",
                "category": "phase6-oneshot-readlist-context-closure",
                "refusal_condition": "Phase6 oneshot READLIST-context direct-read browser evidence not proven",
                "evidence": [
                    phase6_browser_summary,
                    phase6_browser_route,
                    phase6_browser_selectors,
                    phase6_browser_gate_check,
                ],
                "mode": "discovery_markers",
                "marker_map": {
                    phase6_browser_summary: [
                        '"route": "browse-oneshot"',
                        '"observedOwnershipLabel": "readlist-detail-native-owned"',
                        '"label": "readlist-detail"',
                        '"observedFallbackRequests": []',
                    ],
                    phase6_browser_route: [
                        '"route": "browse-oneshot"',
                        '"observedOwnershipLabel": "readlist-detail-native-owned"',
                        '"label": "readlist-detail"',
                        '"observedFallbackRequests": []',
                    ],
                    phase6_browser_selectors: [
                        'PASS tests/unit/views/browse-oneshot-smoke-selectors.spec.ts',
                        'given oneshot readlist context smoke contract when enumerated then it should keep exact owned inventory and exclude fallback-only branches',
                        'given oneshot readlist context source flow when inspected then it should keep native readlist detail and sibling requests wired from route context',
                    ],
                    phase6_browser_gate_check: [
                        'ok=True',
                        'browse-oneshot proves exact owned labels: oneshot-series-detail, oneshot-series-collections, oneshot-bootstrap-books-list, oneshot-book-readlists, readlist-detail, readlist-books-unpaged, readlist-book-next, readlist-book-previous',
                        'browse-oneshot keeps READLIST-context fallback inventory empty after readlist detail promotion',
                    ],
                },
                "success_note": "Phase6 browser evidence proves exact eight-label owned inventory while keeping READLIST-context fallback empty after readlist-detail promotion.",
            },
            {
                "id": "phase6_oneshot_readlist_context_closure_regression",
                "category": "phase6-oneshot-readlist-context-closure",
                "refusal_condition": "Phase6 regression and exclusion containment evidence not proven",
                "evidence": [phase6_regression, phase6_adjacent_regression, phase6_adjacent_contract],
                "mode": "text",
            },
        ]

    phase7_checks: list[dict[str, object]] = []
    if is_phase7_series_oneshot_query_closure:
        phase7_contract = evidence_root / "task-1-contract" / "phase7-series-oneshot-exact-route.txt"
        phase7_adjacent_contract = evidence_root / "task-1-contract" / "phase7-adjacent-query-exclusions.txt"
        phase7_runtime_ownership = evidence_root / "task-2-runtime" / "phase7-exact-oneshot-native.txt"
        phase7_runtime_regression = evidence_root / "task-2-runtime" / "phase7-plain-series-detail-regression.txt"
        phase7_parity_semantics = evidence_root / "task-3-parity" / "phase7-missing-restricted-parity.txt"
        phase7_parity_variants = evidence_root / "task-3-parity" / "phase7-query-variant-shadow.txt"
        phase7_case_inventory = evidence_root / "task-4-compat" / "phase7-case-inventory.txt"
        phase7_contract_vs_compat = evidence_root / "task-4-compat" / "phase7-contract-vs-compat.txt"
        phase7_browser_summary = evidence_root / "task-5-browser-smoke" / "summary.json"
        phase7_browser_route = evidence_root / "task-5-browser-smoke" / "browse-oneshot.json"

        phase7_checks = [
            {
                "id": "phase7_series_oneshot_query_closure_contract",
                "category": "phase7-series-oneshot-query-closure",
                "refusal_condition": "Phase7 exact oneshot=true contract and compat evidence not proven",
                "evidence": [
                    phase7_contract,
                    phase7_adjacent_contract,
                    phase7_case_inventory,
                    phase7_contract_vs_compat,
                ],
                "mode": "discovery_markers",
                "marker_map": {
                    phase7_contract: [
                        "phase7_series_oneshot_exact_route_shape_is_frozen",
                        "GET /api/v1/series/{seriesId}?oneshot=true",
                    ],
                    phase7_adjacent_contract: [
                        "phase7_adjacent_oneshot_query_variants_remain_explicitly_non_native",
                        "oneshot=false, duplicate oneshot=true, oneshot=TRUE, and oneshot=true&other=value stay explicit non-native",
                    ],
                    phase7_case_inventory: [
                        "phase7_series_oneshot_case_inventory_loads",
                        "P7-ONESHOT-SERIES-DETAIL-EXACT-OWNED",
                    ],
                    phase7_contract_vs_compat: [
                        "phase7 contract vs compat exact-route ownership aligned",
                        "P7-ONESHOT-SERIES-DETAIL-EXACT-OWNED",
                    ],
                },
                "success_note": "Phase7 contract evidence proves only the exact `?oneshot=true` series detail shape is newly owned while adjacent query variants remain explicit non-native.",
            },
            {
                "id": "phase7_series_oneshot_query_closure_shadow",
                "category": "phase7-series-oneshot-query-closure",
                "refusal_condition": "Phase7 runtime and parity evidence for exact oneshot=true ownership not proven",
                "evidence": [
                    phase7_runtime_ownership,
                    phase7_runtime_regression,
                    phase7_parity_semantics,
                    phase7_parity_variants,
                ],
                "mode": "discovery_markers",
                "marker_map": {
                    phase7_runtime_ownership: [
                        "phase7_exact_oneshot_true_series_detail_is_native",
                        "/api/v1/series/series-1?oneshot=true => native-rust-owned",
                    ],
                    phase7_runtime_regression: [
                        "series_detail_and_collections_are_native_owned",
                        "plain series detail and collections remain native-owned",
                    ],
                    phase7_parity_semantics: [
                        "phase7_missing_and_restricted_series_oneshot_detail_matches_plain_detail_semantics",
                        "missing and restricted oneshot=true series detail semantics match plain detail",
                    ],
                    phase7_parity_variants: [
                        "phase7_series_oneshot_query_variants_remain_non_native",
                        "adjacent oneshot query variants still emit explicit non-native diagnostics",
                    ],
                },
                "success_note": "Phase7 runtime/parity evidence proves exact `?oneshot=true` ownership without widening plain detail semantics or adjacent query variants.",
            },
            {
                "id": "phase7_series_oneshot_query_closure_browser",
                "category": "phase7-series-oneshot-query-closure",
                "refusal_condition": "Phase7 browse-oneshot browser governance evidence not proven",
                "evidence": [phase7_browser_summary, phase7_browser_route],
                "mode": "discovery_markers",
                "marker_map": {
                    phase7_browser_summary: [
                        '"route": "browse-oneshot"',
                        '"captureMode": "source-contract-fallback"',
                        '"type": "oneshot-readlist-fallback"',
                        '"observedOwnershipLabel": "readlist-detail-native-owned"',
                        '"label": "oneshot-series-detail"',
                        '"label": "oneshot-series-collections"',
                        '"label": "oneshot-bootstrap-books-list"',
                        '"label": "oneshot-book-readlists"',
                        '"label": "readlist-detail"',
                        '"label": "readlist-books-unpaged"',
                        '"label": "readlist-book-next"',
                        '"label": "readlist-book-previous"',
                    ],
                    phase7_browser_route: [
                        '"route": "browse-oneshot"',
                        '"captureMode": "source-contract-fallback"',
                        '"type": "oneshot-readlist-fallback"',
                        '"observedOwnershipLabel": "readlist-detail-native-owned"',
                        '"label": "oneshot-series-detail"',
                        '"label": "oneshot-series-collections"',
                        '"label": "oneshot-bootstrap-books-list"',
                        '"label": "oneshot-book-readlists"',
                        '"label": "readlist-detail"',
                        '"label": "readlist-books-unpaged"',
                        '"label": "readlist-book-next"',
                        '"label": "readlist-book-previous"',
                    ],
                },
                "success_note": "Phase7 browser smoke remains governance-only evidence: browse-oneshot stays `captureMode=source-contract-fallback` / `oneshot-readlist-fallback` with unchanged owned-request inventory and no browse-closure claim.",
            },
        ]

    phase8_checks: list[dict[str, object]] = []
    if is_phase8_readlist_books_family_closure:
        phase8_contract = evidence_root / "task-1-contract-matrix" / "phase8-readlist-books-family-contract.txt"
        phase8_exclusions = evidence_root / "task-1-contract-matrix" / "phase8-readlist-books-family-exclusions.txt"
        phase8_runtime_green = evidence_root / "task-3-native-query-runtime" / "readlist-books-family-green.txt"
        phase8_runtime_restrictions = evidence_root / "task-3-native-query-runtime" / "restrictions.txt"
        phase8_shadow_routing = evidence_root / "task-4-http-parity" / "shadow-routing.txt"
        phase8_shadow_exclusions = evidence_root / "task-4-http-parity" / "exclusion-routing.txt"
        phase8_http_diff = evidence_root / "task-5-compat-shadow" / "http-json-diff.txt"
        phase8_negative_inventory = evidence_root / "task-5-compat-shadow" / "negative-inventory.txt"
        phase8_browser_summary_text = evidence_root / "task-6-browser-smoke" / "browser-summary.txt"
        phase8_browser_summary_json = evidence_root / "task-6-browser-smoke" / "summary.json"
        phase8_browser_route = evidence_root / "task-6-browser-smoke" / "browse-readlist.json"

        phase8_checks = [
            {
                "id": "phase8_readlist_books_family_contract",
                "category": "phase8-readlist-books-family-closure",
                "refusal_condition": "Phase8 readlist-books family contract matrix is not frozen",
                "evidence": [phase8_contract],
                "mode": "discovery_markers",
                "marker_map": {
                    phase8_contract: [
                        "phase8_readlist_books_family_matrix_is_frozen",
                        "New ownership (Phase 8 only)",
                        "GET /api/v1/readlists/{readListId}/books",
                        "GET /api/v1/readlists/{readListId}/books?tag={tag} (including repeated tag)",
                        "GET /api/v1/readlists/{readListId}/books?author={name,role} (including repeated author)",
                        "supported filter combinations with default paging or explicit page/size",
                        "GET /api/v1/readlists/{readListId}/books?unpaged=false",
                        "Dependency-only (regression-only, not reopened)",
                        "GET /api/v1/readlists/{readListId}/books?unpaged=true",
                        "GET /api/v1/readlists/{readListId}",
                        "GET /api/v1/books/{bookId}/readlists (Phase 3)",
                        "Boundary rule:",
                        "Result: PASS",
                    ],
                },
                "success_note": "Phase8 contract evidence freezes exact direct readlist-books paged/filter ownership while keeping Phase 4/6 routes dependency-only/regression-only.",
            },
            {
                "id": "phase8_readlist_books_family_exclusions",
                "category": "phase8-readlist-books-family-closure",
                "refusal_condition": "Phase8 exclusion ledger is incomplete or drifted",
                "evidence": [phase8_exclusions],
                "mode": "discovery_markers",
                "marker_map": {
                    phase8_exclusions: [
                        "phase8_adjacent_routes_remain_explicitly_non_native",
                        "GET /api/v1/readlists",
                        "GET /api/v1/readlists/{readListId}/read-progress/tachiyomi",
                        "no list-family ownership",
                        "no Tachiyomi ownership",
                        "no admin/write/media/reader/SSE/whole-cutover claims",
                        "Result: PASS",
                        "shadow-java-writer",
                    ],
                },
                "success_note": "Phase8 exclusion ledger keeps list-family, Tachiyomi, admin/write/media/reader/SSE, and whole-cutover branches explicit non-native.",
            },
            {
                "id": "phase8_readlist_books_family_runtime",
                "category": "phase8-readlist-books-family-closure",
                "refusal_condition": "Phase8 paged/filter readlist-books runtime parity evidence not proven",
                "evidence": [
                    phase8_runtime_green,
                    phase8_runtime_restrictions,
                ],
                "mode": "text",
            },
            {
                "id": "phase8_readlist_books_family_compat",
                "category": "phase8-readlist-books-family-closure",
                "refusal_condition": "Phase8 compat inventory/diff evidence not proven",
                "evidence": [
                    phase8_http_diff,
                    phase8_negative_inventory,
                ],
                "mode": "text",
            },
            {
                "id": "phase8_readlist_books_family_browser",
                "category": "phase8-readlist-books-family-closure",
                "refusal_condition": "Phase8 BrowseReadList paged/filter browser evidence not proven",
                "evidence": [
                    phase8_browser_summary_text,
                    phase8_browser_summary_json,
                    phase8_browser_route,
                ],
                "mode": "phase8_browser_smoke",
            },
            {
                "id": "phase8_readlist_books_family_regression",
                "category": "phase8-readlist-books-family-closure",
                "refusal_condition": "Phase8 dependency-only routing and excluded-route protections are not proven",
                "evidence": [
                    phase8_shadow_routing,
                    phase8_shadow_exclusions,
                ],
                "mode": "text",
            },
        ]

    phase9_checks: list[dict[str, object]] = []
    if is_phase9_readlists_list_browse_closure:
        phase9_contract = evidence_root / "task-1-contract-matrix" / "phase9-readlists-list-browse-contract.txt"
        phase9_exclusions = evidence_root / "task-1-contract-matrix" / "phase9-readlists-list-browse-exclusions.txt"
        phase9_compat = evidence_root / "task-6-compat-shadow" / "readlists_list_browse.txt"
        phase9_compat_negative = evidence_root / "task-6-compat-shadow" / "readlists_list_browse_negative.txt"
        phase9_browser_summary_text = evidence_root / "task-7-browser-smoke" / "browser-summary.txt"
        phase9_browser_summary_json = evidence_root / "task-7-browser-smoke" / "summary.json"
        phase9_browser_route = evidence_root / "task-7-browser-smoke" / "browse-readlists.json"

        phase9_checks = [
            {
                "id": "phase9_readlists_list_browse_contract",
                "category": "phase9-readlists-list-browse-closure",
                "refusal_condition": "Phase9 readlists browse/list contract matrix is not frozen",
                "evidence": [phase9_contract],
                "mode": "discovery_markers",
                "marker_map": {
                    phase9_contract: [
                        "phase9_readlists_list_browse_matrix_is_frozen",
                        "GET /api/v1/readlists (default browse; search/unpaged/explicit sort omitted)",
                        "GET /api/v1/readlists?page={page}&size=0 (matches JVM exactly)",
                        "GET /api/v1/readlists?library_id={libraryId...}&page={page}&size={size}",
                        "Pre-owned dependencies (regression-only, not reopened):",
                        "Direct paged/filter GET /api/v1/readlists/{readListId}/books family (Phase 8, regression-only here)",
                        "Boundary rule:",
                        "Result: PASS",
                    ],
                },
                "success_note": "Phase9 contract evidence freezes exact browse/list ownership and keeps Phase 6/8 direct readlist surfaces dependency-only/regression-only.",
            },
            {
                "id": "phase9_readlists_list_browse_exclusions",
                "category": "phase9-readlists-list-browse-closure",
                "refusal_condition": "Phase9 readlists browse/list exclusion ledger is incomplete or drifted",
                "evidence": [phase9_exclusions],
                "mode": "discovery_markers",
                "marker_map": {
                    phase9_exclusions: [
                        "phase9_adjacent_routes_remain_explicitly_non_native",
                        "GET /api/v1/readlists?search={term} and every search-bearing variant",
                        "GET /api/v1/readlists?unpaged=true",
                        "GET /api/v1/readlists?sort=... and every explicit custom-sort/relevance variant",
                        "GET /api/v1/readlists/{readListId}/read-progress/tachiyomi",
                        "no search/unpaged/explicit-sort ownership",
                        "no Tachiyomi ownership",
                        "no write/media/reader/SSE/whole-cutover claims",
                        "Result: PASS",
                        "shadow-java-writer",
                    ],
                },
                "success_note": "Phase9 exclusion ledger keeps search, unpaged, sort, Tachiyomi, dialogs/admin, and broader write/media/reader/SSE branches explicit non-native.",
            },
            {
                "id": "phase9_readlists_list_browse_compat",
                "category": "phase9-readlists-list-browse-closure",
                "refusal_condition": "Phase9 compat inventory/diff evidence for readlists browse/list ownership is not proven",
                "evidence": [phase9_compat, phase9_compat_negative],
                "mode": "discovery_markers",
                "marker_map": {
                    phase9_compat: [
                        'readlists_list_browse',
                        'COMMAND_EXIT_CODE="0"',
                    ],
                    phase9_compat_negative: [
                        'readlists_list_browse_negative',
                        'COMMAND_EXIT_CODE="0"',
                    ],
                },
                "success_note": "Phase9 compat evidence proves the exact five browse/list owned shapes and negative browse exclusions are both represented in http_json_diff inventory.",
            },
            {
                "id": "phase9_readlists_list_browse_browser",
                "category": "phase9-readlists-list-browse-closure",
                "refusal_condition": "Phase9 BrowseReadLists browser governance evidence is not proven",
                "evidence": [
                    phase9_browser_summary_text,
                    phase9_browser_summary_json,
                    phase9_browser_route,
                ],
                "mode": "phase9_browser_smoke",
            },
        ]

    phase10_checks: list[dict[str, object]] = []
    if is_phase10_readlists_search_closure:
        phase10_runtime_queries = evidence_root / "task-4-native-search" / "readlists-list-browse-queries.txt"
        phase10_runtime_shadow = evidence_root / "task-5-runtime-boundaries" / "readlists-list-browse-shadow.txt"
        phase10_compat = evidence_root / "task-6-compat-gate" / "http-json-diff.txt"

        phase10_checks = [
            {
                "id": "phase10_readlists_search_runtime",
                "category": "phase10-readlists-search-closure",
                "refusal_condition": "Phase10 runtime/query ownership evidence for readlists search is not proven",
                "evidence": [phase10_runtime_queries, phase10_runtime_shadow],
                "mode": "text",
            },
            {
                "id": "phase10_readlists_search_compat",
                "category": "phase10-readlists-search-closure",
                "refusal_condition": "Phase10 compat inventory/diff evidence for readlists search ownership and exclusions is not proven",
                "evidence": [phase10_compat],
                "mode": "text",
            },
        ]

    checks = base_checks + discovery_checks + phase3_checks + phase4_checks + phase5_checks + phase6_checks + phase7_checks + phase8_checks + phase9_checks + phase10_checks
    return checks, discovery_checks, phase3_checks, phase4_checks, phase5_checks, phase6_checks, phase8_checks, phase9_checks, phase10_checks
