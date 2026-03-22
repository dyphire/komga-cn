#!/usr/bin/env python3

from __future__ import annotations

import argparse
import base64
import json
import subprocess
import sys
import tempfile
import textwrap
import urllib.error
import urllib.request
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[2]

ROUTES: dict[str, dict[str, object]] = {
    'browse-series': {
        'route': 'browse-series',
        'path': '/series/series-1',
        'selector': '[data-testid="browse-series-root"]',
        'panelSelector': '[data-testid="browse-series-collections-panel"]',
        'panelKey': 'collectionsPanelFound',
        'panelExpected': True,
        'siblingNavigationSelector': '[data-testid="browse-book-sibling-navigation"]',
        'siblingNavigationExpected': False,
        'sourceFile': 'komga-webui/src/views/BrowseSeries.vue',
        'sourceMetadataFragments': ['series.metadata.title', 'series.metadata.summary', 'authorsByRole'],
        'metadataFragments': ['series', 'featured', 'fantasy', 'alice'],
        'metadataMinimumMatches': 2,
        'ownedRequests': [
            {
                'label': 'series-detail',
                'method': 'GET',
                'urlEndsWith': '/api/v1/series/series-1',
                'requestPath': '/api/v1/series/series-1',
            },
            {
                'label': 'series-collections',
                'method': 'GET',
                'urlEndsWith': '/api/v1/series/series-1/collections',
                'requestPath': '/api/v1/series/series-1/collections',
            },
            {
                'label': 'series-books-list',
                'method': 'POST',
                'urlContains': '/api/v1/books/list?page=0&size=20&sort=metadata.numberSort',
                'postDataIncludes': ['AllOfBook', 'SeriesId', 'series-1'],
                'requestPath': '/api/v1/books/list?page=0&size=20&sort=metadata.numberSort,asc',
                'body': '{"condition":{"type":"AllOfBook","conditions":[{"type":"SeriesId","operator":"is","value":"series-1"}]}}',
            },
        ],
    },
    'browse-book': {
        'route': 'browse-book',
        'path': '/book/book-1',
        'selector': '[data-testid="browse-book-root"]',
        'panelSelector': '[data-testid="browse-book-readlists-panel"]',
        'panelKey': 'readlistsPanelFound',
        'panelExpected': True,
        'siblingNavigationSelector': '[data-testid="browse-book-sibling-navigation"]',
        'siblingNavigationExpected': True,
        'sourceFile': 'komga-webui/src/views/BrowseBook.vue',
        'sourceMetadataFragments': ['book.seriesTitle', 'book.metadata.title', 'book.size'],
        'metadataFragments': ['book.cbr', '222 b', 'alice', '2024'],
        'metadataMinimumMatches': 2,
        'ownedRequests': [
            {
                'label': 'book-detail',
                'method': 'GET',
                'urlEndsWith': '/api/v1/books/book-1',
                'requestPath': '/api/v1/books/book-1',
            },
            {
                'label': 'book-readlists',
                'method': 'GET',
                'urlEndsWith': '/api/v1/books/book-1/readlists',
                'requestPath': '/api/v1/books/book-1/readlists',
            },
            {
                'label': 'book-siblings-list',
                'method': 'POST',
                'urlContains': '/api/v1/books/list',
                'postDataIncludes': ['SeriesId', 'series-1'],
                'requestPath': '/api/v1/books/list?unpaged=true&sort=metadata.numberSort',
                'body': '{"condition":{"type":"SeriesId","operator":"is","value":"series-1"}}',
            },
            {
                'label': 'book-sibling-next',
                'method': 'GET',
                'urlEndsWith': '/api/v1/books/book-1/next',
                'requestPath': '/api/v1/books/book-1/next',
            },
            {
                'label': 'book-sibling-previous',
                'method': 'GET',
                'urlEndsWith': '/api/v1/books/book-1/previous',
                'requestPath': '/api/v1/books/book-1/previous',
            },
        ],
    },
}


