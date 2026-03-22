from pathlib import Path


PHASE2_DISCOVERY_LABEL = "phase2-catalog-discovery"
PHASE3_DETAIL_READ_LABEL = "phase3-detail-read"
PHASE4_READLIST_CONTEXT_READ_LABEL = "phase4-readlist-context-read"
PHASE5_ONESHOT_CLOSURE_LABEL = "phase5-oneshot-closure"
PHASE6_ONESHOT_READLIST_CONTEXT_CLOSURE_LABEL = "phase6-oneshot-readlist-context-closure"

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
]:
    is_phase2_discovery = run_label == PHASE2_DISCOVERY_LABEL
    is_phase3_detail_read = run_label == PHASE3_DETAIL_READ_LABEL
    is_phase4_readlist_context_read = run_label == PHASE4_READLIST_CONTEXT_READ_LABEL
    is_phase5_oneshot_closure = run_label == PHASE5_ONESHOT_CLOSURE_LABEL
    is_phase6_oneshot_readlist_context_closure = run_label == PHASE6_ONESHOT_READLIST_CONTEXT_CLOSURE_LABEL

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

    checks = base_checks + discovery_checks + phase3_checks + phase4_checks + phase5_checks + phase6_checks
    return checks, discovery_checks, phase3_checks, phase4_checks, phase5_checks, phase6_checks
