import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]
GATE_SCRIPT = REPO_ROOT / 'tools' / 'rust-cutover' / 'gate.sh'


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


def run_gate(evidence_root: Path, output_dir: Path, *, label: str) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env['JRELEASER_GITHUB_TOKEN'] = 'test-token'
    return subprocess.run(
        [
            'bash',
            str(GATE_SCRIPT),
            '--require-all',
            '--evidence-root',
            str(evidence_root),
            '--output-dir',
            str(output_dir),
            '--label',
            label,
        ],
        cwd=REPO_ROOT,
        env=env,
        text=True,
        capture_output=True,
        check=False,
    )


class GateRegressionTests(unittest.TestCase):
    def test_rejects_admin_queue_structured_parity_failure(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            evidence_root = temp_path / 'evidence'
            output_dir = temp_path / 'output'
            seed_common_evidence(
                evidence_root,
                neutral_transcripts=False,
                admin_queue_payload=build_admin_queue_payload(
                    status='action-exercised-parity-failed',
                    can_claim_admin_queue_parity=False,
                ),
            )

            result = run_gate(evidence_root, output_dir, label='structured-admin-failure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            task_check = next(item for item in summary['checks'] if item['id'] == 'task_ownership')
            self.assertEqual(task_check['status'], 'fail')
            self.assertTrue(any('parity' in detail.lower() or 'status' in detail.lower() for detail in task_check['details']))

    def test_rejects_admin_queue_when_parity_flag_is_false(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            evidence_root = temp_path / 'evidence'
            output_dir = temp_path / 'output'
            seed_common_evidence(
                evidence_root,
                neutral_transcripts=False,
                admin_queue_payload=build_admin_queue_payload(
                    status='action-exercised-parity-ok',
                    can_claim_admin_queue_parity=False,
                ),
            )

            result = run_gate(evidence_root, output_dir, label='structured-parity-flag-false')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            task_check = next(item for item in summary['checks'] if item['id'] == 'task_ownership')
            self.assertEqual(task_check['status'], 'fail')
            self.assertTrue(any('canclaimadminqueueparity' in detail.lower() or 'rejects admin queue parity' in detail.lower() for detail in task_check['details']))

    def test_accepts_neutral_zero_failed_transcripts(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            evidence_root = temp_path / 'evidence'
            output_dir = temp_path / 'output'
            seed_common_evidence(
                evidence_root,
                neutral_transcripts=True,
                admin_queue_payload=build_admin_queue_payload(
                    status='action-exercised-parity-ok',
                    can_claim_admin_queue_parity=True,
                ),
            )

            result = run_gate(evidence_root, output_dir, label='neutral-transcripts')

            self.assertEqual(
                result.returncode,
                0,
                msg=f'gate unexpectedly failed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'pass')
            self.assertTrue((output_dir / 'report.md').exists())

    def test_phase3_detail_read_label_passes_with_slice_evidence_only(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            evidence_root = temp_path / 'evidence'
            output_dir = temp_path / 'output'
            seed_common_evidence(
                evidence_root,
                neutral_transcripts=True,
                admin_queue_payload=build_admin_queue_payload(
                    status='action-exercised-parity-ok',
                    can_claim_admin_queue_parity=True,
                ),
            )
            seed_phase3_detail_evidence(evidence_root, include_browse_book=True)

            result = run_gate(evidence_root, output_dir, label='phase3-detail-read')

            self.assertEqual(
                result.returncode,
                0,
                msg=f'gate unexpectedly failed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'pass')
            self.assertEqual(summary.get('evaluation_scope'), 'phase3-detail-read-shadow')
            self.assertFalse(summary['governance']['cutover']['allowed'])
            self.assertFalse(summary['governance']['phase3_media_and_write']['allowed'])

            phase3_browser_check = next(item for item in summary['checks'] if item['id'] == 'phase3_detail_browser')
            self.assertEqual(phase3_browser_check['status'], 'pass')
            self.assertTrue(any('source-contract-fallback' in detail for detail in phase3_browser_check['details']))

    def test_phase3_detail_read_label_fails_closed_when_required_artifact_is_missing(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            evidence_root = temp_path / 'evidence'
            output_dir = temp_path / 'output'
            seed_common_evidence(
                evidence_root,
                neutral_transcripts=True,
                admin_queue_payload=build_admin_queue_payload(
                    status='action-exercised-parity-ok',
                    can_claim_admin_queue_parity=True,
                ),
            )
            seed_phase3_detail_evidence(evidence_root, include_browse_book=False)

            result = run_gate(evidence_root, output_dir, label='phase3-detail-read')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(item for item in summary['checks'] if item['id'] == 'phase3_detail_parity')
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase4_readlist_context_label_passes_with_slice_evidence_only(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            evidence_root = temp_path / 'evidence'
            output_dir = temp_path / 'output'
            seed_common_evidence(
                evidence_root,
                neutral_transcripts=True,
                admin_queue_payload=build_admin_queue_payload(
                    status='action-exercised-parity-ok',
                    can_claim_admin_queue_parity=True,
                ),
            )
            seed_phase4_readlist_context_evidence(evidence_root, include_exclusions=True)

            result = run_gate(evidence_root, output_dir, label='phase4-readlist-context-read')

            self.assertEqual(
                result.returncode,
                0,
                msg=f'gate unexpectedly failed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'pass')
            self.assertEqual(summary.get('evaluation_scope'), 'phase4-readlist-context-read-shadow')
            self.assertFalse(summary['governance']['cutover']['allowed'])
            self.assertFalse(summary['governance']['phase4_readlist_context_non_claims']['allowed'])
            self.assertEqual(
                summary['readlist_context_read_slice']['owned_routes'],
                [
                    'GET /api/v1/readlists/{readlistId}/books?unpaged=true',
                    'GET /api/v1/readlists/{readlistId}/books/{bookId}/previous',
                    'GET /api/v1/readlists/{readlistId}/books/{bookId}/next',
                ],
            )
            self.assertEqual(
                summary['readlist_context_read_slice']['out_of_slice'],
                [
                    'paged readlist books variants',
                    'library_id readlist-context variants',
                    'readlist list/detail routes',
                    'media delivery (/thumbnail, /file, /pages*, /manifest, /resource/*, /positions)',
                    'read-progress write/progression routes',
                    'oneshot closure',
                    'reader handoff and download branches',
                    'SSE/live-refresh parity',
                    'collection/readlist removals',
                    'admin edit/delete and broader write-path claims',
                    'full cutover/direct-serving approval',
                ],
            )

            contract_check = next(item for item in summary['checks'] if item['id'] == 'phase4_readlist_context_contract')
            self.assertEqual(contract_check['status'], 'pass')
            self.assertTrue(any('readlist-context' in detail for detail in contract_check['details']))

            shadow_check = next(item for item in summary['checks'] if item['id'] == 'phase4_readlist_context_shadow')
            self.assertEqual(shadow_check['status'], 'pass')
            self.assertTrue(any('native ownership' in detail.lower() for detail in shadow_check['details']))

            browser_check = next(item for item in summary['checks'] if item['id'] == 'phase4_readlist_context_browser')
            self.assertEqual(browser_check['status'], 'pass')
            self.assertTrue(any('source-contract-fallback' in detail for detail in browser_check['details']))

            report = (output_dir / 'report.md').read_text(encoding='utf-8')
            self.assertIn('Owned routes (exactly 3)', report)
            self.assertIn('`GET /api/v1/readlists/{readlistId}/books?unpaged=true`', report)
            self.assertIn('`GET /api/v1/readlists/{readlistId}/books/{bookId}/previous`', report)
            self.assertIn('`GET /api/v1/readlists/{readlistId}/books/{bookId}/next`', report)
            self.assertIn('Excluded branches still out of scope', report)
            self.assertIn('full cutover/direct-serving approval', report)

    def test_phase4_readlist_context_label_fails_closed_when_exclusion_artifact_is_missing(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            evidence_root = temp_path / 'evidence'
            output_dir = temp_path / 'output'
            seed_common_evidence(
                evidence_root,
                neutral_transcripts=True,
                admin_queue_payload=build_admin_queue_payload(
                    status='action-exercised-parity-ok',
                    can_claim_admin_queue_parity=True,
                ),
            )
            seed_phase4_readlist_context_evidence(evidence_root, include_exclusions=False)

            result = run_gate(evidence_root, output_dir, label='phase4-readlist-context-read')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(item for item in summary['checks'] if item['id'] == 'phase4_readlist_context_exclusions')
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase4_readlist_context_label_fails_closed_when_shadow_artifact_is_missing(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            evidence_root = temp_path / 'evidence'
            output_dir = temp_path / 'output'
            seed_common_evidence(
                evidence_root,
                neutral_transcripts=True,
                admin_queue_payload=build_admin_queue_payload(
                    status='action-exercised-parity-ok',
                    can_claim_admin_queue_parity=True,
                ),
            )
            seed_phase4_readlist_context_evidence(
                evidence_root,
                include_exclusions=True,
                include_shadow=False,
            )

            result = run_gate(evidence_root, output_dir, label='phase4-readlist-context-read')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(item for item in summary['checks'] if item['id'] == 'phase4_readlist_context_shadow')
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase4_readlist_context_label_fails_closed_when_browser_artifact_is_missing(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_path = Path(temp_dir)
            evidence_root = temp_path / 'evidence'
            output_dir = temp_path / 'output'
            seed_common_evidence(
                evidence_root,
                neutral_transcripts=True,
                admin_queue_payload=build_admin_queue_payload(
                    status='action-exercised-parity-ok',
                    can_claim_admin_queue_parity=True,
                ),
            )
            seed_phase4_readlist_context_evidence(
                evidence_root,
                include_exclusions=True,
                include_browser=False,
            )

            result = run_gate(evidence_root, output_dir, label='phase4-readlist-context-read')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(item for item in summary['checks'] if item['id'] == 'phase4_readlist_context_browser')
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))


if __name__ == '__main__':
    unittest.main()
