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

function responsePasses(spec, response) {
  if (!response) return false
  if (spec.responseStatuses && spec.responseStatuses.length > 0) {
    return spec.responseStatuses.includes(response.status)
  }
  return true
}

function summarizeExpectedRequests(specs, requests, responses) {
  return specs.map(spec => {
    const matchedRequest = requests.find(request => matchesRequest(spec, request)) || null
    const matchedResponse = matchedRequest
      ? responses.find(response => response.url === matchedRequest.url && responsePasses(spec, response)) || null
      : null
    const requiresResponseMatch = spec.responseStatuses && spec.responseStatuses.length > 0
    return {
      label: spec.label,
      method: spec.method,
      urlContains: spec.urlContains || null,
      urlEndsWith: spec.urlEndsWith || null,
      postDataIncludes: spec.postDataIncludes || [],
      responseStatuses: spec.responseStatuses || [],
      pass: matchedRequest !== null && (!requiresResponseMatch || matchedResponse !== null),
      matchedRequest,
      matchedResponse,
    }
  })
}

function withOwnership(checks, ownership) {
  return checks.map(check => ({ ...check, ownership }))
}

function summarizeObservedUnownedRequests(specs, requests, ownershipLabel) {
  return requests
    .filter(request => !specs.some(spec => matchesRequest(spec, request)))
    .map(request => ({
      ownership: ownershipLabel,
      method: request.method,
      url: request.url,
      resourceType: request.resourceType,
      postData: request.postData,
    }))
}

function hasExpectedContext(url, scenario) {
  try {
    const parsed = new URL(url)
    return parsed.searchParams.get('context') === scenario.contextOrigin
      && parsed.searchParams.get('contextId') === scenario.contextId
  } catch {
    return false
  }
}

async function runReadlistOriginEntryScenario(page, route) {
  const scenario = route.scenario
  const linkLocator = page.locator(`${route.selector} .item-card a.link-underline[href*="/book/"]`).first()
  const linkCount = await linkLocator.count()
  const failures = []

  if (linkCount === 0) {
    return {
      scenario: {
        type: scenario.type,
        entryBookLinkFound: false,
      },
      signals: {
        entryBookLinkFound: false,
        entryBookContextRetained: false,
      },
      failures: ['readlist entry book link was not found'],
    }
  }

  const entryBookHref = await linkLocator.getAttribute('href')
  const expectedHrefFragment = `${scenario.entryBookPath}`
  const entryBookLinkFound = Boolean(entryBookHref && entryBookHref.includes(expectedHrefFragment))

  if (!entryBookLinkFound) {
    failures.push(`readlist entry book link did not keep expected context (${expectedHrefFragment})`)
  }

  await linkLocator.click()
  await page.waitForURL(`**${scenario.entryBookPath.replace('?', '\\?')}*`, { timeout: 30000 })
  await page.waitForSelector('[data-testid="browse-book-root"]', { timeout: 30000 })
  await page.waitForLoadState('networkidle')

  const navigatedUrl = page.url()
  const entryBookContextRetained = hasExpectedContext(navigatedUrl, scenario)
  if (!entryBookContextRetained) {
    failures.push('readlist-origin navigation lost context query parameters')
  }

  const contextBannerText = compactText(await page.locator('[data-testid="browse-book-root"]').textContent())
  const contextBannerVisible = contextBannerText.includes(scenario.readListName)
  if (!contextBannerVisible) {
    failures.push(`browse-book did not surface readlist name ${scenario.readListName}`)
  }

  await page.goBack({ waitUntil: 'domcontentloaded' })
  await page.waitForURL(`**${route.path.replace('?', '\\?')}*`, { timeout: 30000 })
  await page.waitForSelector(route.selector, { timeout: 30000 })
  await page.waitForLoadState('networkidle')

  return {
    scenario: {
      type: scenario.type,
      entryBookHref,
      expectedEntryBookPath: scenario.entryBookPath,
      navigatedBookUrl: navigatedUrl,
      contextBannerVisible,
      returnedToReadlist: true,
    },
    signals: {
      entryBookLinkFound,
      entryBookContextRetained,
      contextBannerVisible,
      returnedToReadlist: true,
    },
    failures,
  }
}

