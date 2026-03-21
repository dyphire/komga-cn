#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

EVIDENCE_ROOT="$REPO_ROOT/.sisyphus/evidence"
OUTPUT_DIR="$EVIDENCE_ROOT/task-16-cutover"
RUN_LABEL="current"
REQUIRE_ALL="false"

usage() {
  cat <<'EOF'
Usage: tools/rust-cutover/gate.sh --require-all [--evidence-root PATH] [--output-dir PATH] [--label NAME]

Options:
  --require-all         Fail closed on any missing/regressed prerequisite.
  --evidence-root PATH  Override evidence root (default: .sisyphus/evidence).
  --output-dir PATH     Override output directory (default: .sisyphus/evidence/task-16-cutover).
  --label NAME          Suffix for per-run output files (default: current).
  --help                Show this message.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --require-all)
      REQUIRE_ALL="true"
      shift
      ;;
    --evidence-root)
      EVIDENCE_ROOT="$2"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="$2"
      shift 2
      ;;
    --label)
      RUN_LABEL="$2"
      shift 2
      ;;
    --help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ "$REQUIRE_ALL" != "true" ]]; then
  echo "Refusing to run without --require-all (fail-closed mode is mandatory for cutover gate)." >&2
  exit 2
fi

rtk mkdir -p "$OUTPUT_DIR"

python3 - "$REPO_ROOT" "$EVIDENCE_ROOT" "$OUTPUT_DIR" "$RUN_LABEL" <<'PY'
import json
import os
import re
import sys
from datetime import datetime, timezone
from pathlib import Path


repo_root = Path(sys.argv[1])
evidence_root = Path(sys.argv[2])
output_dir = Path(sys.argv[3])
run_label = sys.argv[4]

timestamp = datetime.now(timezone.utc).isoformat()


def rel(path: Path) -> str:
    try:
        return str(path.relative_to(repo_root))
    except ValueError:
        return str(path)


def read_json_file(path: Path) -> tuple[bool, object | None, str | None]:
    if not path.exists():
        return False, None, f"Missing evidence file: {rel(path)}"
    try:
        return True, json.loads(path.read_text(encoding="utf-8")), None
    except json.JSONDecodeError as exc:
        return False, None, f"Invalid JSON in {rel(path)}: {exc}"


def is_neutral_success_line(line: str) -> bool:
    neutral_patterns = [
        re.compile(r"\btest result:\s+ok\b", re.IGNORECASE),
        re.compile(r"\b0\s+(?:fail|failed|error|errors|blocker|blockers|missing)\b", re.IGNORECASE),
    ]
    return any(pattern.search(line) for pattern in neutral_patterns)


def has_explicit_failure_marker(line: str) -> bool:
    failure_patterns = [
        re.compile(r"\btest result:\s+FAILED\b", re.IGNORECASE),
        re.compile(r"\bBUILD FAILED\b", re.IGNORECASE),
        re.compile(r"\bGate result:\s+FAIL\b", re.IGNORECASE),
        re.compile(r"\bOverall result:\s+\*\*FAIL\*\*\b", re.IGNORECASE),
        re.compile(r"\bExit status:\s+non-zero\b", re.IGNORECASE),
        re.compile(r"\bAssertionError\b", re.IGNORECASE),
        re.compile(r"\bTraceback \(most recent call last\):\b", re.IGNORECASE),
        re.compile(r"\bpanic(?:!|:)\b", re.IGNORECASE),
        re.compile(r"^FAIL:\s", re.IGNORECASE),
        re.compile(r"^FAILED\s", re.IGNORECASE),
        re.compile(r"contains failure/blocker markers", re.IGNORECASE),
    ]
    return any(pattern.search(line) for pattern in failure_patterns)


def eval_text_evidence(paths: list[Path]) -> tuple[bool, list[str], list[str]]:
    missing = [rel(p) for p in paths if not p.exists()]
    if missing:
        return False, [f"Missing evidence file: {p}" for p in missing], []

    details: list[str] = []
    blockers: list[str] = []

    for p in paths:
        text = p.read_text(encoding="utf-8", errors="replace")
        if not text.strip():
            blockers.append(f"{rel(p)} is empty")
            continue

        file_has_blocker = False
        for line in text.splitlines():
            if is_neutral_success_line(line):
                continue
            if has_explicit_failure_marker(line):
                blockers.append(f"{rel(p)} contains explicit failure markers: {line.strip()}")
                file_has_blocker = True
                break

        if file_has_blocker:
            continue

        details.append(f"{rel(p)} present and non-empty with no explicit failure markers")

    return len(blockers) == 0, blockers if blockers else details, []


