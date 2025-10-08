<template>
  <div
    v-touch="{
               left: () => {if(swipe) {turnRight()}},
               right: () => {if(swipe) {turnLeft()}},
               up: () => {if(swipe) {verticalNext()}},
               down: () => {if(swipe) {verticalPrev()}}
             }"
  >
    <v-carousel v-model="carouselPage"
                :show-arrows="false"
                :continuous="false"
                :reverse="flipDirection"
                :vertical="vertical"
                hide-delimiters
                touchless
                height="100%"
    >
      <!--  Carousel: pages  -->
      <v-carousel-item v-for="(spread, i) in spreads"
                       :key="`spread${i}`"
                       :eager="eagerLoad(i)"
                       class="full-height"
                       :class="preRender(i) ? 'pre-render' : ''"
                       :transition="animations ? undefined : false"
                       :reverse-transition="animations ? undefined : false"
      >
        <div class="full-height d-flex flex-column justify-center">
          <div :class="`d-flex flex-row${flipDirection ? '-reverse' : ''} justify-center px-0 mx-0`">
            <img v-for="(page, j) in spread"
                  :alt="`Page ${page.number}`"
                  :key="`spread${i}-${j}`"
                  :src="getSplitImageUrlReactive(page)"
                  :class="getImageClass(page, spread)"
                  :style="`${getImageStyle(page)}; display: ${getSplitImageUrlReactive(page) ? 'block' : 'none'};`"
                  :data-page-url="page.url"
                  class="img-fit-all"
            />
          </div>
        </div>
      </v-carousel-item>
    </v-carousel>

    <!--  clickable zone: left  -->
    <div v-if="!vertical"
         @click="turnLeft()"
         class="left-quarter"
         style="z-index: 1;"
    />

    <!--  clickable zone: right  -->
    <div v-if="!vertical"
         @click="turnRight()"
         class="right-quarter"
         style="z-index: 1;"
    />

    <!--  clickable zone: top  -->
    <div v-if="vertical"
         @click="verticalPrev()"
         class="top-quarter"
         style="z-index: 1;"
    />

    <!--  clickable zone: bottom  -->
    <div v-if="vertical"
         @click="verticalNext()"
         class="bottom-quarter"
         style="z-index: 1;"
    />

    <!--  clickable zone: menu  -->
    <div @click="centerClick()"
         :class="`${vertical ? 'center-vertical' : 'center-horizontal'}`"
         style="z-index: 1;"
    />
  </div>
</template>

<script lang="ts">
import Vue from 'vue'
import {ReadingDirection} from '@/types/enum-books'
import {PagedReaderLayout, ScaleType} from '@/types/enum-reader'
import {shortcutsLTR, shortcutsRTL, shortcutsVertical} from '@/functions/shortcuts/paged-reader'
import {PageDtoWithUrl} from '@/types/komga-books'
import {buildSpreads} from '@/functions/book-spreads'
import {LRUCache} from '@/functions/lru-cache'