function buttonState(button) {
  if (!button) return { href: null, disabled: false }
  const href = button.getAttribute('href') || button.href || null
  const disabled = button.hasAttribute('disabled')
    || button.getAttribute('aria-disabled') === 'true'
    || button.classList.contains('v-btn--disabled')
  return { href, disabled }
}

async function siblingNavigationState(page, selector, scenario) {
  return page.evaluate(({ selector: navSelector, scenarioConfig }) => {
    const navigation = document.querySelector(navSelector)
    const buttons = Array.from(navigation?.querySelectorAll('.v-btn') || [])
    const previous = buttons[0]
    const next = buttons[2]
    const rootText = (document.querySelector('[data-testid="browse-book-root"]')?.textContent || '').replace(/\s+/g, ' ').trim()

    const describe = button => {
      if (!button) return { href: null, disabled: false }
      return {
        href: button.getAttribute('href') || button.href || null,
        disabled: button.hasAttribute('disabled')
          || button.getAttribute('aria-disabled') === 'true'
          || button.classList.contains('v-btn--disabled'),
      }
    }

    return {
      previous: describe(previous),
      next: describe(next),
      readListNameVisible: rootText.includes(scenarioConfig.readListName),
    }
  }, { selector, scenarioConfig: scenario })
}

async function runReadlistSiblingNavigationScenario(page, route) {
  const scenario = route.scenario
  const failures = []
  const initialState = await siblingNavigationState(page, route.siblingNavigationSelector, scenario)
  const initialContextRetained = hasExpectedContext(page.url(), scenario)
  const initialPreviousBoundary = initialState.previous.disabled && !initialState.previous.href
  const initialNextWithinReadlist = Boolean(
    initialState.next.href
    && initialState.next.href.includes(`/book/${scenario.nextBookId}`)
    && initialState.next.href.includes(`context=${scenario.contextOrigin}`)
    && initialState.next.href.includes(`contextId=${scenario.contextId}`)
    && !initialState.next.href.includes(`/book/${scenario.seriesNextBookId}`),
  )

  if (!initialContextRetained) {
    failures.push('browse-book entry lost readlist context query parameters')
  }
  if (!initialState.readListNameVisible) {
    failures.push(`browse-book did not surface readlist name ${scenario.readListName}`)
  }
  if (!initialPreviousBoundary) {
    failures.push('readlist previous boundary was not preserved on the first book')
  }
  if (!initialNextWithinReadlist) {
    failures.push(`readlist next navigation did not stay inside readlist order (expected ${scenario.nextBookId})`)
  }

  const nextLocator = page.locator(`${route.siblingNavigationSelector} a[href*="/book/${scenario.nextBookId}"]`).first()
  if (await nextLocator.count() === 0) {
    failures.push(`next navigation link to ${scenario.nextBookId} was not rendered`)
    return {
      scenario: {
        type: scenario.type,
        initialState,
      },
      signals: {
        initialContextRetained,
        initialPreviousBoundary,
        initialNextWithinReadlist,
        readListNameVisible: initialState.readListNameVisible,
        nextNavigationRetainedContext: false,
        previousNavigationRetainedContext: false,
        nextThenPreviousLoopClosed: false,
      },
      failures,
    }
  }

  await nextLocator.click()
  await page.waitForURL(`**${scenario.nextBookPath.replace('?', '\\?')}*`, { timeout: 30000 })
  await page.waitForSelector(route.selector, { timeout: 30000 })
  await page.waitForLoadState('networkidle')

  const nextVisitedUrl = page.url()
  const nextNavigationRetainedContext = hasExpectedContext(nextVisitedUrl, scenario)
  const nextState = await siblingNavigationState(page, route.siblingNavigationSelector, scenario)
  const previousBackWithinReadlist = Boolean(
    nextState.previous.href
    && nextState.previous.href.includes(`/book/${scenario.entryBookId}`)
    && nextState.previous.href.includes(`context=${scenario.contextOrigin}`)
    && nextState.previous.href.includes(`contextId=${scenario.contextId}`),
  )

  if (!nextNavigationRetainedContext) {
    failures.push('next navigation lost readlist context query parameters')
  }
  if (!previousBackWithinReadlist) {
    failures.push(`previous navigation from ${scenario.nextBookId} did not return to ${scenario.entryBookId}`)
  }

  const previousLocator = page.locator(`${route.siblingNavigationSelector} a[href*="/book/${scenario.entryBookId}"]`).first()
  if (await previousLocator.count() === 0) {
    failures.push(`previous navigation link back to ${scenario.entryBookId} was not rendered on ${scenario.nextBookId}`)
    return {
      scenario: {
        type: scenario.type,
        initialState,
        nextState,
        nextVisitedUrl,
      },
      signals: {
        initialContextRetained,
        initialPreviousBoundary,
        initialNextWithinReadlist,
        readListNameVisible: initialState.readListNameVisible,
        nextNavigationRetainedContext,
        previousNavigationRetainedContext: false,
        nextThenPreviousLoopClosed: false,
      },
      failures,
    }
  }

  await previousLocator.click()
  await page.waitForURL(`**${scenario.entryBookPath.replace('?', '\\?')}*`, { timeout: 30000 })
  await page.waitForSelector(route.selector, { timeout: 30000 })
  await page.waitForLoadState('networkidle')

  const previousVisitedUrl = page.url()
  const previousNavigationRetainedContext = hasExpectedContext(previousVisitedUrl, scenario)
  const nextThenPreviousLoopClosed = previousVisitedUrl.includes(scenario.entryBookPath)

  if (!previousNavigationRetainedContext) {
    failures.push('previous navigation lost readlist context query parameters')
  }
  if (!nextThenPreviousLoopClosed) {
    failures.push('next then previous navigation did not return to the anchor readlist book')
  }

  return {
    scenario: {
      type: scenario.type,
      initialState,
      nextState,
      nextVisitedUrl,
      previousVisitedUrl,
      expectedSeriesNextBookId: scenario.seriesNextBookId,
    },
    signals: {
      initialContextRetained,
      initialPreviousBoundary,
      initialNextWithinReadlist,
      readListNameVisible: initialState.readListNameVisible,
      nextNavigationRetainedContext,
      previousNavigationRetainedContext,
      nextThenPreviousLoopClosed,
    },
    failures,
  }
}

