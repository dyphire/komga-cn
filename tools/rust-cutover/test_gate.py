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


if __name__ == '__main__':
    unittest.main()