export default Vue.extend({
  name: 'PagedReader',
  data: function () {
    return {
      logger: 'PagedReader',
      carouselPage: 0,
      spreads: [] as PageDtoWithUrl[][],
      imageCache: new LRUCache<string, string>(30),
      imageCacheReactive: {} as Record<string, string>,
      processingImages: new Set<string>(),
    }
  },
  props: {
    pages: {
      type: Array as () => PageDtoWithUrl[],
      required: true,
    },
    page: {
      type: Number,
      required: true,
    },
    pageLayout: {
      type: String as () => PagedReaderLayout,
      required: true,
    },
    animations: {
      type: Boolean,
      required: true,
    },
    swipe: {
      type: Boolean,
      required: true,
    },
    readingDirection: {
      type: String as () => ReadingDirection,
      required: true,
    },
    scale: {
      type: String as () => ScaleType,
      required: true,
    },
    rotation: {
      type: Number,
      required: true,
    },
  },
  watch: {
    pages: {
      handler(val) {
        this.spreads = buildSpreads(val, this.pageLayout)
        this.$nextTick(() => {
          this.preGenerateImages()
        })
      },
      immediate: true,
    },
    carouselPage(val, old) {
      if (this.carouselPage >= 0 && this.carouselPage < this.spreads.length && this.spreads.length > 0) {
        const currentSpread = this.spreads[this.carouselPage]
        const currentPage = (currentSpread.length == 2 && currentSpread[1] && currentSpread[1].mediaType) ? currentSpread[1] : currentSpread[0]
        this.$emit('update:page', currentPage.number)

        this.preGenerateImages()
      } else {
        this.$emit('update:page', 1)
      }
    },
    page(val) {
      const spreadIndex = this.toSpreadIndex(val)
      if (this.carouselPage !== spreadIndex) {
        this.carouselPage = spreadIndex
      } else {
        this.$nextTick(() => {
          this.preGenerateImages()
        })
      }
    },
    pageLayout: {
      handler(val) {
        const current = this.page
        this.spreads = buildSpreads(this.pages, val)
        this.carouselPage = this.toSpreadIndex(current)
        this.$nextTick(() => {
          this.preGenerateImages()
        })
      },
      immediate: true,
    },
  },
  created() {
    window.addEventListener('keydown', this.keyPressed)
  },
  destroyed() {
    window.removeEventListener('keydown', this.keyPressed)
  },
  computed: {
    shortcuts(): any {
      const shortcuts = []
      switch (this.readingDirection) {
        case ReadingDirection.LEFT_TO_RIGHT:
          shortcuts.push(...shortcutsLTR)
          break
        case ReadingDirection.RIGHT_TO_LEFT:
          shortcuts.push(...shortcutsRTL)
          break
        case ReadingDirection.VERTICAL:
          shortcuts.push(...shortcutsVertical)
          break
      }
      return this.$_.keyBy(shortcuts, x => x.key)
    },
    flipDirection(): boolean {
      return this.readingDirection === ReadingDirection.RIGHT_TO_LEFT
    },
    vertical(): boolean {
      return this.readingDirection === ReadingDirection.VERTICAL
    },
    currentSlide(): number {
      return this.carouselPage + 1
    },
    slidesCount(): number {
      return this.spreads.length
    },
    canPrev(): boolean {
      return this.currentSlide > 1
    },
    canNext(): boolean {
      return this.currentSlide < this.slidesCount
    },
    isDoublePages(): boolean {
      return this.pageLayout === PagedReaderLayout.DOUBLE_PAGES || this.pageLayout === PagedReaderLayout.DOUBLE_NO_COVER
    },
    getSplitImageUrlReactive(): (page: PageDtoWithUrl) => string | undefined {
      return (page: PageDtoWithUrl) => {
        if (!page.url) {
          return undefined
        }

        const originalUrl = page.url.split('#')[0]
        const fragment = page.url.split('#')[1]
        const cacheKey = fragment ? `${originalUrl}-${fragment}` : originalUrl

        // For split images, only return URL if it's already cached
        // This prevents showing the original image before split is complete
        if (fragment && (fragment === 'split-left' || fragment === 'split-right')) {
          const cached = this.imageCacheReactive[cacheKey]
          return cached || undefined
        }

        // For non-split images, return original URL immediately
        const cached = this.imageCacheReactive[cacheKey]
        if (cached) return cached
        return originalUrl
      }
    },
  },
  methods: {
    keyPressed(e: KeyboardEvent) {
      this.shortcuts[e.key]?.execute(this)
    },
    imgClass(spread: PageDtoWithUrl[]): string {
      const double = spread.length > 1
      switch (this.scale) {
        case ScaleType.WIDTH:
          return double ? 'img-double-fit-width' : 'img-fit-width'
        case ScaleType.WIDTH_SHRINK_ONLY:
          return double ? 'img-double-fit-width-shrink-only' : 'img-fit-width-shrink-only'
        case ScaleType.HEIGHT:
          return 'img-fit-height'
        case ScaleType.SCREEN:
          return double ? 'img-double-fit-screen' : 'img-fit-screen'
        default:
          return 'img-fit-original'
      }
    },
    getImageClass(page: PageDtoWithUrl, spread: PageDtoWithUrl[]): string {
      return this.imgClass(spread)
    },
    eagerLoad(spreadIndex: number): boolean {
      return Math.abs(this.carouselPage - spreadIndex) <= 2
    },
    preRender(spreadIndex: number): boolean {
      return Math.abs(this.carouselPage - spreadIndex) > (this.animations ? 1 : 0)
    },
    centerClick() {
      this.$emit('menu')
    },
    turnRight() {
      if (!this.vertical)
        this.flipDirection ? this.prev() : this.next()
    },
    turnLeft() {
      if (!this.vertical)
        this.flipDirection ? this.next() : this.prev()
    },
    verticalPrev() {
      if (this.vertical) this.prev()
    },
    verticalNext() {
      if (this.vertical) this.next()
    },
    prev() {
      if (this.canPrev) {
        this.carouselPage--
        window.scrollTo(0, 0)
      } else {
        this.$emit('jump-previous')
      }
    },
    next() {
      if (this.canNext) {
        this.carouselPage++
        window.scrollTo(0, 0)
      } else {
        this.$emit('jump-next')
      }
    },
    toSpreadIndex(i: number): number {
      if (this.spreads.length > 0) {
        // Always search for the correct spread index by page number
        // This handles both single page and double page layouts correctly
        for (let j = 0; j < this.spreads.length; j++) {
          for (let k = 0; k < this.spreads[j].length; k++) {
            if (this.spreads[j][k].number === i) {
              return j
            }
          }
        }
      }

      // Fallback: assume single page layout with consecutive page numbers
      return i - 1
    },
    getImageStyle(page: PageDtoWithUrl): string {
      return this.rotation ? `transform: rotate(${this.rotation}deg);` : ''
    },

    async generateImage(page: PageDtoWithUrl) {
      if (!page.url) {
          return
      }

      if (page.url.startsWith('data:')) {
        const originalUrl = page.url.split('#')[0]
        const fragment = page.url.split('#')[1]
        const cacheKey = fragment ? `${originalUrl}-${fragment}` : originalUrl
        this.imageCache.set(cacheKey, page.url)
        this.$set(this.imageCacheReactive, cacheKey, page.url)
        return
      }

      const originalUrl = page.url.split('#')[0]
      const fragment = page.url.split('#')[1]
      const cacheKey = fragment ? `${originalUrl}-${fragment}` : originalUrl

      if (this.processingImages.has(cacheKey) || this.imageCache.has(cacheKey)) {
        return
      }
      this.processingImages.add(cacheKey)

      const img = new Image()
      img.crossOrigin = 'anonymous'

      img.onload = () => {
        try {
          const canvas = document.createElement('canvas')
          const ctx = canvas.getContext('2d')!
          if (!fragment || fragment === 'split-left') {
            const w = fragment === 'split-left' ? Math.floor(img.naturalWidth / 2) : img.naturalWidth
            canvas.width = w
            canvas.height = img.naturalHeight
            ctx.drawImage(img, fragment === 'split-left' ? 0 : 0, 0, w, img.naturalHeight, 0, 0, w, img.naturalHeight)
          } else if (fragment === 'split-right') {
            const half = Math.floor(img.naturalWidth / 2)
            canvas.width = img.naturalWidth - half
            canvas.height = img.naturalHeight
            ctx.drawImage(img, half, 0, img.naturalWidth - half, img.naturalHeight, 0, 0, img.naturalWidth - half, img.naturalHeight)
          }

          const dataUrl = canvas.toDataURL('image/jpeg', 0.9)
          this.imageCache.set(cacheKey, dataUrl)
          this.$set(this.imageCacheReactive, cacheKey, dataUrl)
          // Force Vue to re-render the image by triggering reactivity
          this.$forceUpdate()
        } catch (err) {
          this.imageCache.set(cacheKey, '')
        } finally {
          this.processingImages.delete(cacheKey)
        }
      }

      img.onerror = () => {
        this.imageCache.set(cacheKey, '')
        this.processingImages.delete(cacheKey)
      }

      img.src = originalUrl
    },

    preGenerateImages(range: number = 2) {
      const pagesToPreload = new Set<PageDtoWithUrl>()

      for (let i = this.carouselPage - range; i <= this.carouselPage + range; i++) {
        if (i >= 0 && i < this.spreads.length) {
          this.spreads[i].forEach(p => pagesToPreload.add(p))
        }
      }

      pagesToPreload.forEach(p => this.generateImage(p))
    },
  },
})
</script>

