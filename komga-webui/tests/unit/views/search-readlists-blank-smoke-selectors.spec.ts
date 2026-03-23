import fs from 'fs'
import path from 'path'

type SearchReadlistsBlankSmokeSelectorInventoryEntry = {
  route: 'search-readlists-blank-native'
  path: '/search'
  selector: string
  stableSelectors: string[]
  sourceFile: 'SearchView.vue'
}

const searchReadlistsBlankSmokeSelectorInventory: SearchReadlistsBlankSmokeSelectorInventoryEntry[] = [
  {
    route: 'search-readlists-blank-native',
    path: '/search',
    selector: '[data-testid="search-results-root"]',
    stableSelectors: [
      '[data-testid="search-results-root"]',
      '[data-testid="search-results-query"]',
      '[data-testid="search-results-readlists"]',
      '[data-testid="search-results-empty-summary"]',
    ],
    sourceFile: 'SearchView.vue',
  },
]

const viewsRoot = path.resolve(__dirname, '../../../src/views')
const repoRoot = path.resolve(__dirname, '../../../..')
const browserSmokeRoutesPath = path.resolve(repoRoot, 'tools/rust-cutover/browser_smoke_routes.py')
const bannedSelectorTerms = /(?:^|[-])(action|actions|admin|write|edit|download|delete|remove|dialog|sort|unpaged)(?:[-]|$)/
const expectedGovernanceLabels = ['readlists-search-blank-whitespace']

const selectorToAttribute = (selector: string): string => {
  const match = selector.match(/^\[data-testid="([^"]+)"\]$/)

  expect(match).not.toBeNull()

  return `data-testid="${match?.[1]}"`
}

const readViewSource = (sourceFile: SearchReadlistsBlankSmokeSelectorInventoryEntry['sourceFile']): string =>
  fs.readFileSync(path.resolve(viewsRoot, sourceFile), 'utf8')

const readBrowserSmokeRoutesSource = (): string => fs.readFileSync(browserSmokeRoutesPath, 'utf8')

const routeSection = (source: string, routeName: string): string => {
  const routeStart = source.indexOf(`    '${routeName}': {`)

  expect(routeStart).toBeGreaterThanOrEqual(0)

  const nextRouteStart = source.indexOf('\n    \'', routeStart + 1)
  const routeEnd = nextRouteStart >= 0 ? nextRouteStart : source.lastIndexOf('\n}')

  expect(routeEnd).toBeGreaterThan(routeStart)

  return source.slice(routeStart, routeEnd)
}

const routeLabelsFor = (section: string, blockName: 'governanceEvidenceRequests'): string[] => {
  const blockStart = section.indexOf(`'${blockName}': [`)

  expect(blockStart).toBeGreaterThanOrEqual(0)

  const blockEnd = section.indexOf('\n        ],', blockStart)

  expect(blockEnd).toBeGreaterThan(blockStart)

  return Array.from(section.slice(blockStart, blockEnd).matchAll(/'label': '([^']+)'/g), match => match[1])
}

describe('search readlists blank smoke selectors', () => {
  test('given blank search readlists smoke inventory when enumerated then it should expose only stable selector anchors', () => {
    expect(searchReadlistsBlankSmokeSelectorInventory.map(entry => ({
      route: entry.route,
      path: entry.path,
      selector: entry.selector,
      stableSelectors: entry.stableSelectors,
    }))).toStrictEqual([
      {
        route: 'search-readlists-blank-native',
        path: '/search',
        selector: '[data-testid="search-results-root"]',
        stableSelectors: [
          '[data-testid="search-results-root"]',
          '[data-testid="search-results-query"]',
          '[data-testid="search-results-readlists"]',
          '[data-testid="search-results-empty-summary"]',
        ],
      },
    ])

    searchReadlistsBlankSmokeSelectorInventory.forEach(entry => {
      expect(entry.stableSelectors[0]).toEqual(entry.selector)
      expect(new Set(entry.stableSelectors).size).toEqual(entry.stableSelectors.length)
      entry.stableSelectors.forEach(selector => {
        expect(selector).toMatch(/^\[data-testid="[a-z0-9-]+"\]$/)
        expect(selector).not.toMatch(bannedSelectorTerms)
      })
    })
  })

  test('given blank search readlists smoke inventory when matched against source then each selector should exist in the owning view', () => {
    searchReadlistsBlankSmokeSelectorInventory.forEach(entry => {
      const source = readViewSource(entry.sourceFile)

      entry.stableSelectors.forEach(selector => {
        expect(source).toContain(selectorToAttribute(selector))
      })

      expect(source).toContain('this.$komgaReadLists.getReadLists(undefined, pageable, search)')
      expect(source).not.toContain('this.$komgaReadLists.getReadLists(undefined, pageable, search,')
    })
  })

  test('given blank search readlists smoke route when inspected then it should exercise only the whitespace search shape', () => {
    const section = routeSection(readBrowserSmokeRoutesSource(), 'search-readlists-blank-native')

    expect(section).toContain('\'path\': \'/search?q=%20%20\'')
    expect(section).toContain('\'selector\': \'[data-testid="search-results-root"]\'')
    expect(section).toContain('\'panelSelector\': \'[data-testid="search-results-readlists"]\'')
    expect(section).toContain('\'signalKey\': \'searchQueryFound\'')
    expect(section).toContain('\'selector\': \'[data-testid="search-results-query"]\'')
    expect(section).toContain('\'signalKey\': \'emptySummaryFound\'')
    expect(section).toContain('data-testid=\\"search-results-empty-summary\\"')
    expect(routeLabelsFor(section, 'governanceEvidenceRequests')).toStrictEqual(expectedGovernanceLabels)
    expect(section).toContain('\'requestPath\': \'/api/v1/readlists?search=%20%20\'')
    expect(section).toContain('\'urlEndsWith\': \'/api/v1/readlists?search=%20%20\'')
    expect(section).not.toContain('search=alpha')
    expect(section).not.toContain('sort=')
    expect(section).not.toContain('unpaged=true')
  })
})
