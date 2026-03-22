import json
from pathlib import Path


TEXT_CHECK_FILES = {
    'task-9-api-key/api-key-diff.txt': 'api key parity verified',
    'task-7-libraries/library-diff.txt': 'libraries parity verified',
    'task-6-opds-v1/opds-v1-diff.txt': 'opds parity verified',
    'task-8-query-cache/binary-metadata-diff.txt': 'binary metadata parity verified',
    'task-10-read-progress/read-progress-diff.txt': 'read progress parity verified',
    'task-12-search/java-search-lifecycle.txt': 'java search writer remains owner',
    'task-13-tasks/task-ownership.txt': 'shadow mode prevents dual consumers',
    'task-15-packaging/runtime-startup.txt': 'runtime startup contract verified',
    'task-15-packaging/tray-compat.txt': 'tray compatibility verified',
    'task-3-shadow-governance/config-precedence.txt': 'config precedence deterministic',
}


def write_text(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content + '\n', encoding='utf-8')


def write_json(path: Path, payload: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


def build_pass_browser_summary() -> list[dict[str, object]]:
    return [
        {'route': 'browse-libraries', 'pass': True},
        {'route': 'browse-series', 'pass': True},
        {'route': 'search', 'pass': True},
        {'route': 'server-management', 'pass': True},
    ]


def build_pass_server_management() -> dict[str, object]:
    return {
        'route': 'server-management',
        'selector': '[data-testid="server-management-root"]',
        'pass': True,
    }


def build_phase3_browser_row(*, route: str, capture_mode: str = 'source-contract-fallback') -> dict[str, object]:
    if route == 'browse-series':
        return {
            'route': 'browse-series',
            'pass': True,
            'captureMode': capture_mode,
            'signals': {
                'rootFound': True,
                'detailMetadataVisible': True,
                'collectionsPanelFound': True,
            },
            'expectedOwnedRequests': [
                {'label': 'series-detail', 'pass': True},
                {'label': 'series-collections', 'pass': True},
                {'label': 'series-books-list', 'pass': True},
            ],
        }

    if route == 'browse-book':
        return {
            'route': 'browse-book',
            'pass': True,
            'captureMode': capture_mode,
            'signals': {
                'rootFound': True,
                'detailMetadataVisible': True,
                'readlistsPanelFound': True,
                'siblingNavigationFound': True,
            },
            'expectedOwnedRequests': [
                {'label': 'book-detail', 'pass': True},
                {'label': 'book-readlists', 'pass': True},
                {'label': 'book-siblings-list', 'pass': True},
                {'label': 'book-sibling-next', 'pass': True},
                {'label': 'book-sibling-previous', 'pass': True},
            ],
        }

    raise AssertionError(f'Unsupported phase3 browser route fixture: {route}')


def build_phase4_browser_row(*, route: str, capture_mode: str = 'source-contract-fallback') -> dict[str, object]:
    if route == 'browse-readlist':
        return {
            'route': 'browse-readlist',
            'pass': True,
            'captureMode': capture_mode,
            'signals': {
                'rootFound': True,
                'detailMetadataVisible': True,
                'itemBrowserFound': True,
                'entryBookLinkFound': True,
                'entryBookContextRetained': True,
                'contextBannerVisible': True,
                'returnedToReadlist': True,
            },
            'scenario': {
                'type': 'readlist-origin-entry',
                'captureMode': capture_mode,
                'contextPropagationFound': True,
                'bookLinkQueryFound': True,
                'contextEnumFound': True,
            },
            'expectedOwnedRequests': [],
        }

    if route == 'browse-book':
        return {
            'route': 'browse-book',
            'pass': True,
            'captureMode': capture_mode,
            'signals': {
                'rootFound': True,
                'detailMetadataVisible': True,
                'readlistsPanelFound': True,
                'siblingNavigationFound': True,
                'initialContextRetained': True,
                'initialPreviousBoundary': True,
                'initialNextWithinReadlist': True,
                'readListNameVisible': True,
                'nextNavigationRetainedContext': True,
                'previousNavigationRetainedContext': True,
                'nextThenPreviousLoopClosed': True,
            },
            'scenario': {
                'type': 'readlist-sibling-navigation',
                'captureMode': capture_mode,
                'contextParseFound': True,
                'readlistListRequestFound': True,
                'readlistNextRequestFound': True,
                'readlistPreviousRequestFound': True,
                'expectedReadlistNextBookId': 'book-2',
            },
            'expectedOwnedRequests': [
                {'label': 'readlist-books-unpaged', 'pass': True},
                {'label': 'readlist-book-next', 'pass': True},
                {'label': 'readlist-book-previous', 'pass': True},
            ],
        }

    raise AssertionError(f'Unsupported phase4 browser route fixture: {route}')


def build_phase5_browser_row(*, capture_mode: str = 'source-contract-fallback') -> dict[str, object]:
    return {
        'route': 'browse-oneshot',
        'pass': True,
        'captureMode': capture_mode,
        'signals': {
            'rootFound': True,
            'detailMetadataVisible': True,
            'collectionsPanelFound': True,
            'readlistsPanelFound': True,
            'readlistContextNavigationFound': True,
            'returnedToDirectOneshot': True,
        },
        'scenario': {
            'type': 'oneshot-readlist-fallback',
            'captureMode': capture_mode,
            'contextParseFound': True,
            'contextNameRequestFound': True,
            'readlistBooksRequestFound': True,
            'readlistNextRequestFound': True,
            'readlistPreviousRequestFound': True,
            'readlistContextNavigationFound': True,
            'observedOwnershipLabel': 'readlist-detail-native-owned',
        },
        'expectedOwnedRequests': [
            {'label': 'oneshot-series-detail', 'pass': True},
            {'label': 'oneshot-series-collections', 'pass': True},
            {'label': 'oneshot-bootstrap-books-list', 'pass': True},
            {'label': 'oneshot-book-readlists', 'pass': True},
            {'label': 'readlist-detail', 'pass': True},
            {'label': 'readlist-books-unpaged', 'pass': True},
            {'label': 'readlist-book-next', 'pass': True},
            {'label': 'readlist-book-previous', 'pass': True},
        ],
        'observedFallbackRequests': [],
    }


def seed_phase6_oneshot_readlist_context_closure_evidence(
    root: Path,
    *,
    include_contract: bool = True,
    include_browser: bool = True,
    include_regression: bool = True,
) -> None:
    if include_contract:
        write_text(root / 'task-1-contract-matrix/phase6_readlist_detail_route_shape_is_frozen.txt', 'cargo test: 1 passed, 8 filtered out (1 suite, 0.01s)')
        write_text(root / 'task-1-contract-matrix/phase6_readlist_detail_case_inventory_loads.txt', 'cargo test: 1 passed, 19 filtered out (1 suite, 0.01s)')
        write_text(root / 'task-1-contract-matrix/phase6_adjacent_branches_remain_explicitly_non_native.txt', 'cargo test: 1 passed, 8 filtered out (1 suite, 0.01s)')

    write_text(root / 'task-2-query/readlist-detail-query-contract.txt', 'cargo test: 2 passed, 0 failed (1 suite, 0.01s)')
    write_text(root / 'task-2-query/readlist-detail-visible-filtered-not-found.txt', 'cargo test: 2 passed, 0 failed (1 suite, 0.02s)')
    write_text(root / 'task-3-runtime/phase6_readlist_detail_runtime_ownership_is_native.txt', 'cargo test: 1 passed, 16 filtered out (1 suite, 0.03s)')
    write_text(root / 'task-3-runtime/phase6_readlist_detail_404_and_filtered_semantics_match_contract.txt', 'cargo test: 1 passed, 16 filtered out (1 suite, 0.02s)')

    if include_browser:
        browse_oneshot_row = build_phase5_browser_row()
        write_json(root / 'task-4-browser-smoke/browse-oneshot.json', browse_oneshot_row)
        write_json(root / 'task-4-browser-smoke/summary.json', [browse_oneshot_row])
        write_text(
            root / 'task-4-browser-smoke/browse-oneshot-smoke-selectors.log',
            '\n'.join([
                'PASS tests/unit/views/browse-oneshot-smoke-selectors.spec.ts',
                'given oneshot readlist context smoke contract when enumerated then it should keep exact owned inventory and exclude fallback-only branches',
                'given oneshot readlist context source flow when inspected then it should keep native readlist detail and sibling requests wired from route context',
            ]),
        )
        write_text(
            root / 'task-4-browser-smoke/gate-evaluator-check.log',
            '\n'.join([
                'ok=True',
                'DETAIL: browse-oneshot captureMode=source-contract-fallback (accepted in this environment)',
                'DETAIL: browse-oneshot proves exact owned labels: oneshot-series-detail, oneshot-series-collections, oneshot-bootstrap-books-list, oneshot-book-readlists, readlist-detail, readlist-books-unpaged, readlist-book-next, readlist-book-previous',
                'DETAIL: browse-oneshot keeps READLIST-context fallback inventory empty after readlist detail promotion',
            ]),
        )

    if include_regression:
        write_text(root / 'task-6-regression/phase4-phase5-regression.txt', 'cargo test: 2 passed, 0 failed (1 suite, 0.03s)')
        write_text(root / 'task-6-regression/adjacent-exclusions-stay-shadow.txt', 'cargo test: 1 passed, 0 failed (1 suite, 0.02s)')


def build_admin_queue_payload(*, status: str, can_claim_admin_queue_parity: bool) -> dict[str, object]:
    return {
        'task': 'T13 admin task endpoint parity remains executable',
        'status': status,
        'adminQueueActionEvidence': {
            'clickAttempted': True,
            'deleteRequestObserved': True,
        },
        'parityConclusion': {
            'canClaimPageVisible': True,
            'canClaimDeleteActionTriggered': True,
            'canClaimAdminQueueParity': can_claim_admin_queue_parity,
            'reason': 'Synthetic regression fixture',
        },
    }


def seed_common_evidence(root: Path, *, neutral_transcripts: bool, admin_queue_payload: dict[str, object]) -> None:
    transcript = 'test result: ok. 1 passed; 0 failed; 0 ignored'
    shadow_text = transcript if neutral_transcripts else 'shadow writer guardrail verified'
    search_text = transcript if neutral_transcripts else 'search ownership parity verified'

    for relative_path, text in TEXT_CHECK_FILES.items():
        write_text(root / relative_path, text)

    write_text(root / 'task-3-shadow-governance/shadow-safety.txt', shadow_text)
    write_text(root / 'task-12-search/rust-search-parity.txt', search_text)

    write_json(root / 'task-11-browser-smoke/summary.json', build_pass_browser_summary())
    write_json(root / 'task-14-ops/server-management.json', build_pass_server_management())
    write_json(root / 'task-13-tasks/admin-task-queue.json', admin_queue_payload)


def seed_phase3_detail_evidence(root: Path, *, include_browse_book: bool = True) -> None:
    write_text(
        root / 'task-1-contract-matrix/direct-browse-contract.txt',
        '\n'.join([
            'Task: T1 direct-browse contract freeze',
            'Scenario: Phase 3 direct-browse contract is frozen',
            'Command:',
            '  cargo test --manifest-path komga-rust/Cargo.toml --test catalog_detail_contract in_scope_direct_browse_shapes_are_frozen -- --exact --nocapture',
            'Observed:',
            '  - cargo test: 1 passed, 3 filtered out (1 suite, 0.01s)',
            'Result: PASS',
        ]),
    )
    write_text(
        root / 'task-1-contract-matrix/excluded-branches.txt',
        '\n'.join([
            'Task: T1 excluded branches freeze',
            'Scenario: Excluded branches still emit explicit non-native markers',
            'Command:',
            '  cargo test --manifest-path komga-rust/Cargo.toml --test catalog_detail_contract excluded_media_context_and_write_shapes_remain_non_native -- --exact --nocapture',
            'Observed:',
            '  - cargo test: 1 passed, 3 filtered out (1 suite, 0.00s)',
            'Result: PASS',
            'Notes:',
            '  - The excluded branches remain Phase 3 non-native and stay on the `shadow-java-writer` path.',
        ]),
    )

    write_text(
        root / 'task-8-exclusions/ui-visible-non-native.txt',
        '\n'.join([
            'catalog_detail_contract contextual_media_and_write_branches_are_explicitly_non_native',
            'browse-oneshot closure',
            'READLIST context routing',
            'pages / thumbnails / file media delivery',
            'read-progress mutation / progression',
            'collection / readlist removal actions',
            'admin edit/delete affordances',
            'SSE / live-refresh parity',
        ]),
    )
    write_text(
        root / 'task-8-exclusions/excluded-branch-markers.txt',
        '\n'.join([
            'catalog_detail_contract contextual_media_and_write_branches_are_explicitly_non_native',
            'catalog_detail_shadow excluded_detail_branches_emit_shadow_marker',
            'Result: PASS',
            'shadow-java-writer',
        ]),
    )

    series_row = build_phase3_browser_row(route='browse-series')
    write_json(root / 'task-9-browser-smoke/browse-series.json', series_row)

    summary_rows = [series_row]
    if include_browse_book:
        book_row = build_phase3_browser_row(route='browse-book')
        write_json(root / 'task-9-browser-smoke/browse-book.json', book_row)
        summary_rows.append(book_row)

    write_json(root / 'task-9-browser-smoke/summary.json', summary_rows)


def seed_phase4_readlist_context_evidence(
    root: Path,
    *,
    include_exclusions: bool = True,
    include_shadow: bool = True,
    include_browser: bool = True,
) -> None:
    write_text(
        root / 'task-1-contract-matrix/readlist-context-contract.txt',
        '\n'.join([
            'Task: T1 readlist-context contract freeze',
            'Scenario: Phase 4 readlist-context contract is frozen',
            'Command:',
            '  cargo test --manifest-path komga-rust/Cargo.toml --test catalog_detail_contract in_scope_readlist_context_shapes_are_frozen -- --exact --nocapture',
            'Owned route matrix:',
            '  - GET /api/v1/readlists/{readlistId}/books?unpaged=true',
            '  - GET /api/v1/readlists/{readlistId}/books/{bookId}/previous',
            '  - GET /api/v1/readlists/{readlistId}/books/{bookId}/next',
            'in_scope_readlist_context_shapes_are_frozen',
            'Result: PASS',
        ]),
    )

    if include_exclusions:
        write_text(
            root / 'task-1-contract-matrix/readlist-context-excluded-branches.txt',
            '\n'.join([
                'Task: T1 readlist-context exclusions freeze',
                'Scenario: Readlist-context excluded branches still emit explicit non-native markers',
                'Command:',
                '  cargo test --manifest-path komga-rust/Cargo.toml --test catalog_detail_contract excluded_readlist_context_and_write_shapes_remain_non_native -- --exact --nocapture',
                'excluded_readlist_context_and_write_shapes_remain_non_native',
                'paged readlist books stay explicit non-native',
                'readlist list/detail routes stay explicit non-native',
                'library_id variants stay explicit non-native',
                'media routes stay explicit non-native',
                'read-progress/progression stay explicit non-native',
                'oneshot + reader handoff stay explicit non-native',
                'SSE stays explicit non-native',
                'removal/admin write branches stay explicit non-native',
                'Result: PASS',
                'shadow-java-writer',
            ]),
        )

    if include_shadow:
        write_text(
            root / 'task-4-shadow/readlist-books-runtime-ownership.txt',
            '\n'.join([
                'Task 4 shadow evidence: readlist books runtime ownership',
                'readlist_books_runtime_ownership_stays_narrow',
                '/api/v1/readlists/readlist-2/books?unpaged=true => native-rust-owned',
                '/api/v1/readlists/readlist-2/books?page=0&size=20 => shadow-java-writer',
                '/api/v1/readlists/readlist-2/books?unpaged=true&library_id=1 => shadow-java-writer',
                '_compat.discoveryOwnership = non-native',
                'UnsupportedBookFilter(paged)',
                'UnsupportedBookFilter(LibraryId)',
                'Result: PASS',
            ]),
        )
        write_text(
            root / 'task-5-shadow/readlist-prev-next-runtime-ownership.txt',
            '\n'.join([
                'Task 5 shadow evidence: readlist previous/next runtime ownership',
                '/api/v1/readlists/readlist-2/books/book-1/previous => 404 without shadow marker',
                '/api/v1/readlists/readlist-2/books/book-1/next => 200 without _compat',
                'Only readlist-context previous/next were newly wired to native ownership.',
                'Result: PASS',
            ]),
        )
        write_text(
            root / 'task-5-shadow/readlist-prev-next-legacy-parity.txt',
            '\n'.join([
                'Task 5 JVM parity evidence: readlist previous/next legacy controller behavior',
                'ReadListControllerTest',
                'boundary, membership, library filtering, and unordered ordering semantics',
                'BUILD SUCCESSFUL',
                'Result: PASS',
            ]),
        )

    if include_browser:
        browse_readlist_row = build_phase4_browser_row(route='browse-readlist')
        browse_book_row = build_phase4_browser_row(route='browse-book')

        write_json(root / 'task-6-browser-smoke/browse-readlist.json', browse_readlist_row)
        write_json(root / 'task-6-browser-smoke/browse-book.json', browse_book_row)
        write_json(root / 'task-6-browser-smoke/summary.json', [browse_readlist_row, browse_book_row])


def seed_phase5_oneshot_closure_evidence(
    root: Path,
    *,
    include_contract: bool = True,
    include_exclusions: bool = True,
    include_shadow: bool = True,
    include_browser: bool = True,
) -> None:
    if include_contract:
        write_text(
            root / 'task-1-contract-matrix/oneshot-contract.txt',
            '\n'.join([
                'Task: T1 oneshot closure contract freeze',
                'Scenario: Phase 5 oneshot closure contract is frozen',
                'Command:',
                '  cargo test --manifest-path komga-rust/Cargo.toml --test catalog_detail_contract oneshot_direct_route_shape_is_frozen -- --exact --nocapture',
                'Owned route matrix:',
                '  - POST /api/v1/books/list',
                '  - body condition: SeriesId(seriesId) only',
                'in_scope_oneshot_closure_shapes_are_frozen',
                'Result: PASS',
            ]),
        )

    if include_exclusions:
        write_text(
            root / 'task-1-contract-matrix/oneshot-excluded-branches.txt',
            '\n'.join([
                'Task: T1 oneshot excluded branches freeze',
                'Scenario: excluded oneshot-adjacent branches remain explicit non-native',
                'excluded_oneshot_closure_and_adjacent_branches_remain_non_native',
                'GET /api/v1/series/{seriesId}?oneshot=true stays explicit non-native',
                'READLIST-context oneshot fallback stays explicit non-native',
                'media routes stay explicit non-native',
                'reader handoff and download branches stay explicit non-native',
                'read-progress/progression stay explicit non-native',
                'collection/readlist removals stay explicit non-native',
                'admin/write branches stay explicit non-native',
                'SSE/live-refresh parity stays explicit non-native',
                'generic books/list widening stays explicit non-native',
                'full cutover/direct-serving approval stays refused',
                'Result: PASS',
                'shadow-java-writer',
            ]),
        )
        write_text(
            root / 'task-5-exclusions/oneshot-visible-vs-owned.txt',
            '\n'.join([
                'oneshot_context_media_reader_and_write_branches_are_explicitly_non_native',
                'visible in BrowseOneshot does not imply native ownership',
                'READLIST context fallback remains out of slice',
            ]),
        )
        write_text(
            root / 'task-5-exclusions/oneshot-excluded-branch-markers.txt',
            '\n'.join([
                'excluded_oneshot_branches_emit_shadow_marker',
                'Result: PASS',
                'shadow-java-writer',
            ]),
        )

    if include_shadow:
        write_text(
            root / 'task-4-runtime/oneshot-runtime-ownership.txt',
            '\n'.join([
                'Task 4 runtime evidence: direct oneshot happy path',
                'browse_oneshot_happy_path_uses_native_bootstrap_shape',
                'oneshot-bootstrap-books-list => native-rust-owned',
                'Result: PASS',
            ]),
        )
        write_text(
            root / 'task-4-runtime/phase3-phase4-regression.txt',
            '\n'.join([
                'Task 4 runtime regression evidence',
                'phase3_phase4_owned_routes_do_not_regress_with_oneshot_bootstrap',
                'Result: PASS',
            ]),
        )

    if include_browser:
        browse_oneshot_row = build_phase5_browser_row()
        write_json(root / 'task-6-browser-smoke/browse-oneshot.json', browse_oneshot_row)
        write_json(root / 'task-6-browser-smoke/summary.json', [browse_oneshot_row])
        write_text(
            root / 'task-6-browser-smoke/direct-oneshot-parity.txt',
            '\n'.join([
                'direct_oneshot_admin_user_limited_restricted_matrix',
                'Result: PASS',
            ]),
        )
