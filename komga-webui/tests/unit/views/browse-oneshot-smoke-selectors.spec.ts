import fs from 'fs'
import path from 'path'

type BrowseOneshotSmokeSelectorInventoryEntry = {
  route: 'browse-oneshot'
  path: '/oneshot/:seriesId'
  selector: string
  stableSelectors: string[]
  sourceFile: 'BrowseOneshot.vue'
}

const browseOneshotSmokeSelectorInventory: BrowseOneshotSmokeSelectorInventoryEntry[] = [
  {
    route: 'browse-oneshot',
    path: '/oneshot/:seriesId',
    selector: '[data-testid="browse-oneshot-root"]',
    stableSelectors: [
      '[data-testid="browse-oneshot-root"]',
      '[data-testid="browse-oneshot-collections-panel"]',
      '[data-testid="browse-oneshot-readlists-panel"]',
      '[data-testid="browse-oneshot-readlist-context-navigation"]',
    ],
    sourceFile: 'BrowseOneshot.vue',
  },
]

const viewsRoot = path.resolve(__dirname, '../../../src/views')
const repoRoot = path.resolve(__dirname, '../../../..')
const browserSmokeRoutesPath = path.resolve(repoRoot, 'tools/rust-cutover/browser_smoke_routes.py')
const expectedOwnedReadlistContextLabels = [
  'oneshot-series-detail',
  'oneshot-series-collections',
  'oneshot-bootstrap-books-list',
  'oneshot-book-readlists',
  'readlist-detail',
  'readlist-books-unpaged',
  'readlist-book-next',
  'readlist-book-previous',
]
const excludedOwnedReadlistContextLabels = [
  'readlist-detail-fallback',
  'readlist-books-unpaged-fallback',
  'readlist-book-next-fallback',
  'readlist-book-previous-fallback',
  'oneshot-book-media',
  'oneshot-book-reader',
  'oneshot-book-progress',
  'oneshot-book-write',
  'oneshot-sse-events',
]

const selectorToAttribute = (selector: string): string => {
  const match = selector.match(/^\[data-testid="([^"]+)"\]$/)

  expect(match).not.toBeNull()

  return `data-testid="${match?.[1]}"`
}

const readViewSource = (sourceFile: BrowseOneshotSmokeSelectorInventoryEntry['sourceFile']): string =>
  fs.readFileSync(path.resolve(viewsRoot, sourceFile), 'utf8')

const readBrowserSmokeRoutesSource = (): string => fs.readFileSync(browserSmokeRoutesPath, 'utf8')

const browseOneshotRouteSection = (source: string): string => {
  const routeStart = source.indexOf('    \'browse-oneshot\': {')

  expect(routeStart).toBeGreaterThanOrEqual(0)

  const routeEnd = source.indexOf('\n    },\n}', routeStart)

  expect(routeEnd).toBeGreaterThan(routeStart)

  return source.slice(routeStart, routeEnd)
}

const routeLabelsFor = (section: string, blockName: 'ownedRequests' | 'observedRequests'): string[] => {
  const blockStart = section.indexOf(`'${blockName}': [`)

  expect(blockStart).toBeGreaterThanOrEqual(0)

  if (section.includes(`'${blockName}': [],`)) {
    return []
  }

  const blockEnd = section.indexOf('\n        ],', blockStart)

  expect(blockEnd).toBeGreaterThan(blockStart)

  return Array.from(section.slice(blockStart, blockEnd).matchAll(/'label': '([^']+)'/g), match => match[1])
}

describe('browse oneshot smoke selectors', () => {
  test('given oneshot smoke inventory when enumerated then it should expose only stable selector anchors', () => {
    expect(browseOneshotSmokeSelectorInventory.map(entry => ({
      route: entry.route,
      path: entry.path,
      selector: entry.selector,
      stableSelectors: entry.stableSelectors,
    }))).toStrictEqual([
      {
        route: 'browse-oneshot',
        path: '/oneshot/:seriesId',
        selector: '[data-testid="browse-oneshot-root"]',
        stableSelectors: [
          '[data-testid="browse-oneshot-root"]',
          '[data-testid="browse-oneshot-collections-panel"]',
          '[data-testid="browse-oneshot-readlists-panel"]',
          '[data-testid="browse-oneshot-readlist-context-navigation"]',
        ],
      },
    ])

    browseOneshotSmokeSelectorInventory.forEach(entry => {
      expect(entry.stableSelectors[0]).toEqual(entry.selector)
      expect(new Set(entry.stableSelectors).size).toEqual(entry.stableSelectors.length)
      entry.stableSelectors.forEach(selector => {
        expect(selector).toMatch(/^\[data-testid="[a-z0-9-]+"\]$/)
        expect(selector).not.toMatch(/(?:^|[-])(action|actions|admin|write|edit|download|delete|remove)(?:[-]|$)/)
      })
    })
  })

  test('given oneshot smoke inventory when matched against source then each selector should exist in the owning view', () => {
    browseOneshotSmokeSelectorInventory.forEach(entry => {
      const source = readViewSource(entry.sourceFile)

      entry.stableSelectors.forEach(selector => {
        expect(source).toContain(selectorToAttribute(selector))
      })
    })
  })

  test('given oneshot readlist context smoke contract when enumerated then it should keep exact owned inventory and exclude fallback-only branches', () => {
    const routeSection = browseOneshotRouteSection(readBrowserSmokeRoutesSource())

    expect(routeLabelsFor(routeSection, 'ownedRequests')).toStrictEqual(expectedOwnedReadlistContextLabels)
    expect(routeLabelsFor(routeSection, 'observedRequests')).toStrictEqual([])

    excludedOwnedReadlistContextLabels.forEach(label => {
      expect(routeSection).not.toContain(`'label': '${label}'`)
    })
  })

  test('given oneshot readlist context source flow when inspected then it should keep native readlist detail and sibling requests wired from route context', () => {
    const source = readViewSource('BrowseOneshot.vue')

    expect(source).toContain('this.$route.query.contextId')
    expect(source).toContain('ContextOrigin.READLIST')
    expect(source).toContain('this.$komgaReadLists.getOneReadList(this.context.id)')
    expect(source).toContain('this.$komgaReadLists.getBooks(this.context.id, {unpaged: true} as PageRequest)')
    expect(source).toContain('this.$komgaReadLists.getBookSiblingNext(this.context.id, this.book.id)')
    expect(source).toContain('this.$komgaReadLists.getBookSiblingPrevious(this.context.id, this.book.id)')
  })
})
