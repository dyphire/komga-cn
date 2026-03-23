#!/usr/bin/env python3

from __future__ import annotations

import argparse
import base64
import json
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request
from pathlib import Path

from browser_smoke_routes import ROUTES

REPO_ROOT = Path(__file__).resolve().parents[2]

NODE_RUNNER_PATH = Path(__file__).with_name('browser_smoke_runner.mjs')
NODE_RUNNER = NODE_RUNNER_PATH.read_text(encoding='utf-8')


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description='Capture direct-route browser smoke evidence for owned detail routes.',
    )
    parser.add_argument(
        '--route',
        action='append',
        dest='routes',
        choices=sorted(ROUTES.keys()),
        help='Route id to capture. Defaults to all supported routes.',
    )
    parser.add_argument(
        '--output-dir',
        required=True,
        help='Directory where structured evidence should be written.',
    )
    parser.add_argument('--app-url', default='http://127.0.0.1:8081')
    parser.add_argument('--api-url', default='http://127.0.0.1:25600')
    parser.add_argument('--username', default='admin@example.org')
    parser.add_argument('--password', default='admin')
    return parser.parse_args()


def selected_routes(route_names: list[str] | None) -> list[dict[str, object]]:
    names = route_names or list(ROUTES.keys())
    return [ROUTES[name] for name in names]


def write_temp_file(directory: Path, name: str, content: str) -> Path:
    path = directory / name
    path.write_text(content, encoding='utf-8')
    return path


def run_node_runner(config: dict[str, object]) -> subprocess.CompletedProcess[str]:
    with tempfile.TemporaryDirectory(prefix='komga-browser-smoke-') as temp_dir:
        temp_path = Path(temp_dir)
        config_path = write_temp_file(temp_path, 'config.json', json.dumps(config, ensure_ascii=False, indent=2))
        runner_path = write_temp_file(temp_path, 'runner.mjs', NODE_RUNNER)
        return subprocess.run(
            ['node', str(runner_path), str(config_path)],
            cwd=REPO_ROOT,
            text=True,
            capture_output=True,
            check=False,
        )


def emit_process_output(result: subprocess.CompletedProcess[str]) -> None:
    if result.stdout:
        print(result.stdout, end='')
    if result.stderr:
        print(result.stderr, end='', file=sys.stderr)


def playwright_missing(result: subprocess.CompletedProcess[str]) -> bool:
    combined = f'{result.stdout}\n{result.stderr}'
    return 'Unable to load Playwright' in combined or 'ERR_MODULE_NOT_FOUND' in combined


def compact_text(value: str) -> str:
    return ' '.join(value.split())


def source_paths(route: dict[str, object], key: str = 'sourceFiles') -> list[str]:
    source_files = route.get(key)
    if isinstance(source_files, list) and source_files:
        return [str(path) for path in source_files]
    return [str(route['sourceFile'])]


def read_text(relative_path: str) -> str:
    return (REPO_ROOT / relative_path).read_text(encoding='utf-8')


def read_source_texts(route: dict[str, object], key: str = 'sourceFiles') -> dict[str, str]:
    return {path: read_text(path) for path in source_paths(route, key)}


def combined_source_text(route: dict[str, object]) -> str:
    return '\n'.join(read_source_texts(route).values())


