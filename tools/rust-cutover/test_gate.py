import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
if str(SCRIPT_DIR) not in sys.path:
    sys.path.insert(0, str(SCRIPT_DIR))

import test_gate_fixtures as fixtures


REPO_ROOT = Path(__file__).resolve().parents[2]
GATE_SCRIPT = REPO_ROOT / 'tools' / 'rust-cutover' / 'gate.sh'

build_admin_queue_payload = fixtures.build_admin_queue_payload
seed_common_evidence = fixtures.seed_common_evidence
seed_phase3_detail_evidence = fixtures.seed_phase3_detail_evidence
seed_phase4_readlist_context_evidence = fixtures.seed_phase4_readlist_context_evidence
seed_phase5_oneshot_closure_evidence = fixtures.seed_phase5_oneshot_closure_evidence
seed_phase6_oneshot_readlist_context_closure_evidence = fixtures.seed_phase6_oneshot_readlist_context_closure_evidence
seed_phase7_series_oneshot_query_closure_evidence = fixtures.seed_phase7_series_oneshot_query_closure_evidence
seed_phase8_readlist_books_family_closure_evidence = fixtures.seed_phase8_readlist_books_family_closure_evidence
seed_phase9_readlists_list_browse_closure_evidence = fixtures.seed_phase9_readlists_list_browse_closure_evidence


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

    def test_phase5_oneshot_closure_label_passes_with_slice_evidence_only(self) -> None:
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
            seed_phase5_oneshot_closure_evidence(evidence_root)

            result = run_gate(evidence_root, output_dir, label='phase5-oneshot-closure')

            self.assertEqual(
                result.returncode,
                0,
                msg=f'gate unexpectedly failed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'pass')
            self.assertEqual(summary.get('evaluation_scope'), 'phase5-oneshot-closure-shadow')
            self.assertFalse(summary['governance']['cutover']['allowed'])
            self.assertFalse(summary['governance']['phase5_oneshot_non_claims']['allowed'])
            self.assertEqual(
                summary['oneshot_closure_slice']['owned_routes'],
                ['POST /api/v1/books/list (oneshot-bootstrap SeriesId-only family for direct /oneshot/:seriesId closure)'],
            )
            self.assertEqual(
                summary['oneshot_closure_slice']['excluded_branches'],
                [
                    'GET /api/v1/series/{seriesId}?oneshot=true',
                    'READLIST-context fallback and readlist detail/list/next/previous branches',
                    'generic books/list widening beyond oneshot-bootstrap SeriesId-only',
                    'media delivery (/thumbnail, /file, /pages*, /manifest, /resource/*, /positions)',
                    'reader handoff and download branches',
                    'read-progress write/progression routes',
                    'collection/readlist removals',
                    'admin edit/delete and broader write-path claims',
                    'SSE/live-refresh parity',
                    'full cutover/direct-serving approval',
                ],
            )

            browser_check = next(item for item in summary['checks'] if item['id'] == 'phase5_oneshot_closure_browser')
            self.assertEqual(browser_check['status'], 'pass')
            self.assertTrue(any('oneshot-bootstrap-books-list' in detail for detail in browser_check['details']))

            report = (output_dir / 'report.md').read_text(encoding='utf-8')
            self.assertIn('Owned surface (newly owned exactly 1 family)', report)
            self.assertIn('`POST /api/v1/books/list`', report)
            self.assertIn('`GET /api/v1/series/{seriesId}?oneshot=true`', report)
            self.assertIn('READLIST-context fallback', report)
            self.assertIn('reader handoff and download branches', report)
            self.assertIn('whole cutover/direct-serving', report)

    def test_phase5_oneshot_closure_label_fails_closed_when_contract_artifact_is_missing(self) -> None:
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
            seed_phase5_oneshot_closure_evidence(evidence_root, include_contract=False)

            result = run_gate(evidence_root, output_dir, label='phase5-oneshot-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(item for item in summary['checks'] if item['id'] == 'phase5_oneshot_closure_contract')
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase5_oneshot_closure_label_fails_closed_when_exclusion_artifact_is_missing(self) -> None:
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
            seed_phase5_oneshot_closure_evidence(evidence_root, include_exclusions=False)

            result = run_gate(evidence_root, output_dir, label='phase5-oneshot-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(item for item in summary['checks'] if item['id'] == 'phase5_oneshot_closure_exclusions')
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase5_oneshot_closure_label_fails_closed_when_shadow_artifact_is_missing(self) -> None:
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
            seed_phase5_oneshot_closure_evidence(evidence_root, include_shadow=False)

            result = run_gate(evidence_root, output_dir, label='phase5-oneshot-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(item for item in summary['checks'] if item['id'] == 'phase5_oneshot_closure_shadow')
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase5_oneshot_closure_label_fails_closed_when_browser_artifact_is_missing(self) -> None:
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
            seed_phase5_oneshot_closure_evidence(evidence_root, include_browser=False)

            result = run_gate(evidence_root, output_dir, label='phase5-oneshot-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(item for item in summary['checks'] if item['id'] == 'phase5_oneshot_closure_browser')
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase6_oneshot_readlist_context_closure_label_passes_with_complete_seeded_evidence(self) -> None:
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
            seed_phase6_oneshot_readlist_context_closure_evidence(evidence_root)

            result = run_gate(evidence_root, output_dir, label='phase6-oneshot-readlist-context-closure')

            self.assertEqual(
                result.returncode,
                0,
                msg=f'gate unexpectedly failed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'pass')
            self.assertEqual(summary.get('evaluation_scope'), 'phase6-oneshot-readlist-context-closure-shadow')
            self.assertFalse(summary['governance']['cutover']['allowed'])
            self.assertFalse(summary['governance']['phase6_oneshot_readlist_context_non_claims']['allowed'])
            self.assertEqual(
                summary['oneshot_readlist_context_closure_slice']['owned_routes'],
                ['GET /api/v1/readlists/{readListId} (oneshot READLIST-context direct-read closure only)'],
            )
            self.assertEqual(
                summary['oneshot_readlist_context_closure_slice']['required_pre_owned_dependencies'],
                [
                    'GET /api/v1/series/{seriesId}',
                    'GET /api/v1/series/{seriesId}/collections',
                    'POST /api/v1/books/list (exact oneshot-bootstrap SeriesId-only family)',
                    'GET /api/v1/books/{bookId}/readlists',
                    'GET /api/v1/readlists/{readListId}/books?unpaged=true',
                    'GET /api/v1/readlists/{readListId}/books/{bookId}/previous',
                    'GET /api/v1/readlists/{readListId}/books/{bookId}/next',
                ],
            )
            self.assertEqual(
                summary['oneshot_readlist_context_closure_slice']['excluded_branches'],
                [
                    'GET /api/v1/series/{seriesId}?oneshot=true',
                    'GET /api/v1/readlists and other readlist list-family routes',
                    'paged or library_id readlist books variants',
                    'browse-readlist page closure',
                    'generic books/list widening beyond the exact oneshot-bootstrap SeriesId-only family',
                    'media delivery (/thumbnail, /file, /pages*, /manifest, /resource/*, /positions)',
                    'reader handoff and download branches',
                    'read-progress write/progression routes',
                    'collection/readlist removals',
                    'admin edit/delete and broader write-path claims',
                    'SSE/live-refresh parity',
                    'full cutover/direct-serving approval',
                ],
            )

            browser_check = next(
                item for item in summary['checks'] if item['id'] == 'phase6_oneshot_readlist_context_closure_browser'
            )
            self.assertEqual(browser_check['status'], 'pass')
            self.assertTrue(any('readlist-detail' in detail for detail in browser_check['details']))

            report = (output_dir / 'report.md').read_text(encoding='utf-8')
            self.assertIn('Owned surface (newly owned exactly 1 route)', report)
            self.assertIn('`GET /api/v1/readlists/{readListId}`', report)
            self.assertIn('oneshot READLIST-context direct-read', report)
            self.assertIn('`GET /api/v1/readlists`', report)
            self.assertIn('browse-readlist', report)
            self.assertIn('whole cutover/direct-serving', report)

    def test_phase6_oneshot_readlist_context_closure_label_fails_closed_when_contract_artifact_is_missing(self) -> None:
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
            seed_phase6_oneshot_readlist_context_closure_evidence(evidence_root, include_contract=False)

            result = run_gate(evidence_root, output_dir, label='phase6-oneshot-readlist-context-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(
                item for item in summary['checks'] if item['id'] == 'phase6_oneshot_readlist_context_closure_contract'
            )
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase6_oneshot_readlist_context_closure_label_fails_closed_when_browser_artifact_is_missing(self) -> None:
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
            seed_phase6_oneshot_readlist_context_closure_evidence(evidence_root, include_browser=False)

            result = run_gate(evidence_root, output_dir, label='phase6-oneshot-readlist-context-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(
                item for item in summary['checks'] if item['id'] == 'phase6_oneshot_readlist_context_closure_browser'
            )
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase6_oneshot_readlist_context_closure_label_fails_closed_when_regression_artifact_is_missing(self) -> None:
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
            seed_phase6_oneshot_readlist_context_closure_evidence(evidence_root, include_regression=False)

            result = run_gate(evidence_root, output_dir, label='phase6-oneshot-readlist-context-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(
                item for item in summary['checks'] if item['id'] == 'phase6_oneshot_readlist_context_closure_regression'
            )
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase7_series_oneshot_query_closure_label_passes_with_slice_evidence_only(self) -> None:
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
            seed_phase7_series_oneshot_query_closure_evidence(evidence_root)

            result = run_gate(evidence_root, output_dir, label='phase7-series-oneshot-query-closure')

            self.assertEqual(
                result.returncode,
                0,
                msg=f'gate unexpectedly failed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'pass')
            self.assertEqual(summary.get('evaluation_scope'), 'phase7-series-oneshot-query-closure-shadow')
            self.assertFalse(summary['governance']['cutover']['allowed'])
            self.assertFalse(summary['governance']['phase7_series_oneshot_query_non_claims']['allowed'])
            self.assertEqual(
                summary['series_oneshot_query_closure_slice']['owned_routes'],
                ['GET /api/v1/series/{seriesId}?oneshot=true'],
            )
            self.assertEqual(
                summary['series_oneshot_query_closure_slice']['required_pre_owned_dependencies'],
                [
                    'GET /api/v1/series/{seriesId}',
                    'GET /api/v1/series/{seriesId}/collections',
                    'POST /api/v1/books/list (exact oneshot-bootstrap SeriesId-only family)',
                    'GET /api/v1/books/{bookId}/readlists',
                    'GET /api/v1/readlists/{readListId}',
                    'GET /api/v1/readlists/{readListId}/books?unpaged=true',
                    'GET /api/v1/readlists/{readListId}/books/{bookId}/previous',
                    'GET /api/v1/readlists/{readListId}/books/{bookId}/next',
                ],
            )
            self.assertEqual(
                summary['series_oneshot_query_closure_slice']['excluded_branches'],
                [
                    'negative/mixed oneshot query variants beyond exact ?oneshot=true',
                    'browse-oneshot page closure or browser-owned inventory promotion',
                    'GET /api/v1/readlists and other readlist list-family routes',
                    'browse-readlist page closure',
                    'paged or library_id readlist books variants',
                    'generic books/list widening beyond the exact oneshot-bootstrap SeriesId-only family',
                    'media delivery (/thumbnail, /file, /pages*, /manifest, /resource/*, /positions)',
                    'reader handoff and download branches',
                    'read-progress write/progression routes',
                    'collection/readlist removals',
                    'admin edit/delete and broader write-path claims',
                    'SSE/live-refresh parity',
                    'full cutover/direct-serving approval',
                ],
            )

            browser_check = next(
                item for item in summary['checks'] if item['id'] == 'phase7_series_oneshot_query_closure_browser'
            )
            self.assertEqual(browser_check['status'], 'pass')
            self.assertTrue(any('source-contract-fallback' in detail for detail in browser_check['details']))
            self.assertTrue(any('governance-only evidence' in detail for detail in browser_check['details']))

            report = (output_dir / 'report.md').read_text(encoding='utf-8')
            self.assertIn('Owned surface (newly owned exactly 1 route)', report)
            self.assertIn('`GET /api/v1/series/{seriesId}?oneshot=true`', report)
            self.assertIn('browse-oneshot remains source-contract-fallback / scenario-level fallback only', report)
            self.assertIn('`GET /api/v1/readlists`', report)
            self.assertIn('browse-readlist', report)
            self.assertIn('whole cutover/direct-serving', report)

    def test_phase7_series_oneshot_query_closure_label_fails_closed_when_contract_artifact_is_missing(self) -> None:
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
            seed_phase7_series_oneshot_query_closure_evidence(evidence_root, include_contract=False)

            result = run_gate(evidence_root, output_dir, label='phase7-series-oneshot-query-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(
                item for item in summary['checks'] if item['id'] == 'phase7_series_oneshot_query_closure_contract'
            )
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase7_series_oneshot_query_closure_label_fails_closed_when_shadow_artifact_is_missing(self) -> None:
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
            seed_phase7_series_oneshot_query_closure_evidence(evidence_root, include_shadow=False)

            result = run_gate(evidence_root, output_dir, label='phase7-series-oneshot-query-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(
                item for item in summary['checks'] if item['id'] == 'phase7_series_oneshot_query_closure_shadow'
            )
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase7_series_oneshot_query_closure_label_fails_closed_when_browser_artifact_is_missing(self) -> None:
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
            seed_phase7_series_oneshot_query_closure_evidence(evidence_root, include_browser=False)

            result = run_gate(evidence_root, output_dir, label='phase7-series-oneshot-query-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(
                item for item in summary['checks'] if item['id'] == 'phase7_series_oneshot_query_closure_browser'
            )
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase8_readlist_books_family_closure_label_passes_with_slice_evidence_only(self) -> None:
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
            seed_phase8_readlist_books_family_closure_evidence(evidence_root)

            result = run_gate(evidence_root, output_dir, label='phase8-readlist-books-family-closure')

            self.assertEqual(
                result.returncode,
                0,
                msg=f'gate unexpectedly failed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'pass')
            self.assertEqual(summary.get('evaluation_scope'), 'phase8-readlist-books-family-closure-shadow')
            self.assertFalse(summary['governance']['cutover']['allowed'])
            self.assertFalse(summary['governance']['phase8_readlist_books_family_non_claims']['allowed'])
            self.assertEqual(
                summary['readlist_books_family_closure_slice']['required_pre_owned_dependencies'],
                [
                    'GET /api/v1/readlists/{readListId} (Phase 6, regression-only here)',
                    'GET /api/v1/readlists/{readListId}/books?unpaged=true (Phase 4, regression-only here)',
                    'GET /api/v1/readlists/{readListId}/books/{bookId}/previous (Phase 4, regression-only here)',
                    'GET /api/v1/readlists/{readListId}/books/{bookId}/next (Phase 4, regression-only here)',
                    'GET /api/v1/books/{bookId}/readlists (Phase 3, regression-only here)',
                ],
            )
            self.assertIn(
                'GET /api/v1/readlists and every list-family variant (search/unpaged/paging/library filters)',
                summary['readlist_books_family_closure_slice']['excluded_branches'],
            )
            self.assertIn(
                'GET /api/v1/readlists/{readListId}/read-progress/tachiyomi',
                summary['readlist_books_family_closure_slice']['excluded_branches'],
            )

            browser_check = next(
                item for item in summary['checks'] if item['id'] == 'phase8_readlist_books_family_browser'
            )
            self.assertEqual(browser_check['status'], 'pass')
            self.assertTrue(any('browse-readlist' in detail for detail in browser_check['details']))
            self.assertTrue(any('source-contract-fallback' in detail for detail in browser_check['details']))

            compat_check = next(
                item for item in summary['checks'] if item['id'] == 'phase8_readlist_books_family_compat'
            )
            self.assertEqual(compat_check['status'], 'pass')

            regression_check = next(
                item for item in summary['checks'] if item['id'] == 'phase8_readlist_books_family_regression'
            )
            self.assertEqual(regression_check['status'], 'pass')

            report = (output_dir / 'report.md').read_text(encoding='utf-8')
            self.assertIn('Phase8 Readlist-Books-Family-Closure Runbook', report)
            self.assertIn('`GET /api/v1/readlists/{readListId}/books`', report)
            self.assertIn('`GET /api/v1/readlists` list-family', report)
            self.assertIn('`GET /api/v1/readlists/{readListId}/read-progress/tachiyomi`', report)
            self.assertIn('BrowseReadList evidence stays limited', report)

    def test_phase8_readlist_books_family_closure_label_fails_closed_when_exclusion_artifact_is_missing(self) -> None:
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
            seed_phase8_readlist_books_family_closure_evidence(
                evidence_root,
                include_exclusions=False,
            )

            result = run_gate(evidence_root, output_dir, label='phase8-readlist-books-family-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(
                item for item in summary['checks'] if item['id'] == 'phase8_readlist_books_family_exclusions'
            )
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase8_readlist_books_family_closure_label_fails_closed_when_browser_artifact_is_missing(self) -> None:
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
            seed_phase8_readlist_books_family_closure_evidence(
                evidence_root,
                include_browser=False,
            )

            result = run_gate(evidence_root, output_dir, label='phase8-readlist-books-family-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(
                item for item in summary['checks'] if item['id'] == 'phase8_readlist_books_family_browser'
            )
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase8_readlist_books_family_closure_label_fails_closed_when_compat_artifact_is_missing(self) -> None:
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
            seed_phase8_readlist_books_family_closure_evidence(
                evidence_root,
                include_compat=False,
            )

            result = run_gate(evidence_root, output_dir, label='phase8-readlist-books-family-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(
                item for item in summary['checks'] if item['id'] == 'phase8_readlist_books_family_compat'
            )
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase8_readlist_books_family_closure_label_fails_closed_when_contract_matrix_is_incomplete(self) -> None:
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
            seed_phase8_readlist_books_family_closure_evidence(
                evidence_root,
                complete_contract_matrix=False,
            )

            result = run_gate(evidence_root, output_dir, label='phase8-readlist-books-family-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(
                item for item in summary['checks'] if item['id'] == 'phase8_readlist_books_family_contract'
            )
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('missing required discovery markers' in detail for detail in failing_check['details']))

    def test_phase9_readlists_list_browse_closure_label_passes_with_slice_evidence_only(self) -> None:
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
            seed_phase9_readlists_list_browse_closure_evidence(evidence_root)

            result = run_gate(evidence_root, output_dir, label='phase9-readlists-list-browse-closure')

            self.assertEqual(
                result.returncode,
                0,
                msg=f'gate unexpectedly failed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'pass')
            self.assertEqual(summary.get('evaluation_scope'), 'phase9-readlists-list-browse-closure-shadow')
            self.assertFalse(summary['governance']['cutover']['allowed'])
            self.assertFalse(summary['governance']['phase9_readlists_list_browse_non_claims']['allowed'])
            self.assertEqual(
                summary['readlists_list_browse_closure_slice']['required_pre_owned_dependencies'],
                [
                    'GET /api/v1/readlists/{readListId} (Phase 6, regression-only here)',
                    'GET /api/v1/readlists/{readListId}/books?unpaged=true (Phase 4, regression-only here)',
                    'GET /api/v1/readlists/{readListId}/books/{bookId}/previous (Phase 4, regression-only here)',
                    'GET /api/v1/readlists/{readListId}/books/{bookId}/next (Phase 4, regression-only here)',
                    'Direct paged/filter GET /api/v1/readlists/{readListId}/books family (Phase 8, regression-only here)',
                ],
            )
            self.assertIn(
                'GET /api/v1/readlists?search={term} and every search-bearing variant',
                summary['readlists_list_browse_closure_slice']['excluded_branches'],
            )
            self.assertIn(
                'GET /api/v1/readlists/{readListId}/read-progress/tachiyomi',
                summary['readlists_list_browse_closure_slice']['excluded_branches'],
            )

            browser_check = next(
                item for item in summary['checks'] if item['id'] == 'phase9_readlists_list_browse_browser'
            )
            self.assertEqual(browser_check['status'], 'pass')
            self.assertTrue(any('browse-readlists' in detail for detail in browser_check['details']))
            self.assertTrue(any('source-contract-fallback' in detail for detail in browser_check['details']))

            compat_check = next(
                item for item in summary['checks'] if item['id'] == 'phase9_readlists_list_browse_compat'
            )
            self.assertEqual(compat_check['status'], 'pass')

            exclusions_check = next(
                item for item in summary['checks'] if item['id'] == 'phase9_readlists_list_browse_exclusions'
            )
            self.assertEqual(exclusions_check['status'], 'pass')

            report = (output_dir / 'report.md').read_text(encoding='utf-8')
            self.assertIn('Phase9 Readlists-List-Browse-Closure Runbook', report)
            self.assertIn('`GET /api/v1/readlists (default browse; search/unpaged/explicit sort omitted)`', report)
            self.assertIn('`GET /api/v1/readlists?unpaged=true`', report)
            self.assertIn('`GET /api/v1/readlists/{readListId}/read-progress/tachiyomi`', report)
            self.assertIn('BrowseReadLists evidence stays limited', report)

    def test_phase9_readlists_list_browse_closure_label_fails_closed_when_exclusion_artifact_is_missing(self) -> None:
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
            seed_phase9_readlists_list_browse_closure_evidence(
                evidence_root,
                include_exclusions=False,
            )

            result = run_gate(evidence_root, output_dir, label='phase9-readlists-list-browse-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(
                item for item in summary['checks'] if item['id'] == 'phase9_readlists_list_browse_exclusions'
            )
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase9_readlists_list_browse_closure_label_fails_closed_when_browser_artifact_is_missing(self) -> None:
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
            seed_phase9_readlists_list_browse_closure_evidence(
                evidence_root,
                include_browser=False,
            )

            result = run_gate(evidence_root, output_dir, label='phase9-readlists-list-browse-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(
                item for item in summary['checks'] if item['id'] == 'phase9_readlists_list_browse_browser'
            )
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase9_readlists_list_browse_closure_label_fails_closed_when_compat_artifact_is_missing(self) -> None:
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
            seed_phase9_readlists_list_browse_closure_evidence(
                evidence_root,
                include_compat=False,
            )

            result = run_gate(evidence_root, output_dir, label='phase9-readlists-list-browse-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(
                item for item in summary['checks'] if item['id'] == 'phase9_readlists_list_browse_compat'
            )
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('Missing evidence file' in detail for detail in failing_check['details']))

    def test_phase9_readlists_list_browse_closure_label_fails_closed_when_contract_matrix_is_incomplete(self) -> None:
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
            seed_phase9_readlists_list_browse_closure_evidence(
                evidence_root,
                complete_contract_matrix=False,
            )

            result = run_gate(evidence_root, output_dir, label='phase9-readlists-list-browse-closure')

            self.assertEqual(
                result.returncode,
                1,
                msg=f'gate unexpectedly passed\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}',
            )

            summary = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
            self.assertEqual(summary['overall'], 'fail')
            failing_check = next(
                item for item in summary['checks'] if item['id'] == 'phase9_readlists_list_browse_contract'
            )
            self.assertEqual(failing_check['status'], 'fail')
            self.assertTrue(any('missing required discovery markers' in detail for detail in failing_check['details']))


if __name__ == '__main__':
    unittest.main()
