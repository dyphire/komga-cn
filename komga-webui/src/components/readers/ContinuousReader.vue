<template>
  <div>
    <div :class="`d-flex flex-column px-0 mx-0` "
         v-scroll="onScroll"
    >
      <img v-for="(page, i) in pages"
            :key="`page${i}`"
            :alt="`Page ${page.number}`"
            :src="shouldLoad(i) ? getSplitImageUrlReactive(page) : undefined"
            :height="calcHeight(page)"
            :width="calcWidth(page)"
            :id="`page${page.number}`"
            :style="`margin: ${i === 0 ? 0 : pageMargin}px auto; transform: rotate(${rotation}deg);`"
            v-intersect="onIntersect"
      />
    </div>

    <!--  clickable zone: top  -->
    <div @click="prev()"
         class="top-quarter"
         style="z-index: 1;"
    />

    <!--  clickable zone: bottom  -->
    <div @click="next()"
         class="bottom-quarter"
         style="z-index: 1;"
    />

    <!--  clickable zone: menu  -->
    <div @click="centerClick()"
         class="center-vertical"
         style="z-index: 1;"
    />
  </div>
</template>

<script lang="ts">
import Vue from 'vue'
import {ContinuousScaleType} from '@/types/enum-reader'
import {PageDtoWithUrl} from '@/types/komga-books'
import {throttle} from 'lodash'
import {LRUCache} from '@/functions/lru-cache'

export default Vue.extend({
  name: 'ContinuousReader',
  data: () => {
    return {
      offsetTop: 0,
      totalHeight: 1000,
      currentPage: 1,
      seen: [] as boolean[],
      imageCache: new LRUCache<string, string>(50),
      imageCacheReactive: {} as Record<string, string>,
      processingImages: new Set<string>(),
    }
  },
  props: {
    pages: {
      type: Array as () => PageDtoWithUrl[],
      required: true,
    },
    animations: {
      type: Boolean,
      required: true,
    },
    page: {
      type: Number,
      required: true,
    },
    scale: {
      type: String as () => ContinuousScaleType,
      required: true,
    },
    sidePadding: {
      type: Number,
      required: true,
    },
    pageMargin: {
      type: Number,
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
        this.seen = new Array(val.length).fill(false)
        if (this.page === 1) window.scrollTo(0, 0)
        this.$nextTick(() => {
          this.preGenerateImages()
        })
      },
      immediate: true,
    },
    page: {
      handler(val) {
        if (val != this.currentPage) {
          this.$vuetify.goTo(`#page${val}`, {
            duration: 0,
          })
        }
        this.preGenerateImages()
      },
      immediate: false,
    },
  },
  created() {
    window.addEventListener('keydown', this.keyPressed)
  },
  destroyed() {
    window.removeEventListener('keydown', this.keyPressed)
  },
  mounted() {
    if (this.page != this.currentPage) {
      this.$vuetify.goTo(`#page${this.page}`, {
        duration: 0,
      })
    }
  },
  computed: {
    canPrev(): boolean {
      return this.offsetTop > 0
    },
    canNext(): boolean {
      return this.offsetTop + this.$vuetify.breakpoint.height < this.totalHeight
    },
    goToOptions(): object | undefined {
      if (this.animations) return undefined
      return {duration: 0}
    },
    totalSidePadding(): number {
      return this.sidePadding * 2
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
    keyPressed: throttle(function (this: any, e: KeyboardEvent) {
      switch (e.key) {
        case ' ':
        case 'PageDown':
        case 'ArrowDown':
          if (!this.canNext) this.$emit('jump-next')
          break
        case 'PageUp':
        case 'ArrowUp':
          if (!this.canPrev) this.$emit('jump-previous')
          break
      }
    }, 500),
    onScroll(e: any) {
      this.offsetTop = e.target.scrollingElement.scrollTop
      this.totalHeight = e.target.scrollingElement.scrollHeight
    },
    onIntersect(entries: any) {
      if (entries[0].isIntersecting) {
        const page = parseInt(entries[0].target.id.replace('page', ''))
        this.seen.splice(page - 1, 1, true)
        this.currentPage = page
        this.$emit('update:page', page)
      }
    },
    shouldLoad(page: number): boolean {
      // Increased loading range for better split image support
      const loadRange = 3
      return page == 0 || this.seen[page] || Math.abs((this.currentPage - 1) - page) <= loadRange
    },
    calcHeight(page: PageDtoWithUrl): number | undefined {
      switch (this.scale) {
        case ContinuousScaleType.WIDTH:
          if (page.height && page.width)
            return page.height / (page.width / (this.$vuetify.breakpoint.width - (this.$vuetify.breakpoint.width * this.totalSidePadding) / 100))
          return undefined
        case ContinuousScaleType.ORIGINAL:
          return page.height || undefined
        default:
          return undefined
      }
    },
    calcWidth(page: PageDtoWithUrl): number | undefined {
      switch (this.scale) {
        case ContinuousScaleType.WIDTH:
          return this.$vuetify.breakpoint.width - (this.$vuetify.breakpoint.width * this.totalSidePadding) / 100
        case ContinuousScaleType.ORIGINAL:
          return page.width || undefined
        default:
          return undefined
      }
    },
    centerClick() {
      this.$emit('menu')
    },
    prev() {
      if (this.canPrev) {
        const step = this.$vuetify.breakpoint.height * 0.95
        this.$vuetify.goTo(this.offsetTop - step, this.goToOptions)
      } else {
        this.$emit('jump-previous')
      }
    },
    next() {
      if (this.canNext) {
        const step = this.$vuetify.breakpoint.height * 0.95
        this.$vuetify.goTo(this.offsetTop + step, this.goToOptions)
      } else {
        this.$emit('jump-next')
      }
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

      if (this.processingImages.has(cacheKey) || this.imageCache.has(cacheKey)) return
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

    preGenerateImages() {
      const pagesToPreload = new Set<PageDtoWithUrl>()

      // Preload current page and adjacent pages (increased range for split images)
      const preloadRange = 3
      for (let i = -preloadRange; i <= preloadRange; i++) {
        const pageIndex = this.currentPage - 1 + i
        if (pageIndex >= 0 && pageIndex < this.pages.length) {
          pagesToPreload.add(this.pages[pageIndex])
        }
      }

      pagesToPreload.forEach(p => {
        this.generateImage(p)
      })
    },

    getVisiblePages(): PageDtoWithUrl[] {
      const visiblePages: PageDtoWithUrl[] = []
      if (this.currentPage >= 1 && this.currentPage <= this.pages.length) {
        visiblePages.push(this.pages[this.currentPage - 1])
      }

      const currentIndex = this.currentPage - 1
      if (currentIndex > 0 && this.pages[currentIndex - 1]) {
        visiblePages.unshift(this.pages[currentIndex - 1])
      }
      if (currentIndex < this.pages.length - 1 && this.pages[currentIndex + 1]) {
        visiblePages.push(this.pages[currentIndex + 1])
      }

      return visiblePages
    },

    // Public method to expose visible pages for immersive background
    getVisiblePagesForImmersive(): PageDtoWithUrl[] {
      return this.getVisiblePages()
    },
  },
})
</script>
<style scoped>
.top-quarter {
  top: 0;
  height: 25vh;
  width: 100%;
  position: fixed;
}

.bottom-quarter {
  top: 75vh;
  height: 25vh;
  width: 100%;
  position: fixed;
}

.center-vertical {
  top: 25vh;
  height: 50vh;
  width: 100%;
  position: fixed;
}
</style>
