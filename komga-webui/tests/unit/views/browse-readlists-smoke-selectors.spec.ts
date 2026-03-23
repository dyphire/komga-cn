import fs from 'fs'
import path from 'path'

type BrowseReadListsSmokeSelectorInventoryEntry = {
  route: 'browse-readlists'
  path: '/libraries/:libraryId/readlists'
  selector: string
  stableSelectors: string[]
  sourceFile: 'BrowseReadLists.vue'
}

const browseReadListsSmokeSelectorInventory: BrowseReadListsSmokeSelectorInventoryEntry[] = [
  {
    route: 'browse-readlists',
    path: '/libraries/:libraryId/readlists',
    selector: '[data-testid="browse-readlists-root"]',
    stableSelectors: [
      '[data-testid="browse-readlists-root"]',
      '[data-testid="browse-readlists-item-browser"]',
      '[data-testid="browse-readlists-pagination-top"]',
      '[data-testid="browse-readlists-pagination-bottom"]',
      '[data-testid="browse-readlists-total-count"]',
    ],
    sourceFile: 'BrowseReadLists.vue',
  },
]

const viewsRoot = path.resolve(__dirname, '../../../src/views')
const repoRoot = path.resolve(__dirname, '../../../..')
const browserSmokeRoutesPath = path.resolve(repoRoot, 'tools/rust-cutover/browser_smoke_routes.py')
const bannedSelectorTerms = /(?:^|[-])(action|actions|admin|write|edit|download|delete|remove|dialog)(?:[-]|$)/

const selectorToAttribute = (selector: string): string => {
  const match = selector.match(/^\[data-testid="([^"]+)"\]$/)

  expect(match).not.toBeNull()

  return `data-testid="${match?.[1]}"`
}

const readViewSource = (sourceFile: BrowseReadListsSmokeSelectorInventoryEntry['sourceFile']): string =>
  fs.readFileSync(path.resolve(viewsRoot, sourceFile), 'utf8')

const readBrowserSmokeRoutesSource = (): string => fs.readFileSync(browserSmokeRoutesPath, 'utf8')

const browseReadListsRouteSection = (source: string): string => {
  const routeStart = source.indexOf('    \'browse-readlists\': {')

  expect(routeStart).toBeGreaterThanOrEqual(0)

  const routeEnd = source.indexOf('\n    \'browse-readlist\': {', routeStart)

  expect(routeEnd).toBeGreaterThan(routeStart)

  return source.slice(routeStart, routeEnd)
}

describe('browse readlists smoke selectors', () => {
  test('given browse readlists smoke inventory when enumerated then it should expose only stable selector anchors', () => {
    expect(browseReadListsSmokeSelectorInventory.map(entry => ({
      route: entry.route,
      path: entry.path,
      selector: entry.selector,
      stableSelectors: entry.stableSelectors,
    }))).toStrictEqual([
      {
        route: 'browse-readlists',
        path: '/libraries/:libraryId/readlists',
        selector: '[data-testid="browse-readlists-root"]',
        stableSelectors: [
          '[data-testid="browse-readlists-root"]',
          '[data-testid="browse-readlists-item-browser"]',
          '[data-testid="browse-readlists-pagination-top"]',
          '[data-testid="browse-readlists-pagination-bottom"]',
          '[data-testid="browse-readlists-total-count"]',
        ],
      },
    ])

    browseReadListsSmokeSelectorInventory.forEach(entry => {
      expect(entry.stableSelectors[0]).toEqual(entry.selector)
      expect(new Set(entry.stableSelectors).size).toEqual(entry.stableSelectors.length)
      entry.stableSelectors.forEach(selector => {
        expect(selector).toMatch(/^\[data-testid="[a-z0-9-]+"\]$/)
        expect(selector).not.toMatch(bannedSelectorTerms)
      })
    })
  })

  test('given browse readlists smoke inventory when matched against source then each selector should exist in the owning view', () => {
    browseReadListsSmokeSelectorInventory.forEach(entry => {
      const source = readViewSource(entry.sourceFile)

      entry.stableSelectors.forEach(selector => {
        expect(source).toContain(selectorToAttribute(selector))
      })
    })
  })

  test('given browse readlists smoke route inventory when inspected then it should use browse-readlists specific selectors', () => {
    const routeSection = browseReadListsRouteSection(readBrowserSmokeRoutesSource())

    expect(routeSection).toContain('\'selector\': \'[data-testid="browse-readlists-root"]\'')
    expect(routeSection).toContain('\'panelSelector\': \'[data-testid="browse-readlists-item-browser"]\'')
    expect(routeSection).toContain('\'signalKey\': \'topPaginationFound\'')
    expect(routeSection).toContain('\'selector\': \'[data-testid="browse-readlists-pagination-top"]\'')
    expect(routeSection).toContain('\'signalKey\': \'bottomPaginationFound\'')
    expect(routeSection).toContain('\'selector\': \'[data-testid="browse-readlists-pagination-bottom"]\'')
    expect(routeSection).toContain('\'signalKey\': \'totalCountFound\'')
    expect(routeSection).toContain('\'selector\': \'[data-testid="browse-readlists-total-count"]\'')
    expect(routeSection).not.toContain('\'selector\': \'[data-testid="item-browser-root"]\'')
    expect(routeSection).not.toContain('\'panelSelector\': \'[data-testid="item-browser-root"]\'')
  })
})
