from typing import Any

import gate_eval_data as data


def build_governance(
    run_label: str,
    overall_pass: bool,
    shadow_safety_pass: bool,
    search_task_guardrails: bool,
    discovery_shadow_pass: bool,
    phase3_detail_shadow_pass: bool,
    phase4_readlist_context_shadow_pass: bool,
    phase5_oneshot_closure_shadow_pass: bool,
    phase6_oneshot_readlist_context_closure_shadow_pass: bool,
) -> dict[str, Any]:
    is_phase2_discovery = run_label == data.PHASE2_DISCOVERY_LABEL
    is_phase3_detail_read = run_label == data.PHASE3_DETAIL_READ_LABEL
    is_phase4_readlist_context_read = run_label == data.PHASE4_READLIST_CONTEXT_READ_LABEL
    is_phase5_oneshot_closure = run_label == data.PHASE5_ONESHOT_CLOSURE_LABEL
    is_phase6_oneshot_readlist_context_closure = run_label == data.PHASE6_ONESHOT_READLIST_CONTEXT_CLOSURE_LABEL

    if is_phase2_discovery:
        return {
            "shadow_mode": {
                "allowed": shadow_safety_pass,
                "rule": "Shadow mode must keep Java as stateful writer unless explicitly isolated",
            },
            "canary_mode": {
                "allowed": search_task_guardrails,
                "rule": "Search/task ownership guardrails stay proven, but this discovery label is not direct-serving approval.",
            },
            "discovery_slice_shadow": {
                "allowed": discovery_shadow_pass,
                "scope": "Selected discovery slice is shadow-ready only for libraries, series/list-search, books/list, and books/latest under Java single-writer governance.",
            },
            "discovery_slice_direct_serving": {
                "allowed": False,
                "scope": "Refused: detail endpoints, pages, binary/file/thumbnail delivery, read-progress, and write paths remain outside the discovery slice.",
            },
            "cutover": {
                "allowed": False,
                "scope": "phase2-catalog-discovery is a slice-only shadow runbook; whole cutover stays refused until detail endpoints, pages, binary/file delivery, read-progress, write paths, and release credentials are all proven.",
            },
            "rollback": {
                "ready": discovery_shadow_pass,
                "trigger": "Any discovery-slice regression or out-of-slice expansion forces rollback/no-cutover.",
            },
        }

    if is_phase3_detail_read:
        return {
            "shadow_mode": {
                "allowed": shadow_safety_pass,
                "rule": "Shadow mode must keep Java as stateful writer unless explicitly isolated",
            },
            "canary_mode": {
                "allowed": search_task_guardrails,
                "rule": "Search/task ownership guardrails remain required, but this phase3 label is not whole-cutover approval.",
            },
            "phase3_detail_read_shadow": {
                "allowed": phase3_detail_shadow_pass,
                "scope": "Direct BrowseSeries/BrowseBook detail-read slice is shadow-ready for series detail, collections, page-scoped books/list, book detail, previous/next, and readlists.",
            },
            "phase3_contextual_closure": {
                "allowed": False,
                "scope": "Refused: READLIST-context closure and browse-oneshot closure remain outside this direct-browse detail-read slice.",
            },
            "phase3_media_and_write": {
                "allowed": False,
                "scope": "Refused: media delivery (thumbnail/file/pages/manifest/resource/positions), read-progress writes/progression, and write-path claims remain out of slice.",
            },
            "cutover": {
                "allowed": False,
                "scope": "phase3-detail-read is a slice-only runbook; whole cutover/direct-serving remains refused until broader runtime/media/write/release conditions are proven.",
            },
            "rollback": {
                "ready": phase3_detail_shadow_pass,
                "trigger": "Any detail-slice regression or out-of-slice expansion forces rollback/no-cutover.",
            },
        }

    if is_phase4_readlist_context_read:
        return {
            "shadow_mode": {
                "allowed": shadow_safety_pass,
                "rule": "Shadow mode must keep Java as stateful writer unless explicitly isolated",
            },
            "canary_mode": {
                "allowed": search_task_guardrails,
                "rule": "Search/task ownership guardrails remain required, but this phase4 label is not whole-cutover approval.",
            },
            "phase4_readlist_context_read_shadow": {
                "allowed": phase4_readlist_context_shadow_pass,
                "scope": "Readlist-context read slice is shadow-ready only for unpaged readlist books and readlist previous/next routes.",
            },
            "phase4_readlist_context_non_claims": {
                "allowed": False,
                "scope": "Refused: paged readlist books, readlist list/detail, library_id variants, media delivery, read-progress writes/progression, oneshot/reader handoff, SSE, removals, and admin/write claims remain out of slice.",
            },
            "cutover": {
                "allowed": False,
                "scope": "phase4-readlist-context-read is a slice-only runbook; whole cutover/direct-serving remains refused until broader runtime/media/write/release conditions are proven.",
            },
            "rollback": {
                "ready": phase4_readlist_context_shadow_pass,
                "trigger": "Any readlist-context slice regression or out-of-slice expansion forces rollback/no-cutover.",
            },
        }

    if is_phase5_oneshot_closure:
        return {
            "shadow_mode": {
                "allowed": shadow_safety_pass,
                "rule": "Shadow mode must keep Java as stateful writer unless explicitly isolated",
            },
            "canary_mode": {
                "allowed": search_task_guardrails,
                "rule": "Search/task ownership guardrails remain required, but this phase5 label is not whole-cutover approval.",
            },
            "phase5_oneshot_closure_shadow": {
                "allowed": phase5_oneshot_closure_shadow_pass,
                "scope": "Oneshot closure slice is shadow-ready only for direct /oneshot/:seriesId closure with exact oneshot-bootstrap SeriesId-only books/list ownership.",
            },
            "phase5_oneshot_non_claims": {
                "allowed": False,
                "scope": "Refused: GET /api/v1/series/{seriesId}?oneshot=true, READLIST-context fallback, generic books/list widening, media, reader handoff/download, read-progress/progression, removals, admin/write, SSE, and whole cutover claims remain out of slice.",
            },
            "cutover": {
                "allowed": False,
                "scope": "phase5-oneshot-closure is a slice-only runbook; whole cutover/direct-serving remains refused until broader runtime/media/write/release conditions are proven.",
            },
            "rollback": {
                "ready": phase5_oneshot_closure_shadow_pass,
                "trigger": "Any oneshot-closure slice regression or out-of-slice expansion forces rollback/no-cutover.",
            },
        }

    if is_phase6_oneshot_readlist_context_closure:
        return {
            "shadow_mode": {
                "allowed": shadow_safety_pass,
                "rule": "Shadow mode must keep Java as stateful writer unless explicitly isolated",
            },
            "canary_mode": {
                "allowed": search_task_guardrails,
                "rule": "Search/task ownership guardrails remain required, but this phase6 label is not whole-cutover approval.",
            },
            "phase6_oneshot_readlist_context_closure_shadow": {
                "allowed": phase6_oneshot_readlist_context_closure_shadow_pass,
                "scope": "Oneshot READLIST-context direct-read slice is shadow-ready only for GET /api/v1/readlists/{readListId}, while all supporting oneshot/readlist sibling routes remain regression-only pre-owned dependencies.",
            },
            "phase6_oneshot_readlist_context_non_claims": {
                "allowed": False,
                "scope": "Refused: GET /api/v1/series/{seriesId}?oneshot=true, GET /api/v1/readlists and browse-readlist/list-family support, paged/library_id readlist variants, generic books/list widening, media, reader handoff/download, read-progress/progression, removals, admin/write, SSE, and whole cutover claims remain out of slice.",
            },
            "cutover": {
                "allowed": False,
                "scope": "phase6-oneshot-readlist-context-closure is a slice-only runbook; whole cutover/direct-serving remains refused until broader runtime/media/write/release conditions are proven.",
            },
            "rollback": {
                "ready": phase6_oneshot_readlist_context_closure_shadow_pass,
                "trigger": "Any oneshot READLIST-context direct-read slice regression or out-of-slice expansion forces rollback/no-cutover.",
            },
        }

    return {
        "shadow_mode": {
            "allowed": shadow_safety_pass,
            "rule": "Shadow mode must keep Java as stateful writer unless explicitly isolated",
        },
        "canary_mode": {
            "allowed": search_task_guardrails,
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


def build_summary(
    *,
    timestamp: str,
    run_label: str,
    evidence_root_rel: str,
    overall_pass: bool,
    results: list[dict[str, Any]],
    refusals: list[dict[str, Any]],
    governance: dict[str, Any],
    non_blocking: list[dict[str, Any]],
    discovery_checks: list[dict[str, Any]],
    phase3_checks: list[dict[str, Any]],
    phase4_checks: list[dict[str, Any]],
    phase5_checks: list[dict[str, Any]],
    phase6_checks: list[dict[str, Any]],
    discovery_shadow_pass: bool,
    phase3_detail_shadow_pass: bool,
    phase4_readlist_context_shadow_pass: bool,
    phase5_oneshot_closure_shadow_pass: bool,
    phase6_oneshot_readlist_context_closure_shadow_pass: bool,
) -> dict[str, Any]:
    is_phase2_discovery = run_label == data.PHASE2_DISCOVERY_LABEL
    is_phase3_detail_read = run_label == data.PHASE3_DETAIL_READ_LABEL
    is_phase4_readlist_context_read = run_label == data.PHASE4_READLIST_CONTEXT_READ_LABEL
    is_phase5_oneshot_closure = run_label == data.PHASE5_ONESHOT_CLOSURE_LABEL
    is_phase6_oneshot_readlist_context_closure = run_label == data.PHASE6_ONESHOT_READLIST_CONTEXT_CLOSURE_LABEL

    summary: dict[str, Any] = {
        "task": "T16",
        "generated_at": timestamp,
        "run_label": run_label,
        "require_all": True,
        "evidence_root": evidence_root_rel,
        "overall": "pass" if overall_pass else "fail",
        "checks": results,
        "refusals": refusals,
        "governance": governance,
    }

    if is_phase2_discovery:
        summary["evaluation_scope"] = "phase2-catalog-discovery-shadow"
        summary["discovery_slice"] = {
            "shadow_ready": discovery_shadow_pass,
            "supported_scope": data.discovery_supported_scope,
            "out_of_slice": data.discovery_out_of_slice,
            "non_claims": [
                "This does not claim whole cutover readiness.",
                "This does not claim direct-serving readiness for detail endpoints, pages, binaries, or write paths.",
                "This does not claim release credential verification.",
            ],
            "check_ids": [check["id"] for check in discovery_checks],
        }
        if non_blocking:
            summary["non_blocking"] = non_blocking
    elif is_phase5_oneshot_closure:
        summary["evaluation_scope"] = "phase5-oneshot-closure-shadow"
        summary["oneshot_closure_slice"] = {
            "shadow_ready": phase5_oneshot_closure_shadow_pass,
            "newly_owned_surface": data.phase5_oneshot_owned_scope[0],
            "owned_routes": data.phase5_oneshot_owned_scope,
            "supported_scope": data.phase5_oneshot_owned_scope,
            "required_pre_owned_dependencies": data.phase5_oneshot_pre_owned_dependencies,
            "excluded_branches": data.phase5_oneshot_out_of_slice,
            "out_of_slice": data.phase5_oneshot_out_of_slice,
            "non_claims": [
                "This does not claim whole cutover readiness.",
                "This does not claim GET /api/v1/series/{seriesId}?oneshot=true or READLIST-context closure ownership.",
                "This does not claim media, reader-handoff/download, read-progress/progression, removals, admin/write, or SSE ownership.",
            ],
            "check_ids": [check["id"] for check in phase5_checks],
        }
        if non_blocking:
            summary["non_blocking"] = non_blocking
    elif is_phase6_oneshot_readlist_context_closure:
        summary["evaluation_scope"] = "phase6-oneshot-readlist-context-closure-shadow"
        summary["oneshot_readlist_context_closure_slice"] = {
            "shadow_ready": phase6_oneshot_readlist_context_closure_shadow_pass,
            "newly_owned_surface": data.phase6_oneshot_readlist_context_owned_scope[0],
            "owned_routes": data.phase6_oneshot_readlist_context_owned_scope,
            "supported_scope": data.phase6_oneshot_readlist_context_owned_scope,
            "required_pre_owned_dependencies": data.phase6_oneshot_readlist_context_pre_owned_dependencies,
            "excluded_branches": data.phase6_oneshot_readlist_context_out_of_slice,
            "out_of_slice": data.phase6_oneshot_readlist_context_out_of_slice,
            "non_claims": [
                "This does not claim whole cutover readiness.",
                "This does not claim browse-readlist or GET /api/v1/readlists list-family ownership.",
                "This does not claim media, reader-handoff/download, read-progress/progression, removals, admin/write, or SSE ownership.",
            ],
            "check_ids": [check["id"] for check in phase6_checks],
        }
        if non_blocking:
            summary["non_blocking"] = non_blocking
    elif is_phase3_detail_read:
        summary["evaluation_scope"] = "phase3-detail-read-shadow"
        summary["detail_read_slice"] = {
            "shadow_ready": phase3_detail_shadow_pass,
            "supported_scope": data.phase3_detail_supported_scope,
            "out_of_slice": data.phase3_detail_out_of_slice,
            "non_claims": [
                "This does not claim whole cutover readiness.",
                "This does not claim generic direct-serving/media delivery readiness.",
                "This does not claim contextual READLIST closure, oneshot closure, read-progress writes, or broader write-path ownership.",
            ],
            "check_ids": [check["id"] for check in phase3_checks],
        }
        if non_blocking:
            summary["non_blocking"] = non_blocking
    elif is_phase4_readlist_context_read:
        summary["evaluation_scope"] = "phase4-readlist-context-read-shadow"
        summary["readlist_context_read_slice"] = {
            "shadow_ready": phase4_readlist_context_shadow_pass,
            "owned_routes": data.phase4_readlist_context_supported_scope,
            "supported_scope": data.phase4_readlist_context_supported_scope,
            "excluded_branches": data.phase4_readlist_context_out_of_slice,
            "out_of_slice": data.phase4_readlist_context_out_of_slice,
            "non_claims": [
                "This does not claim whole cutover readiness.",
                "This does not claim paged readlist/list-detail/library_id variant ownership.",
                "This does not claim media, read-progress/progression, oneshot/reader-handoff, SSE, removal, or broader write-path ownership.",
            ],
            "check_ids": [check["id"] for check in phase4_checks],
        }
        if non_blocking:
            summary["non_blocking"] = non_blocking

    return summary


def build_report_text(
    *,
    timestamp: str,
    run_label: str,
    evidence_root_rel: str,
    summary_overall: str,
    results: list[dict[str, Any]],
    refusals: list[dict[str, Any]],
    non_blocking: list[dict[str, Any]],
    governance: dict[str, Any],
    discovery_shadow_pass: bool,
    phase3_detail_shadow_pass: bool,
    phase4_readlist_context_shadow_pass: bool,
    phase5_oneshot_closure_shadow_pass: bool,
    phase6_oneshot_readlist_context_closure_shadow_pass: bool,
) -> str:
    is_phase2_discovery = run_label == data.PHASE2_DISCOVERY_LABEL
    is_phase3_detail_read = run_label == data.PHASE3_DETAIL_READ_LABEL
    is_phase4_readlist_context_read = run_label == data.PHASE4_READLIST_CONTEXT_READ_LABEL
    is_phase5_oneshot_closure = run_label == data.PHASE5_ONESHOT_CLOSURE_LABEL
    is_phase6_oneshot_readlist_context_closure = run_label == data.PHASE6_ONESHOT_READLIST_CONTEXT_CLOSURE_LABEL

    lines = []
    lines.append(f"# Rust Cutover Readiness Gate ({run_label})")
    lines.append("")
    lines.append(f"- Generated at: {timestamp}")
    lines.append(f"- Evidence root: `{evidence_root_rel}`")
    lines.append(f"- Overall result: **{summary_overall.upper()}**")
    if is_phase2_discovery:
        lines.append("- Evaluation scope: `phase2-catalog-discovery-shadow`")
        lines.append("- Non-claim: this label records shadow readiness for the selected discovery slice only, not whole cutover readiness")
    elif is_phase3_detail_read:
        lines.append("- Evaluation scope: `phase3-detail-read-shadow`")
        lines.append("- Non-claim: this label records direct-browse detail-read readiness only, not whole cutover/direct-serving/media/write readiness")
    elif is_phase4_readlist_context_read:
        lines.append("- Evaluation scope: `phase4-readlist-context-read-shadow`")
        lines.append("- Non-claim: this label records readlist-context-read readiness only, not whole cutover/direct-serving/media/write readiness")
    elif is_phase5_oneshot_closure:
        lines.append("- Evaluation scope: `phase5-oneshot-closure-shadow`")
        lines.append("- Non-claim: this label records direct oneshot closure readiness only, not whole cutover/direct-serving/media/write readiness")
    elif is_phase6_oneshot_readlist_context_closure:
        lines.append("- Evaluation scope: `phase6-oneshot-readlist-context-closure-shadow`")
        lines.append("- Non-claim: this label records oneshot READLIST-context direct-read readiness only, not browse-readlist/list-family or whole cutover/direct-serving/media/write readiness")
    lines.append("")

    if is_phase2_discovery:
        lines.append("## Discovery Slice Runbook")
        lines.append("")
        lines.append(f"- Shadow-ready target: **{'PASS' if discovery_shadow_pass else 'FAIL'}**")
        lines.append(f"- Supported scope: {', '.join(f'`{item}`' for item in data.discovery_supported_scope)}")
        lines.append(
            "- Direct-serving/cutover: **REFUSE** — detail endpoints, pages, binary/file/thumbnail delivery, read-progress, and write paths remain out of slice"
        )
        lines.append(
            "- Packaging/release credentials: **NOT CLAIMED** — `JRELEASER_GITHUB_TOKEN` is not part of this shadow-slice pass condition and still must be proven for broader cutover/release"
        )
        lines.append("")
    elif is_phase3_detail_read:
        lines.append("## Phase3 Detail-Read Runbook")
        lines.append("")
        lines.append(f"- Shadow-ready target: **{'PASS' if phase3_detail_shadow_pass else 'FAIL'}**")
        lines.append(f"- Supported scope: {', '.join(f'`{item}`' for item in data.phase3_detail_supported_scope)}")
        lines.append(
            "- Direct-browse closure only: **ALLOW (slice-only)** — series detail/collections/page-scoped books-list + book detail/prev/next/readlists with browser smoke evidence"
        )
        lines.append(
            "- Whole cutover/direct-serving: **REFUSE** — this label is not a full cutover approval"
        )
        lines.append(
            "- Media/context/write closure: **REFUSE** — contextual READLIST closure, oneshot closure, media delivery, read-progress writes/progression, and write-path claims remain out of slice"
        )
        lines.append(
            "- Browser capture note: `captureMode=source-contract-fallback` is accepted and recorded as honest environment evidence, not richer runtime capture"
        )
        lines.append("")
    elif is_phase4_readlist_context_read:
        lines.append("## Phase4 Readlist-Context-Read Runbook")
        lines.append("")
        lines.append(f"- Shadow-ready target: **{'PASS' if phase4_readlist_context_shadow_pass else 'FAIL'}**")
        lines.append(f"- Owned routes (exactly 3): {', '.join(f'`{item}`' for item in data.phase4_readlist_context_supported_scope)}")
        lines.append(
            "- Slice closure: **ALLOW (slice-only)** — unpaged readlist books + readlist previous/next only"
        )
        lines.append(
            "- Out-of-slice governance: **REFUSE** — paged/list-detail/library_id/media/progress/oneshot/reader-handoff/SSE/removal/admin-write branches stay explicit non-native"
        )
        lines.append(
            f"- Excluded branches still out of scope: {', '.join(f'`{item}`' for item in data.phase4_readlist_context_out_of_slice)}"
        )
        lines.append(
            "- Whole cutover/direct-serving: **REFUSE** — this label is not a full cutover approval"
        )
        lines.append("")
    elif is_phase5_oneshot_closure:
        lines.append("## Phase5 Oneshot-Closure Runbook")
        lines.append("")
        lines.append(f"- Shadow-ready target: **{'PASS' if phase5_oneshot_closure_shadow_pass else 'FAIL'}**")
        lines.append(f"- Owned surface (newly owned exactly 1 family): {', '.join(f'`{item}`' for item in data.phase5_oneshot_owned_scope)}")
        lines.append(
            "- Direct `/oneshot/:seriesId` closure: **ALLOW (slice-only)** — newly owned surface is only the oneshot-bootstrap `POST /api/v1/books/list` SeriesId-only family"
        )
        lines.append(
            f"- Required pre-owned dependencies (regression-only): {', '.join(f'`{item}`' for item in data.phase5_oneshot_pre_owned_dependencies)}"
        )
        lines.append(
            "- Out-of-slice governance: **REFUSE** — `GET /api/v1/series/{seriesId}?oneshot=true`, READLIST-context fallback, generic books/list widening, media, reader handoff/download, progress/progression, removals, admin/write, SSE, and whole cutover claims remain explicit non-native"
        )
        lines.append(
            f"- Excluded branches still out of scope: {', '.join(f'`{item}`' for item in data.phase5_oneshot_out_of_slice)}"
        )
        lines.append(
            "- Whole cutover/direct-serving: **REFUSE** — this label is not a full cutover approval"
        )
        lines.append(
            "- Browser capture note: `captureMode=source-contract-fallback` is accepted only when owned labels and READLIST-context fallback labels are both explicitly proven"
        )
        lines.append("")
    elif is_phase6_oneshot_readlist_context_closure:
        lines.append("## Phase6 Oneshot-Readlist-Context-Closure Runbook")
        lines.append("")
        lines.append(f"- Shadow-ready target: **{'PASS' if phase6_oneshot_readlist_context_closure_shadow_pass else 'FAIL'}**")
        lines.append(f"- Owned surface (newly owned exactly 1 route): {', '.join(f'`{item}`' for item in data.phase6_oneshot_readlist_context_owned_scope)}")
        lines.append(
            "- User-visible closure: **ALLOW (slice-only)** — oneshot READLIST-context direct-read closure owns only `GET /api/v1/readlists/{readListId}` and does not claim browse-readlist or readlist list-family support"
        )
        lines.append(
            f"- Required pre-owned dependencies (regression-only): {', '.join(f'`{item}`' for item in data.phase6_oneshot_readlist_context_pre_owned_dependencies)}"
        )
        lines.append(
            "- Out-of-slice governance: **REFUSE** — `GET /api/v1/series/{seriesId}?oneshot=true`, `GET /api/v1/readlists`, browse-readlist/page closure, paged/library_id variants, generic books/list widening, media, reader handoff/download, progress/progression, removals, admin/write, SSE, and whole cutover claims remain explicit non-native"
        )
        lines.append(
            f"- Excluded branches still out of scope: {', '.join(f'`{item}`' for item in data.phase6_oneshot_readlist_context_out_of_slice)}"
        )
        lines.append(
            "- Whole cutover/direct-serving: **REFUSE** — this label is not a full cutover approval"
        )
        lines.append(
            "- Browser capture note: `captureMode=source-contract-fallback` is accepted only when browser evidence proves `readlist-detail-native-owned`, the exact eight-label owned inventory, and an empty `observedFallbackRequests` array"
        )
        lines.append("")

    lines.append("## Check Results")
    lines.append("")
    status_icons = {
        "pass": "✅",
        "fail": "❌",
        "waived": "⚪",
        "skipped": "⚪",
    }
    for r in results:
        emoji = status_icons.get(r["status"], "⚪")
        blocking_note = "blocking" if r.get("blocking", True) else "non-blocking"
        lines.append(f"- {emoji} `{r['id']}` ({r['category']}, {blocking_note}): {r['refusal_condition']}")
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

    if non_blocking:
        lines.append("")
        lines.append("## Non-Blocking Findings")
        lines.append("")
        for item in non_blocking:
            lines.append(f"- `{item['id']}` ({item['status']})")
            for msg in item["details"]:
                lines.append(f"  - {msg}")

    lines.append("")
    lines.append("## Governance")
    lines.append("")
    for key in governance.keys():
        entry = governance[key]
        flag = entry.get("allowed", entry.get("ready", False))
        lines.append(f"- `{key}`: {'ALLOW' if flag else 'REFUSE'} — {entry.get('rule', entry.get('scope', entry.get('trigger', '')))}")

    return "\n".join(lines) + "\n"