def eval_browser_smoke(summary_path: Path) -> tuple[bool, list[str], list[str]]:
    ok, payload, error = read_json_file(summary_path)
    if not ok:
        return False, [error], []

    required_routes = [
        "browse-libraries",
        "browse-series",
        "search",
        "server-management",
    ]
    if not isinstance(payload, list):
        return False, [f"Browser summary must be a JSON array: {rel(summary_path)}"], []

    rows = payload
    by_route = {row.get("route"): row for row in rows if isinstance(row, dict)}
    missing = [route for route in required_routes if route not in by_route]
    if missing:
        return False, [f"Missing route evidence in browser summary: {', '.join(missing)}"], []

    failed_routes = [route for route in required_routes if not bool(by_route[route].get("pass"))]
    if failed_routes:
        reasons = []
        for route in failed_routes:
            err = by_route[route].get("error") or "route did not pass"
            reasons.append(f"{route}: {err}")
        return False, [f"Browser smoke regressions: {'; '.join(reasons)}"], []

    return True, ["Browser smoke routes passed required acceptance slice"], []


def eval_task_ownership(task_ownership_path: Path, admin_queue_path: Path) -> tuple[bool, list[str], list[str]]:
    ok_text, text_messages, _ = eval_text_evidence([task_ownership_path])

    ok_json, payload, error = read_json_file(admin_queue_path)
    json_messages: list[str] = []
    if not ok_json:
        return False, text_messages + [error], []

    if not isinstance(payload, dict):
        return False, text_messages + [f"{rel(admin_queue_path)} must be a JSON object"], []

    if "pass" in payload and not bool(payload.get("pass")):
        return False, text_messages + [f"{rel(admin_queue_path)} indicates pass=false"], []

    if "status" in payload:
        status_text = str(payload.get("status", "")).strip().lower()
        if re.search(r"(^|[-_])(fail|failed|error|refuse|refused)([-_]|$)", status_text):
            return False, text_messages + [f"{rel(admin_queue_path)} has failing status: {status_text}"], []

    parity = payload.get("parityConclusion")
    if not isinstance(parity, dict):
        return False, text_messages + [f"{rel(admin_queue_path)} is missing parityConclusion object"], []

    if "canClaimAdminQueueParity" not in parity:
        return False, text_messages + [f"{rel(admin_queue_path)} is missing parityConclusion.canClaimAdminQueueParity"], []

    if parity.get("canClaimAdminQueueParity") is not True:
        reason = parity.get("reason") or "parityConclusion.canClaimAdminQueueParity is not true"
        return False, text_messages + [f"{rel(admin_queue_path)} rejects admin queue parity: {reason}"], []

    if isinstance(payload, (dict, list)) and len(payload) == 0:
        return False, text_messages + [f"{rel(admin_queue_path)} is an empty JSON artifact"], []

    json_messages.append(f"{rel(admin_queue_path)} confirms parityConclusion.canClaimAdminQueueParity=true")
    messages = text_messages + json_messages
    return ok_text, messages, []


def eval_ops_server_management(path: Path) -> tuple[bool, list[str], list[str]]:
    ok, payload, error = read_json_file(path)
    if not ok:
        return False, [error], []

    if not isinstance(payload, dict):
        return False, [f"{rel(path)} must be a JSON object"], []

    if "pass" in payload and not bool(payload.get("pass")):
        detail = payload.get("error") or "pass=false"
        return False, [f"{rel(path)} indicates failure: {detail}"], []

    if "route" in payload and payload.get("route") != "server-management":
        return False, [f"{rel(path)} route mismatch: expected server-management"], []

    if "selector" in payload and payload.get("selector") != "[data-testid=\"server-management-root\"]":
        return False, [f"{rel(path)} selector mismatch for server-management root"], []

    return True, [f"{rel(path)} is valid structured server-management evidence"], []


def eval_packaging_artifacts(runtime_startup_path: Path, tray_compat_path: Path) -> tuple[bool, list[str], list[str]]:
    ok_text, text_messages, _ = eval_text_evidence([runtime_startup_path, tray_compat_path])
    return ok_text, text_messages, []


def eval_release_credentials() -> tuple[bool, list[str], list[str]]:
    token = os.getenv("JRELEASER_GITHUB_TOKEN", "")
    if token.strip():
        return True, ["JRELEASER_GITHUB_TOKEN is set"], []
    return False, ["Missing external release credential: JRELEASER_GITHUB_TOKEN"], []


checks = [
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
    },
]

results = []
refusals = []

