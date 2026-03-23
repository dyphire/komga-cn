import json
import os
import re
from pathlib import Path


class GateEvaluators:
    def __init__(self, repo_root: Path):
        self.repo_root = repo_root

    def rel(self, path: Path) -> str:
        try:
            return str(path.relative_to(self.repo_root))
        except ValueError:
            return str(path)

    def read_json_file(self, path: Path) -> tuple[bool, object | None, str | None]:
        if not path.exists():
            return False, None, f"Missing evidence file: {self.rel(path)}"
        try:
            return True, json.loads(path.read_text(encoding="utf-8")), None
        except json.JSONDecodeError as exc:
            return False, None, f"Invalid JSON in {self.rel(path)}: {exc}"

    @staticmethod
    def is_neutral_success_line(line: str) -> bool:
        neutral_patterns = [
            re.compile(r"\btest result:\s+ok\b", re.IGNORECASE),
            re.compile(r"\bcargo test:\s+\d+\s+passed\b", re.IGNORECASE),
            re.compile(r"\b0\s+(?:fail|failed|error|errors|blocker|blockers|missing)\b", re.IGNORECASE),
        ]
        return any(pattern.search(line) for pattern in neutral_patterns)

    @staticmethod
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

    def eval_text_evidence(self, paths: list[Path]) -> tuple[bool, list[str], list[str]]:
        missing = [self.rel(p) for p in paths if not p.exists()]
        if missing:
            return False, [f"Missing evidence file: {p}" for p in missing], []

        details: list[str] = []
        blockers: list[str] = []

        for p in paths:
            text = p.read_text(encoding="utf-8", errors="replace")
            if not text.strip():
                blockers.append(f"{self.rel(p)} is empty")
                continue

            file_has_blocker = False
            for line in text.splitlines():
                if self.is_neutral_success_line(line):
                    continue
                if self.has_explicit_failure_marker(line):
                    blockers.append(f"{self.rel(p)} contains explicit failure markers: {line.strip()}")
                    file_has_blocker = True
                    break

            if file_has_blocker:
                continue

            details.append(f"{self.rel(p)} present and non-empty with no explicit failure markers")

        return len(blockers) == 0, blockers if blockers else details, []

    def eval_text_evidence_with_markers(
        self,
        paths: list[Path],
        marker_map: dict[Path, list[str]] | None = None,
        success_note: str | None = None,
    ) -> tuple[bool, list[str], list[str]]:
        ok, messages, _ = self.eval_text_evidence(paths)
        if not ok:
            return False, messages, []

        details = list(messages)
        if marker_map:
            for path, markers in marker_map.items():
                text = path.read_text(encoding="utf-8", errors="replace")
                missing = [marker for marker in markers if marker not in text]
                if missing:
                    return False, details + [f"{self.rel(path)} missing required discovery markers: {', '.join(missing)}"], []
                details.append(f"{self.rel(path)} contains required discovery markers: {', '.join(markers)}")

        if success_note:
            details.append(success_note)

        return True, details, []

    def eval_browser_smoke(self, summary_path: Path) -> tuple[bool, list[str], list[str]]:
        ok, payload, error = self.read_json_file(summary_path)
        if not ok:
            return False, [error], []

        required_routes = [
            "browse-libraries",
            "browse-series",
            "search",
            "server-management",
        ]
        if not isinstance(payload, list):
            return False, [f"Browser summary must be a JSON array: {self.rel(summary_path)}"], []

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

    def eval_phase3_browser_smoke(self, summary_path: Path) -> tuple[bool, list[str], list[str]]:
        ok, payload, error = self.read_json_file(summary_path)
        if not ok:
            return False, [error], []

        if not isinstance(payload, list):
            return False, [f"Phase3 browser summary must be a JSON array: {self.rel(summary_path)}"], []

        rows = payload
        required_routes = {
            "browse-series": {
                "signals": ["rootFound", "detailMetadataVisible", "collectionsPanelFound"],
                "expected_owned_labels": ["series-detail", "series-collections", "series-books-list"],
            },
            "browse-book": {
                "signals": ["rootFound", "detailMetadataVisible", "readlistsPanelFound", "siblingNavigationFound"],
                "expected_owned_labels": [
                    "book-detail",
                    "book-readlists",
                    "book-siblings-list",
                    "book-sibling-next",
                    "book-sibling-previous",
                ],
            },
        }

        by_route = {row.get("route"): row for row in rows if isinstance(row, dict)}
        missing_routes = [route for route in required_routes.keys() if route not in by_route]
        if missing_routes:
            return False, [f"Missing phase3 detail browser routes in summary: {', '.join(missing_routes)}"], []

        failures: list[str] = []
        details: list[str] = []

        for route, expectations in required_routes.items():
            row = by_route[route]
            if not bool(row.get("pass")):
                err = row.get("error") or "route did not pass"
                failures.append(f"{route}: {err}")
                continue

            capture_mode = row.get("captureMode")
            if not capture_mode:
                failures.append(f"{route}: captureMode is missing")
            elif capture_mode != "source-contract-fallback":
                details.append(f"{route} captureMode={capture_mode}")
            else:
                details.append(f"{route} captureMode=source-contract-fallback (accepted in this environment)")

            signals = row.get("signals")
            if not isinstance(signals, dict):
                failures.append(f"{route}: signals object is missing")
            else:
                for signal in expectations["signals"]:
                    if not bool(signals.get(signal)):
                        failures.append(f"{route}: required signal {signal}=true not observed")

            expected_requests = row.get("expectedOwnedRequests")
            if not isinstance(expected_requests, list):
                failures.append(f"{route}: expectedOwnedRequests array is missing")
                continue

            by_label = {
                item.get("label"): item
                for item in expected_requests
                if isinstance(item, dict) and "label" in item
            }
            missing_labels = [label for label in expectations["expected_owned_labels"] if label not in by_label]
            if missing_labels:
                failures.append(f"{route}: missing expectedOwnedRequests labels {', '.join(missing_labels)}")
                continue

            failed_labels = [label for label in expectations["expected_owned_labels"] if not bool(by_label[label].get("pass"))]
            if failed_labels:
                failures.append(f"{route}: expectedOwnedRequests failed for {', '.join(failed_labels)}")
                continue

            details.append(f"{route} includes all required owned request labels")

        if failures:
            return False, [f"Phase3 direct-route browser smoke regressions: {'; '.join(failures)}"], []

        details.append("Phase3 browser smoke proves direct-browse detail-read route readiness without over-claiming richer capture")
        return True, details, []

    def eval_phase4_browser_smoke(
        self,
        summary_path: Path,
        browse_readlist_path: Path,
        browse_book_path: Path,
    ) -> tuple[bool, list[str], list[str]]:
        ok_summary, payload, error = self.read_json_file(summary_path)
        if not ok_summary:
            return False, [error], []

        ok_readlist, browse_readlist_payload, readlist_error = self.read_json_file(browse_readlist_path)
        if not ok_readlist:
            return False, [readlist_error], []

        ok_book, browse_book_payload, book_error = self.read_json_file(browse_book_path)
        if not ok_book:
            return False, [book_error], []

        if not isinstance(payload, list):
            return False, [f"Phase4 browser summary must be a JSON array: {self.rel(summary_path)}"], []

        rows = payload
        by_route = {row.get("route"): row for row in rows if isinstance(row, dict)}
        required_routes = ["browse-readlist", "browse-book"]
        missing_routes = [route for route in required_routes if route not in by_route]
        if missing_routes:
            return False, [f"Missing phase4 readlist-context browser routes in summary: {', '.join(missing_routes)}"], []

        details: list[str] = []
        failures: list[str] = []

        route_rows = {
            "browse-readlist": browse_readlist_payload,
            "browse-book": browse_book_payload,
        }
        expected_route_shapes = {
            "browse-readlist": {
                "signals": [
                    "rootFound",
                    "detailMetadataVisible",
                    "itemBrowserFound",
                    "entryBookLinkFound",
                    "entryBookContextRetained",
                    "contextBannerVisible",
                    "returnedToReadlist",
                ],
                "scenario": [
                    "contextPropagationFound",
                    "bookLinkQueryFound",
                    "contextEnumFound",
                ],
                "owned_labels": [],
            },
            "browse-book": {
                "signals": [
                    "rootFound",
                    "detailMetadataVisible",
                    "readlistsPanelFound",
                    "siblingNavigationFound",
                    "initialContextRetained",
                    "initialPreviousBoundary",
                    "initialNextWithinReadlist",
                    "readListNameVisible",
                    "nextNavigationRetainedContext",
                    "previousNavigationRetainedContext",
                    "nextThenPreviousLoopClosed",
                ],
                "scenario": [
                    "contextParseFound",
                    "readlistListRequestFound",
                    "readlistNextRequestFound",
                    "readlistPreviousRequestFound",
                ],
                "owned_labels": [
                    "readlist-books-unpaged",
                    "readlist-book-next",
                    "readlist-book-previous",
                ],
            },
        }

        for route, expectations in expected_route_shapes.items():
            summary_row = by_route[route]
            detail_row = route_rows[route]

            if not isinstance(detail_row, dict):
                failures.append(f"{route}: route artifact is not a JSON object")
                continue

            if summary_row != detail_row:
                failures.append(f"{route}: summary row does not exactly match {self.rel(browse_readlist_path if route == 'browse-readlist' else browse_book_path)}")
                continue

            if not bool(summary_row.get("pass")):
                err = summary_row.get("error") or "route did not pass"
                failures.append(f"{route}: {err}")
                continue

            capture_mode = summary_row.get("captureMode")
            if not capture_mode:
                failures.append(f"{route}: captureMode is missing")
            elif capture_mode != "source-contract-fallback":
                details.append(f"{route} captureMode={capture_mode}")
            else:
                details.append(f"{route} captureMode=source-contract-fallback (accepted in this environment)")

            signals = summary_row.get("signals")
            if not isinstance(signals, dict):
                failures.append(f"{route}: signals object is missing")
            else:
                for signal in expectations["signals"]:
                    if not bool(signals.get(signal)):
                        failures.append(f"{route}: required signal {signal}=true not observed")

            scenario = summary_row.get("scenario")
            if not isinstance(scenario, dict):
                failures.append(f"{route}: scenario object is missing")
            else:
                for signal in expectations["scenario"]:
                    if not bool(scenario.get(signal)):
                        failures.append(f"{route}: required scenario flag {signal}=true not observed")

            expected_requests = summary_row.get("expectedOwnedRequests")
            if not isinstance(expected_requests, list):
                failures.append(f"{route}: expectedOwnedRequests array is missing")
                continue

            if route == "browse-readlist":
                if expected_requests:
                    failures.append("browse-readlist: expectedOwnedRequests must stay empty because ownership is proven from readlist entry into browse-book")
                else:
                    details.append("browse-readlist keeps entry/context retention explicit without over-claiming extra owned requests")
                continue

            by_label = {
                item.get("label"): item
                for item in expected_requests
                if isinstance(item, dict) and "label" in item
            }
            missing_labels = [label for label in expectations["owned_labels"] if label not in by_label]
            if missing_labels:
                failures.append(f"{route}: missing expectedOwnedRequests labels {', '.join(missing_labels)}")
                continue

            failed_labels = [label for label in expectations["owned_labels"] if not bool(by_label[label].get("pass"))]
            if failed_labels:
                failures.append(f"{route}: expectedOwnedRequests failed for {', '.join(failed_labels)}")
                continue

            details.append(
                "browse-book proves exactly the three owned phase4 requests: readlist-books-unpaged, readlist-book-next, readlist-book-previous"
            )

        if failures:
            return False, [f"Phase4 readlist-context browser smoke regressions: {'; '.join(failures)}"], []

        details.append("Phase4 browser smoke proves readlist-origin context retention and the exact three owned readlist-context routes without over-claiming media or writer scope")
        return True, details, []

    def eval_phase5_browser_smoke(
        self,
        summary_path: Path,
        browse_oneshot_path: Path,
        direct_parity_path: Path,
    ) -> tuple[bool, list[str], list[str]]:
        ok_summary, payload, error = self.read_json_file(summary_path)
        if not ok_summary:
            return False, [error], []

        ok_browse, browse_oneshot_payload, browse_error = self.read_json_file(browse_oneshot_path)
        if not ok_browse:
            return False, [browse_error], []

        if not isinstance(payload, list):
            return False, [f"Phase5 browser summary must be a JSON array: {self.rel(summary_path)}"], []

        rows = payload
        by_route = {row.get("route"): row for row in rows if isinstance(row, dict)}
        if "browse-oneshot" not in by_route:
            return False, ["Missing phase5 browse-oneshot route in browser summary"], []

        summary_row = by_route["browse-oneshot"]
        if not isinstance(browse_oneshot_payload, dict):
            return False, [f"{self.rel(browse_oneshot_path)} must be a JSON object"], []

        if summary_row != browse_oneshot_payload:
            return False, [f"browse-oneshot: summary row does not exactly match {self.rel(browse_oneshot_path)}"], []

        if not bool(summary_row.get("pass")):
            err = summary_row.get("error") or "route did not pass"
            return False, [f"browse-oneshot: {err}"], []

        details: list[str] = []
        failures: list[str] = []

        capture_mode = summary_row.get("captureMode")
        if not capture_mode:
            failures.append("browse-oneshot: captureMode is missing")
        elif capture_mode == "source-contract-fallback":
            details.append("browse-oneshot captureMode=source-contract-fallback (accepted in this environment)")
        else:
            details.append(f"browse-oneshot captureMode={capture_mode}")

        signals = summary_row.get("signals")
        if not isinstance(signals, dict):
            failures.append("browse-oneshot: signals object is missing")
        else:
            for signal in [
                "rootFound",
                "detailMetadataVisible",
                "collectionsPanelFound",
                "readlistContextNavigationFound",
                "returnedToDirectOneshot",
            ]:
                if not bool(signals.get(signal)):
                    failures.append(f"browse-oneshot: required signal {signal}=true not observed")

        scenario = summary_row.get("scenario")
        if not isinstance(scenario, dict):
            failures.append("browse-oneshot: scenario object is missing")
        else:
            for signal in [
                "contextParseFound",
                "contextNameRequestFound",
                "readlistBooksRequestFound",
                "readlistNextRequestFound",
                "readlistPreviousRequestFound",
                "readlistContextNavigationFound",
            ]:
                if not bool(scenario.get(signal)):
                    failures.append(f"browse-oneshot: required scenario flag {signal}=true not observed")

            observed_ownership = str(scenario.get("observedOwnershipLabel", ""))
            if capture_mode == "source-contract-fallback" and "native-owned" not in observed_ownership:
                failures.append(
                    "browse-oneshot: source-contract fallback must positively report native readlist detail ownership"
                )
            elif not observed_ownership:
                failures.append("browse-oneshot: scenario.observedOwnershipLabel is missing")

        expected_requests = summary_row.get("expectedOwnedRequests")
        if not isinstance(expected_requests, list):
            failures.append("browse-oneshot: expectedOwnedRequests array is missing")
        else:
            expected_labels = [
                "oneshot-series-detail",
                "oneshot-series-collections",
                "oneshot-bootstrap-books-list",
                "oneshot-book-readlists",
                "readlist-detail",
                "readlist-books-unpaged",
                "readlist-book-next",
                "readlist-book-previous",
            ]
            observed_labels = [
                item.get("label")
                for item in expected_requests
                if isinstance(item, dict) and "label" in item
            ]
            if observed_labels != expected_labels:
                failures.append(
                    "browse-oneshot: expectedOwnedRequests must exactly equal "
                    f"{', '.join(expected_labels)}"
                )
            else:
                by_label = {
                    item.get("label"): item
                    for item in expected_requests
                    if isinstance(item, dict) and "label" in item
                }
                failed_labels = [label for label in expected_labels if not bool(by_label[label].get("pass"))]
                if failed_labels:
                    failures.append(f"browse-oneshot: expectedOwnedRequests failed for {', '.join(failed_labels)}")
                else:
                    details.append(
                        "browse-oneshot proves exact owned labels: oneshot-series-detail, oneshot-series-collections, oneshot-bootstrap-books-list, oneshot-book-readlists, readlist-detail, readlist-books-unpaged, readlist-book-next, readlist-book-previous"
                    )

        fallback_requests = summary_row.get("observedFallbackRequests")
        if not isinstance(fallback_requests, list):
            failures.append("browse-oneshot: observedFallbackRequests array is missing")
        elif fallback_requests:
            failures.append("browse-oneshot: observedFallbackRequests must stay empty for READLIST-context oneshot evidence")
        else:
            details.append("browse-oneshot keeps READLIST-context fallback inventory empty after readlist detail promotion")

        ok_text, text_messages, _ = self.eval_text_evidence_with_markers(
            [direct_parity_path],
            {
                direct_parity_path: [
                    "direct_oneshot_admin_user_limited_restricted_matrix",
                    "Result: PASS",
                ],
            },
            "Direct oneshot cross-principal parity evidence is present and marker-verified.",
        )
        if not ok_text:
            failures.extend(text_messages)
        else:
            details.extend(text_messages)

        if failures:
            return False, [f"Phase5 oneshot browser smoke regressions: {'; '.join(failures)}"], []

        details.append(
            "Phase5 browser smoke proves direct /oneshot/:seriesId closure with exact READLIST-context owned surface while excluded media, reader, progress, write, and SSE branches remain out of owned inventory."
        )
        return True, details, []

    def eval_phase8_browser_smoke(
        self,
        browser_summary_path: Path,
        summary_json_path: Path,
        browse_readlist_path: Path,
    ) -> tuple[bool, list[str], list[str]]:
        ok_text, text_messages, _ = self.eval_text_evidence_with_markers(
            [browser_summary_path],
            {
                browser_summary_path: [
                    'route=browse-readlist',
                    'captureMode=source-contract-fallback',
                    'page-load-dependency: label=readlist-detail-preowned',
                    'paged-fetch: label=readlist-books-paged-unpaged-false',
                    'filtered-fetch: label=readlist-books-filtered-read-status',
                    'empty-result: label=readlist-books-empty-result',
                    'restricted-visibility: label=readlist-books-restricted-visible-only',
                    'non-claims: edit/live-refresh/admin/list-family/Tachiyomi remain outside this browse-readlist paged/filter closure evidence',
                ],
            },
            'Phase8 browser summary text stays scoped to BrowseReadList page-load plus paged/filter/empty/restricted governance evidence only.',
        )
        if not ok_text:
            return False, text_messages, []

        ok_summary, summary_payload, summary_error = self.read_json_file(summary_json_path)
        if not ok_summary:
            return False, text_messages + [summary_error], []

        ok_route, browse_readlist_payload, route_error = self.read_json_file(browse_readlist_path)
        if not ok_route:
            return False, text_messages + [route_error], []

        if not isinstance(summary_payload, list):
            return False, text_messages + [f"Phase8 browser summary must be a JSON array: {self.rel(summary_json_path)}"], []

        if len(summary_payload) != 1:
            return False, text_messages + ["Phase8 browser summary must contain exactly one BrowseReadList route row"], []

        summary_row = summary_payload[0]
        if not isinstance(summary_row, dict):
            return False, text_messages + ["Phase8 browser summary row must be a JSON object"], []

        if not isinstance(browse_readlist_payload, dict):
            return False, text_messages + [f"{self.rel(browse_readlist_path)} must be a JSON object"], []

        if summary_row != browse_readlist_payload:
            return False, text_messages + [
                f"browse-readlist: summary row does not exactly match {self.rel(browse_readlist_path)}"
            ], []

        failures: list[str] = []
        details = list(text_messages)

        if summary_row.get('route') != 'browse-readlist':
            failures.append('browse-readlist: route field must equal browse-readlist')

        if not bool(summary_row.get('pass')):
            err = summary_row.get('error') or 'route did not pass'
            failures.append(f'browse-readlist: {err}')

        capture_mode = summary_row.get('captureMode')
        if not capture_mode:
            failures.append('browse-readlist: captureMode is missing')
        elif capture_mode == 'source-contract-fallback':
            details.append('browse-readlist captureMode=source-contract-fallback (accepted in this environment)')
        else:
            details.append(f'browse-readlist captureMode={capture_mode}')

        signals = summary_row.get('signals')
        required_signals = [
            'rootFound',
            'detailMetadataVisible',
            'itemBrowserFound',
            'entryBookLinkFound',
            'entryBookContextRetained',
            'contextBannerVisible',
            'returnedToReadlist',
            'readListPageLoadFound',
            'pagedBooksFetchFound',
            'filterStateRestoreFound',
            'emptyStateRenderingFound',
        ]
        if not isinstance(signals, dict):
            failures.append('browse-readlist: signals object is missing')
        else:
            for signal in required_signals:
                if not bool(signals.get(signal)):
                    failures.append(f'browse-readlist: required signal {signal}=true not observed')
            if signals.get('siblingNavigationExpected') not in (False, None):
                failures.append('browse-readlist: sibling navigation must remain outside this paged/filter browser slice')

        scenario = summary_row.get('scenario')
        if not isinstance(scenario, dict):
            failures.append('browse-readlist: scenario object is missing')
        else:
            if scenario.get('type') != 'readlist-origin-entry':
                failures.append('browse-readlist: scenario.type must stay readlist-origin-entry')
            for signal in ['contextPropagationFound', 'bookLinkQueryFound', 'contextEnumFound']:
                if not bool(scenario.get(signal)):
                    failures.append(f'browse-readlist: required scenario flag {signal}=true not observed')

        expected_owned_requests = summary_row.get('expectedOwnedRequests')
        if not isinstance(expected_owned_requests, list):
            failures.append('browse-readlist: expectedOwnedRequests array is missing')
        elif expected_owned_requests:
            failures.append('browse-readlist: expectedOwnedRequests must stay empty for Phase8 governance-only browser evidence')
        else:
            details.append('browse-readlist keeps expectedOwnedRequests empty and records the owned inventory only in governanceOwnedRequests')

        owned_request_inventory = summary_row.get('ownedRequestInventory')
        if not isinstance(owned_request_inventory, list):
            failures.append('browse-readlist: ownedRequestInventory array is missing')
        elif owned_request_inventory:
            failures.append('browse-readlist: ownedRequestInventory must stay empty for Phase8 governance-only browser evidence')

        fallback_requests = summary_row.get('observedFallbackRequests')
        if not isinstance(fallback_requests, list):
            failures.append('browse-readlist: observedFallbackRequests array is missing')
        elif fallback_requests:
            failures.append('browse-readlist: observedFallbackRequests must stay empty')

        governance_requests = summary_row.get('governanceOwnedRequests')
        if not isinstance(governance_requests, list):
            failures.append('browse-readlist: governanceOwnedRequests array is missing')
        else:
            expected_requests = [
                ('readlist-detail-preowned', 'page-load-dependency', 'pre-owned-dependency', None),
                ('readlist-books-paged-unpaged-false', 'paged-fetch', 'phase8-owned', None),
                ('readlist-books-filtered-read-status', 'filtered-fetch', 'phase8-owned', None),
                ('readlist-books-empty-result', 'empty-result', 'phase8-owned', None),
                ('readlist-books-restricted-visible-only', 'restricted-visibility', 'phase8-owned', 'restricted@example.org'),
            ]
            observed_labels = [
                item.get('label')
                for item in governance_requests
                if isinstance(item, dict) and 'label' in item
            ]
            expected_labels = [item[0] for item in expected_requests]
            if observed_labels != expected_labels:
                failures.append(
                    'browse-readlist: governanceOwnedRequests must exactly equal '
                    f"{', '.join(expected_labels)}"
                )
            else:
                by_label = {
                    item.get('label'): item
                    for item in governance_requests
                    if isinstance(item, dict) and 'label' in item
                }
                for label, purpose, ownership_class, persona in expected_requests:
                    request = by_label[label]
                    if not bool(request.get('pass')):
                        failures.append(f'browse-readlist: governanceOwnedRequests failed for {label}')
                    if request.get('purpose') != purpose:
                        failures.append(f'browse-readlist: {label} purpose must equal {purpose}')
                    if request.get('ownershipClass') != ownership_class:
                        failures.append(
                            f'browse-readlist: {label} ownershipClass must equal {ownership_class}'
                        )
                    if persona is not None and request.get('persona') != persona:
                        failures.append(f'browse-readlist: {label} persona must equal {persona}')
                details.append(
                    'browse-readlist proves exactly one pre-owned readlist-detail dependency and four phase8-owned paged/filter governance requests'
                )

        if failures:
            return False, [f"Phase8 browse-readlist browser smoke regressions: {'; '.join(failures)}"], []

        details.append(
            'Phase8 browser evidence stays limited to BrowseReadList page load plus paged/filter/empty/restricted list-surface requests and keeps list-family, Tachiyomi, edit, admin, and live-refresh out of claim scope.'
        )
        return True, details, []

    def eval_phase9_browser_smoke(
        self,
        browser_summary_path: Path,
        summary_json_path: Path,
        browse_readlists_path: Path,
    ) -> tuple[bool, list[str], list[str]]:
        ok_text, text_messages, _ = self.eval_text_evidence_with_markers(
            [browser_summary_path],
            {
                browser_summary_path: [
                    'route=browse-readlists',
                    'captureMode=source-contract-fallback',
                    'default-browse: label=readlists-browse-default',
                    'paged-browse: label=readlists-browse-paged',
                    'repeated-library-browse: label=readlists-browse-repeated-library-id',
                    'repeated-library-paged-browse: label=readlists-browse-repeated-library-id-paged',
                    'count-flow: label=readlists-browse-size-zero-count',
                    'non-claims: search/unpaged=true/dialogs/admin-actions/Tachiyomi remain outside this browse-readlists browse/list closure evidence',
                ],
            },
            'Phase9 browser summary text stays scoped to BrowseReadLists browse/list evidence only, with exact five-request governance inventory and explicit non-claims.',
        )
        if not ok_text:
            return False, text_messages, []

        ok_summary, summary_payload, summary_error = self.read_json_file(summary_json_path)
        if not ok_summary:
            return False, text_messages + [summary_error], []

        ok_route, browse_readlists_payload, route_error = self.read_json_file(browse_readlists_path)
        if not ok_route:
            return False, text_messages + [route_error], []

        if not isinstance(summary_payload, list):
            return False, text_messages + [f"Phase9 browser summary must be a JSON array: {self.rel(summary_json_path)}"], []

        if len(summary_payload) != 1:
            return False, text_messages + ['Phase9 browser summary must contain exactly one BrowseReadLists route row'], []

        summary_row = summary_payload[0]
        if not isinstance(summary_row, dict):
            return False, text_messages + ['Phase9 browser summary row must be a JSON object'], []

        if not isinstance(browse_readlists_payload, dict):
            return False, text_messages + [f"{self.rel(browse_readlists_path)} must be a JSON object"], []

        if summary_row != browse_readlists_payload:
            return False, text_messages + [
                f"browse-readlists: summary row does not exactly match {self.rel(browse_readlists_path)}"
            ], []

        failures: list[str] = []
        details = list(text_messages)

        if summary_row.get('route') != 'browse-readlists':
            failures.append('browse-readlists: route field must equal browse-readlists')

        if not bool(summary_row.get('pass')):
            err = summary_row.get('error') or 'route did not pass'
            failures.append(f'browse-readlists: {err}')

        capture_mode = summary_row.get('captureMode')
        if not capture_mode:
            failures.append('browse-readlists: captureMode is missing')
        elif capture_mode == 'source-contract-fallback':
            details.append('browse-readlists captureMode=source-contract-fallback (accepted in this environment)')
        else:
            details.append(f'browse-readlists captureMode={capture_mode}')

        signals = summary_row.get('signals')
        required_signals = [
            'rootFound',
            'detailMetadataVisible',
            'itemBrowserFound',
            'routePaginationStateFound',
            'browseRequestFound',
            'repeatedLibraryBrowseFound',
            'paginationControlsFound',
            'totalCountChipFound',
            'countFlowFound',
        ]
        if not isinstance(signals, dict):
            failures.append('browse-readlists: signals object is missing')
        else:
            for signal in required_signals:
                if not bool(signals.get(signal)):
                    failures.append(f'browse-readlists: required signal {signal}=true not observed')
            if signals.get('siblingNavigationExpected') not in (False, None):
                failures.append('browse-readlists: sibling navigation must remain outside this browse/list browser slice')

        expected_owned_requests = summary_row.get('expectedOwnedRequests')
        if not isinstance(expected_owned_requests, list):
            failures.append('browse-readlists: expectedOwnedRequests array is missing')
        elif expected_owned_requests:
            failures.append('browse-readlists: expectedOwnedRequests must stay empty for Phase9 governance-only browser evidence')
        else:
            details.append('browse-readlists keeps expectedOwnedRequests empty and records the owned inventory only in governanceOwnedRequests')

        owned_request_inventory = summary_row.get('ownedRequestInventory')
        if not isinstance(owned_request_inventory, list):
            failures.append('browse-readlists: ownedRequestInventory array is missing')
        elif owned_request_inventory:
            failures.append('browse-readlists: ownedRequestInventory must stay empty for Phase9 governance-only browser evidence')

        fallback_requests = summary_row.get('observedFallbackRequests')
        if not isinstance(fallback_requests, list):
            failures.append('browse-readlists: observedFallbackRequests array is missing')
        elif fallback_requests:
            failures.append('browse-readlists: observedFallbackRequests must stay empty')

        governance_requests = summary_row.get('governanceOwnedRequests')
        if not isinstance(governance_requests, list):
            failures.append('browse-readlists: governanceOwnedRequests array is missing')
        else:
            expected_requests = [
                ('readlists-browse-default', 'default-browse', 'phase9-owned', None),
                ('readlists-browse-paged', 'paged-browse', 'phase9-owned', None),
                ('readlists-browse-repeated-library-id', 'repeated-library-browse', 'phase9-owned', None),
                ('readlists-browse-repeated-library-id-paged', 'repeated-library-paged-browse', 'phase9-owned', None),
                ('readlists-browse-size-zero-count', 'count-flow', 'phase9-owned', None),
            ]
            observed_labels = [
                item.get('label')
                for item in governance_requests
                if isinstance(item, dict) and 'label' in item
            ]
            expected_labels = [item[0] for item in expected_requests]
            if observed_labels != expected_labels:
                failures.append(
                    'browse-readlists: governanceOwnedRequests must exactly equal '
                    f"{', '.join(expected_labels)}"
                )
            else:
                by_label = {
                    item.get('label'): item
                    for item in governance_requests
                    if isinstance(item, dict) and 'label' in item
                }
                for label, purpose, ownership_class, persona in expected_requests:
                    request = by_label[label]
                    if not bool(request.get('pass')):
                        failures.append(f'browse-readlists: governanceOwnedRequests failed for {label}')
                    if request.get('purpose') != purpose:
                        failures.append(f'browse-readlists: {label} purpose must equal {purpose}')
                    if request.get('ownershipClass') != ownership_class:
                        failures.append(
                            f'browse-readlists: {label} ownershipClass must equal {ownership_class}'
                        )
                    if persona is not None and request.get('persona') != persona:
                        failures.append(f'browse-readlists: {label} persona must equal {persona}')
                details.append(
                    'browse-readlists proves exactly five phase9-owned browse/list governance requests and keeps search, unpaged=true, dialogs, admin actions, and Tachiyomi outside claim scope'
                )

        if failures:
            return False, [f"Phase9 browse-readlists browser smoke regressions: {'; '.join(failures)}"], []

        details.append(
            'Phase9 browser evidence stays limited to BrowseReadLists browse/list load, route-driven pagination state, repeated-library browse, and LibraryNavigation size=0 count flow only.'
        )
        return True, details, []

    def eval_task_ownership(self, task_ownership_path: Path, admin_queue_path: Path) -> tuple[bool, list[str], list[str]]:
        ok_text, text_messages, _ = self.eval_text_evidence([task_ownership_path])

        ok_json, payload, error = self.read_json_file(admin_queue_path)
        json_messages: list[str] = []
        if not ok_json:
            return False, text_messages + [error], []

        if not isinstance(payload, dict):
            return False, text_messages + [f"{self.rel(admin_queue_path)} must be a JSON object"], []

        if "pass" in payload and not bool(payload.get("pass")):
            return False, text_messages + [f"{self.rel(admin_queue_path)} indicates pass=false"], []

        if "status" in payload:
            status_text = str(payload.get("status", "")).strip().lower()
            if re.search(r"(^|[-_])(fail|failed|error|refuse|refused)([-_]|$)", status_text):
                return False, text_messages + [f"{self.rel(admin_queue_path)} has failing status: {status_text}"], []

        parity = payload.get("parityConclusion")
        if not isinstance(parity, dict):
            return False, text_messages + [f"{self.rel(admin_queue_path)} is missing parityConclusion object"], []

        if "canClaimAdminQueueParity" not in parity:
            return False, text_messages + [f"{self.rel(admin_queue_path)} is missing parityConclusion.canClaimAdminQueueParity"], []

        if parity.get("canClaimAdminQueueParity") is not True:
            reason = parity.get("reason") or "parityConclusion.canClaimAdminQueueParity is not true"
            return False, text_messages + [f"{self.rel(admin_queue_path)} rejects admin queue parity: {reason}"], []

        if isinstance(payload, (dict, list)) and len(payload) == 0:
            return False, text_messages + [f"{self.rel(admin_queue_path)} is an empty JSON artifact"], []

        json_messages.append(f"{self.rel(admin_queue_path)} confirms parityConclusion.canClaimAdminQueueParity=true")
        messages = text_messages + json_messages
        return ok_text, messages, []

    def eval_ops_server_management(self, path: Path) -> tuple[bool, list[str], list[str]]:
        ok, payload, error = self.read_json_file(path)
        if not ok:
            return False, [error], []

        if not isinstance(payload, dict):
            return False, [f"{self.rel(path)} must be a JSON object"], []

        if "pass" in payload and not bool(payload.get("pass")):
            detail = payload.get("error") or "pass=false"
            return False, [f"{self.rel(path)} indicates failure: {detail}"], []

        if "route" in payload and payload.get("route") != "server-management":
            return False, [f"{self.rel(path)} route mismatch: expected server-management"], []

        if "selector" in payload and payload.get("selector") != "[data-testid=\"server-management-root\"]":
            return False, [f"{self.rel(path)} selector mismatch for server-management root"], []

        return True, [f"{self.rel(path)} is valid structured server-management evidence"], []

    def eval_packaging_artifacts(self, runtime_startup_path: Path, tray_compat_path: Path) -> tuple[bool, list[str], list[str]]:
        ok_text, text_messages, _ = self.eval_text_evidence([runtime_startup_path, tray_compat_path])
        return ok_text, text_messages, []

    @staticmethod
    def eval_release_credentials() -> tuple[bool, list[str], list[str]]:
        token = os.getenv("JRELEASER_GITHUB_TOKEN", "")
        if token.strip():
            return True, ["JRELEASER_GITHUB_TOKEN is set"], []
        return False, ["Missing external release credential: JRELEASER_GITHUB_TOKEN"], []
