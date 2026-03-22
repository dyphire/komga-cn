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

python3 "$SCRIPT_DIR/gate_eval.py" "$REPO_ROOT" "$EVIDENCE_ROOT" "$OUTPUT_DIR" "$RUN_LABEL"
