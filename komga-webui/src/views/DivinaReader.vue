<template>
  <v-container class="ma-0 pa-0 full-height" fluid v-if="pages.length > 0"
                :style="`width: 100%; background: ${actualBackgroundColor}`"
  >
    <div>
      <v-slide-y-transition>
        <!-- Top Toolbar-->
        <v-toolbar
          dense elevation="1"
          v-if="showToolbars"
          class="settings full-width"
          style="position: fixed; top: 0"
        >
          <v-btn
            icon
            @click="closeBook"
          >
            <v-icon>mdi-arrow-left</v-icon>
          </v-btn>
          <v-toolbar-title> {{ bookTitle }}</v-toolbar-title>
          <v-spacer></v-spacer>

          <v-tooltip bottom v-if="incognito">
            <template v-slot:activator="{ on }">
              <v-icon v-on="on">mdi-incognito</v-icon>
            </template>
            <span>{{ $t('bookreader.tooltip_incognito') }}</span>
          </v-tooltip>

          <v-btn
            icon
            :disabled="!screenfull.isEnabled"
            @click="screenfull.isFullscreen ? screenfull.exit() : enterFullscreen()">
            <v-icon>{{ fullscreenIcon }}</v-icon>
          </v-btn>

          <v-btn
            icon
            @click="showHelp = !showHelp">
            <v-icon>mdi-help-circle</v-icon>
          </v-btn>

          <v-btn
            icon
            @click="showExplorer = !showExplorer"
          >
            <v-icon>mdi-view-grid</v-icon>
          </v-btn>
          <v-btn
            v-if="hasToc"
            icon
            @click="showToc = !showToc"
          >
            <v-icon>mdi-table-of-contents</v-icon>
          </v-btn>

          <v-btn
            icon
            @click="showSettings = !showSettings"
          >
            <v-icon>mdi-cog</v-icon>
          </v-btn>

          <v-menu offset-y>
            <template v-slot:activator="{ on }">
              <v-btn icon v-on="on" @click.prevent="">
                <v-icon>mdi-dots-vertical</v-icon>
              </v-btn>
            </template>
            <v-list>
              <v-list-item @click="downloadCurrentPage">
                <v-list-item-title>{{ $t('bookreader.download_current_page') }}</v-list-item-title>
              </v-list-item>
              <v-list-item @click="setCurrentPageAsPoster(ItemTypes.BOOK)">
                <v-list-item-title>{{ $t('bookreader.set_current_page_as_book_poster') }}</v-list-item-title>
              </v-list-item>
              <v-list-item v-if="!book.oneshot" @click="setCurrentPageAsPoster(ItemTypes.SERIES)">
                <v-list-item-title>{{ $t('bookreader.set_current_page_as_series_poster') }}</v-list-item-title>
              </v-list-item>
              <v-list-item v-if="contextReadList" @click="setCurrentPageAsPoster(ItemTypes.READLIST)">
                <v-list-item-title>{{ $t('bookreader.set_current_page_as_readlist_poster') }}</v-list-item-title>
              </v-list-item>
            </v-list>
          </v-menu>
        </v-toolbar>
      </v-slide-y-transition>

      <v-slide-y-reverse-transition>
        <!-- Bottom Toolbar-->
        <v-toolbar
          dense
          elevation="1"
          class="settings full-width"
          style="position: fixed; bottom: 0"
          horizontal
          v-if="showToolbars"
        >
          <v-row justify="center">
            <!--  Menu: page slider  -->
            <v-col class="px-0">
              <v-slider
                hide-details
                thumb-label
                @change="goTo"
                v-model="goToPage"
                class="align-center"
                min="1"
                :max="pagesCount"
              >
                <template v-slot:prepend>
                  <v-icon @click="previousBook" class="">mdi-undo</v-icon>
                  <v-icon @click="goToFirst" class="mx-2">mdi-skip-previous</v-icon>
                  <v-label>
                    {{ page }}
                  </v-label>
                </template>
                <template v-slot:append>
                  <v-label>
                    {{ pagesCount }}
                  </v-label>
                  <v-icon @click="goToLast" class="mx-1">mdi-skip-next</v-icon>
                  <v-icon @click="nextBook" class="">mdi-redo</v-icon>
                </template>
              </v-slider>
            </v-col>
          </v-row>

        </v-toolbar>
      </v-slide-y-reverse-transition>
    </div>

    <div class="full-height">
      <continuous-reader
        ref="continuousReader"
        v-if="continuousReader"
        :pages="pages"
        :page.sync="page"
        :animations="animations"
        :scale="continuousScale"
        :sidePadding="sidePadding"
        :page-margin="pageMargin"
        :rotation="rotation"
        @menu="toggleToolbars()"
        @jump-previous="jumpToPrevious()"
        @jump-next="jumpToNext()"
      ></continuous-reader>

      <paged-reader
        ref="pagedReader"
        v-else
        :pages="pages"
        :page.sync="page"
        :reading-direction="readingDirection"
        :page-layout="pageLayout"
        :scale="scale"
        :animations="animations"
        :swipe="swipe"
        :rotation="rotation"
        @menu="toggleToolbars()"
        @jump-previous="jumpToPrevious()"
        @jump-next="jumpToNext()"
      ></paged-reader>
    </div>

    <!-- EPUB TOC drawer (for EPUBs rendered in Divina) -->
    <v-navigation-drawer
	  v-model="showToc"
		right
		app
		temporary
		:width="$vuetify.breakpoint.smAndUp ? 420 : $vuetify.breakpoint.width - 56"
        v-if="hasToc"
    >
      <v-toolbar dense flat>
        <v-toolbar-title>Table of contents</v-toolbar-title>
        <v-spacer/>
        <v-btn icon @click="showToc = false"><v-icon>mdi-close</v-icon></v-btn>
      </v-toolbar>
      <toc-list :toc="toc" @goto="goToEntry" class="pa-2"></toc-list>

    </v-navigation-drawer>
<thumbnail-explorer-dialog
      v-model="showExplorer"
      :bookId="bookId"
      @go="goToOriginal"
      :pagesCount="originalPagesCount"
    ></thumbnail-explorer-dialog>

    <v-bottom-sheet
      v-model="showSettings"
      :close-on-content-click="false"
      max-width="500"
      @keydown.esc.stop=""
      scrollable
    >
      <v-card>
        <v-toolbar dark color="primary">
          <v-btn icon dark @click="showSettings = false">
            <v-icon>mdi-close</v-icon>
          </v-btn>
          <v-toolbar-title>{{ $t('bookreader.reader_settings') }}</v-toolbar-title>
        </v-toolbar>

        <v-card-text class="pa-0">
          <v-list class="full-height full-width">
            <v-subheader class="font-weight-black text-h6">{{ $t('bookreader.settings.general') }}</v-subheader>
            <v-list-item>
              <settings-select
                :items="readingDirs"
                v-model="readingDirection"
                :label="$t('bookreader.settings.reading_mode')"
              />
            </v-list-item>

            <v-list-item>
              <settings-switch v-model="animations"
                               :label="$t('bookreader.settings.animate_page_transitions')"/>
            </v-list-item>

            <v-list-item>
              <settings-switch v-model="swipe" :label="$t('bookreader.settings.gestures')"/>
            </v-list-item>

            <v-list-item>
              <settings-switch v-model="alwaysFullscreen" :label="$t('bookreader.settings.always_fullscreen')"
                               :disabled="!screenfull.isEnabled"/>
            </v-list-item>

            <v-list-item>
              <settings-switch v-model="splitWidePages" :label="$t('bookreader.settings.split_wide_pages')"/>
            </v-list-item>

            <v-list-item v-if="splitWidePages">
              <settings-switch v-model="swapSplitPages" :label="$t('bookreader.settings.swap_split_pages')"/>
            </v-list-item>

            <v-subheader class="font-weight-black text-h6">{{ $t('bookreader.settings.display') }}</v-subheader>
            <v-list-item>
              <settings-select
                :items="backgroundColors"
                v-model="backgroundColor"
                :label="$t('bookreader.settings.background_color')"
              />
            </v-list-item>

            <v-list-item>
              <settings-select
                :items="rotationOptions"
                v-model="rotation"
                :label="$t('bookreader.settings.rotation')"
              />
            </v-list-item>

            <template v-if="continuousReader">
              <v-subheader class="font-weight-black text-h6">{{ $t('bookreader.settings.webtoon') }}</v-subheader>
              <v-list-item>
                <settings-select
                  :items="continuousScaleTypes"
                  v-model="continuousScale"
                  :label="$t('bookreader.settings.scale_type')"
                />
              </v-list-item>
              <v-list-item>
                <settings-select
                  :items="paddingPercentages"
                  v-model="sidePadding"
                  :label="$t('bookreader.settings.side_padding')"
                />
              </v-list-item>
              <v-list-item>
                <settings-select
                  :items="marginValues"
                  v-model="pageMargin"
                  :label="$t('bookreader.settings.page_margin')"
                />
              </v-list-item>
            </template>

            <template v-if="!continuousReader">
              <v-subheader class="font-weight-black text-h6">{{ $t('bookreader.settings.paged') }}</v-subheader>
              <v-list-item>
                <settings-select
                  :items="scaleTypes"
                  v-model="scale"
                  :label="$t('bookreader.settings.scale_type')"
                />
              </v-list-item>

              <v-list-item>
                <settings-select
                  :items="pageLayouts"
                  v-model="pageLayout"
                  :label="$t('bookreader.settings.page_layout')"
                />
              </v-list-item>
            </template>


          </v-list>
        </v-card-text>
      </v-card>
    </v-bottom-sheet>
    <v-snackbar
      v-model="jumpToPreviousBook"
      :timeout="jumpConfirmationDelay"
      top
      color="rgba(0, 0, 0, 0.8)"
      multi-line
      class="mt-12"
    >
      <div class="body-1 pa-6">
        <p>{{ $t('bookreader.beginning_of_book') }}</p>
        <p v-if="!$_.isEmpty(siblingPrevious)">{{ $t('bookreader.move_previous') }}</p>
      </div>
    </v-snackbar>

    <v-snackbar
      v-model="jumpToNextBook"
      :timeout="jumpConfirmationDelay"
      top
      color="rgba(0, 0, 0, 0.8)"
      multi-line
      class="mt-12"
    >
      <div class="text-body-1 pa-6">
        <p>{{ $t('bookreader.end_of_book') }}</p>
        <p v-if="!$_.isEmpty(siblingNext)">{{ $t('bookreader.move_next') }}</p>
        <p v-else>{{ $t('bookreader.move_next_exit') }}</p>
      </div>
    </v-snackbar>

    <v-snackbar
      v-model="notificationReadingDirection.enabled"
      color="rgba(0, 0, 0, 0.8)"
      bottom
      timeout="3000"
    >
      <p class="text-body-1 text-center ma-0">
        {{
          readingDirectionText
        }}{{ notificationReadingDirection.fromMetadata ? '(' + $t('bookreader.from_series_metadata') + ')' : '' }}
      </p>
    </v-snackbar>

    <v-snackbar
      v-model="notification.enabled"
      color="rgba(0, 0, 0, 0.8)"
      centered
      :timeout="notification.timeout"
    >
      <p class="text-h6 text-center ma-0">
        {{ notification.message }}
      </p>
    </v-snackbar>

    <shortcut-help-dialog
      v-model="showHelp"
      :shortcuts="shortcutsHelp"
    />
  </v-container>