async function runOneshotReadlistFallbackScenario(page, route) {
  const scenario = route.scenario
  const failures = []
  const contextTarget = new URL(scenario.readlistContextPath, page.url()).toString()
  const directTarget = new URL(route.path, page.url()).toString()

  await page.goto(contextTarget, { waitUntil: 'domcontentloaded' })
  await page.waitForURL(`**${scenario.readlistContextPath.replace('?', '\\?')}*`, { timeout: 30000 })
  await page.waitForSelector(route.selector, { timeout: 30000 })
  await page.waitForLoadState('networkidle')

  const contextUrl = page.url()
  const contextText = compactText(await page.locator(route.selector).textContent())
  const readlistContextRetained = hasExpectedContext(contextUrl, {
    contextOrigin: 'READLIST',
    contextId: scenario.readListId,
  })
  const readlistContextBannerVisible = contextText.includes(scenario.readListName)
  const readlistContextNavigationFound = await page.locator(scenario.readlistContextNavigationSelector).count() > 0

  if (!readlistContextRetained) {
    failures.push('oneshot readlist-context route lost query parameters')
  }
  if (!readlistContextBannerVisible) {
    failures.push(`oneshot readlist-context route did not surface ${scenario.readListName}`)
  }
  if (!readlistContextNavigationFound) {
    failures.push('oneshot readlist-context route did not render readlist context navigation selector')
  }

  await page.goto(directTarget, { waitUntil: 'domcontentloaded' })
  await page.waitForURL(`**${route.path.replace('?', '\\?')}*`, { timeout: 30000 })
  await page.waitForSelector(route.selector, { timeout: 30000 })
  await page.waitForLoadState('networkidle')

  const returnedToDirectOneshot = page.url().includes(route.path)
  if (!returnedToDirectOneshot) {
    failures.push('oneshot scenario did not return to direct route before DOM capture')
  }

  return {
    scenario: {
      type: scenario.type,
      directPath: route.path,
      readlistContextPath: scenario.readlistContextPath,
      observedOwnershipLabel: 'non-native-observed',
      readlistContextUrl: contextUrl,
      directRouteUrl: page.url(),
    },
    signals: {
      readlistContextRetained,
      readlistContextBannerVisible,
      readlistContextNavigationFound,
      returnedToDirectOneshot,
    },
    failures,
  }
}

