import json
import sys
from datetime import datetime, timezone
from pathlib import Path

import gate_eval_data as data
import gate_eval_reporting as reporting
from gate_eval_evaluators import GateEvaluators


def main() -> int:
    repo_root = Path(sys.argv[1])
    evidence_root = Path(sys.argv[2])
    output_dir = Path(sys.argv[3])
    run_label = sys.argv[4]

    timestamp = datetime.now(timezone.utc).isoformat()
    evaluators = GateEvaluators(repo_root)

    checks, discovery_checks, phase3_checks, phase4_checks, phase5_checks, phase6_checks, phase8_checks, phase9_checks = data.build_checks(run_label, evidence_root)

    is_phase2_discovery = run_label == data.PHASE2_DISCOVERY_LABEL
    is_phase3_detail_read = run_label == data.PHASE3_DETAIL_READ_LABEL
    is_phase4_readlist_context_read = run_label == data.PHASE4_READLIST_CONTEXT_READ_LABEL
    is_phase5_oneshot_closure = run_label == data.PHASE5_ONESHOT_CLOSURE_LABEL
    is_phase6_oneshot_readlist_context_closure = run_label == data.PHASE6_ONESHOT_READLIST_CONTEXT_CLOSURE_LABEL
    is_phase8_readlist_books_family_closure = run_label == data.PHASE8_READLIST_BOOKS_FAMILY_CLOSURE_LABEL
    is_phase9_readlists_list_browse_closure = run_label == data.PHASE9_READLISTS_LIST_BROWSE_CLOSURE_LABEL

    results = []
    refusals = []
    non_blocking = []

    for check in checks:
        mode = check["mode"]
        evidence_paths: list[Path] = check["evidence"]
        override = check.get("profile_overrides", {}).get(run_label)
        blocking = check.get("blocking", True)

        if override:
            ok = override.get("status") == "pass"
            status = override["status"]
            messages = list(override.get("details", []))
            blocking = override.get("blocking", blocking)
        elif is_phase3_detail_read and check["id"] in data.phase3_skipped_base_checks:
            ok = False
            status = "skipped"
            blocking = False
            messages = [
                data.phase3_skipped_base_checks[check["id"]],
                "This label proves direct-browse detail-read readiness only and does not approve whole-cutover/media/write scope.",
            ]
        elif is_phase4_readlist_context_read and check["id"] in data.phase4_skipped_base_checks:
            ok = False
            status = "skipped"
            blocking = False
            messages = [
                data.phase4_skipped_base_checks[check["id"]],
                "This label proves readlist-context-read readiness only and does not approve whole-cutover/media/write scope.",
            ]
        elif is_phase5_oneshot_closure and check["id"] in data.phase5_skipped_base_checks:
            ok = False
            status = "skipped"
            blocking = False
            messages = [
                data.phase5_skipped_base_checks[check["id"]],
                "This label proves oneshot-closure readiness only and does not approve whole-cutover/media/write scope.",
            ]
        elif is_phase6_oneshot_readlist_context_closure and check["id"] in data.phase6_skipped_base_checks:
            ok = False
            status = "skipped"
            blocking = False
            messages = [
                data.phase6_skipped_base_checks[check["id"]],
                "This label proves oneshot READLIST-context direct-read readiness only and does not approve browse-readlist/list-family/media/write/whole-cutover scope.",
            ]
        elif is_phase9_readlists_list_browse_closure and check["id"] in data.phase9_skipped_base_checks:
            ok = False
            status = "skipped"
            blocking = False
            messages = [
                data.phase9_skipped_base_checks[check["id"]],
                "This label proves readlists browse/list closure readiness only and does not approve search/dialog/admin/Tachiyomi/media/write/whole-cutover scope.",
            ]
        else:
            if mode == "browser_ops":
                browser_summary = evidence_paths[0]
                browser_ok, browser_msgs, _ = evaluators.eval_browser_smoke(browser_summary)
                ops_ok, ops_msgs, _ = evaluators.eval_ops_server_management(evidence_paths[1])
                ok = browser_ok and ops_ok
                messages = browser_msgs + ops_msgs
            elif mode == "phase3_browser_smoke":
                ok, messages, _ = evaluators.eval_phase3_browser_smoke(evidence_paths[0])
            elif mode == "phase4_browser_smoke":
                ok, messages, _ = evaluators.eval_phase4_browser_smoke(evidence_paths[0], evidence_paths[1], evidence_paths[2])
            elif mode == "phase5_browser_smoke":
                ok, messages, _ = evaluators.eval_phase5_browser_smoke(evidence_paths[0], evidence_paths[1], evidence_paths[2])
            elif mode == "phase8_browser_smoke":
                ok, messages, _ = evaluators.eval_phase8_browser_smoke(
                    evidence_paths[0],
                    evidence_paths[1],
                    evidence_paths[2],
                )
            elif mode == "phase9_browser_smoke":
                ok, messages, _ = evaluators.eval_phase9_browser_smoke(
                    evidence_paths[0],
                    evidence_paths[1],
                    evidence_paths[2],
                )
            elif mode == "task_ownership":
                ok, messages, _ = evaluators.eval_task_ownership(evidence_paths[0], evidence_paths[1])
            elif mode == "packaging":
                ok, messages, _ = evaluators.eval_packaging_artifacts(evidence_paths[0], evidence_paths[1])
            elif mode == "credential":
                ok, messages, _ = evaluators.eval_release_credentials()
            elif mode == "discovery_markers":
                ok, messages, _ = evaluators.eval_text_evidence_with_markers(
                    evidence_paths,
                    check.get("marker_map"),
                    check.get("success_note"),
                )
            else:
                ok, messages, _ = evaluators.eval_text_evidence(evidence_paths)
            status = "pass" if ok else "fail"

        result = {
            "id": check["id"],
            "category": check["category"],
            "status": status,
            "blocking": blocking,
            "refusal_condition": check["refusal_condition"],
            "evidence": [evaluators.rel(p) for p in evidence_paths],
            "details": messages,
        }
        results.append(result)
        if status == "fail" and blocking:
            refusals.append({
                "id": check["id"],
                "reason": check["refusal_condition"],
                "details": messages,
            })
        elif status != "pass" and not blocking:
            non_blocking.append({
                "id": check["id"],
                "status": status,
                "details": messages,
            })

    overall_pass = len(refusals) == 0

    search_task_guardrails = all(
        next(r for r in results if r["id"] == check_id)["status"] == "pass"
        for check_id in ["search_ownership", "task_ownership"]
    )
    shadow_safety_pass = next(r for r in results if r["id"] == "shadow_safety")["status"] == "pass"

    discovery_shadow_pass = False
    if is_phase2_discovery:
        discovery_shadow_pass = all(
            next(r for r in results if r["id"] == check["id"])["status"] == "pass"
            for check in discovery_checks
        ) and shadow_safety_pass and search_task_guardrails

    phase3_detail_shadow_pass = False
    if is_phase3_detail_read:
        phase3_detail_shadow_pass = all(
            next(r for r in results if r["id"] == check["id"])["status"] == "pass"
            for check in phase3_checks
        ) and shadow_safety_pass and search_task_guardrails

    phase4_readlist_context_shadow_pass = False
    if is_phase4_readlist_context_read:
        phase4_readlist_context_shadow_pass = all(
            next(r for r in results if r["id"] == check["id"])["status"] == "pass"
            for check in phase4_checks
        ) and shadow_safety_pass and search_task_guardrails

    phase5_oneshot_closure_shadow_pass = False
    if is_phase5_oneshot_closure:
        phase5_oneshot_closure_shadow_pass = all(
            next(r for r in results if r["id"] == check["id"])["status"] == "pass"
            for check in phase5_checks
        ) and shadow_safety_pass and search_task_guardrails

    phase6_oneshot_readlist_context_closure_shadow_pass = False
    if is_phase6_oneshot_readlist_context_closure:
        phase6_oneshot_readlist_context_closure_shadow_pass = all(
            next(r for r in results if r["id"] == check["id"])["status"] == "pass"
            for check in phase6_checks
        ) and shadow_safety_pass and search_task_guardrails

    phase8_readlist_books_family_closure_shadow_pass = False
    if is_phase8_readlist_books_family_closure:
        phase8_readlist_books_family_closure_shadow_pass = all(
            next(r for r in results if r["id"] == check["id"])["status"] == "pass"
            for check in phase8_checks
        ) and shadow_safety_pass and search_task_guardrails

    phase9_readlists_list_browse_closure_shadow_pass = False
    if is_phase9_readlists_list_browse_closure:
        phase9_readlists_list_browse_closure_shadow_pass = all(
            next(r for r in results if r["id"] == check["id"])["status"] == "pass"
            for check in phase9_checks
        ) and shadow_safety_pass and search_task_guardrails

    governance = reporting.build_governance(
        run_label=run_label,
        overall_pass=overall_pass,
        shadow_safety_pass=shadow_safety_pass,
        search_task_guardrails=search_task_guardrails,
        discovery_shadow_pass=discovery_shadow_pass,
        phase3_detail_shadow_pass=phase3_detail_shadow_pass,
        phase4_readlist_context_shadow_pass=phase4_readlist_context_shadow_pass,
        phase5_oneshot_closure_shadow_pass=phase5_oneshot_closure_shadow_pass,
        phase6_oneshot_readlist_context_closure_shadow_pass=phase6_oneshot_readlist_context_closure_shadow_pass,
        phase8_readlist_books_family_closure_shadow_pass=phase8_readlist_books_family_closure_shadow_pass,
        phase9_readlists_list_browse_closure_shadow_pass=phase9_readlists_list_browse_closure_shadow_pass,
    )

    summary = reporting.build_summary(
        timestamp=timestamp,
        run_label=run_label,
        evidence_root_rel=evaluators.rel(evidence_root),
        overall_pass=overall_pass,
        results=results,
        refusals=refusals,
        governance=governance,
        non_blocking=non_blocking,
        discovery_checks=discovery_checks,
        phase3_checks=phase3_checks,
        phase4_checks=phase4_checks,
        phase5_checks=phase5_checks,
        phase6_checks=phase6_checks,
        phase8_checks=phase8_checks,
        phase9_readlists_list_browse_closure_shadow_pass=phase9_readlists_list_browse_closure_shadow_pass,
        discovery_shadow_pass=discovery_shadow_pass,
        phase3_detail_shadow_pass=phase3_detail_shadow_pass,
        phase4_readlist_context_shadow_pass=phase4_readlist_context_shadow_pass,
        phase5_oneshot_closure_shadow_pass=phase5_oneshot_closure_shadow_pass,
        phase6_oneshot_readlist_context_closure_shadow_pass=phase6_oneshot_readlist_context_closure_shadow_pass,
        phase8_readlist_books_family_closure_shadow_pass=phase8_readlist_books_family_closure_shadow_pass,
    )

    output_dir.mkdir(parents=True, exist_ok=True)
    summary_latest = output_dir / "summary.json"
    summary_labeled = output_dir / f"summary-{run_label}.json"
    summary_text = json.dumps(summary, ensure_ascii=False, indent=2) + "\n"
    summary_latest.write_text(summary_text, encoding="utf-8")
    summary_labeled.write_text(summary_text, encoding="utf-8")

    report_text = reporting.build_report_text(
        timestamp=timestamp,
        run_label=run_label,
        evidence_root_rel=evaluators.rel(evidence_root),
        summary_overall=summary["overall"],
        results=results,
        refusals=refusals,
        non_blocking=non_blocking,
        governance=governance,
        discovery_shadow_pass=discovery_shadow_pass,
        phase3_detail_shadow_pass=phase3_detail_shadow_pass,
        phase4_readlist_context_shadow_pass=phase4_readlist_context_shadow_pass,
        phase5_oneshot_closure_shadow_pass=phase5_oneshot_closure_shadow_pass,
        phase6_oneshot_readlist_context_closure_shadow_pass=phase6_oneshot_readlist_context_closure_shadow_pass,
        phase8_readlist_books_family_closure_shadow_pass=phase8_readlist_books_family_closure_shadow_pass,
        phase9_readlists_list_browse_closure_shadow_pass=phase9_readlists_list_browse_closure_shadow_pass,
    )
    report_latest = output_dir / "report.md"
    report_labeled = output_dir / f"report-{run_label}.md"
    report_latest.write_text(report_text, encoding="utf-8")
    report_labeled.write_text(report_text, encoding="utf-8")

    print(f"Gate result: {summary['overall'].upper()}")
    print(f"Summary: {evaluators.rel(summary_latest)}")
    print(f"Report:  {evaluators.rel(report_latest)}")

    return 0 if overall_pass else 1


if __name__ == "__main__":
    sys.exit(main())