</template>

<script lang="ts">
import {debounce} from 'lodash'
import SettingsSelect from '@/components/SettingsSelect.vue'
import SettingsSwitch from '@/components/SettingsSwitch.vue'
import ThumbnailExplorerDialog from '@/components/dialogs/ThumbnailExplorerDialog.vue'
import ShortcutHelpDialog from '@/components/dialogs/ShortcutHelpDialog.vue'
import {getBookTitleCompact} from '@/functions/book-title'
import {checkImageSupport, ImageFeature} from '@/functions/check-image'
import {bookPageUrl} from '@/functions/urls'

import TocList from '@/components/TocList.vue'
import {bookManifestUrl} from '@/functions/urls'
import {TocEntry} from '@/types/epub'
import {getFileFromUrl} from '@/functions/file'
import {resizeImageFile} from '@/functions/resize-image'
import {ReadingDirection} from '@/types/enum-books'
import Vue from 'vue'
import {Location} from 'vue-router'
import PagedReader from '@/components/readers/PagedReader.vue'
import ContinuousReader from '@/components/readers/ContinuousReader.vue'
import {ContinuousScaleType, MarginValues, PaddingPercentage, PagedReaderLayout, ScaleType} from '@/types/enum-reader'
import {
  shortcutsLTR,
  shortcutsRTL,
  shortcutsSettingsPaged,
  shortcutsVertical,
} from '@/functions/shortcuts/paged-reader'
import {shortcutsMenus, shortcutsSettings} from '@/functions/shortcuts/bookreader'
import {shortcutsAll} from '@/functions/shortcuts/reader'
import {shortcutsSettingsContinuous} from '@/functions/shortcuts/continuous-reader'
import {BookDto, PageDto, PageDtoWithUrl} from '@/types/komga-books'
import {Context, ContextOrigin} from '@/types/context'
import {SeriesDto} from '@/types/komga-series'
import jsFileDownloader from 'js-file-downloader'
import screenfull from 'screenfull'
import {ItemTypes} from '@/types/items'
import {getBookReadRouteFromMedia} from '@/functions/book-format'