for check in checks:
    mode = check["mode"]
    evidence_paths: list[Path] = check["evidence"]

    if mode == "browser_ops":
        browser_summary = evidence_paths[0]
        browser_ok, browser_msgs, _ = eval_browser_smoke(browser_summary)
        ops_ok, ops_msgs, _ = eval_ops_server_management(evidence_paths[1])
        ok = browser_ok and ops_ok
        messages = browser_msgs + ops_msgs
    elif mode == "task_ownership":
        ok, messages, _ = eval_task_ownership(evidence_paths[0], evidence_paths[1])
    elif mode == "packaging":
        ok, messages, _ = eval_packaging_artifacts(evidence_paths[0], evidence_paths[1])
    elif mode == "credential":
        ok, messages, _ = eval_release_credentials()
    else:
        ok, messages, _ = eval_text_evidence(evidence_paths)

    result = {
        "id": check["id"],
        "category": check["category"],
        "status": "pass" if ok else "fail",
        "refusal_condition": check["refusal_condition"],
        "evidence": [rel(p) for p in evidence_paths],
        "details": messages,
    }
    results.append(result)
    if not ok:
        refusals.append({
            "id": check["id"],
            "reason": check["refusal_condition"],
            "details": messages,
        })

overall_pass = len(refusals) == 0

governance = {
    "shadow_mode": {
        "allowed": next(r for r in results if r["id"] == "shadow_safety")["status"] == "pass",
        "rule": "Shadow mode must keep Java as stateful writer unless explicitly isolated",
    },
    "canary_mode": {
        "allowed": next(r for r in results if r["id"] in {"search_ownership", "task_ownership"})["status"] == "pass",
        "rule": "Canary requires explicit ownership guardrails for search/tasks before any writable scope",
    },
    "cutover": {
        "allowed": overall_pass,
        "scope": "All refusal conditions must pass before traffic cutover",
    },
    "rollback": {
        "ready": overall_pass,
        "trigger": "Any gate regression or missing prerequisite forces rollback/no-cutover",
    },
}

summary = {
    "task": "T16",
    "generated_at": timestamp,
    "run_label": run_label,
    "require_all": True,
    "evidence_root": rel(evidence_root),
    "overall": "pass" if overall_pass else "fail",
    "checks": results,
    "refusals": refusals,
    "governance": governance,
}

output_dir.mkdir(parents=True, exist_ok=True)
summary_latest = output_dir / "summary.json"
summary_labeled = output_dir / f"summary-{run_label}.json"
summary_text = json.dumps(summary, ensure_ascii=False, indent=2) + "\n"
summary_latest.write_text(summary_text, encoding="utf-8")
summary_labeled.write_text(summary_text, encoding="utf-8")

lines = []
lines.append(f"# Rust Cutover Readiness Gate ({run_label})")
lines.append("")
lines.append(f"- Generated at: {timestamp}")
lines.append(f"- Evidence root: `{rel(evidence_root)}`")
lines.append(f"- Overall result: **{summary['overall'].upper()}**")
lines.append("")
lines.append("## Check Results")
lines.append("")
for r in results:
    emoji = "✅" if r["status"] == "pass" else "❌"
    lines.append(f"- {emoji} `{r['id']}` ({r['category']}): {r['refusal_condition']}")
    for ev in r["evidence"]:
        lines.append(f"  - evidence: `{ev}`")
    for msg in r["details"]:
        lines.append(f"  - detail: {msg}")

lines.append("")
lines.append("## Refusal Reasons")
lines.append("")
if refusals:
    for ref in refusals:
        lines.append(f"- `{ref['id']}`: {ref['reason']}")
        for msg in ref["details"]:
            lines.append(f"  - {msg}")
else:
    lines.append("- None")

lines.append("")
lines.append("## Governance")
lines.append("")
for key in ["shadow_mode", "canary_mode", "cutover", "rollback"]:
    entry = governance[key]
    flag = entry.get("allowed", entry.get("ready", False))
    lines.append(f"- `{key}`: {'ALLOW' if flag else 'REFUSE'} — {entry.get('rule', entry.get('scope', entry.get('trigger', '')))}")

report_text = "\n".join(lines) + "\n"
report_latest = output_dir / "report.md"
report_labeled = output_dir / f"report-{run_label}.md"
report_latest.write_text(report_text, encoding="utf-8")
report_labeled.write_text(report_text, encoding="utf-8")

print(f"Gate result: {summary['overall'].upper()}")
print(f"Summary: {rel(summary_latest)}")
print(f"Report:  {rel(report_latest)}")

sys.exit(0 if overall_pass else 1)
PY