async function runRouteScenario(page, route) {
  if (!route.scenario) return { scenario: null, signals: {}, failures: [] }
  if (route.scenario.type === 'readlist-origin-entry') {
    return runReadlistOriginEntryScenario(page, route)
  }
  if (route.scenario.type === 'readlist-sibling-navigation') {
    return runReadlistSiblingNavigationScenario(page, route)
  }
  if (route.scenario.type === 'oneshot-readlist-fallback') {
    return runOneshotReadlistFallbackScenario(page, route)
  }
  return { scenario: null, signals: {}, failures: [] }
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
    captureMode: 'playwright',
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
      const scenarioResult = await runRouteScenario(page, route)

      const dom = await page.evaluate(routeConfig => {
        const normalizeText = value => (value || '').replace(/\s+/g, ' ').trim()
        const root = document.querySelector(routeConfig.selector)
        const panel = document.querySelector(routeConfig.panelSelector)
        const siblingNavigation = document.querySelector(routeConfig.siblingNavigationSelector)
        const extraSignals = Object.fromEntries(
          (routeConfig.extraSelectors || []).map(extra => [
            extra.signalKey,
            Boolean(document.querySelector(extra.selector)),
          ]),
        )
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
          extraSignals,
        }
      }, route)

      const routeRequests = (routeRequestLog.get(route.route) || []).filter(backendOnly)
      const routeResponses = (routeResponseLog.get(route.route) || []).filter(backendOnly)
      const routePageErrors = routeErrorLog.get(route.route) || []
      const expectedOwnedRequests = summarizeExpectedRequests(route.ownedRequests, routeRequests, routeResponses)
      const observedFallbackRequests = withOwnership(
        summarizeExpectedRequests(route.observedRequests || [], routeRequests, routeResponses),
        'non-native-observed',
      )
      const unownedObservedRequests = summarizeObservedUnownedRequests(
        [...route.ownedRequests, ...(route.observedRequests || [])],
        routeRequests,
        'non-native-observed',
      )
      const signals = {
        rootFound: dom.rootFound,
        detailMetadataVisible: dom.detailMetadataVisible,
        [route.panelKey]: dom.panelFound,
        siblingNavigationFound: dom.siblingNavigationFound,
        siblingNavigationExpected: route.siblingNavigationExpected,
        ...dom.extraSignals,
        ...scenarioResult.signals,
      }
      const failures = [...scenarioResult.failures]

      if (!signals.rootFound) failures.push(`missing root selector ${route.selector}`)
      if (!signals.detailMetadataVisible) failures.push('detail metadata fragments were not visible')
      if (dom.panelFound !== route.panelExpected) failures.push(`panel expectation failed for ${route.panelSelector}`)
      if (dom.siblingNavigationFound !== route.siblingNavigationExpected) {
        failures.push(`sibling navigation expectation failed for ${route.siblingNavigationSelector}`)
      }
      for (const extraSelector of route.extraSelectors || []) {
        if (signals[extraSelector.signalKey] !== extraSelector.expected) {
          failures.push(`extra selector expectation failed for ${extraSelector.selector}`)
        }
      }
      for (const requestCheck of expectedOwnedRequests) {
        if (!requestCheck.pass) failures.push(`missing expected owned request ${requestCheck.label}`)
      }
      for (const requestCheck of observedFallbackRequests) {
        if (!requestCheck.pass) failures.push(`missing expected fallback request ${requestCheck.label}`)
      }
      for (const [signalKey, expectedValue] of Object.entries(route.scenarioSignalExpectations || {})) {
        if (signals[signalKey] !== expectedValue) {
          failures.push(`scenario expectation failed for ${signalKey}`)
        }
      }

      routeResult = {
        route: route.route,
        path: route.path,
        selector: route.selector,
        visitedUrl: page.url(),
        captureMode: 'playwright',
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
        scenario: scenarioResult.scenario,
        metadataFragmentsFound: dom.metadataFragmentsFound,
        expectedOwnedRequests,
        ownedRequestInventory: expectedOwnedRequests,
        observedFallbackRequests,
        unownedObservedRequests,
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
        captureMode: 'playwright',
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
        scenario: null,
        metadataFragmentsFound: [],
        expectedOwnedRequests: [],
        ownedRequestInventory: [],
        observedFallbackRequests: [],
        unownedObservedRequests: [],
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
