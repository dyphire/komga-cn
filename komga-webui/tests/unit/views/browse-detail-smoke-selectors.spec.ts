import fs from 'fs'
import path from 'path'

type BrowseDetailSmokeSelectorInventoryEntry = {
  route: 'browse-series' | 'browse-book'
  path: '/series/:seriesId' | '/book/:bookId'
  selector: string
  stableSelectors: string[]
  sourceFile: 'BrowseSeries.vue' | 'BrowseBook.vue'
}

const browseDetailSmokeSelectorInventory: BrowseDetailSmokeSelectorInventoryEntry[] = [
  {
    route: 'browse-series',
    path: '/series/:seriesId',
    selector: '[data-testid="browse-series-root"]',
    stableSelectors: [
      '[data-testid="browse-series-root"]',
      '[data-testid="browse-series-collections-panel"]',
    ],
    sourceFile: 'BrowseSeries.vue',
  },
  {
    route: 'browse-book',
    path: '/book/:bookId',
    selector: '[data-testid="browse-book-root"]',
    stableSelectors: [
      '[data-testid="browse-book-root"]',
      '[data-testid="browse-book-readlists-panel"]',
      '[data-testid="browse-book-sibling-navigation"]',
    ],
    sourceFile: 'BrowseBook.vue',
  },
]

const viewsRoot = path.resolve(__dirname, '../../../src/views')

const selectorToAttribute = (selector: string): string => {
  const match = selector.match(/^\[data-testid="([^"]+)"\]$/)

  expect(match).not.toBeNull()

  return `data-testid="${match?.[1]}"`
}

const readViewSource = (sourceFile: BrowseDetailSmokeSelectorInventoryEntry['sourceFile']): string =>
  fs.readFileSync(path.resolve(viewsRoot, sourceFile), 'utf8')

describe('browse detail smoke selectors', () => {
  test('given direct browse detail smoke inventory when enumerated then it should expose only stable selector anchors', () => {
    expect(browseDetailSmokeSelectorInventory.map(entry => ({
      route: entry.route,
      path: entry.path,
      selector: entry.selector,
      stableSelectors: entry.stableSelectors,
    }))).toStrictEqual([
      {
        route: 'browse-series',
        path: '/series/:seriesId',
        selector: '[data-testid="browse-series-root"]',
        stableSelectors: [
          '[data-testid="browse-series-root"]',
          '[data-testid="browse-series-collections-panel"]',
        ],
      },
      {
        route: 'browse-book',
        path: '/book/:bookId',
        selector: '[data-testid="browse-book-root"]',
        stableSelectors: [
          '[data-testid="browse-book-root"]',
          '[data-testid="browse-book-readlists-panel"]',
          '[data-testid="browse-book-sibling-navigation"]',
        ],
      },
    ])

    browseDetailSmokeSelectorInventory.forEach(entry => {
      expect(entry.stableSelectors[0]).toEqual(entry.selector)
      expect(new Set(entry.stableSelectors).size).toEqual(entry.stableSelectors.length)
      entry.stableSelectors.forEach(selector => {
        expect(selector).toMatch(/^\[data-testid="[a-z0-9-]+"\]$/)
      })
    })
  })

  test('given direct browse detail smoke inventory when matched against source then each selector should exist in the owning view', () => {
    browseDetailSmokeSelectorInventory.forEach(entry => {
      const source = readViewSource(entry.sourceFile)

      entry.stableSelectors.forEach(selector => {
        expect(source).toContain(selectorToAttribute(selector))
      })
    })
  })
})