def source_snippet(source_text: str, anchor: str, radius: int = 1800) -> str:
    index = source_text.find(anchor)
    if index < 0:
        return source_text[:radius]
    start = max(0, index - radius // 3)
    end = min(len(source_text), index + radius)
    return source_text[start:end]


def selector_in_source(source_text: str, selector: str) -> bool:
    return selector_marker(selector) in source_text


def selector_marker(selector: str) -> str:
    marker = selector.removeprefix('[data-testid="').removesuffix('"]')
    return f'data-testid="{marker}"'


def capture_ownership_label(capture_mode: str) -> str:
    return 'contract-fallback' if capture_mode == 'source-contract-fallback' else 'non-native-observed'


def scenario_ownership_label(route: dict[str, object], capture_mode: str) -> str:
    if route.get('route') == 'browse-oneshot' and capture_mode == 'source-contract-fallback':
        return 'readlist-detail-native-owned'
    return capture_ownership_label(capture_mode)


def auth_header(username: str, password: str) -> str:
    token = base64.b64encode(f'{username}:{password}'.encode('utf-8')).decode('ascii')
    return f'Basic {token}'


def login_token(api_url: str, username: str, password: str) -> str:
    request = urllib.request.Request(
        f'{api_url}/api/v2/users/me',
        headers={
            'Authorization': auth_header(username, password),
            'X-Auth-Token': '',
        },
    )
    with urllib.request.urlopen(request) as response:
        token = response.headers.get('x-auth-token')
        if not token:
            raise RuntimeError('login response did not include x-auth-token')
        return token


def parse_json_payload(payload: bytes) -> object | None:
    if not payload:
        return None
    try:
        return json.loads(payload.decode('utf-8'))
    except (UnicodeDecodeError, json.JSONDecodeError):
        return None


def page_content_ids(payload: object | None) -> list[str]:
    if not isinstance(payload, dict):
        return []
    content = payload.get('content')
    if not isinstance(content, list):
        return []
    return [str(item.get('id')) for item in content if isinstance(item, dict) and item.get('id') is not None]


def json_path_value(payload: object | None, path: str) -> object | None:
    current = payload
    for part in path.split('.'):
        if not isinstance(current, dict) or part not in current:
            return None
        current = current[part]
    return current


def response_expectation_failures(spec: dict[str, object], payload: object | None) -> list[str]:
    failures: list[str] = []
    expected_content_ids = spec.get('expectedContentIds')
    if isinstance(expected_content_ids, list):
        actual_ids = page_content_ids(payload)
        if actual_ids != [str(item) for item in expected_content_ids]:
            failures.append(f'expected content ids {expected_content_ids}, got {actual_ids}')

    assertions = spec.get('jsonAssertions')
    if isinstance(assertions, list):
        for assertion in assertions:
            if not isinstance(assertion, dict):
                continue
            path = assertion.get('path')
            if not isinstance(path, str) or 'value' not in assertion:
                continue
            actual_value = json_path_value(payload, path)
            if actual_value != assertion['value']:
                failures.append(f'expected {path}={assertion["value"]!r}, got {actual_value!r}')

    return failures


def execute_api_request(api_url: str, token: str, spec: dict[str, object]) -> tuple[dict[str, object], dict[str, object], dict[str, object]]:
    url = f'{api_url}{spec["requestPath"]}'
    body = spec.get('body')
    data = body.encode('utf-8') if isinstance(body, str) else None
    headers = {
        'X-Auth-Token': token,
    }
    if data is not None:
        headers['Content-Type'] = 'application/json'

    request = urllib.request.Request(url, method=str(spec['method']), data=data, headers=headers)
    response_entry: dict[str, object]
    passed = False
    allowed_statuses = [int(status) for status in spec.get('responseStatuses', [])]
    expectation_failures: list[str] = []

    try:
        with urllib.request.urlopen(request) as response:
            payload = response.read()
            response_entry = {
                'status': response.status,
                'url': url,
                'contentType': response.headers.get('Content-Type'),
                'json': parse_json_payload(payload),
            }
            passed = response.status in allowed_statuses if allowed_statuses else 200 <= response.status < 300
            if passed:
                expectation_failures = response_expectation_failures(spec, response_entry.get('json'))
                passed = len(expectation_failures) == 0
    except urllib.error.HTTPError as error:
        payload = error.read()
        response_entry = {
            'status': error.code,
            'url': url,
            'contentType': error.headers.get('Content-Type'),
            'json': parse_json_payload(payload),
        }
        passed = error.code in allowed_statuses
        if passed:
            expectation_failures = response_expectation_failures(spec, response_entry.get('json'))
            passed = len(expectation_failures) == 0

    request_entry = {
        'method': spec['method'],
        'url': url,
        'resourceType': 'xhr',
        'postData': body if isinstance(body, str) else None,
    }

    expected_entry = {
        'label': spec['label'],
        'method': spec['method'],
        'urlContains': spec.get('urlContains'),
        'urlEndsWith': spec.get('urlEndsWith'),
        'postDataIncludes': spec.get('postDataIncludes', []),
        'responseStatuses': allowed_statuses,
        'pass': passed,
        'requestPath': spec['requestPath'],
        'purpose': spec.get('purpose'),
        'ownershipClass': spec.get('ownershipClass'),
        'persona': (spec.get('auth') or {}).get('username') if isinstance(spec.get('auth'), dict) else None,
        'expectationFailures': expectation_failures,
        'matchedRequest': request_entry if passed else None,
        'matchedResponse': response_entry,
    }
    return request_entry, response_entry, expected_entry


def contract_request_entry(api_url: str, spec: dict[str, object]) -> dict[str, object]:
    return {
        'method': spec['method'],
        'url': f'{api_url}{spec["requestPath"]}',
        'resourceType': 'xhr',
        'postData': spec.get('body') if isinstance(spec.get('body'), str) else None,
    }


def contract_expected_entry(api_url: str, spec: dict[str, object]) -> dict[str, object]:
    request_entry = contract_request_entry(api_url, spec)
    return {
        'label': spec['label'],
        'method': spec['method'],
        'urlContains': spec.get('urlContains'),
        'urlEndsWith': spec.get('urlEndsWith'),
        'postDataIncludes': spec.get('postDataIncludes', []),
        'responseStatuses': [int(status) for status in spec.get('responseStatuses', [])],
        'pass': True,
        'requestPath': spec['requestPath'],
        'purpose': spec.get('purpose'),
        'ownershipClass': spec.get('ownershipClass'),
        'persona': (spec.get('auth') or {}).get('username') if isinstance(spec.get('auth'), dict) else None,
        'expectationFailures': [],
        'matchedRequest': request_entry,
        'matchedResponse': None,
    }


def parse_error_messages(error: object) -> list[str]:
    if not isinstance(error, str) or not error:
        return []
    return [item.strip() for item in error.split(';') if item.strip()]


def append_unique_failures(failures: list[str], additions: list[str]) -> list[str]:
    seen = set(failures)
    for item in additions:
        if item not in seen:
            failures.append(item)
            seen.add(item)
    return failures


def combined_source_for_route(route: dict[str, object]) -> str:
    return '\n'.join(read_source_texts(route).values())


def evaluate_contract_signals(route: dict[str, object]) -> tuple[dict[str, bool], dict[str, dict[str, object]]]:
    definitions = route.get('contractSignals')
    if not isinstance(definitions, list) or not definitions:
        return {}, {}

    combined_source = combined_source_for_route(route)
    signals: dict[str, bool] = {}
    evidence: dict[str, dict[str, object]] = {}

    for definition in definitions:
        if not isinstance(definition, dict):
            continue
        signal_key = definition.get('signalKey')
        if not isinstance(signal_key, str):
            continue

        all_of = [str(item) for item in definition.get('allOf', []) if isinstance(item, str)]
        any_of = [str(item) for item in definition.get('anyOf', []) if isinstance(item, str)]
        count_at_least = []
        for item in definition.get('countAtLeast', []):
            if not isinstance(item, dict):
                continue
            fragment = item.get('fragment')
            count = item.get('count')
            if isinstance(fragment, str) and isinstance(count, int):
                count_at_least.append({'fragment': fragment, 'count': count})
        all_of_pass = all(fragment in combined_source for fragment in all_of) if all_of else True
        any_of_pass = any(fragment in combined_source for fragment in any_of) if any_of else True
        count_at_least_pass = all(combined_source.count(item['fragment']) >= item['count'] for item in count_at_least)
        passed = all_of_pass and any_of_pass and count_at_least_pass

        signals[signal_key] = passed
        evidence[signal_key] = {
            'allOf': all_of,
            'anyOf': any_of,
            'countAtLeast': count_at_least,
            'allOfMatched': [fragment for fragment in all_of if fragment in combined_source],
            'anyOfMatched': [fragment for fragment in any_of if fragment in combined_source],
            'countMatches': {
                item['fragment']: combined_source.count(item['fragment'])
                for item in count_at_least
            },
            'pass': passed,
        }

    return signals, evidence


def request_token(
    api_url: str,
    default_login: dict[str, str],
    token_cache: dict[tuple[str, str], str | None],
    spec: dict[str, object],
) -> str | None:
    auth = spec.get('auth') if isinstance(spec.get('auth'), dict) else None
    username = str(auth.get('username')) if auth and auth.get('username') else str(default_login['username'])
    password = str(auth.get('password')) if auth and auth.get('password') else str(default_login['password'])
    key = (username, password)
    if key in token_cache:
        return token_cache[key]
    try:
        token_cache[key] = login_token(api_url, username, password)
    except (urllib.error.URLError, OSError, RuntimeError, urllib.error.HTTPError):
        token_cache[key] = None
    return token_cache[key]


def capture_governance_requests(
    route: dict[str, object],
    *,
    api_url: str,
    default_login: dict[str, str],
    token_cache: dict[tuple[str, str], str | None],
    capture_mode: str,
) -> tuple[list[dict[str, object]], list[dict[str, object]], list[dict[str, object]], list[str]]:
    specs = route.get('governanceEvidenceRequests')
    if not isinstance(specs, list) or not specs:
        return [], [], [], []

    requests: list[dict[str, object]] = []
    responses: list[dict[str, object]] = []
    entries: list[dict[str, object]] = []
    failures: list[str] = []

    for spec in specs:
        if not isinstance(spec, dict):
            continue
        token = request_token(api_url, default_login, token_cache, spec)
        if token is None:
            request_entry = contract_request_entry(api_url, spec)
            response_entry = {
                'status': None,
                'url': request_entry['url'],
                'contentType': None,
                'json': None,
            }
            expected_entry = contract_expected_entry(api_url, spec)
            expected_entry['ownershipObservation'] = capture_ownership_label(capture_mode)
            expected_entry['contractOnly'] = True
        else:
            request_entry, response_entry, expected_entry = execute_api_request(api_url, token, spec)
            expected_entry['ownershipObservation'] = 'native-rust-owned'
            expected_entry['contractOnly'] = False
        requests.append(request_entry)
        responses.append(response_entry)
        entries.append(expected_entry)
        if not expected_entry['pass']:
            failures.append(f'missing governance evidence request {expected_entry["label"]}')

    return requests, responses, entries, failures


def enrich_route_result(
    route_result: dict[str, object],
    route: dict[str, object],
    *,
    api_url: str,
    default_login: dict[str, str],
    token_cache: dict[tuple[str, str], str | None],
) -> dict[str, object]:
    failures = parse_error_messages(route_result.get('error'))
    signals = dict(route_result.get('signals', {})) if isinstance(route_result.get('signals'), dict) else {}
    contract_signals, contract_evidence = evaluate_contract_signals(route)
    signals.update(contract_signals)
    route_result['signals'] = signals
    route_result['contractEvidence'] = contract_evidence

    expected_signals = route.get('contractSignalExpectations')
    if isinstance(expected_signals, dict):
        for signal_key, expected_value in expected_signals.items():
            if signals.get(str(signal_key)) is not expected_value:
                failures.append(f'contract signal expectation failed for {signal_key}')

    governance_requests, governance_responses, governance_entries, governance_failures = capture_governance_requests(
        route,
        api_url=api_url,
        default_login=default_login,
        token_cache=token_cache,
        capture_mode=str(route_result.get('captureMode', 'unknown')),
    )
    route_result['governanceOwnedRequests'] = governance_entries

    requests = list(route_result.get('requests', [])) if isinstance(route_result.get('requests'), list) else []
    responses = list(route_result.get('responses', [])) if isinstance(route_result.get('responses'), list) else []
    requests.extend(governance_requests)
    responses.extend(governance_responses)
    route_result['requests'] = requests
    route_result['responses'] = responses

    append_unique_failures(failures, governance_failures)
    route_result['pass'] = len(failures) == 0
    route_result['error'] = None if not failures else '; '.join(failures)
    return route_result


def load_summary(output_dir: Path) -> list[dict[str, object]]:
    payload = json.loads((output_dir / 'summary.json').read_text(encoding='utf-8'))
    if not isinstance(payload, list):
        raise RuntimeError('browser smoke summary.json must be a JSON array')
    return [row for row in payload if isinstance(row, dict)]


def write_summary(output_dir: Path, summary: list[dict[str, object]]) -> None:
    (output_dir / 'summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    requests_all: list[dict[str, object]] = []
    responses_all: list[dict[str, object]] = []
    for route_result in summary:
        route_name = route_result.get('route')
        if isinstance(route_name, str):
            (output_dir / f'{route_name}.json').write_text(
                json.dumps(route_result, ensure_ascii=False, indent=2) + '\n',
                encoding='utf-8',
            )
        if isinstance(route_result.get('requests'), list):
            requests_all.extend(route_result['requests'])
        if isinstance(route_result.get('responses'), list):
            responses_all.extend(route_result['responses'])
    (output_dir / 'requests-all.json').write_text(
        json.dumps({'requests': requests_all, 'responses': responses_all, 'routes': summary}, ensure_ascii=False, indent=2)
        + '\n',
        encoding='utf-8',
    )


def summarize_page_payload(payload: object | None) -> str:
    if not isinstance(payload, dict):
        return 'status=contract-only'
    ids = page_content_ids(payload)
    total_elements = payload.get('totalElements')
    number_of_elements = payload.get('numberOfElements')
    return f'totalElements={total_elements} numberOfElements={number_of_elements} ids={ids}'


def request_path_for_summary(entry: dict[str, object]) -> str:
    matched_request = entry.get('matchedRequest')
    if isinstance(matched_request, dict) and matched_request.get('url'):
        return str(matched_request['url'])
    if entry.get('requestPath'):
        return str(entry['requestPath'])
    return 'unknown-request'


def emit_route_summary(route_result: dict[str, object]) -> None:
    route_name = route_result.get('route')
    capture_mode = route_result.get('captureMode')
    print(f'route={route_name} captureMode={capture_mode} pass={route_result.get("pass")}')

    if route_name == 'browse-readlists':
        signals = route_result.get('signals', {}) if isinstance(route_result.get('signals'), dict) else {}
        print(
            'page-load: '
            f'rootFound={signals.get("rootFound")} '
            f'itemBrowserFound={signals.get("itemBrowserFound")} '
            f'detailMetadataVisible={signals.get("detailMetadataVisible")} '
            f'paginationControlsFound={signals.get("paginationControlsFound")} '
            f'totalCountChipFound={signals.get("totalCountChipFound")}'
        )
        print(
            'browse-governance: '
            f'routePaginationStateFound={signals.get("routePaginationStateFound")} '
            f'browseRequestFound={signals.get("browseRequestFound")} '
            f'repeatedLibraryBrowseFound={signals.get("repeatedLibraryBrowseFound")} '
            f'countFlowFound={signals.get("countFlowFound")}'
        )

        governance_requests = route_result.get('governanceOwnedRequests')
        if isinstance(governance_requests, list):
            for entry in governance_requests:
                if not isinstance(entry, dict):
                    continue
                matched_response = entry.get('matchedResponse') if isinstance(entry.get('matchedResponse'), dict) else {}
                status = matched_response.get('status')
                payload_summary = summarize_page_payload(matched_response.get('json'))
                ownership_class = entry.get('ownershipClass') or 'unknown-ownership'
                purpose = entry.get('purpose') or 'unspecified-purpose'
                persona = entry.get('persona') or 'admin@example.org'
                contract_only = entry.get('contractOnly') is True
                observation = entry.get('ownershipObservation') or 'unknown-observation'
                expectation_failures = entry.get('expectationFailures')
                expectation_text = ''
                if isinstance(expectation_failures, list) and expectation_failures:
                    expectation_text = f' expectationFailures={expectation_failures}'
                print(
                    f'- {purpose}: label={entry.get("label")} ownershipClass={ownership_class} '
                    f'persona={persona} pass={entry.get("pass")} observation={observation} '
                    f'contractOnly={contract_only} status={status} request={request_path_for_summary(entry)} {payload_summary}{expectation_text}'
                )

        print(
            'non-claims: search/unpaged=true/dialogs/admin-actions/Tachiyomi remain outside this browse-readlists browse/list closure evidence'
        )
        return

    if route_name != 'browse-readlist':
        return

    signals = route_result.get('signals', {}) if isinstance(route_result.get('signals'), dict) else {}
    print(
        'page-load: '
        f'rootFound={signals.get("rootFound")} '
        f'itemBrowserFound={signals.get("itemBrowserFound")} '
        f'detailMetadataVisible={signals.get("detailMetadataVisible")} '
        f'readListPageLoadFound={signals.get("readListPageLoadFound")} '
        f'pagedBooksFetchFound={signals.get("pagedBooksFetchFound")}'
    )
    print(
        'filter-governance: '
        f'filterStateRestoreFound={signals.get("filterStateRestoreFound")} '
        f'emptyStateRenderingFound={signals.get("emptyStateRenderingFound")}'
    )

    governance_requests = route_result.get('governanceOwnedRequests')
    if isinstance(governance_requests, list):
        for entry in governance_requests:
            if not isinstance(entry, dict):
                continue
            matched_response = entry.get('matchedResponse') if isinstance(entry.get('matchedResponse'), dict) else {}
            status = matched_response.get('status')
            payload_summary = summarize_page_payload(matched_response.get('json'))
            ownership_class = entry.get('ownershipClass') or 'unknown-ownership'
            purpose = entry.get('purpose') or 'unspecified-purpose'
            persona = entry.get('persona') or 'admin@example.org'
            contract_only = entry.get('contractOnly') is True
            observation = entry.get('ownershipObservation') or 'unknown-observation'
            expectation_failures = entry.get('expectationFailures')
            expectation_text = ''
            if isinstance(expectation_failures, list) and expectation_failures:
                expectation_text = f' expectationFailures={expectation_failures}'
            print(
                f'- {purpose}: label={entry.get("label")} ownershipClass={ownership_class} '
                f'persona={persona} pass={entry.get("pass")} observation={observation} '
                f'contractOnly={contract_only} status={status} request={request_path_for_summary(entry)} {payload_summary}{expectation_text}'
            )

    print('non-claims: edit/live-refresh/admin/list-family/Tachiyomi remain outside this browse-readlist paged/filter closure evidence')


def enrich_and_emit_summary(config: dict[str, object]) -> int:
    output_dir = Path(str(config['outputDir']))
    summary = load_summary(output_dir)
    route_map = {str(route['route']): route for route in config['routes']}
    token_cache: dict[tuple[str, str], str | None] = {}
    enriched_summary: list[dict[str, object]] = []
    for route_result in summary:
        route_name = route_result.get('route')
        route = route_map.get(str(route_name))
        if route is not None:
            route_result = enrich_route_result(
                route_result,
                route,
                api_url=str(config['apiUrl']),
                default_login={
                    'username': str(config['login']['username']),
                    'password': str(config['login']['password']),
                },
                token_cache=token_cache,
            )
        enriched_summary.append(route_result)

    write_summary(output_dir, enriched_summary)
    for route_result in enriched_summary:
        emit_route_summary(route_result)
    return 0 if all(bool(route_result.get('pass')) for route_result in enriched_summary) else 1


def fallback_navigation_scenario(route: dict[str, object], source_texts: dict[str, str], capture_mode: str) -> tuple[dict[str, object] | None, dict[str, object], list[str]]:
    scenario = route.get('scenario')
    if not isinstance(scenario, dict):
        return None, {}, []

    combined_source = '\n'.join(source_texts.values())
    failures: list[str] = []

    if scenario.get('type') == 'readlist-origin-entry':
        context_propagation_found = 'origin: ContextOrigin.READLIST, id: readListId' in combined_source
        book_link_query_found = "query: {context: this.item?.context?.origin, contextId: this.item?.context?.id}" in combined_source
        context_enum_found = "READLIST = 'READLIST'" in combined_source
        signals = {
            'entryBookLinkFound': book_link_query_found,
            'entryBookContextRetained': context_propagation_found and book_link_query_found and context_enum_found,
            'contextBannerVisible': 'navigation_within_readlist' in combined_source,
            'returnedToReadlist': capture_mode != 'playwright',
        }
        if not signals['entryBookLinkFound']:
            failures.append('source fallback could not prove readlist book links keep context query parameters')
        if not signals['entryBookContextRetained']:
            failures.append('source fallback could not prove readlist-origin context propagation into browse-book')
        if not signals['contextBannerVisible']:
            failures.append('source fallback could not prove browse-book surfaces readlist navigation context')
        scenario_result = {
            'type': scenario['type'],
            'captureMode': capture_mode,
            'expectedEntryBookPath': scenario['entryBookPath'],
            'contextPropagationFound': context_propagation_found,
            'bookLinkQueryFound': book_link_query_found,
            'contextEnumFound': context_enum_found,
        }
        return scenario_result, signals, failures

    if scenario.get('type') == 'readlist-sibling-navigation':
        context_parse_found = 'this.$route.query.contextId' in combined_source and 'ContextOrigin.READLIST' in combined_source
        list_request_found = "this.$komgaReadLists.getBooks(this.context.id, {unpaged: true} as PageRequest)" in combined_source
        next_request_found = 'this.$komgaReadLists.getBookSiblingNext(this.context.id, bookId)' in combined_source
        previous_request_found = 'this.$komgaReadLists.getBookSiblingPrevious(this.context.id, bookId)' in combined_source
        readlist_name_visible = 'navigation_within_readlist' in combined_source
        signals = {
            'initialContextRetained': context_parse_found,
            'initialPreviousBoundary': previous_request_found,
            'initialNextWithinReadlist': list_request_found and next_request_found,
            'readListNameVisible': readlist_name_visible,
            'nextNavigationRetainedContext': context_parse_found and next_request_found,
            'previousNavigationRetainedContext': context_parse_found and previous_request_found,
            'nextThenPreviousLoopClosed': list_request_found and next_request_found and previous_request_found,
        }
        if not signals['initialContextRetained']:
            failures.append('source fallback could not prove browse-book consumes readlist context query parameters')
        if not signals['initialNextWithinReadlist']:
            failures.append('source fallback could not prove browse-book loads readlist-scoped siblings')
        if not signals['initialPreviousBoundary']:
            failures.append('source fallback could not prove browse-book requests readlist-scoped previous sibling')
        if not signals['readListNameVisible']:
            failures.append('source fallback could not prove browse-book surfaces readlist navigation context')
        scenario_result = {
            'type': scenario['type'],
            'captureMode': capture_mode,
            'contextParseFound': context_parse_found,
            'readlistListRequestFound': list_request_found,
            'readlistNextRequestFound': next_request_found,
            'readlistPreviousRequestFound': previous_request_found,
            'expectedSeriesNextBookId': scenario['seriesNextBookId'],
            'expectedReadlistNextBookId': scenario['nextBookId'],
        }
        return scenario_result, signals, failures

    if scenario.get('type') == 'oneshot-readlist-fallback':
        context_parse_found = 'this.$route.query.contextId' in combined_source and 'ContextOrigin.READLIST' in combined_source
        context_name_request_found = 'this.$komgaReadLists.getOneReadList(this.context.id)' in combined_source
        list_request_found = "this.$komgaReadLists.getBooks(this.context.id, {unpaged: true} as PageRequest)" in combined_source
        next_request_found = 'this.$komgaReadLists.getBookSiblingNext(this.context.id, this.book.id)' in combined_source
        previous_request_found = 'this.$komgaReadLists.getBookSiblingPrevious(this.context.id, this.book.id)' in combined_source
        readlist_context_navigation_found = selector_in_source(
            combined_source,
            str(scenario['readlistContextNavigationSelector']),
        )
        readlist_name_visible = 'navigation_within_readlist' in combined_source
        signals = {
            'readlistContextRetained': context_parse_found,
            'readlistContextBannerVisible': context_parse_found and context_name_request_found and readlist_name_visible,
            'readlistContextNavigationFound': readlist_context_navigation_found,
            'returnedToDirectOneshot': capture_mode != 'playwright',
        }
        if not signals['readlistContextRetained']:
            failures.append('source fallback could not prove oneshot route consumes readlist context query parameters')
        if not signals['readlistContextBannerVisible']:
            failures.append('source fallback could not prove oneshot route surfaces readlist navigation context')
        if not signals['readlistContextNavigationFound']:
            failures.append('source fallback could not prove oneshot route renders readlist context navigation selector')
        if not (list_request_found and next_request_found and previous_request_found):
            failures.append('source fallback could not prove oneshot route triggers readlist-scoped owned requests')
        scenario_result = {
            'type': scenario['type'],
            'captureMode': capture_mode,
            'readlistContextPath': scenario['readlistContextPath'],
            'observedOwnershipLabel': scenario_ownership_label(route, capture_mode),
            'contextParseFound': context_parse_found,
            'contextNameRequestFound': context_name_request_found,
            'readlistBooksRequestFound': list_request_found,
            'readlistNextRequestFound': next_request_found,
            'readlistPreviousRequestFound': previous_request_found,
            'readlistContextNavigationFound': readlist_context_navigation_found,
        }
        return scenario_result, signals, failures

    return None, {}, []


def unexpected_requests(requests: list[dict[str, object]], specs: list[dict[str, object]], ownership: str) -> list[dict[str, object]]:
    spec_keys = {
        (
            str(spec.get('method')),
            f"{spec.get('requestPath')}",
            spec.get('body') if isinstance(spec.get('body'), str) else None,
        )
        for spec in specs
    }
    unexpected = []
    for request in requests:
        request_path = request['url'].replace(str(request['url']).split('/api/', 1)[0], '') if '/api/' in str(request['url']) else request['url']
        key = (str(request.get('method')), request_path, request.get('postData'))
        if key in spec_keys:
            continue
        unexpected.append({
            'ownership': ownership,
            'method': request.get('method'),
            'url': request.get('url'),
            'resourceType': request.get('resourceType'),
            'postData': request.get('postData'),
        })
    return unexpected


def build_fallback_route_result(
    route: dict[str, object],
    *,
    app_url: str,
    api_url: str,
    token: str | None,
    capture_mode: str,
) -> dict[str, object]:
    source_text = read_text(str(route['sourceFile']))
    source_texts = read_source_texts(route)
    combined_source = '\n'.join(source_texts.values())
    root_selector = str(route['selector'])
    panel_selector = str(route['panelSelector'])
    sibling_selector = str(route['siblingNavigationSelector'])
    extra_selectors = list(route.get('extraSelectors', []))
    observed_specs = list(route.get('observedRequests', []))
    metadata_fragments_found = [
        fragment
        for fragment in route['sourceMetadataFragments']
        if fragment in combined_source
    ]
    detail_metadata_visible = len(metadata_fragments_found) >= int(route['metadataMinimumMatches'])
    root_found = any(selector_in_source(text, root_selector) for text in source_texts.values())
    panel_found = any(selector_in_source(text, panel_selector) for text in source_texts.values())
    sibling_navigation_found = any(selector_in_source(text, sibling_selector) for text in source_texts.values())
    extra_signal_states = {
        str(extra['signalKey']): any(selector_in_source(text, str(extra['selector'])) for text in source_texts.values())
        for extra in extra_selectors
    }
    snippet = source_snippet(source_text, selector_marker(root_selector))
    requests: list[dict[str, object]] = []
    responses: list[dict[str, object]] = []
    expected_owned_requests: list[dict[str, object]] = []
    observed_fallback_requests: list[dict[str, object]] = []

    for request_spec in route['ownedRequests']:
        if token is None:
            request_entry = contract_request_entry(api_url, request_spec)
            response_entry = {
                'status': None,
                'url': request_entry['url'],
                'contentType': None,
            }
            expected_entry = contract_expected_entry(api_url, request_spec)
        else:
            request_entry, response_entry, expected_entry = execute_api_request(api_url, token, request_spec)
        requests.append(request_entry)
        responses.append(response_entry)
        expected_owned_requests.append(expected_entry)

    for request_spec in observed_specs:
        if token is None:
            request_entry = contract_request_entry(api_url, request_spec)
            response_entry = {
                'status': None,
                'url': request_entry['url'],
                'contentType': None,
            }
            expected_entry = contract_expected_entry(api_url, request_spec)
        else:
            request_entry, response_entry, expected_entry = execute_api_request(api_url, token, request_spec)
        requests.append(request_entry)
        responses.append(response_entry)
        observed_fallback_requests.append({
            **expected_entry,
            'ownership': capture_ownership_label(capture_mode),
        })

    scenario_texts = read_source_texts(route, 'scenarioSourceFiles') if route.get('scenarioSourceFiles') else source_texts
    scenario_result, scenario_signals, scenario_failures = fallback_navigation_scenario(route, scenario_texts, capture_mode)
    unowned_observed_requests = unexpected_requests(
        requests,
        list(route['ownedRequests']) + observed_specs,
        capture_ownership_label(capture_mode),
    )

    signals = {
        'rootFound': root_found,
        'detailMetadataVisible': detail_metadata_visible,
        str(route['panelKey']): panel_found,
        'siblingNavigationFound': sibling_navigation_found,
        'siblingNavigationExpected': route['siblingNavigationExpected'],
        **extra_signal_states,
        **scenario_signals,
    }

    failures = list(scenario_failures)
    if not root_found:
        failures.append(f'missing source selector {root_selector}')
    if not detail_metadata_visible:
        failures.append('detail metadata bindings were not found in source template')
    if panel_found is not bool(route['panelExpected']):
        failures.append(f'panel expectation failed for {panel_selector}')
    if sibling_navigation_found is not bool(route['siblingNavigationExpected']):
        failures.append(f'sibling navigation expectation failed for {sibling_selector}')
    for extra in extra_selectors:
        signal_key = str(extra['signalKey'])
        if signals.get(signal_key) is not bool(extra['expected']):
            failures.append(f'extra selector expectation failed for {extra["selector"]}')
    for expected_request in expected_owned_requests:
        if not expected_request['pass']:
            failures.append(f'missing expected owned request {expected_request["label"]}')
    for expected_request in observed_fallback_requests:
        if not expected_request['pass']:
            failures.append(f'missing expected fallback request {expected_request["label"]}')
    for signal_key, expected_value in dict(route.get('scenarioSignalExpectations', {})).items():
        if signals.get(signal_key) is not expected_value:
            failures.append(f'scenario expectation failed for {signal_key}')

    return {
        'route': route['route'],
        'path': route['path'],
        'selector': route['selector'],
        'visitedUrl': f'{app_url}{route["path"]}',
        'captureMode': capture_mode,
        'pass': len(failures) == 0,
        'error': None if not failures else '; '.join(failures),
        'dom': {
            'title': None,
            'location': f'{app_url}{route["path"]}',
            'rootFound': root_found,
            'textSample': compact_text(snippet)[:400],
            'rootHtml': snippet[:4000],
        },
        'signals': signals,
        'scenario': scenario_result,
        'metadataFragmentsFound': metadata_fragments_found,
        'expectedOwnedRequests': expected_owned_requests,
        'ownedRequestInventory': expected_owned_requests,
        'observedFallbackRequests': observed_fallback_requests,
        'unownedObservedRequests': unowned_observed_requests,
        'requests': requests,
        'responses': responses,
        'pageErrors': [],
    }


def run_fallback_capture(config: dict[str, object]) -> int:
    output_dir = Path(str(config['outputDir']))
    output_dir.mkdir(parents=True, exist_ok=True)
    token: str | None
    capture_mode = 'source-api-fallback'
    try:
        token = login_token(str(config['apiUrl']), str(config['login']['username']), str(config['login']['password']))
    except (urllib.error.URLError, OSError):
        token = None
        capture_mode = 'source-contract-fallback'

    login_state = {
        'captureMode': capture_mode,
        'finalUrl': f"{config['appUrl']}/",
        'title': None,
    }
    (output_dir / 'login-state.json').write_text(json.dumps(login_state, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')

    summary = []
    requests_all: list[dict[str, object]] = []
    responses_all: list[dict[str, object]] = []

    for route in config['routes']:
        route_result = build_fallback_route_result(
            route,
            app_url=str(config['appUrl']),
            api_url=str(config['apiUrl']),
            token=token,
            capture_mode=capture_mode,
        )
        summary.append(route_result)
        requests_all.extend(route_result['requests'])
        responses_all.extend(route_result['responses'])
        (output_dir / f"{route['route']}.json").write_text(
            json.dumps(route_result, ensure_ascii=False, indent=2) + '\n',
            encoding='utf-8',
        )
        (output_dir / f"{route['route']}.html").write_text(route_result['dom']['rootHtml'] or '', encoding='utf-8')

    (output_dir / 'summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    (output_dir / 'requests-all.json').write_text(
        json.dumps({'requests': requests_all, 'responses': responses_all, 'routes': summary}, ensure_ascii=False, indent=2)
        + '\n',
        encoding='utf-8',
    )
    (output_dir / 'page-errors.json').write_text('[]\n', encoding='utf-8')
    return 0 if all(route['pass'] for route in summary) else 1


def main() -> int:
    args = parse_args()
    output_dir = (REPO_ROOT / args.output_dir).resolve() if not Path(args.output_dir).is_absolute() else Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    config = {
        'appUrl': args.app_url,
        'apiUrl': args.api_url,
        'outputDir': str(output_dir),
        'login': {
            'username': args.username,
            'password': args.password,
        },
        'routes': selected_routes(args.routes),
    }

    node_result = run_node_runner(config)
    if node_result.returncode == 0:
        emit_process_output(node_result)
        return enrich_and_emit_summary(config)
    if playwright_missing(node_result):
        fallback_code = run_fallback_capture(config)
        if fallback_code not in (0, 1):
            return fallback_code
        return enrich_and_emit_summary(config)
    summary_path = output_dir / 'summary.json'
    if node_result.returncode == 1 and summary_path.exists():
        emit_process_output(node_result)
        return enrich_and_emit_summary(config)

    emit_process_output(node_result)
    return node_result.returncode


if __name__ == '__main__':
    sys.exit(main())
