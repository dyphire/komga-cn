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

const selectorToAttribute = (selector: string): string => {
  const match = selector.match(/^\[data-testid="([^"]+)"\]$/)

  expect(match).not.toBeNull()

  return `data-testid="${match?.[1]}"`
}

const readViewSource = (sourceFile: BrowseOneshotSmokeSelectorInventoryEntry['sourceFile']): string =>
  fs.readFileSync(path.resolve(viewsRoot, sourceFile), 'utf8')

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
})