<style scoped>
.full-height {
  height: 100%;
}

.left-quarter {
  top: 0;
  left: 0;
  width: 25%;
  height: 100%;
  position: absolute;
}

.right-quarter {
  top: 0;
  right: 0;
  width: 25%;
  height: 100%;
  position: absolute;
}

.top-quarter {
  top: 0;
  height: 25%;
  width: 100%;
  position: absolute;
}

.bottom-quarter {
  bottom: 0;
  height: 25%;
  width: 100%;
  position: absolute;
}

.center-horizontal {
  top: 0;
  left: 25%;
  width: 50%;
  height: 100%;
  position: absolute;
}

.center-vertical {
  top: 25%;
  height: 50%;
  width: 100%;
  position: absolute;
}

.img-fit-all {
  object-fit: contain;
  object-position: center;
}

.img-fit-width {
  width: 100vw;
  min-height: 100vh;
  align-self: flex-start;
}

.img-double-fit-width {
  width: 50vw;
  min-height: 100vh;
  align-self: flex-start;
}

.img-fit-width-shrink-only {
  max-width: 100vw;
  align-self: flex-start;
}

.img-double-fit-width-shrink-only {
  max-width: 50vw;
  align-self: flex-start;
}

.img-fit-original {
  width: auto;
  height: auto;
}

.img-fit-height {
  min-height: 100vh;
  height: 100vh;
}

.img-fit-screen {
  width: 100vw;
  height: 100vh;
}

.img-double-fit-screen {
  max-width: 50vw;
  height: 100vh;
}

.pre-render {
  display: block !important;
  position: fixed;
  right: -1000vw;
  top: -1000vh;
}
</style>
