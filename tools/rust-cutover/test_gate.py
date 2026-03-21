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


if __name__ == '__main__':
    unittest.main()