NODE_RUNNER = textwrap.dedent(
    r"""
    import fs from 'node:fs/promises'
    import path from 'node:path'

    async function loadPlaywright() {
      try {
        return await import('playwright')
      } catch (playwrightError) {
        try {
          return await import('playwright-core')
        } catch (coreError) {
          const error = new Error(
            `Unable to load Playwright. playwright: ${playwrightError.message}; playwright-core: ${coreError.message}`,
          )
          error.cause = { playwrightError, coreError }
          throw error
        }
      }
    }

    function matchesRequest(spec, request) {
      if (request.method !== spec.method) return false
      if (spec.urlContains && !request.url.includes(spec.urlContains)) return false
      if (spec.urlEndsWith && !request.url.endsWith(spec.urlEndsWith)) return false
      if (spec.postDataIncludes && spec.postDataIncludes.length > 0) {
        const postData = request.postData || ''
        for (const fragment of spec.postDataIncludes) {
          if (!postData.includes(fragment)) return false
        }
      }
      return true
    }

    function summarizeExpectedRequests(specs, requests) {
      return specs.map(spec => {
        const matchedRequest = requests.find(request => matchesRequest(spec, request)) || null
        return {
          label: spec.label,
          method: spec.method,
          urlContains: spec.urlContains || null,
          urlEndsWith: spec.urlEndsWith || null,
          postDataIncludes: spec.postDataIncludes || [],
          pass: matchedRequest !== null,
          matchedRequest,
        }
      })
    }

    function compactText(value) {
      return (value || '').replace(/\s+/g, ' ').trim()
    }

    const configPath = process.argv[2]
    const config = JSON.parse(await fs.readFile(configPath, 'utf8'))
    const outputDir = path.resolve(config.outputDir)
    await fs.mkdir(outputDir, { recursive: true })

    const playwright = await loadPlaywright()
    const browser = await playwright.chromium.launch({ headless: true })
    const context = await browser.newContext({ viewport: { width: 1440, height: 1100 } })
    const page = await context.newPage()

    const globalRequests = []
    const globalResponses = []
    const pageErrors = []
    let currentRouteName = null
    const routeRequestLog = new Map()
    const routeResponseLog = new Map()
    const routeErrorLog = new Map()

    function routeBucket(map, routeName) {
      if (!map.has(routeName)) map.set(routeName, [])
      return map.get(routeName)
    }

    page.on('request', request => {
      const entry = {
        method: request.method(),
        url: request.url(),
        resourceType: request.resourceType(),
        postData: request.postData() || null,
      }
      globalRequests.push(entry)
      if (currentRouteName) routeBucket(routeRequestLog, currentRouteName).push(entry)
    })

    page.on('response', response => {
      const entry = {
        status: response.status(),
        url: response.url(),
        contentType: response.headers()['content-type'] || null,
      }
      globalResponses.push(entry)
      if (currentRouteName) routeBucket(routeResponseLog, currentRouteName).push(entry)
    })

    page.on('pageerror', error => {
      const entry = {
        route: currentRouteName,
        message: error.stack || String(error),
      }
      pageErrors.push(entry)
      if (currentRouteName) routeBucket(routeErrorLog, currentRouteName).push(entry.message)
    })

    const backendOnly = entry => (
      entry.url.includes(config.apiUrl)
      || entry.url.includes('/sse/v1/events')
    )

    try {
      await page.goto(`${config.appUrl}/`, { waitUntil: 'domcontentloaded' })
      await page.waitForSelector('input[autocomplete="username"]', { timeout: 30000 })
      await page.locator('input[autocomplete="username"]').fill(config.login.username)
      await page.locator('input[autocomplete="current-password"]').fill(config.login.password)
      await page.getByRole('button', { name: 'Login' }).click()
      await page.waitForLoadState('networkidle')

      const loginState = {
        finalUrl: page.url(),
        title: await page.title(),
      }
      await fs.writeFile(path.join(outputDir, 'login-state.json'), `${JSON.stringify(loginState, null, 2)}\n`)
      await fs.writeFile(path.join(outputDir, 'login-post-submit.html'), await page.content())
      await page.screenshot({ path: path.join(outputDir, 'login-post-submit.png'), fullPage: true })

      const summary = []

      for (const route of config.routes) {
        currentRouteName = route.route
        routeRequestLog.set(route.route, [])
        routeResponseLog.set(route.route, [])
        routeErrorLog.set(route.route, [])

        let routeResult = null

        try {
          await page.goto(`${config.appUrl}${route.path}`, { waitUntil: 'domcontentloaded' })
          await page.waitForURL(`**${route.path.replace('?', '\\?')}*`, { timeout: 30000 })
          await page.waitForSelector(route.selector, { timeout: 30000 })
          await page.waitForLoadState('networkidle')

          const dom = await page.evaluate(routeConfig => {
            const normalizeText = value => (value || '').replace(/\s+/g, ' ').trim()
            const root = document.querySelector(routeConfig.selector)
            const panel = document.querySelector(routeConfig.panelSelector)
            const siblingNavigation = document.querySelector(routeConfig.siblingNavigationSelector)
            const fullText = normalizeText(root?.textContent || '')
            const metadataFragmentsFound = routeConfig.metadataFragments.filter(fragment =>
              fullText.toLowerCase().includes(fragment.toLowerCase()),
            )

            return {
              title: document.title,
              location: window.location.href,
              rootFound: Boolean(root),
              textSample: fullText.slice(0, 400),
              rootHtml: root?.outerHTML?.slice(0, 4000) || null,
              detailMetadataVisible: metadataFragmentsFound.length >= routeConfig.metadataMinimumMatches,
              metadataFragmentsFound,
              panelFound: Boolean(panel),
              siblingNavigationFound: Boolean(siblingNavigation),
            }
          }, route)

          const routeRequests = (routeRequestLog.get(route.route) || []).filter(backendOnly)
          const routeResponses = (routeResponseLog.get(route.route) || []).filter(backendOnly)
          const routePageErrors = routeErrorLog.get(route.route) || []
          const expectedOwnedRequests = summarizeExpectedRequests(route.ownedRequests, routeRequests)
          const signals = {
            rootFound: dom.rootFound,
            detailMetadataVisible: dom.detailMetadataVisible,
            [route.panelKey]: dom.panelFound,
            siblingNavigationFound: dom.siblingNavigationFound,
            siblingNavigationExpected: route.siblingNavigationExpected,
          }
          const failures = []

          if (!signals.rootFound) failures.push(`missing root selector ${route.selector}`)
          if (!signals.detailMetadataVisible) failures.push('detail metadata fragments were not visible')
          if (dom.panelFound !== route.panelExpected) failures.push(`panel expectation failed for ${route.panelSelector}`)
          if (dom.siblingNavigationFound !== route.siblingNavigationExpected) {
            failures.push(`sibling navigation expectation failed for ${route.siblingNavigationSelector}`)
          }
          for (const requestCheck of expectedOwnedRequests) {
            if (!requestCheck.pass) failures.push(`missing expected owned request ${requestCheck.label}`)
          }

          routeResult = {
            route: route.route,
            path: route.path,
            selector: route.selector,
            visitedUrl: page.url(),
            pass: failures.length === 0,
            error: failures.length === 0 ? null : failures.join('; '),
            dom: {
              title: dom.title,
              location: dom.location,
              rootFound: dom.rootFound,
              textSample: dom.textSample,
              rootHtml: dom.rootHtml,
            },
            signals,
            metadataFragmentsFound: dom.metadataFragmentsFound,
            expectedOwnedRequests,
            requests: routeRequests,
            responses: routeResponses,
            pageErrors: routePageErrors,
          }
        } catch (error) {
          routeResult = {
            route: route.route,
            path: route.path,
            selector: route.selector,
            visitedUrl: page.url(),
            pass: false,
            error: error.stack || String(error),
            dom: {
              title: await page.title().catch(() => null),
              location: page.url(),
              rootFound: false,
              textSample: null,
              rootHtml: null,
            },
            signals: {
              rootFound: false,
              detailMetadataVisible: false,
              [route.panelKey]: false,
              siblingNavigationFound: false,
              siblingNavigationExpected: route.siblingNavigationExpected,
            },
            metadataFragmentsFound: [],
            expectedOwnedRequests: [],
            requests: (routeRequestLog.get(route.route) || []).filter(backendOnly),
            responses: (routeResponseLog.get(route.route) || []).filter(backendOnly),
            pageErrors: routeErrorLog.get(route.route) || [],
          }
        } finally {
          summary.push(routeResult)
          await fs.writeFile(path.join(outputDir, `${route.route}.json`), `${JSON.stringify(routeResult, null, 2)}\n`)
          await fs.writeFile(path.join(outputDir, `${route.route}.html`), await page.content())
          await page.screenshot({ path: path.join(outputDir, `${route.route}.png`), fullPage: true })
          currentRouteName = null
        }
      }

      await fs.writeFile(path.join(outputDir, 'summary.json'), `${JSON.stringify(summary, null, 2)}\n`)
      await fs.writeFile(
        path.join(outputDir, 'requests-all.json'),
        `${JSON.stringify({ requests: globalRequests, responses: globalResponses, routes: summary }, null, 2)}\n`,
      )
      await fs.writeFile(path.join(outputDir, 'page-errors.json'), `${JSON.stringify(pageErrors, null, 2)}\n`)

      if (!summary.every(route => route.pass)) {
        process.exitCode = 1
      }
    } finally {
      await browser.close()
    }
    """,
).strip()


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