export default Vue.extend({
  name: 'DivinaReader',
  components: {
    ContinuousReader,
    PagedReader,
    SettingsSwitch,
    SettingsSelect,
    ThumbnailExplorerDialog,
    ShortcutHelpDialog,
    TocList,
  },
  data: function () {
    return {
      // TOC (for EPUBs opened with Divina)
      showToc: false,
      toc: [] as TocEntry[],
      spinePaths: [] as string[],

      actualBackgroundColor: 'black',
      systemThemeMediaQuery: null as MediaQueryList | null,

      ItemTypes,
      screenfull,
      fullscreenIcon: 'mdi-fullscreen',
      book: {} as BookDto,
      series: {} as SeriesDto,
      context: {} as Context,
      contextName: '',
      incognito: false,
      siblingPrevious: {} as BookDto,
      siblingNext: {} as BookDto,
      jumpToNextBook: false,
      jumpToPreviousBook: false,
      jumpConfirmationDelay: 3000,
      notificationReadingDirection: {
        enabled: false,
        fromMetadata: false,
      },
      pages: [] as PageDtoWithUrl[],
      originalPagesCount: 0,
      splitPageMapping: [] as { originalPage: number, splitPages: number[] }[],
      page: undefined as unknown as number,
      initialized: false,
      supportedMediaTypes: ['image/jpeg', 'image/png', 'image/gif'],
      convertTo: 'jpeg',
      showExplorer: false,
      showToolbars: false,
      showSettings: false,
      showHelp: false,
      goToPage: 1,
      settings: {
        pageLayout: PagedReaderLayout.SINGLE_PAGE,
        swipe: false,
        alwaysFullscreen: false,
        animations: true,
        scale: ScaleType.SCREEN,
        continuousScale: ContinuousScaleType.WIDTH,
        sidePadding: 0,
        pageMargin: 0,
        readingDirection: ReadingDirection.LEFT_TO_RIGHT,
        backgroundColor: 'black',
        rotation: 0,
        splitWidePages: false,
        swapSplitPages: false,
      },
      shortcuts: {} as any,
      notification: {
        enabled: false,
        message: '',
        timeout: 4000,
      },
      readingDirs: Object.values(ReadingDirection).map(x => ({
        text: this.$i18n.t(`enums.reading_direction.${x}`),
        value: x,
      })),
      scaleTypes: Object.values(ScaleType).map(x => ({
        text: this.$i18n.t(x),
        value: x,
      })),
      continuousScaleTypes: Object.values(ContinuousScaleType).map(x => ({
        text: this.$i18n.t(x),
        value: x,
      })),
      pageLayouts: Object.values(PagedReaderLayout).map(x => ({
        text: this.$i18n.t(x),
        value: x,
      })),
      paddingPercentages: Object.values(PaddingPercentage).map(x => ({
        text: x === 0 ? this.$i18n.t('bookreader.settings.side_padding_none').toString() : `${x}%`,
        value: x,
      })),
      marginValues: Object.values(MarginValues).map(x => ({
        text: x === 0 ? this.$i18n.t('bookreader.settings.side_padding_none').toString() : `${x}px`,
        value: x,
      })),
      backgroundColors: [
        {text: this.$t('bookreader.settings.background_colors.white').toString(), value: 'white'},
        {text: this.$t('bookreader.settings.background_colors.gray').toString(), value: '#212121'},
        {text: this.$t('bookreader.settings.background_colors.black').toString(), value: 'black'},
        {text: this.$t('bookreader.settings.background_colors.system').toString(), value: 'system'},
        {text: this.$t('bookreader.settings.background_colors.immersive').toString(), value: 'immersive'},
      ],
      rotationOptions: [
        {text: '0°', value: 0},
        {text: '90°', value: 90},
        {text: '180°', value: 180},
        {text: '270°', value: 270},
      ],
    }
  },
  created() {
    this.$vuetify.rtl = false
    checkImageSupport(ImageFeature.WEBP_LOSSY, (isSupported) => {
      if (isSupported) this.supportedMediaTypes.push('image/webp')
    })
    checkImageSupport(ImageFeature.JPEG_XL, (isSupported) => {
      if (isSupported) this.supportedMediaTypes.push('image/jxl')
    })
    checkImageSupport(ImageFeature.AVIF, (isSupported) => {
      if (isSupported) this.supportedMediaTypes.push('image/avif')
    })
    this.shortcuts = this.$_.keyBy([...shortcutsSettings, ...shortcutsSettingsPaged, ...shortcutsSettingsContinuous, ...shortcutsMenus, ...shortcutsAll], x => x.key)
    window.addEventListener('keydown', this.keyPressed)
    if (screenfull.isEnabled) screenfull.on('change', this.fullscreenChanged)

    // Listen for system theme changes
    this.systemThemeMediaQuery = window.matchMedia('(prefers-color-scheme: dark)')
    this.systemThemeMediaQuery.addEventListener('change', this.handleSystemThemeChange)
  },
  async mounted() {
    document.documentElement.classList.add('html-reader')

    this.$debug('[mounted]', 'route.query:', this.$route.query)

    this.readingDirection = this.$store.state.persistedState.webreader.readingDirection
    this.animations = this.$store.state.persistedState.webreader.animations
    this.pageLayout = this.$store.state.persistedState.webreader.paged.pageLayout
    this.swipe = this.$store.state.persistedState.webreader.swipe
    this.alwaysFullscreen = this.$store.state.persistedState.webreader.alwaysFullscreen
    this.scale = this.$store.state.persistedState.webreader.paged.scale
    this.continuousScale = this.$store.state.persistedState.webreader.continuous.scale
    this.sidePadding = this.$store.state.persistedState.webreader.continuous.padding
    this.pageMargin = this.$store.state.persistedState.webreader.continuous.margin
    this.backgroundColor = this.$store.state.persistedState.webreader.background
    this.rotation = this.$store.state.persistedState.webreader.rotation || 0
    this.splitWidePages = this.$store.state.persistedState.webreader.splitWidePages || false
    this.swapSplitPages = this.$store.state.persistedState.webreader.swapSplitPages || false

    // Initialize actual background color based on stored setting
    const validColors = ['white', '#212121', 'black', 'system', 'immersive']

    if (this.settings.backgroundColor === 'immersive') {
      this.actualBackgroundColor = '#2a2a2a' // Default fallback for immersive
      // Delay the immersive background update to ensure images are loaded
      setTimeout(() => {
        this.updateImmersiveBackground()
      }, 50)
    } else if (this.settings.backgroundColor === 'system') {
      this.actualBackgroundColor = this.getSystemThemeColor()
    } else if (validColors.includes(this.settings.backgroundColor)) {
      this.actualBackgroundColor = this.settings.backgroundColor
    } else {
      // Fallback to black if stored color is invalid
      this.actualBackgroundColor = 'black'
      this.settings.backgroundColor = 'black'
      this.$store.commit('setWebreaderBackground', 'black')
    }

    this.setup(this.bookId, Number(this.$route.query.page))
  },
  destroyed() {
    document.documentElement.classList.remove('html-reader')

    this.$vuetify.rtl = (this.$t('common.locale_rtl') === 'true')
    window.removeEventListener('keydown', this.keyPressed)
    if (screenfull.isEnabled) {
      screenfull.off('change', this.fullscreenChanged)
      screenfull.exit()
    }

    // Remove system theme change listener
    if (this.systemThemeMediaQuery) {
      this.systemThemeMediaQuery.removeEventListener('change', this.handleSystemThemeChange)
    }

    // Restore status bar color to app theme
    const currentTheme = this.$vuetify.theme.dark ? 'dark' : 'light'
    const themeColor = String(this.$vuetify.theme.themes[currentTheme]['contrast-1'] || (this.$vuetify.theme.dark ? '#424242' : '#fafafa'))
    const metaThemeColor = document.querySelector('meta[name="theme-color"]')
    if (metaThemeColor) {
      metaThemeColor.setAttribute('content', themeColor)
    }
  },
  props: {
    bookId: {
      type: String,
      required: true,
    },
  },
  async beforeRouteUpdate(to, from, next) {
    if (to.params.bookId !== from.params.bookId) {
      // route update means either:
      // - going to previous/next book, in this case the query.page is not set, so it will default to first page
      // - pressing the back button of the browser and navigating to the previous book, in this case the query.page is set, so we honor it
      this.$debug('[beforeRouteUpdate]', 'to.query:', to.query)
      await this.setup(to.params.bookId, Number(to.query.page))

      // Update immersive background after setup is complete
      if (this.settings.backgroundColor === 'immersive') {
        setTimeout(() => {
          this.updateImmersiveBackground()
        }, 50)
      }
    }
    next()
  },
  watch: {
    page: {
      handler(val, old) {
        if (val && this.initialized) {
          this.markProgress(val)
          this.goToPage = val
          this.updateRoute()
          // Only update immersive background when page changes if immersive mode is active
          if (this.settings.backgroundColor === 'immersive') {
            // Small delay to ensure the page has fully loaded
            setTimeout(() => {
              this.updateImmersiveBackground()
            }, 20)
          }
        } else if (val) {
          this.goToPage = val
          this.updateRoute()
        }
      },
      immediate: true,
    },
  },
  computed: {
    hasToc(): boolean {
      return this.toc && this.toc.length > 0
    },

    continuousReader(): boolean {
      return this.readingDirection === ReadingDirection.WEBTOON
    },
    progress(): number {
      return this.page / this.pagesCount * 100
    },
    pagesCount(): number {
      return this.pages.length
    },
    bookTitle(): string {
      return getBookTitleCompact(this.book.metadata.title, this.series.metadata.title, this.book.oneshot ? undefined : this.book.metadata.number)
    },
    readingDirectionText(): string {
      return this.$t(`enums.reading_direction.${this.readingDirection}`).toString()
    },
    shortcutsHelp(): object {
      let nav = []
      switch (this.readingDirection) {
        case ReadingDirection.LEFT_TO_RIGHT:
          nav.push(...shortcutsLTR, ...shortcutsAll)
          break
        case ReadingDirection.RIGHT_TO_LEFT:
          nav.push(...shortcutsRTL, ...shortcutsAll)
          break
        case ReadingDirection.VERTICAL:
          nav.push(...shortcutsVertical, ...shortcutsAll)
          break
        default:
          nav.push(...shortcutsAll)
      }
      let settings = [...shortcutsSettings]
      if (this.continuousReader) {
        settings.push(...shortcutsSettingsContinuous)
      } else {
        settings.push(...shortcutsSettingsPaged)
      }
      return {
        [this.$t('bookreader.shortcuts.reader_navigation').toString()]: nav,
        [this.$t('bookreader.shortcuts.settings').toString()]: settings,
        [this.$t('bookreader.shortcuts.menus').toString()]: shortcutsMenus,
      }
    },
    contextReadList(): boolean {
      return this.context.origin === ContextOrigin.READLIST
    },
    currentPage(): PageDtoWithUrl {
      return this.pages[this.page - 1]
    },

    animations: {
      get: function (): boolean {
        return this.settings.animations
      },
      set: function (animations: boolean): void {
        this.settings.animations = animations
        this.$store.commit('setWebreaderAnimations', animations)
      },
    },
    scale: {
      get: function (): ScaleType {
        return this.settings.scale
      },
      set: function (scale: ScaleType): void {
        if (Object.values(ScaleType).includes(scale)) {
          this.settings.scale = scale
          this.$store.commit('setWebreaderPagedScale', scale)
        }
      },
    },
    continuousScale: {
      get: function (): ContinuousScaleType {
        return this.settings.continuousScale
      },
      set: function (scale: ContinuousScaleType): void {
        if (Object.values(ContinuousScaleType).includes(scale)) {
          this.settings.continuousScale = scale
          this.$store.commit('setWebreaderContinuousScale', scale)
        }
      },
    },
    sidePadding: {
      get: function (): number {
        return this.settings.sidePadding
      },
      set: function (padding: number): void {
        if (PaddingPercentage.includes(padding)) {
          this.settings.sidePadding = padding
          this.$store.commit('setWebreaderContinuousPadding', padding)
        }
      },
    },
    pageMargin: {
      get: function (): number {
        return this.settings.pageMargin
      },
      set: function (margin: number): void {
        if (MarginValues.includes(margin)) {
          this.settings.pageMargin = margin
          this.$store.commit('setWebreaderContinuousMargin', margin)
        }
      },
    },
    backgroundColor: {
      get: function (): string {
        return this.settings.backgroundColor
      },
      set: function (color: string): void {
        const validColors = ['white', '#212121', 'black', 'system', 'immersive']

        if (validColors.includes(color)) {
          this.settings.backgroundColor = color
          this.$store.commit('setWebreaderBackground', color)

          if (color === 'immersive') {
            this.actualBackgroundColor = '#2a2a2a'
            // Delay to ensure the setting change is processed and images are ready
            setTimeout(() => {
              this.updateImmersiveBackground()
            }, 30)
          } else if (color === 'system') {
            this.actualBackgroundColor = this.getSystemThemeColor()
          } else {
            this.actualBackgroundColor = color
          }

          this.updateReaderStatusBarColor()
        }
      },
    },
    readingDirection: {
      get: function (): ReadingDirection {
        return this.settings.readingDirection
      },
      set: function (readingDirection: ReadingDirection): void {
        if (Object.values(ReadingDirection).includes(readingDirection)) {
          this.settings.readingDirection = readingDirection
          this.$store.commit('setWebreaderReadingDirection', readingDirection)
          if (this.splitWidePages) {
            this.forceProcessPages()
          }
        }
      },
    },
    pageLayout: {
      get: function (): PagedReaderLayout {
        return this.settings.pageLayout
      },
      set: function (pageLayout: PagedReaderLayout): void {
        if (Object.values(PagedReaderLayout).includes(pageLayout)) {
          this.settings.pageLayout = pageLayout
          this.$store.commit('setWebreaderPagedPageLayout', pageLayout)
          this.processPages()
        }
      },
    },
    swipe: {
      get: function (): boolean {
        return this.settings.swipe
      },
      set: function (swipe: boolean): void {
        this.settings.swipe = swipe
        this.$store.commit('setWebreaderSwipe', swipe)
      },
    },
    alwaysFullscreen: {
      get: function (): boolean {
        return this.settings.alwaysFullscreen
      },
      set: function (alwaysFullscreen: boolean): void {
        this.settings.alwaysFullscreen = alwaysFullscreen
        this.$store.commit('setWebreaderAlwaysFullscreen', alwaysFullscreen)
        if (alwaysFullscreen) this.enterFullscreen()
        else screenfull.isEnabled && screenfull.exit()
      },
    },
    rotation: {
      get: function (): number {
        return this.settings.rotation
      },
      set: function (rotation: number): void {
        if (this.rotationOptions.map(x => x.value).includes(rotation)) {
          this.settings.rotation = rotation
          this.$store.commit('setWebreaderRotation', rotation)
        }
      },
    },
    splitWidePages: {
      get: function (): boolean {
        return this.settings.splitWidePages
      },
      set: function (splitWidePages: boolean): void {
        const currentOriginalPage = this.splitPageToOriginalPage(this.page)

        this.settings.splitWidePages = splitWidePages
        this.$store.commit('setWebreaderSplitWidePages', splitWidePages)
        this.processPages()

        let targetPage = currentOriginalPage
        if (splitWidePages) {
          targetPage = this.originalPageToSplitPage(currentOriginalPage)
        }

        if (targetPage <= this.pagesCount) {
          this.goTo(targetPage)
        } else {
          this.goToFirst()
        }

        // Update immersive background after split pages setting changes
        if (this.settings.backgroundColor === 'immersive') {
          setTimeout(() => {
            this.updateImmersiveBackground()
          }, 30)
        }
      },
    },
    swapSplitPages: {
      get: function (): boolean {
        return this.settings.swapSplitPages
      },
      set: function (swapSplitPages: boolean): void {
        this.settings.swapSplitPages = swapSplitPages
        this.$store.commit('setWebreaderSwapSplitPages', swapSplitPages)
        this.forceProcessPages()
        // Update immersive background after swap split pages setting changes
        if (this.settings.backgroundColor === 'immersive') {
          setTimeout(() => {
            this.updateImmersiveBackground()
          }, 30)
        }
      },
    },
  },
  methods: {
    getSystemThemeColor(): string {
      const isDark = window.matchMedia('(prefers-color-scheme: dark)').matches
      return isDark ? 'black' : 'white'
    },

    handleSystemThemeChange(event: MediaQueryListEvent): void {
      // Only update if system theme is currently selected
      if (this.settings.backgroundColor === 'system') {
        this.actualBackgroundColor = this.getSystemThemeColor()
        this.updateReaderStatusBarColor()
      }
    },

    detectWebtoonFromMetadata(): boolean {
      const bookTags = this.book?.metadata?.tags || []
      const hasWebtoonInBookTags = bookTags.some(tag =>
        tag.toLowerCase().includes('webtoon'),
      )

      const seriesGenres = this.series?.metadata?.genres || []
      const hasWebtoonInSeriesGenres = seriesGenres.some(genre =>
        genre.toLowerCase().includes('webtoon'),
      )

      const seriesTags = this.series?.metadata?.tags || []
      const hasWebtoonInSeriesTags = seriesTags.some(tag =>
        tag.toLowerCase().includes('webtoon'),
      )

      return hasWebtoonInBookTags || hasWebtoonInSeriesGenres || hasWebtoonInSeriesTags
    },

    // Build TOC by reading the Readium manifest and mapping href -> spine index -> page#
    async loadEpubToc(bookId: string) {
      try {
        const res = await fetch(bookManifestUrl(bookId), { credentials: 'include' })
        if (!res.ok) return
        const pub = await res.json()
        const baseUrl = new URL(bookManifestUrl(bookId), window.location.href)
        const normalize = (h: string) => {
          try { return new URL(h, baseUrl).pathname } catch { return (h || '').split('#')[0] }
        }
        // readingOrder = EPUB spine in reading order
        const spine = (pub.readingOrder || []).map((it: any) => normalize(it.href))
        this.spinePaths = spine

        const toEntries = (nodes: any[]): TocEntry[] => (nodes || []).map((n: any) => ({
          title: n.title || n.name || n.href,
          href: n.href,
          children: toEntries(n.children || []),
          page: this.hrefToPage(n.href),
        }))
        // Prefer explicit toc, otherwise fall back to landmarks if present
        const tocTree = (pub.toc && pub.toc.length ? pub.toc : (pub.landmarks || []))
        this.toc = toEntries(tocTree)
      } catch {
        // Non-EPUB or manifest missing → no TOC
        this.toc = []
        this.spinePaths = []
      }
    },
    hrefToPage(href: string): number {
      if (!href) return 1
      const clean = href.split('#')[0]
      let path: string
      try { path = new URL(clean, window.location.href).pathname } catch { path = clean }
      const idx = this.spinePaths.indexOf(path)
      return idx >= 0 ? (idx + 1) : 1
    },
    goToEntry(entry: any) {
      if (entry?.page) this.goTo(entry.page)
      this.showToc = false
    },

    enterFullscreen() {
      if (screenfull.isEnabled) screenfull.request(document.documentElement, {navigationUI: 'hide'})
    },
    switchFullscreen() {
      if (screenfull.isEnabled) screenfull.isFullscreen ? screenfull.exit() : this.enterFullscreen()
    },
    fullscreenChanged() {
      if (screenfull.isEnabled && screenfull.isFullscreen) this.fullscreenIcon = 'mdi-fullscreen-exit'
      else this.fullscreenIcon = 'mdi-fullscreen'
    },
    keyPressed(e: KeyboardEvent) {
      if (e.ctrlKey || e.altKey || e.shiftKey || e.metaKey) return
      this.shortcuts[e.key]?.execute(this)
    },
    async setup(bookId: string, page?: number) {
      // Try to load EPUB navigation (harmless for non-EPUBs)
      await this.loadEpubToc(bookId)

      this.$debug('[setup]', `bookId:${bookId}`, `page:${page}`)
      this.book = await this.$komgaBooks.getBook(bookId)
      this.series = await this.$komgaSeries.getOneSeries(this.book.seriesId)

      // parse query params to get context and contextId
      if (this.$route.query.contextId && this.$route.query.context
        && Object.values(ContextOrigin).includes(this.$route.query.context as ContextOrigin)) {
        this.context = {
          origin: this.$route.query.context as ContextOrigin,
          id: this.$route.query.contextId as string,
        }
        this.book.context = this.context
      }

      if (this?.context.origin === ContextOrigin.READLIST) {
        this.contextName = (await (this.$komgaReadLists.getOneReadList(this.context.id))).name
        document.title = `Komga - ${this.contextName} - ${this.book.metadata.title}`
      } else {
        document.title = `Komga - ${this.bookTitle}`
      }

      // parse query params to get incognito mode
      this.incognito = !!(this.$route.query.incognito && this.$route.query.incognito.toString().toLowerCase() === 'true')

      const pageDtos = (await this.$komgaBooks.getBookPages(bookId))
      pageDtos.forEach((p: any) => p['url'] = this.getPageUrl(p))
      this.pages = pageDtos as PageDtoWithUrl[]
      this.originalPagesCount = pageDtos.length

      await this.processPages()

      if (page && page >= 1 && page <= this.originalPagesCount) {
        const splitPage = this.originalPageToSplitPage(page)
        this.goTo(splitPage)
      } else if (this.book.readProgress?.completed === false) {
        const originalPage = this.book.readProgress?.page!!
        const splitPage = this.originalPageToSplitPage(originalPage)
        this.goTo(splitPage)
      } else {
        this.goToFirst()
      }

      const validColors = ['white', '#212121', 'black', 'system', 'immersive']

      if (this.settings.backgroundColor === 'immersive') {
        this.actualBackgroundColor = '#2a2a2a'
        // Delay the immersive background update to ensure images are loaded
        setTimeout(() => {
          this.updateImmersiveBackground()
        }, 50)
      } else if (this.settings.backgroundColor === 'system') {
        this.actualBackgroundColor = this.getSystemThemeColor()
      } else if (validColors.includes(this.settings.backgroundColor)) {
        this.actualBackgroundColor = this.settings.backgroundColor
      } else {
        this.actualBackgroundColor = 'black'
        this.settings.backgroundColor = 'black'
        this.$store.commit('setWebreaderBackground', 'black')
      }

      const isWebtoonDetected = this.detectWebtoonFromMetadata()
      let readingDirectionChanged = false

      if (isWebtoonDetected && this.readingDirection !== ReadingDirection.WEBTOON) {
        this.settings.readingDirection = ReadingDirection.WEBTOON
        this.sendNotification(`${this.$t('bookreader.changing_reading_direction')}: ${this.$t('enums.reading_direction.WEBTOON')} (${this.$t('bookreader.from_series_metadata')})`)
        readingDirectionChanged = true
      }

      // set non-persistent reading direction if exists in metadata (only if not already changed by webtoon detection)
      if (!readingDirectionChanged && this.series.metadata.readingDirection in ReadingDirection && this.readingDirection !== this.series.metadata.readingDirection) {
        // bypass setter so setting is not persisted
        this.settings.readingDirection = this.series.metadata.readingDirection as ReadingDirection
        this.sendNotificationReadingDirection(true)
      } else if (!readingDirectionChanged) {
        this.sendNotificationReadingDirection(false)
      }

      try {
        if (this?.context.origin === ContextOrigin.READLIST) {
          this.siblingNext = await this.$komgaReadLists.getBookSiblingNext(this.context.id, bookId)
        } else {
          this.siblingNext = await this.$komgaBooks.getBookSiblingNext(bookId)
        }
      } catch (e) {
        this.siblingNext = {} as BookDto
      }
      try {
        if (this?.context.origin === ContextOrigin.READLIST) {
          this.siblingPrevious = await this.$komgaReadLists.getBookSiblingPrevious(this.context.id, bookId)
        } else {
          this.siblingPrevious = await this.$komgaBooks.getBookSiblingPrevious(bookId)
        }
      } catch (e) {
        this.siblingPrevious = {} as BookDto
      }

      // Auto fullscreen in PWA mode
      const isPWA = (window.matchMedia && window.matchMedia('(display-mode: standalone)').matches) ||
                    (window.navigator as any).standalone === true
      if (isPWA) {
        this.enterFullscreen()
      }

      // Update status bar color initially
      this.updateReaderStatusBarColor()

      this.initialized = true
    },
    getPageUrl(page: PageDto): string {
      if (!this.supportedMediaTypes.includes(page.mediaType)) {
        return bookPageUrl(this.bookId, page.number, this.convertTo)
      } else {
        return bookPageUrl(this.bookId, page.number)
      }
    },
    jumpToPrevious() {
      if (this.jumpToPreviousBook) {
        this.previousBook()
      } else {
        this.jumpToPreviousBook = true
      }
    },
    jumpToNext() {
      if (this.jumpToNextBook) {
        this.nextBook()
      } else {
        this.jumpToNextBook = true
      }
    },
    previousBook() {
      if (!this.$_.isEmpty(this.siblingPrevious)) {
        this.jumpToPreviousBook = false
        this.$router.push({
          name: getBookReadRouteFromMedia(this.siblingPrevious.media),
          params: {bookId: this.siblingPrevious.id.toString()},
          query: {context: this.context.origin, contextId: this.context.id, incognito: this.incognito.toString()},
        })
      }
    },
    nextBook() {
      if (this.$_.isEmpty(this.siblingNext)) {
        this.closeBook()
      } else {
        this.jumpToNextBook = false
        this.$router.push({
          name: getBookReadRouteFromMedia(this.siblingNext.media),
          params: {bookId: this.siblingNext.id.toString()},
          query: {context: this.context.origin, contextId: this.context.id, incognito: this.incognito.toString()},
        })
      }
    },
    goTo(page: number) {
      this.$debug('[goTo]', `page:${page}`)
      this.page = page
      if (this.initialized) {
        this.markProgress(page)
      }
    },
    goToOriginal(page: number) {
      const splitPage = this.originalPageToSplitPage(page)
      this.goTo(splitPage)
    },
    goToFirst() {
      this.goTo(1)
    },
    goToLast() {
      this.goTo(this.pagesCount)
    },
    updateRoute() {
      const originalPage = this.splitPageToOriginalPage(this.page)
      this.$router.replace({
        name: this.$route.name,
        params: {bookId: this.$route.params.bookId},
        query: {
          page: originalPage.toString(),
          context: this.context.origin,
          contextId: this.context.id,
          incognito: this.incognito.toString(),
        },
      } as Location)
    },
    closeBook() {
      this.$router.push(
        {
          name: this.book.oneshot ? 'browse-oneshot' : 'browse-book',
          params: {bookId: this.bookId.toString(), seriesId: this.book.seriesId},
          query: {context: this.context.origin, contextId: this.context.id},
        })
    },
    changeReadingDir(dir: ReadingDirection) {
      this.readingDirection = dir
      const text = this.$t(`enums.reading_direction.${this.readingDirection}`)
      this.sendNotification(`${this.$t('bookreader.changing_reading_direction')}: ${text}`)
    },
    cycleScale() {
      if (this.continuousReader) {
        const enumValues = Object.values(ContinuousScaleType)
        const i = (enumValues.indexOf(this.settings.continuousScale) + 1) % (enumValues.length)
        this.continuousScale = enumValues[i]
        const text = this.$t(this.continuousScale)
        this.sendNotification(`${this.$t('bookreader.cycling_scale')}: ${text}`)
      } else {
        const enumValues = Object.values(ScaleType)
        const i = (enumValues.indexOf(this.settings.scale) + 1) % (enumValues.length)
        this.scale = enumValues[i]
        const text = this.$t(this.scale)
        this.sendNotification(`${this.$t('bookreader.cycling_scale')}: ${text}`)
      }
    },
    cycleSidePadding() {
      if (this.continuousReader) {
        const i = (PaddingPercentage.indexOf(this.settings.sidePadding) + 1) % (PaddingPercentage.length)
        this.sidePadding = PaddingPercentage[i]
        const text = this.sidePadding === 0 ? this.$t('bookreader.settings.side_padding_none').toString() : `${this.sidePadding}%`
        this.sendNotification(`${this.$t('bookreader.cycling_side_padding')}: ${text}`)
      }
    },
    cyclePageMargin() {
      if (this.continuousReader) {
        const i = (MarginValues.indexOf(this.settings.pageMargin) + 1) % (MarginValues.length)
        this.pageMargin = MarginValues[i]
        const text = this.pageMargin === 0 ? this.$t('bookreader.settings.side_padding_none').toString() : `${this.pageMargin}px`
        this.sendNotification(`${this.$t('bookreader.cycling_page_margin')}: ${text}`)
      }
    },
    cyclePageLayout() {
      if (this.continuousReader) return
      const enumValues = Object.values(PagedReaderLayout)
      const i = (enumValues.indexOf(this.settings.pageLayout) + 1) % (enumValues.length)
      this.pageLayout = enumValues[i]
      const text = this.$i18n.t(this.pageLayout)
      this.sendNotification(`${this.$t('bookreader.cycling_page_layout')}: ${text}`)
    },
    cycleRotation() {
      const i = (this.rotationOptions.findIndex(x => x.value === this.settings.rotation) + 1) % (this.rotationOptions.length)
      this.rotation = this.rotationOptions[i].value
      const text = this.rotationOptions[i].text
      this.sendNotification(`${this.$t('bookreader.cycling_rotation')}: ${text}`)
    },
    toggleToolbars() {
      this.showToolbars = !this.showToolbars
    },
    toggleExplorer() {
      this.showExplorer = !this.showExplorer
    },
    toggleSettings() {
      this.showSettings = !this.showSettings
    },
    toggleHelp() {
      this.showHelp = !this.showHelp
    },
    closeDialog() {
      if (this.showExplorer) {
        this.showExplorer = false
        return
      }
      if (this.showSettings) {
        this.showSettings = false
        return
      }
      if (this.showToolbars) {
        this.showToolbars = false
        return
      }
      this.closeBook()
    },
    sendNotificationReadingDirection(fromMetadata: boolean) {
      this.notificationReadingDirection.fromMetadata = fromMetadata
      this.notificationReadingDirection.enabled = true
    },
    sendNotification(message: string, timeout: number = 4000) {
      this.notification.timeout = timeout
      this.notification.message = message
      this.notification.enabled = true
    },
    updateReaderStatusBarColor() {
      // Update status bar color based on reader background
      let statusBarColor = '#000000'

      if (this.settings.backgroundColor === 'white') {
        statusBarColor = 'white'   // white background
      } else if (this.settings.backgroundColor === '#212121') {
        statusBarColor = '#212121' // gray background
      } else if (this.settings.backgroundColor === 'black') {
        statusBarColor = 'black'   // black background
      } else if (this.settings.backgroundColor === 'system') {
        // For system theme, use the actual resolved color
        statusBarColor = this.actualBackgroundColor
      } else if (this.settings.backgroundColor === 'immersive') {
        // Extract primary color from immersive background gradient
        const primaryColor = this.extractPrimaryColorFromGradient(this.actualBackgroundColor)
        statusBarColor = primaryColor || '#2a2a2a'
      }

      const metaThemeColor = document.querySelector('meta[name="theme-color"]')
      if (metaThemeColor) {
        metaThemeColor.setAttribute('content', statusBarColor)
      }
    },
    extractPrimaryColorFromGradient(gradient: string): string | null {
      if (!gradient || !gradient.includes('linear-gradient')) {
        return null
      }

      // Extract the first rgb color from the gradient (typically the top edge color)
      const rgbMatch = gradient.match(/rgb\(\s*\d+\s*,\s*\d+\s*,\s*\d+\s*\)/)
      if (rgbMatch) {
        return rgbMatch[0]
      }

      return null
    },
    markProgress: debounce(function (this: any, page: number) {
      if (!this.incognito) {
        const originalPage = this.splitPageToOriginalPage(page)
        this.$komgaBooks.updateReadProgress(this.bookId, {page: originalPage})
      }
    }, 50),
    downloadCurrentPage() {
      new jsFileDownloader({
        url: `${this.currentPage.url}?contentNegotiation=false`,
        filename: `${this.book.name}-${this.currentPage.number}.${this.currentPage.fileName.split('.').pop()}`,
        withCredentials: true,
        forceDesktopMode: true,
      })
    },
    async setCurrentPageAsPoster(type: ItemTypes) {
      const imageFile = await getFileFromUrl(`${this.currentPage.url}?contentNegotiation=false`, 'poster', 'image/jpeg', {credentials: 'include'})
      const newImageFile = await resizeImageFile(imageFile)
      switch (type) {
        case ItemTypes.BOOK:
          await this.$komgaBooks.uploadThumbnail(this.book.id, newImageFile, true)
          this.sendNotification(`${this.$t('bookreader.notification_poster_set_book')}`)
          break
        case ItemTypes.SERIES:
          await this.$komgaSeries.uploadThumbnail(this.series.id, newImageFile, true)
          this.sendNotification(`${this.$t('bookreader.notification_poster_set_series')}`)
          break
        case ItemTypes.READLIST:
          await this.$komgaReadLists.uploadThumbnail(this.context.id, newImageFile, true)
          this.sendNotification(`${this.$t('bookreader.notification_poster_set_readlist')}`)
          break
      }
    },
    extractDominantColorFromLoadedImage(imageUrl: string): Promise<string> {
      return new Promise((resolve) => {
        // Extract base URL without fragment for comparison
        const baseImageUrl = imageUrl.split('#')[0]

        // First, check if the image is already loaded
        const existingImages = Array.from(document.querySelectorAll('img')).filter(
          img => {
            const imgBaseUrl = img.src.split('#')[0]
            const dataPageUrl = img.dataset.pageUrl

            // Match by src URL (base URL comparison)
            if (imgBaseUrl === baseImageUrl || imgBaseUrl === baseImageUrl.replace(/\/$/, '')) {
              return true
            }

            // Match by data-page-url attribute (for PagedReader images)
            if (dataPageUrl) {
              const dataPageBaseUrl = dataPageUrl.split('#')[0]
              if (dataPageBaseUrl === baseImageUrl || dataPageBaseUrl === baseImageUrl.replace(/\/$/, '')) {
                return true
              }
            }

            // Match data URLs (for cached split images)
            if (img.src.startsWith('data:') && dataPageUrl) {
              const dataPageBaseUrl = dataPageUrl.split('#')[0]
              if (dataPageBaseUrl === baseImageUrl || dataPageBaseUrl === baseImageUrl.replace(/\/$/, '')) {
                return true
              }
            }

            return false
          },
        )

        if (existingImages.length > 0) {
          const loadedImage = existingImages.find(img => img.complete && img.naturalWidth > 0) as HTMLImageElement

          if (loadedImage) {
            // Image is already loaded, analyze immediately
            this.analyzeImageColor(loadedImage).then(color => {
              resolve(color)
            }).catch(() => {
              resolve('#2a2a2a')
            })
            return
          }

          // Image exists but not loaded yet, add event listener
          const targetImage = existingImages[0]
          this.attachImageLoadListeners(targetImage, resolve)
          return
        }

        // Image doesn't exist yet, use MutationObserver to watch for its creation
        const observer = new MutationObserver((mutations) => {
          for (const mutation of mutations) {
            if (mutation.type === 'childList') {
              const addedNodes = Array.from(mutation.addedNodes)
              const imgElements = addedNodes.filter(node =>
                node.nodeType === Node.ELEMENT_NODE &&
                (node as Element).tagName === 'IMG',
              ) as HTMLImageElement[]

              for (const img of imgElements) {
                const imgBaseUrl = img.src.split('#')[0]
                const dataPageUrl = img.dataset.pageUrl

                let matches = false

                // Match by src URL
                if (imgBaseUrl === baseImageUrl || imgBaseUrl === baseImageUrl.replace(/\/$/, '')) {
                  matches = true
                }

                // Match by data-page-url attribute
                if (!matches && dataPageUrl) {
                  const dataPageBaseUrl = dataPageUrl.split('#')[0]
                  if (dataPageBaseUrl === baseImageUrl || dataPageBaseUrl === baseImageUrl.replace(/\/$/, '')) {
                    matches = true
                  }
                }

                // Match data URLs with data-page-url
                if (!matches && img.src.startsWith('data:') && dataPageUrl) {
                  const dataPageBaseUrl = dataPageUrl.split('#')[0]
                  if (dataPageBaseUrl === baseImageUrl || dataPageBaseUrl === baseImageUrl.replace(/\/$/, '')) {
                    matches = true
                  }
                }

                if (matches) {
                  observer.disconnect()
                  this.attachImageLoadListeners(img, resolve)
                  return
                }
              }
            } else if (mutation.type === 'attributes' && (mutation.attributeName === 'src' || mutation.attributeName === 'data-page-url')) {
              const target = mutation.target as HTMLImageElement
              if (target.tagName === 'IMG') {
                const targetBaseUrl = target.src.split('#')[0]
                const dataPageUrl = target.dataset.pageUrl

                let matches = false

                // Match by src URL
                if (targetBaseUrl === baseImageUrl || targetBaseUrl === baseImageUrl.replace(/\/$/, '')) {
                  matches = true
                }

                // Match by data-page-url attribute
                if (!matches && dataPageUrl) {
                  const dataPageBaseUrl = dataPageUrl.split('#')[0]
                  if (dataPageBaseUrl === baseImageUrl || dataPageBaseUrl === baseImageUrl.replace(/\/$/, '')) {
                    matches = true
                  }
                }

                // Match data URLs with data-page-url
                if (!matches && target.src.startsWith('data:') && dataPageUrl) {
                  const dataPageBaseUrl = dataPageUrl.split('#')[0]
                  if (dataPageBaseUrl === baseImageUrl || dataPageBaseUrl === baseImageUrl.replace(/\/$/, '')) {
                    matches = true
                  }
                }

                if (matches) {
                  observer.disconnect()
                  this.attachImageLoadListeners(target, resolve)
                  return
                }
              }
            }
          }
        })

        observer.observe(document.body, {
          childList: true,
          subtree: true,
          attributes: true,
          attributeFilter: ['src', 'data-page-url'],
        })

        // Fallback timeout in case the image is never created
        setTimeout(() => {
          observer.disconnect()
          resolve('#2a2a2a')
        }, 30000)
      })
    },

    attachImageLoadListeners(img: HTMLImageElement, resolve: (value: string) => void): void {
      if (img.complete && img.naturalWidth > 0) {
        // Image is already loaded
        this.analyzeImageColor(img).then(color => {
          resolve(color)
        }).catch(() => {
          resolve('#2a2a2a')
        })
        return
      }

      const handleLoad = () => {
        img.removeEventListener('load', handleLoad)
        img.removeEventListener('error', handleError)
        this.analyzeImageColor(img).then(color => {
          resolve(color)
        }).catch(() => {
          resolve('#2a2a2a')
        })
      }

      const handleError = () => {
        img.removeEventListener('load', handleLoad)
        img.removeEventListener('error', handleError)
        resolve('#2a2a2a')
      }

      img.addEventListener('load', handleLoad)
      img.addEventListener('error', handleError)

      // Fallback timeout in case the image never loads
      setTimeout(() => {
        img.removeEventListener('load', handleLoad)
        img.removeEventListener('error', handleError)
        resolve('#2a2a2a')
      }, 60000)
    },

    analyzeImageColor(img: HTMLImageElement): Promise<string> {
      return new Promise((resolve, reject) => {
        try {
          const canvas = document.createElement('canvas')
          const ctx = canvas.getContext('2d')
          if (!ctx) {
            reject(new Error('Could not get canvas context'))
            return
          }

          // Use larger canvas for better edge sampling
          const sampleSize = Math.min(150, Math.min(img.naturalWidth, img.naturalHeight))
          canvas.width = sampleSize
          canvas.height = sampleSize

          ctx.drawImage(img, 0, 0, sampleSize, sampleSize)
          const imageData = ctx.getImageData(0, 0, sampleSize, sampleSize)
          const data = imageData.data

          // Define edge width (fixed 1px for precise edge sampling)
          const edgeWidth = 1

          // Initialize color accumulators for each edge
          const edges = {
            top: { r: 0, g: 0, b: 0, count: 0 },
            bottom: { r: 0, g: 0, b: 0, count: 0 },
            left: { r: 0, g: 0, b: 0, count: 0 },
            right: { r: 0, g: 0, b: 0, count: 0 },
          }

          // Sample pixels from each edge region
          for (let y = 0; y < sampleSize; y++) {
            for (let x = 0; x < sampleSize; x++) {
              const index = (y * sampleSize + x) * 4
              const alpha = data[index + 3]

              if (alpha > 128) { // Skip transparent pixels
                const r = data[index]
                const g = data[index + 1]
                const b = data[index + 2]

                // Determine which edge region this pixel belongs to
                if (y < edgeWidth) {
                  // Top edge
                  edges.top.r += r
                  edges.top.g += g
                  edges.top.b += b
                  edges.top.count++
                } else if (y >= sampleSize - edgeWidth) {
                  // Bottom edge
                  edges.bottom.r += r
                  edges.bottom.g += g
                  edges.bottom.b += b
                  edges.bottom.count++
                }

                if (x < edgeWidth) {
                  // Left edge
                  edges.left.r += r
                  edges.left.g += g
                  edges.left.b += b
                  edges.left.count++
                } else if (x >= sampleSize - edgeWidth) {
                  // Right edge
                  edges.right.r += r
                  edges.right.g += g
                  edges.right.b += b
                  edges.right.count++
                }
              }
            }
          }

          // Calculate average colors for each edge
          const getAverageColor = (edge: typeof edges.top) => {
            if (edge.count === 0) return 'rgb(42, 42, 42)' // Default fallback
            const avgR = Math.round(edge.r / edge.count)
            const avgG = Math.round(edge.g / edge.count)
            const avgB = Math.round(edge.b / edge.count)
            return `rgb(${avgR}, ${avgG}, ${avgB})`
          }

          const topColor = getAverageColor(edges.top)
          const bottomColor = getAverageColor(edges.bottom)
          const leftColor = getAverageColor(edges.left)
          const rightColor = getAverageColor(edges.right)

          // Create four-directional gradient overlay
          const fourWayGradient = `
            linear-gradient(to bottom, ${topColor} 0%, transparent 70%),
            linear-gradient(to top, ${bottomColor} 0%, transparent 70%),
            linear-gradient(to right, ${leftColor} 0%, transparent 70%),
            linear-gradient(to left, ${rightColor} 0%, transparent 70%)
          `.trim().replace(/\s+/g, ' ')

          resolve(fourWayGradient)
        } catch (error) {
          reject(error)
        }
      })
    },
    async updateImmersiveBackground() {
      if (this.settings.backgroundColor !== 'immersive') {
        return
      }

      if (!this.currentPage) {
        return
      }

      try {
        const currentSpread = this.getCurrentSpread()
        const pagesToAnalyze = currentSpread && currentSpread.length > 0 ? currentSpread : [this.currentPage]

        // Extract edge colors from all displayed pages
        // For split pages, use the original URL without fragment for color analysis
        const pageGradients = await Promise.all(
          pagesToAnalyze.map(page => {
            // Use original URL without fragment for color analysis
            const originalUrl = page.url.split('#')[0]
            return this.extractDominantColorFromLoadedImage(originalUrl)
          }),
        )

        // Merge gradients from all pages
        const mergedGradient = this.mergePageGradients(pageGradients)
        this.actualBackgroundColor = mergedGradient

        // Update status bar color when immersive background changes
        if (this.settings.backgroundColor === 'immersive') {
          this.updateReaderStatusBarColor()
        }
      } catch (error) {
        // Fallback to a neutral background color for immersive mode
        this.actualBackgroundColor = '#2a2a2a'
      }
    },
    getCurrentSpread(): PageDtoWithUrl[] | null {
      if (this.continuousReader) {
        const continuousReader = this.$refs.continuousReader as any
        if (continuousReader && continuousReader.getVisiblePagesForImmersive) {
          const visiblePages = continuousReader.getVisiblePagesForImmersive()
          if (visiblePages && visiblePages.length > 0) {
            return visiblePages
          }
        }

        const estimatedVisiblePages: PageDtoWithUrl[] = []
        if (this.currentPage) {
          estimatedVisiblePages.push(this.currentPage)
        }

        const currentIndex = this.page - 1
        if (currentIndex > 0 && this.pages[currentIndex - 1]) {
          estimatedVisiblePages.unshift(this.pages[currentIndex - 1])
        }
        if (currentIndex < this.pages.length - 1 && this.pages[currentIndex + 1]) {
          estimatedVisiblePages.push(this.pages[currentIndex + 1])
        }

        return estimatedVisiblePages.length > 0 ? estimatedVisiblePages : (this.currentPage ? [this.currentPage] : null)
      } else {
        const pagedReader = this.$refs.pagedReader as any
        if (pagedReader && pagedReader.spreads && pagedReader.carouselPage >= 0) {
          const spread = pagedReader.spreads[pagedReader.carouselPage]
          if (spread) {
            const validPages = spread.filter((page: PageDtoWithUrl) => {
              return page &&
                     typeof page === 'object' &&
                     page.number > 0 &&
                     page.number <= this.pagesCount &&
                     page.url
            })

            // For immersive background, we need to ensure we have the correct page objects
            // that match the actual displayed images, even if they have split URLs
            if (validPages.length > 0) {
              return validPages.map(page => {
                // Ensure the page object has the correct URL that matches what's displayed
                // The PagedReader might have modified URLs with fragments for split pages
                return {
                  ...page,
                  // Keep the original URL structure from the PagedReader
                  url: page.url,
                }
              })
            }
            return null
          }
        }

        // Fallback: use current page if no spread is available
        return this.currentPage ? [this.currentPage] : null
      }
    },
    mergePageGradients(gradients: string[]): string {
      if (gradients.length === 0) {
        return '#2a2a2a'
      }

      if (gradients.length === 1) {
        const gradient = gradients[0]
        if (!gradient.includes('linear-gradient')) {
          return '#2a2a2a'
        }
        return gradient
      }

      const allEdgeColors: { top: string[], bottom: string[], left: string[], right: string[] } = {
        top: [],
        bottom: [],
        left: [],
        right: [],
      }

      const validGradients = gradients.filter(gradient => gradient.includes('linear-gradient'))
      let processedGradients = [...validGradients]

      if (this.continuousReader) {
        // For continuous reader, missing pages are less critical
      } else {
        // For paged reader, ensure we have the correct number of gradients
        const currentSpread = this.getCurrentSpread()
        const expectedPages = currentSpread ? currentSpread.length : 1

        if (expectedPages === 2 && validGradients.length === 1) {
          // Double page mode but only one gradient available - use the same gradient for both pages
          processedGradients = [validGradients[0], validGradients[0]]
        } else if (expectedPages === 1 && validGradients.length > 0) {
          // Single page mode - use the first available gradient
          processedGradients = [validGradients[0]]
        }
        // If we have the expected number of gradients, use them as-is
      }

      processedGradients.forEach((gradient, pageIndex) => {
        if (gradient.includes('linear-gradient')) {
          // Extract colors from four-way gradient overlay
          // Format: linear-gradient(to bottom, topColor 0%, transparent 70%), linear-gradient(to top, bottomColor 0%, transparent 70%), ...
          const colorMatches = gradient.match(/rgb\(\d+,\s*\d+,\s*\d+\)/g)
          if (colorMatches && colorMatches.length >= 4) {
            // Map to edges: top, bottom, left, right (in order of appearance)
            const topColor = colorMatches[0]    // First gradient: to bottom
            const bottomColor = colorMatches[1] // Second gradient: to top
            const leftColor = colorMatches[2]   // Third gradient: to right
            const rightColor = colorMatches[3]  // Fourth gradient: to left

            // Handle different reading modes for four-way gradient overlay
            if (this.continuousReader) {
              // Continuous reading mode (vertical scrolling) - exclude vertical middle edges
              if (processedGradients.length > 1) {
                if (pageIndex === 0) {
                  // First page: use top, left, right edges (exclude bottom edge - vertical splice)
                  allEdgeColors.top.push(topColor)
                  allEdgeColors.left.push(leftColor)
                  allEdgeColors.right.push(rightColor)
                  // Skip bottom edge for first page
                } else if (pageIndex === processedGradients.length - 1) {
                  // Last page: use bottom, left, right edges (exclude top edge - vertical splice)
                  allEdgeColors.bottom.push(bottomColor)
                  allEdgeColors.left.push(leftColor)
                  allEdgeColors.right.push(rightColor)
                  // Skip top edge for last page
                } else {
                  // Middle pages: use left and right edges only (exclude both top and bottom)
                  allEdgeColors.left.push(leftColor)
                  allEdgeColors.right.push(rightColor)
                  // Skip both top and bottom edges for middle pages
                }
              } else {
                // Single page in continuous mode
                allEdgeColors.top.push(topColor)
                allEdgeColors.right.push(rightColor)
                allEdgeColors.bottom.push(bottomColor)
                allEdgeColors.left.push(leftColor)
              }
            } else {
              // Paged reading mode (horizontal layout) - handle double page as single spread
              if (processedGradients.length === 2) {
                // Check if we're actually displaying only one page (e.g., last page of book)
                const currentSpread = this.getCurrentSpread()
                if (currentSpread && currentSpread.length === 1) {
                  // Actually displaying only one page, treat as single page mode
                  // Use all four edges from the single page
                  allEdgeColors.top.push(topColor)
                  allEdgeColors.right.push(rightColor)
                  allEdgeColors.bottom.push(bottomColor)
                  allEdgeColors.left.push(leftColor)
                } else {
                  // True double page mode: treat both pages as one spread
                  // Need to consider reading direction for correct edge mapping
                  const isRTL = this.readingDirection === 'RIGHT_TO_LEFT'

                  if (isRTL) {
                    // Right-to-left reading: first page in array is visually on the right
                    if (pageIndex === 0) {
                      // Right page (visually): use top, right, bottom edges
                      allEdgeColors.top.push(topColor)
                      allEdgeColors.bottom.push(bottomColor)
                      allEdgeColors.right.push(rightColor)
                    } else if (pageIndex === 1) {
                      // Left page (visually): use top, left, bottom edges
                      allEdgeColors.top.push(topColor)
                      allEdgeColors.bottom.push(bottomColor)
                      allEdgeColors.left.push(leftColor)
                    }
                  } else {
                    // Left-to-right reading: first page in array is visually on the left
                    if (pageIndex === 0) {
                      // Left page (visually): use top, left, bottom edges
                      allEdgeColors.top.push(topColor)
                      allEdgeColors.bottom.push(bottomColor)
                      allEdgeColors.left.push(leftColor)
                    } else if (pageIndex === 1) {
                      // Right page (visually): use top, right, bottom edges
                      allEdgeColors.top.push(topColor)
                      allEdgeColors.bottom.push(bottomColor)
                      allEdgeColors.right.push(rightColor)
                    }
                  }
                }
              } else {
                // Single page mode or fallback mode, use all edges from the single page
                allEdgeColors.top.push(topColor)
                allEdgeColors.right.push(rightColor)
                allEdgeColors.bottom.push(bottomColor)
                allEdgeColors.left.push(leftColor)
              }
            }
          }
        }
      })

      const averageEdgeColor = (colors: string[]): string => {
        if (colors.length === 0) return 'rgb(42, 42, 42)'

        const rgbValues = colors.map(color => {
          const match = color.match(/rgb\((\d+),\s*(\d+),\s*(\d+)\)/)
          if (match) {
            return {
              r: parseInt(match[1]),
              g: parseInt(match[2]),
              b: parseInt(match[3]),
            }
          }
          return { r: 0, g: 0, b: 0 }
        })

        const avgR = Math.round(rgbValues.reduce((sum, val) => sum + val.r, 0) / rgbValues.length)
        const avgG = Math.round(rgbValues.reduce((sum, val) => sum + val.g, 0) / rgbValues.length)
        const avgB = Math.round(rgbValues.reduce((sum, val) => sum + val.b, 0) / rgbValues.length)

        return `rgb(${avgR}, ${avgG}, ${avgB})`
      }

      const mergedTop = averageEdgeColor(allEdgeColors.top)
      const mergedRight = averageEdgeColor(allEdgeColors.right)
      const mergedBottom = averageEdgeColor(allEdgeColors.bottom)
      const mergedLeft = averageEdgeColor(allEdgeColors.left)

      const gradientComponents: string[] = []

      if (allEdgeColors.top.length > 0 && mergedTop) {
        const topGradient = `linear-gradient(to bottom, ${mergedTop} 0%, transparent 70%)`
        gradientComponents.push(topGradient)
      }
      if (allEdgeColors.bottom.length > 0 && mergedBottom) {
        const bottomGradient = `linear-gradient(to top, ${mergedBottom} 0%, transparent 70%)`
        gradientComponents.push(bottomGradient)
      }
      if (allEdgeColors.left.length > 0 && mergedLeft) {
        const leftGradient = `linear-gradient(to right, ${mergedLeft} 0%, transparent 70%)`
        gradientComponents.push(leftGradient)
      }
      if (allEdgeColors.right.length > 0 && mergedRight) {
        const rightGradient = `linear-gradient(to left, ${mergedRight} 0%, transparent 70%)`
        gradientComponents.push(rightGradient)
      }

      if (gradientComponents.length === 0) {
        return '#2a2a2a'
      }

      return gradientComponents.join(', ')
    },

    averageColors(colors: string[]): string {
      // For four-way gradients, use the first one as primary
      if (colors.length > 0 && colors[0].includes('linear-gradient') && colors[0].includes('transparent')) {
        return colors[0]
      }

      // Fallback for solid colors (shouldn't happen with new gradient implementation)
      const rgbValues = colors.map(color => {
        const match = color.match(/rgb\((\d+),\s*(\d+),\s*(\d+)\)/)
        if (match) {
          return {
            r: parseInt(match[1]),
            g: parseInt(match[2]),
            b: parseInt(match[3]),
          }
        }
        return { r: 0, g: 0, b: 0 }
      })

      const avgR = Math.round(rgbValues.reduce((sum, val) => sum + val.r, 0) / rgbValues.length)
      const avgG = Math.round(rgbValues.reduce((sum, val) => sum + val.g, 0) / rgbValues.length)
      const avgB = Math.round(rgbValues.reduce((sum, val) => sum + val.b, 0) / rgbValues.length)

      return `rgb(${avgR}, ${avgG}, ${avgB})`
    },

    async processPages(forceReload: boolean = false) {
      if (!this.pages || this.pages.length === 0) return

      if (forceReload || (!this.splitWidePages || this.isDoublePageLayout())) {
        const pageDtos = (await this.$komgaBooks.getBookPages(this.bookId))
        pageDtos.forEach((p: any) => p['url'] = this.getPageUrl(p))
        this.pages = pageDtos as PageDtoWithUrl[]
        this.originalPagesCount = pageDtos.length
        this.splitPageMapping = pageDtos.map((p, i) => ({ originalPage: p.number, splitPages: [p.number] }))
      }

      if (this.splitWidePages && (!this.isDoublePageLayout() || this.continuousReader)) {
        this.pages = this.applySplitProcessing(this.pages)
      }
    },

    applySplitProcessing(pages: PageDtoWithUrl[]): PageDtoWithUrl[] {
      const processedPages: PageDtoWithUrl[] = []
      this.splitPageMapping = []
      let currentPageNumber = 1

      for (let i = 0; i < pages.length; i++) {
        const page = pages[i]
        const isWide = this.isWidePageFromMetadata(page)

        if (isWide === null || !isWide) {
          const newPage = { ...page, number: currentPageNumber }
          processedPages.push(newPage)
          this.splitPageMapping.push({ originalPage: page.number, splitPages: [currentPageNumber] })
          currentPageNumber++
        } else {
          const virtualPages = this.createVirtualSplitPages(page, currentPageNumber)
          processedPages.push(...virtualPages)
          this.splitPageMapping.push({ originalPage: page.number, splitPages: [currentPageNumber, currentPageNumber + 1] })
          currentPageNumber += 2
        }
      }

      return processedPages
    },

    async forceProcessPages() {
      await this.processPages(true)
    },

    createVirtualSplitPages(page: PageDtoWithUrl, startPageNumber: number): PageDtoWithUrl[] {
      const originalWidth = page.width || 0
      const originalHeight = page.height || 0
      const halfWidth = Math.floor(originalWidth / 2)

      if (this.swapSplitPages || (this.readingDirection === 'RIGHT_TO_LEFT')) {
        const page_right: PageDtoWithUrl = {
          ...page,
          url: `${page.url}#split-right`,
          width: originalWidth - halfWidth,
          height: originalHeight,
          number: startPageNumber,
        }
        const page_left: PageDtoWithUrl = {
          ...page,
          url: `${page.url}#split-left`,
          width: halfWidth,
          height: originalHeight,
          number: startPageNumber + 1,
        }
        return [page_right, page_left]
      } else {
        const page_left: PageDtoWithUrl = {
          ...page,
          url: `${page.url}#split-left`,
          width: halfWidth,
          height: originalHeight,
          number: startPageNumber,
        }
        const page_right: PageDtoWithUrl = {
          ...page,
          url: `${page.url}#split-right`,
          width: originalWidth - halfWidth,
          height: originalHeight,
          number: startPageNumber + 1,
        }
        return [page_left, page_right]
      }
    },

    isDoublePageLayout(): boolean {
      return this.pageLayout === PagedReaderLayout.DOUBLE_PAGES ||
             this.pageLayout === PagedReaderLayout.DOUBLE_NO_COVER
    },

    isWidePageFromMetadata(page: PageDtoWithUrl): boolean | null {
      if (!page.width || !page.height || page.width === 0 || page.height === 0) {
        return null
      }

      const aspectRatio = page.width / page.height
      return aspectRatio > 1
    },

    originalPageToSplitPage(originalPage: number): number {
      const mapping = this.splitPageMapping.find(m => m.originalPage === originalPage)
      return mapping ? mapping.splitPages[0] : originalPage
    },

    splitPageToOriginalPage(splitPage: number): number {
      const mapping = this.splitPageMapping.find(m => m.splitPages.includes(splitPage))
      return mapping ? mapping.originalPage : splitPage
    },

  },
})
</script>
<style scoped>
.settings {
  z-index: 2;
}

.full-height {
  height: 100%;
}

.full-width {
  width: 100%;
}
</style>
<style>
.html-reader::-webkit-scrollbar {
  display: none;
}

.html-reader {
  scrollbar-width: none;
  overscroll-behavior: none;
}
</style>