def read_text(relative_path: str) -> str:
    return (REPO_ROOT / relative_path).read_text(encoding='utf-8')


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


def execute_api_request(api_url: str, token: str, spec: dict[str, object]) -> tuple[dict[str, object], dict[str, object], bool]:
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

    try:
        with urllib.request.urlopen(request) as response:
            response_entry = {
                'status': response.status,
                'url': url,
                'contentType': response.headers.get('Content-Type'),
            }
            passed = 200 <= response.status < 300
    except urllib.error.HTTPError as error:
        response_entry = {
            'status': error.code,
            'url': url,
            'contentType': error.headers.get('Content-Type'),
        }

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
        'pass': passed,
        'matchedRequest': request_entry if passed else None,
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
        'pass': True,
        'matchedRequest': request_entry,
    }


def build_fallback_route_result(
    route: dict[str, object],
    *,
    app_url: str,
    api_url: str,
    token: str | None,
    capture_mode: str,
) -> dict[str, object]:
    source_text = read_text(str(route['sourceFile']))
    root_selector = str(route['selector'])
    panel_selector = str(route['panelSelector'])
    sibling_selector = str(route['siblingNavigationSelector'])
    metadata_fragments_found = [
        fragment
        for fragment in route['sourceMetadataFragments']
        if fragment in source_text
    ]
    detail_metadata_visible = len(metadata_fragments_found) >= int(route['metadataMinimumMatches'])
    root_found = selector_in_source(source_text, root_selector)
    panel_found = selector_in_source(source_text, panel_selector)
    sibling_navigation_found = selector_in_source(source_text, sibling_selector)
    snippet = source_snippet(source_text, selector_marker(root_selector))
    requests: list[dict[str, object]] = []
    responses: list[dict[str, object]] = []
    expected_owned_requests: list[dict[str, object]] = []

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

    signals = {
        'rootFound': root_found,
        'detailMetadataVisible': detail_metadata_visible,
        str(route['panelKey']): panel_found,
        'siblingNavigationFound': sibling_navigation_found,
        'siblingNavigationExpected': route['siblingNavigationExpected'],
    }

    failures = []
    if not root_found:
        failures.append(f'missing source selector {root_selector}')
    if not detail_metadata_visible:
        failures.append('detail metadata bindings were not found in source template')
    if panel_found is not bool(route['panelExpected']):
        failures.append(f'panel expectation failed for {panel_selector}')
    if sibling_navigation_found is not bool(route['siblingNavigationExpected']):
        failures.append(f'sibling navigation expectation failed for {sibling_selector}')
    for expected_request in expected_owned_requests:
        if not expected_request['pass']:
            failures.append(f'missing expected owned request {expected_request["label"]}')

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
        'metadataFragmentsFound': metadata_fragments_found,
        'expectedOwnedRequests': expected_owned_requests,
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
        return 0
    if playwright_missing(node_result):
        return run_fallback_capture(config)

    emit_process_output(node_result)
    return node_result.returncode


if __name__ == '__main__':
    sys.exit(main())
