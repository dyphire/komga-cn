<template>
  <div>
    <toolbar-sticky v-if="showToolbar">
      <v-toolbar-title>
        <span>{{ $t('search.search_results_for', {name: $route.query.q}) }}</span>
      </v-toolbar-title>
    </toolbar-sticky>

    <multi-select-bar
      v-model="selectedSeries"
      kind="series"
      @unselect-all="selectedSeries = []"
      @mark-read="markSelectedSeriesRead"
      @mark-unread="markSelectedSeriesUnread"
      @add-to-collection="addToCollection"
      @add-to-readlist="addSeriesBooksToReadList"
      @edit="editMultipleSeries"
      @delete="deleteSeries"
    />

    <multi-select-bar
      v-model="selectedBooks"
      kind="books"
      :oneshots="selectedOneshots"
      @unselect-all="selectedBooks = []"
      @mark-read="markSelectedBooksRead"
      @mark-unread="markSelectedBooksUnread"
      @add-to-readlist="addToReadList"
      @add-to-collection="addOneshotsToCollection"
      @edit="editMultipleBooks"
      @bulk-edit="bulkEditMultipleBooks"
      @delete="deleteBooks"
    />

    <multi-select-bar
      v-model="selectedCollections"
      kind="collections"
      @unselect-all="selectedCollections = []"
      @delete="deleteCollections"
    />

    <multi-select-bar
      v-model="selectedReadLists"
      kind="readlists"
      @unselect-all="selectedReadLists = []"
      @delete="deleteReadLists"
    />

    <v-container fluid>
      <empty-state
        v-if="emptyResults"
        :title="$t('search.no_results')"
        :sub-title="$t('search.search_for_something_else')"
        icon="mdi-magnify"
        icon-color="secondary"
        class="my-4"
      >
      </empty-state>

      <template v-else>
        <!-- 移动端：垂直布局 -->
        <div class="search-controls-mobile" v-if="$vuetify.breakpoint.xs">
          <!-- 库选择器 -->
          <v-select
            v-model="selectedLibrary"
            :items="libraryOptions"
            item-value="id"
            :label="$t('common.library')"
            dense
            outlined
            hide-details
            class="mb-3"
            @change="handleLibraryChange"
            :menu-props="{ maxHeight: 250 }"
            :loading="loading"
            :disabled="loading"
          >
            <template v-slot:item="{ item, on, attrs }">
              <v-list-item
                v-bind="attrs"
                :disabled="item.count === 0 && item.id !== LIBRARIES_ALL"
                v-on="item.count === 0 && item.id !== LIBRARIES_ALL ? {} : on"
              >
                <v-list-item-content>
                  <div class="d-flex align-center">
                    <span :class="{ 'text--disabled': item.count === 0 && item.id !== LIBRARIES_ALL }">{{ item.name }}</span>
                    <v-spacer></v-spacer>
                    <v-chip v-if="item.count > 0" x-small>{{ item.count }}</v-chip>
                    <v-chip v-else-if="item.count === 0" x-small color="grey lighten-2" text-color="grey">{{ item.count }}</v-chip>
                  </div>
                </v-list-item-content>
              </v-list-item>
            </template>
            <template v-slot:selection="{ item }">
              <div class="d-flex align-center">
                <span>{{ item.name }}</span>
                <v-spacer></v-spacer>
                <v-chip v-if="item.count > 0" x-small>{{ item.count }}</v-chip>
                <v-chip v-else-if="item.count === 0" x-small color="grey lighten-2" text-color="grey">{{ item.count }}</v-chip>
              </div>
            </template>
          </v-select>

          <!-- 结果类型选择器 -->
          <v-select
            v-if="hasAnyResults"
            v-model="activeTab"
            :items="resultTypeOptions"
            item-value="value"
            item-text="text"
            :label="$t('search.result_type')"
            dense
            outlined
            hide-details
            class="mb-4"
            @change="handleTabChange"
          >
            <template v-slot:item="{ item }">
              <div class="d-flex align-center">
                <span>{{ item.text }}</span>
                <v-spacer></v-spacer>
                <v-chip v-if="item.count > 0" x-small>{{ item.count }}</v-chip>
                <v-chip v-else-if="item.count === 0" x-small color="grey lighten-2" text-color="grey">{{ item.count }}</v-chip>
              </div>
            </template>
            <template v-slot:selection="{ item }">
              <div class="d-flex align-center">
                <span>{{ item.text }}</span>
                <v-spacer></v-spacer>
                <v-chip v-if="item.count > 0" x-small>{{ item.count }}</v-chip>
                <v-chip v-else-if="item.count === 0" x-small color="grey lighten-2" text-color="grey">{{ item.count }}</v-chip>
              </div>
            </template>
          </v-select>
        </div>

        <!-- 桌面端：水平布局 -->
        <div class="d-flex align-center mb-4" v-else>
          <v-select
            v-model="selectedLibrary"
            :items="libraryOptions"
            item-value="id"
            :label="$t('common.library')"
            dense
            outlined
            hide-details
            class="library-select mr-4"
            style="max-width: 200px;"
            @change="handleLibraryChange"
            :menu-props="{ maxHeight: 300 }"
            :loading="loading"
            :disabled="loading"
          >
            <template v-slot:item="{ item, on, attrs }">
              <v-list-item
                v-bind="attrs"
                :disabled="item.count === 0 && item.id !== LIBRARIES_ALL"
                v-on="item.count === 0 && item.id !== LIBRARIES_ALL ? {} : on"
              >
                <v-list-item-content>
                  <div class="d-flex align-center">
                    <span :class="{ 'text--disabled': item.count === 0 && item.id !== LIBRARIES_ALL }">{{ item.name }}</span>
                    <v-spacer></v-spacer>
                    <v-chip v-if="item.count > 0" small>{{ item.count }}</v-chip>
                    <v-chip v-else-if="item.count === 0" small color="grey lighten-2" text-color="grey">{{ item.count }}</v-chip>
                  </div>
                </v-list-item-content>
              </v-list-item>
            </template>
            <template v-slot:selection="{ item }">
              <div class="d-flex align-center">
                <span>{{ item.name }}</span>
                <v-spacer></v-spacer>
                <v-chip v-if="item.count > 0" small>{{ item.count }}</v-chip>
                <v-chip v-else-if="item.count === 0" small color="grey lighten-2" text-color="grey">{{ item.count }}</v-chip>
              </div>
            </template>
          </v-select>

          <v-spacer></v-spacer>

          <v-tabs v-model="activeTabIndex" class="justify-center flex-grow-0" v-if="hasAnyResults">
            <v-tab v-if="hasSeriesResults" value="series">
              {{ $tc('common.series', 2) }}
              <v-chip v-if="seriesTotalElements > 0" small class="ml-2">{{ seriesTotalElements }}</v-chip>
            </v-tab>
            <v-tab v-if="hasBooksResults" value="books">
              {{ $t('common.books') }}
              <v-chip v-if="booksTotalElements > 0" small class="ml-2">{{ booksTotalElements }}</v-chip>
            </v-tab>
            <v-tab v-if="hasCollectionsResults" value="collections">
              {{ $t('common.collections') }}
              <v-chip v-if="collectionsTotalElements > 0" small class="ml-2">{{ collectionsTotalElements }}</v-chip>
            </v-tab>
            <v-tab v-if="hasReadListsResults" value="readlists">
              {{ $t('common.readlists') }}
              <v-chip v-if="readListsTotalElements > 0" small class="ml-2">{{ readListsTotalElements }}</v-chip>
            </v-tab>
          </v-tabs>

          <v-spacer></v-spacer>
        </div>

        <!-- 当没有结果时显示空状态 -->
        <empty-state
          v-if="!hasAnyResults && !loading"
          :title="$t('search.no_results')"
          :sub-title="selectedLibrary === LIBRARIES_ALL ? $t('search.search_for_something_else') : $t('search.try_another_library')"
          icon="mdi-magnify"
          icon-color="secondary"
          class="my-4"
        >
          <v-btn v-if="selectedLibrary !== LIBRARIES_ALL" @click="selectedLibrary = LIBRARIES_ALL; handleLibraryChange()">
            {{ $t('search.search_all_libraries') }}
          </v-btn>
        </empty-state>

        <v-tabs-items v-model="activeTabIndex">
          <!-- 系列结果 -->
          <v-tab-item v-if="hasSeriesResults">
            <div class="tab-content">
              <v-pagination
                v-if="seriesTotalPages > 1"
                v-model="seriesPage"
                :total-visible="paginationVisible"
                :length="seriesTotalPages"
                class="mb-4"
                @input="handleSeriesPageChange"
              />

              <item-browser :items="loaderSeries.items"
                            :edit-function="isAdmin ? singleEditSeries : undefined"
                            :selected.sync="selectedSeries"
                            :selectable="selectedBooks.length === 0 && selectedCollections.length === 0 && selectedReadLists.length === 0"
                            :fixed-item-width="fixedCardWidth"
              />

              <v-pagination
                v-if="seriesTotalPages > 1"
                v-model="seriesPage"
                :total-visible="paginationVisible"
                :length="seriesTotalPages"
                class="mt-4"
                @input="handleSeriesPageChange"
              />
            </div>
          </v-tab-item>

          <!-- 书籍结果 -->
          <v-tab-item v-if="hasBooksResults">
            <div class="tab-content">
              <v-pagination
                v-if="booksTotalPages > 1"
                v-model="booksPage"
                :total-visible="paginationVisible"
                :length="booksTotalPages"
                class="mb-4"
                @input="handleBooksPageChange"
              />

              <item-browser :items="loaderBooks.items"
                            :item-context="[ItemContext.SHOW_SERIES]"
                            :edit-function="isAdmin ? singleEditBook : undefined"
                            :selected.sync="selectedBooks"
                            :selectable="selectedSeries.length === 0 && selectedCollections.length === 0 && selectedReadLists.length === 0"
                            :fixed-item-width="fixedCardWidth"
              />

              <v-pagination
                v-if="booksTotalPages > 1"
                v-model="booksPage"
                :total-visible="paginationVisible"
                :length="booksTotalPages"
                class="mt-4"
                @input="handleBooksPageChange"
              />
            </div>
          </v-tab-item>

          <!-- 收藏结果 -->
          <v-tab-item v-if="hasCollectionsResults">
            <div class="tab-content">
              <v-pagination
                v-if="collectionsTotalPages > 1"
                v-model="collectionsPage"
                :total-visible="paginationVisible"
                :length="collectionsTotalPages"
                class="mb-4"
                @input="handleCollectionsPageChange"
              />

              <item-browser :items="loaderCollections.items"
                            :edit-function="isAdmin ? singleEditCollection : undefined"
                            :selected.sync="selectedCollections"
                            :selectable="isAdmin && selectedSeries.length === 0 && selectedBooks.length === 0 && selectedReadLists.length === 0"
                            :fixed-item-width="fixedCardWidth"
              />

              <v-pagination
                v-if="collectionsTotalPages > 1"
                v-model="collectionsPage"
                :total-visible="paginationVisible"
                :length="collectionsTotalPages"
                class="mt-4"
                @input="handleCollectionsPageChange"
              />
            </div>
          </v-tab-item>

          <!-- 阅读列表结果 -->
          <v-tab-item v-if="hasReadListsResults">
            <div class="tab-content">
              <v-pagination
                v-if="readListsTotalPages > 1"
                v-model="readListsPage"
                :total-visible="paginationVisible"
                :length="readListsTotalPages"
                class="mb-4"
                @input="handleReadListsPageChange"
              />

              <item-browser :items="loaderReadLists.items"
                            :edit-function="isAdmin ? singleEditReadList : undefined"
                            :selected.sync="selectedReadLists"
                            :selectable="isAdmin && selectedSeries.length === 0 && selectedBooks.length === 0 && selectedCollections.length === 0"
                            :fixed-item-width="fixedCardWidth"
              />

              <v-pagination
                v-if="readListsTotalPages > 1"
                v-model="readListsPage"
                :total-visible="paginationVisible"
                :length="readListsTotalPages"
                class="mt-4"
                @input="handleReadListsPageChange"
              />
            </div>
          </v-tab-item>
        </v-tabs-items>
      </template>
    </v-container>

  </div>
</template>

<script lang="ts">
import MultiSelectBar from '@/components/bars/MultiSelectBar.vue'
import ToolbarSticky from '@/components/bars/ToolbarSticky.vue'
import EmptyState from '@/components/EmptyState.vue'
import ItemBrowser from '@/components/ItemBrowser.vue'
import {BookDto} from '@/types/komga-books'
import {
  BOOK_CHANGED,
  BOOK_DELETED,
  COLLECTION_CHANGED,
  COLLECTION_DELETED,
  LIBRARY_DELETED,
  READLIST_CHANGED,
  READLIST_DELETED,
  READPROGRESS_CHANGED,
  READPROGRESS_DELETED,
  READPROGRESS_SERIES_CHANGED,
  READPROGRESS_SERIES_DELETED,
  SERIES_CHANGED,
  SERIES_DELETED,
} from '@/types/events'
import Vue from 'vue'
import {Oneshot, SeriesDto} from '@/types/komga-series'
import {
  BookSseDto,
  CollectionSseDto,
  ReadListSseDto,
  ReadProgressSeriesSseDto,
  ReadProgressSseDto,
  SeriesSseDto,
} from '@/types/komga-sse'
import {throttle} from 'lodash'
import {PageLoader} from '@/types/pageLoader'
import {ItemContext} from '@/types/items'
import {ReadListDto} from '@/types/komga-readlists'
import {CollectionDto} from '@/types/komga-collections'
import {LIBRARIES_ALL} from '@/types/library'
import {
  BookSearch, SearchConditionAnyOfBook, SearchConditionAnyOfSeries,
  SearchConditionLibraryId, SearchConditionOneShot,
  SearchConditionSeriesId, SearchOperatorIs,
  SearchOperatorIsFalse,
  SeriesSearch,
} from '@/types/komga-search'

export default Vue.extend({
  name: 'SearchView',
  components: {
    EmptyState,
    ToolbarSticky,
    ItemBrowser,
    MultiSelectBar,
  },
  data: () => {
    return {
      ItemContext,
      LIBRARIES_ALL,
      loaderSeries: undefined as unknown as PageLoader<SeriesDto>,
      loaderBooks: undefined as unknown as PageLoader<BookDto>,
      loaderCollections: undefined as unknown as PageLoader<CollectionDto>,
      loaderReadLists: undefined as unknown as PageLoader<ReadListDto>,
      pageSize: 20,
      loading: false,
      selectedSeries: [] as SeriesDto[],
      selectedBooks: [] as BookDto[],
      selectedCollections: [] as CollectionDto[],
      selectedReadLists: [] as ReadListDto[],
      activeTab: 'series',
      seriesPage: 1,
      booksPage: 1,
      collectionsPage: 1,
      readListsPage: 1,
      seriesTotalPages: 1,
      booksTotalPages: 1,
      collectionsTotalPages: 1,
      readListsTotalPages: 1,
      seriesTotalElements: 0,
      booksTotalElements: 0,
      collectionsTotalElements: 0,
      readListsTotalElements: 0,
      selectedLibrary: LIBRARIES_ALL,
      librarySearchCounts: {} as Record<string, number>,
      libraryCache: {} as Record<string, {
        series: { data: SeriesDto[], totalPages: number, totalElements: number },
        books: { data: BookDto[], totalPages: number, totalElements: number },
        collections: { data: CollectionDto[], totalPages: number, totalElements: number },
        readLists: { data: ReadListDto[], totalPages: number, totalElements: number }
      }>,
    }
  },
  created() {
    this.$eventHub.$on(LIBRARY_DELETED, this.reloadResults)
    this.$eventHub.$on(SERIES_CHANGED, this.seriesChanged)
    this.$eventHub.$on(SERIES_DELETED, this.seriesChanged)
    this.$eventHub.$on(BOOK_CHANGED, this.bookChanged)
    this.$eventHub.$on(BOOK_DELETED, this.bookChanged)
    this.$eventHub.$on(COLLECTION_CHANGED, this.collectionChanged)
    this.$eventHub.$on(COLLECTION_DELETED, this.collectionChanged)
    this.$eventHub.$on(READLIST_CHANGED, this.readListChanged)
    this.$eventHub.$on(READLIST_DELETED, this.readListChanged)
    this.$eventHub.$on(READPROGRESS_CHANGED, this.readProgressChanged)
    this.$eventHub.$on(READPROGRESS_DELETED, this.readProgressChanged)
    this.$eventHub.$on(READPROGRESS_SERIES_CHANGED, this.readProgressSeriesChanged)
    this.$eventHub.$on(READPROGRESS_SERIES_DELETED, this.readProgressSeriesChanged)
  },
  beforeDestroy() {
    this.$eventHub.$off(LIBRARY_DELETED, this.reloadResults)
    this.$eventHub.$off(SERIES_CHANGED, this.seriesChanged)
    this.$eventHub.$off(SERIES_DELETED, this.seriesChanged)
    this.$eventHub.$off(BOOK_CHANGED, this.bookChanged)
    this.$eventHub.$off(BOOK_DELETED, this.bookChanged)
    this.$eventHub.$off(COLLECTION_CHANGED, this.collectionChanged)
    this.$eventHub.$off(COLLECTION_DELETED, this.collectionChanged)
    this.$eventHub.$off(READLIST_CHANGED, this.readListChanged)
    this.$eventHub.$off(READLIST_DELETED, this.readListChanged)
    this.$eventHub.$off(READPROGRESS_CHANGED, this.readProgressChanged)
    this.$eventHub.$off(READPROGRESS_DELETED, this.readProgressChanged)
    this.$eventHub.$off(READPROGRESS_SERIES_CHANGED, this.readProgressSeriesChanged)
    this.$eventHub.$off(READPROGRESS_SERIES_DELETED, this.readProgressSeriesChanged)
  },
  watch: {
    '$route.query.q': {
      handler: function (val) {
        // 先重置库选择，确保后续操作使用正确的库过滤条件
        this.selectedLibrary = LIBRARIES_ALL
        this.librarySearchCounts = {}
        this.libraryCache = {}

        // 重置所有其他状态
        this.selectedBooks = []
        this.selectedSeries = []
        this.selectedCollections = []
        this.selectedReadLists = []
        this.activeTab = 'series'
        this.seriesPage = 1
        this.booksPage = 1
        this.collectionsPage = 1
        this.readListsPage = 1
        this.seriesTotalElements = 0
        this.booksTotalElements = 0
        this.collectionsTotalElements = 0
        this.readListsTotalElements = 0
        this.seriesTotalPages = 1
        this.booksTotalPages = 1
        this.collectionsTotalPages = 1
        this.readListsTotalPages = 1

        // 现在 setupLoaders 会使用正确的库过滤条件
        this.setupLoaders(val)
        this.loadResults(val)
      },
      deep: true,
      immediate: true,
    },
  },
  computed: {
    isAdmin(): boolean {
      return this.$store.getters.meAdmin
    },
    fixedCardWidth(): number {
      return this.$vuetify.breakpoint.xs ? 120 : 150
    },
    showToolbar(): boolean {
      return this.selectedSeries.length === 0 && this.selectedBooks.length === 0 && this.selectedCollections.length === 0 && this.selectedReadLists.length === 0
    },
    emptyResults(): boolean {
      return !this.loading &&
        this.loaderSeries?.items.length === 0 &&
        this.loaderBooks?.items.length === 0 &&
        this.loaderCollections?.items.length === 0 &&
        this.loaderReadLists?.items.length === 0
    },
    selectedOneshots(): boolean {
      return this.selectedBooks.every(b => b.oneshot)
    },
    hasSeriesResults(): boolean {
      return this.loaderSeries?.items.length > 0
    },
    hasBooksResults(): boolean {
      return this.loaderBooks?.items.length > 0
    },
    hasCollectionsResults(): boolean {
      return this.loaderCollections?.items.length > 0
    },
    hasReadListsResults(): boolean {
      return this.loaderReadLists?.items.length > 0
    },
    hasAnyResults(): boolean {
      return this.hasSeriesResults || this.hasBooksResults || this.hasCollectionsResults || this.hasReadListsResults
    },
    paginationVisible(): number {
      switch (this.$vuetify.breakpoint.name) {
        case 'xs':
          return 3
        case 'sm':
          return 5
        case 'md':
          return 10
        case 'lg':
        case 'xl':
        default:
          return 15
      }
    },
    libraryOptions(): any[] {
      const allCount = Object.values(this.librarySearchCounts).reduce((sum: number, count: number) => sum + count, 0)
      const allOption = {
        id: LIBRARIES_ALL,
        name: this.$t('common.all_libraries').toString(),
        count: allCount,
      }

      const libraryOptions = this.$store.getters.getLibraries.map((library: any) => ({
        id: library.id,
        name: library.name,
        count: this.librarySearchCounts[library.id] || 0,
      }))

      // 确保至少有一个选项可用，全库选项始终可用
      return [allOption, ...libraryOptions]
    },
    activeTabIndex: {
      get(): number {
        const tabOrder = ['series', 'books', 'collections', 'readlists']
        const visibleTabs = tabOrder.filter(tab => {
          switch (tab) {
            case 'series': return this.hasSeriesResults
            case 'books': return this.hasBooksResults
            case 'collections': return this.hasCollectionsResults
            case 'readlists': return this.hasReadListsResults
            default: return false
          }
        })
        return visibleTabs.indexOf(this.activeTab)
      },
      set(value: number) {
        const tabOrder = ['series', 'books', 'collections', 'readlists']
        const visibleTabs = tabOrder.filter(tab => {
          switch (tab) {
            case 'series': return this.hasSeriesResults
            case 'books': return this.hasBooksResults
            case 'collections': return this.hasCollectionsResults
            case 'readlists': return this.hasReadListsResults
            default: return false
          }
        })
        if (value >= 0 && value < visibleTabs.length) {
          this.activeTab = visibleTabs[value]
        }
      },
    },
    resultTypeOptions(): any[] {
      const options = []
      if (this.hasSeriesResults) {
        options.push({
          value: 'series',
          text: this.$tc('common.series', 2),
          count: this.seriesTotalElements,
        })
      }
      if (this.hasBooksResults) {
        options.push({
          value: 'books',
          text: this.$t('common.books'),
          count: this.booksTotalElements,
        })
      }
      if (this.hasCollectionsResults) {
        options.push({
          value: 'collections',
          text: this.$t('common.collections'),
          count: this.collectionsTotalElements,
        })
      }
      if (this.hasReadListsResults) {
        options.push({
          value: 'readlists',
          text: this.$t('common.readlists'),
          count: this.readListsTotalElements,
        })
      }
      return options
    },
  },
  methods: {
    async scrollChanged(loader: PageLoader<any>, percent: number) {
      if (percent > 0.95) await loader.loadNext()
    },
    seriesChanged(event: SeriesSseDto) {
      if (this.loaderSeries?.items.some(x => x.id === event.seriesId)) {
        this.reloadResults()
      }
    },
    bookChanged(event: BookSseDto) {
      if (this.loaderBooks?.items.some(x => x.id === event.bookId)) {
        this.reloadResults()
      }
    },
    readProgressChanged(event: ReadProgressSseDto) {
      if (this.loaderBooks?.items.some(x => x.id === event.bookId)) {
        this.reloadResults()
      }
    },
    readProgressSeriesChanged(event: ReadProgressSeriesSseDto) {
      if (this.loaderSeries?.items.some(x => x.id === event.seriesId)) {
        this.reloadResults()
      }
    },
    collectionChanged(event: CollectionSseDto) {
      if (this.loaderCollections?.items.some(x => x.id === event.collectionId)) {
        this.reloadResults()
      }
    },
    readListChanged(event: ReadListSseDto) {
      if (this.loaderReadLists?.items.some(x => x.id === event.readListId)) {
        this.reloadResults()
      }
    },
    async singleEditSeries(series: SeriesDto) {
      if (series.oneshot) {
        const book = (await this.$komgaBooks.getBooksList({
          condition: new SearchConditionSeriesId(new SearchOperatorIs(series.id)),
        } as BookSearch)).content[0]
        this.$store.dispatch('dialogUpdateOneshots', {series: series, book: book})
      } else
        this.$store.dispatch('dialogUpdateSeries', series)
    },
    async singleEditBook(book: BookDto) {
      if (book.oneshot) {
        const series = (await this.$komgaSeries.getOneSeries(book.seriesId))
        this.$store.dispatch('dialogUpdateOneshots', {series: series, book: book})
      } else
        this.$store.dispatch('dialogUpdateBooks', book)
    },
    singleEditCollection(collection: CollectionDto) {
      this.$store.dispatch('dialogEditCollection', collection)
    },
    singleEditReadList(readList: ReadListDto) {
      this.$store.dispatch('dialogEditReadList', readList)
    },
    async markSelectedSeriesRead() {
      await Promise.all(this.selectedSeries.map(s =>
        this.$komgaSeries.markAsRead(s.id),
      ))
      this.selectedSeries = await Promise.all(this.selectedSeries.map(s =>
        this.$komgaSeries.getOneSeries(s.id),
      ))
    },
    async markSelectedSeriesUnread() {
      await Promise.all(this.selectedSeries.map(s =>
        this.$komgaSeries.markAsUnread(s.id),
      ))
      this.selectedSeries = await Promise.all(this.selectedSeries.map(s =>
        this.$komgaSeries.getOneSeries(s.id),
      ))
    },
    addToCollection() {
      this.$store.dispatch('dialogAddSeriesToCollection', this.selectedSeries.map(s => s.id))
    },
    addToReadList() {
      this.$store.dispatch('dialogAddBooksToReadList', this.selectedBooks.map(b => b.id))
    },
    async addSeriesBooksToReadList() {
      const conditions = this.selectedSeries.map(s => new SearchConditionSeriesId(new SearchOperatorIs(s.id)))
      const books = await this.$komgaBooks.getBooksList({
        condition: new SearchConditionAnyOfBook(conditions),
      } as BookSearch, {unpaged: true})
      this.$store.dispatch('dialogAddBooksToReadList', books.content.map(b => b.id))
    },
    addOneshotsToCollection() {
      this.$store.dispatch('dialogAddSeriesToCollection', this.selectedBooks.map(b => b.seriesId))
    },
    async editMultipleSeries() {
      if (this.selectedSeries.every(s => s.oneshot)) {
        const books = await Promise.all(this.selectedSeries.map(s => this.$komgaBooks.getBooksList({
          condition: new SearchConditionSeriesId(new SearchOperatorIs(s.id)),
        } as BookSearch)))
        const oneshots = this.selectedSeries.map((s, index) => ({series: s, book: books[index].content[0]} as Oneshot))
        this.$store.dispatch('dialogUpdateOneshots', oneshots)
      } else
        this.$store.dispatch('dialogUpdateSeries', this.selectedSeries)
    },
    async editMultipleBooks() {
      if (this.selectedBooks.every(b => b.oneshot)) {
        const series = await Promise.all(this.selectedBooks.map(b => this.$komgaSeries.getOneSeries(b.seriesId)))
        const oneshots = this.selectedBooks.map((b, index) => ({series: series[index], book: b} as Oneshot))
        this.$store.dispatch('dialogUpdateOneshots', oneshots)
      } else
        this.$store.dispatch('dialogUpdateBooks', this.selectedBooks)
    },
    bulkEditMultipleBooks() {
      this.$store.dispatch('dialogUpdateBulkBooks', this.selectedBooks)
    },
    deleteSeries() {
      this.$store.dispatch('dialogDeleteSeries', this.selectedSeries)
    },
    deleteBooks() {
      this.$store.dispatch('dialogDeleteBook', this.selectedBooks)
    },
    deleteCollections() {
      this.$store.dispatch('dialogDeleteCollection', this.selectedCollections)
    },
    deleteReadLists() {
      this.$store.dispatch('dialogDeleteReadList', this.selectedReadLists)
    },
    async markSelectedBooksRead() {
      await Promise.all(this.selectedBooks.map(b =>
        this.$komgaBooks.updateReadProgress(b.id, {completed: true}),
      ))
    },
    async markSelectedBooksUnread() {
      await Promise.all(this.selectedBooks.map(b =>
        this.$komgaBooks.deleteReadProgress(b.id),
      ))
    },
    reloadResults: throttle(function (this: any) {
      this.loadResults(this.$route.query.q.toString(), true)
    }, 500),
    setupLoaders(search: string) {
      if (search) {
        // 构建库过滤条件
        const libraryCondition = this.selectedLibrary !== LIBRARIES_ALL
          ? new SearchConditionLibraryId(new SearchOperatorIs(this.selectedLibrary))
          : undefined

        this.loaderSeries = new PageLoader<SeriesDto>({size: this.pageSize}, (pageable: PageRequest) => {
          const searchParams: SeriesSearch = { fullTextSearch: search }
          if (libraryCondition) {
            searchParams.condition = libraryCondition
          }
          return this.$komgaSeries.getSeriesList(searchParams, pageable)
        })

        this.loaderBooks = new PageLoader<BookDto>({size: this.pageSize}, (pageable: PageRequest) => {
          const searchParams: BookSearch = { fullTextSearch: search }
          if (libraryCondition) {
            searchParams.condition = libraryCondition
          }
          return this.$komgaBooks.getBooksList(searchParams, pageable)
        })

        this.loaderCollections = new PageLoader<CollectionDto>({size: this.pageSize}, (pageable: PageRequest) => {
          const libraryIds = this.selectedLibrary !== LIBRARIES_ALL ? [this.selectedLibrary] : undefined
          return this.$komgaCollections.getCollections(libraryIds, pageable, search)
        })

        this.loaderReadLists = new PageLoader<ReadListDto>({size: this.pageSize}, (pageable: PageRequest) => {
          const libraryIds = this.selectedLibrary !== LIBRARIES_ALL ? [this.selectedLibrary] : undefined
          return this.$komgaReadLists.getReadLists(libraryIds, pageable, search)
        })
      } else {
        this.loaderSeries = null as unknown as PageLoader<SeriesDto>
        this.loaderBooks = null as unknown as PageLoader<BookDto>
        this.loaderCollections = null as unknown as PageLoader<CollectionDto>
        this.loaderReadLists = null as unknown as PageLoader<ReadListDto>
      }
    },
    async loadResults(search: string, reload: boolean = false, fetchCounts: boolean = true) {
      this.selectedBooks = []
      this.selectedSeries = []
      this.selectedCollections = []
      this.selectedReadLists = []

      if (search) {
        this.loading = true

        try {
          // 获取库搜索统计（如果需要）
          const promises = [
            ...(fetchCounts ? [this.fetchLibrarySearchCounts(search)] : []),
          ].filter(Boolean)

          // 添加数据加载逻辑
          if (reload) {
            // 重新加载模式
            promises.push(
              ...(this.loaderSeries ? [this.loaderSeries.reload()] : []),
              ...(this.loaderBooks ? [this.loaderBooks.reload()] : []),
              ...(this.loaderCollections ? [this.loaderCollections.reload()] : []),
              ...(this.loaderReadLists ? [this.loaderReadLists.reload()] : []),
            )
          } else {
            // 首次加载或分页加载模式
            if (this.libraryCache[this.selectedLibrary]) {
              // 有缓存数据，直接使用缓存替换loader数据
              // 使用 Promise.resolve 包装，确保返回 Promise
              promises.push(Promise.resolve().then(() => this.useCachedData(this.selectedLibrary)))
            } else {
              // 没有缓存，从网络加载
              promises.push(
                ...(this.loaderSeries ? [this.loaderSeries.loadNext()] : []),
                ...(this.loaderBooks ? [this.loaderBooks.loadNext()] : []),
                ...(this.loaderCollections ? [this.loaderCollections.loadNext()] : []),
                ...(this.loaderReadLists ? [this.loaderReadLists.loadNext()] : []),
              )
            }
          }

          await Promise.all(promises)
          this.loading = false
          this.updateTotalPages()
        } catch (error) {
          this.loading = false
          // 即使出错也要更新UI状态
          this.updateTotalPages()
        }
      }
    },
    updateTotalPages() {
      // 如果有缓存数据，使用缓存的分页信息
      if (this.libraryCache[this.selectedLibrary]) {
        const cache = this.libraryCache[this.selectedLibrary]
        this.seriesTotalPages = cache.series.totalPages
        this.booksTotalPages = cache.books.totalPages
        this.collectionsTotalPages = cache.collections.totalPages
        this.readListsTotalPages = cache.readLists.totalPages

        this.seriesTotalElements = cache.series.totalElements
        this.booksTotalElements = cache.books.totalElements
        this.collectionsTotalElements = cache.collections.totalElements
        this.readListsTotalElements = cache.readLists.totalElements
      } else {
        // 否则使用loader的分页信息
        this.seriesTotalPages = this.loaderSeries?.currentPageData?.totalPages || 1
        this.booksTotalPages = this.loaderBooks?.currentPageData?.totalPages || 1
        this.collectionsTotalPages = this.loaderCollections?.currentPageData?.totalPages || 1
        this.readListsTotalPages = this.loaderReadLists?.currentPageData?.totalPages || 1

        this.seriesTotalElements = this.loaderSeries?.currentPageData?.totalElements || 0
        this.booksTotalElements = this.loaderBooks?.currentPageData?.totalElements || 0
        this.collectionsTotalElements = this.loaderCollections?.currentPageData?.totalElements || 0
        this.readListsTotalElements = this.loaderReadLists?.currentPageData?.totalElements || 0
      }

      // 等异步加载完成后设置 activeTab
      if (!this.hasAnyResults) {
        this.activeTab = 'series'
      } else {
        if (this.hasSeriesResults) {
          this.activeTab = 'series'
        } else if (this.hasBooksResults) {
          this.activeTab = 'books'
        } else if (this.hasCollectionsResults) {
          this.activeTab = 'collections'
        } else if (this.hasReadListsResults) {
          this.activeTab = 'readlists'
        }
      }
    },
    async handleSeriesPageChange(page: number) {
      this.seriesPage = page
      await this.loadSeriesPage(page)
    },
    async handleBooksPageChange(page: number) {
      this.booksPage = page
      await this.loadBooksPage(page)
    },
    async handleCollectionsPageChange(page: number) {
      this.collectionsPage = page
      await this.loadCollectionsPage(page)
    },
    async handleReadListsPageChange(page: number) {
      this.readListsPage = page
      await this.loadReadListsPage(page)
    },
    handleLibraryChange() {
      // 当库选择改变时，重新设置loader的过滤条件并加载数据
      const search = this.$route.query.q.toString()
      if (search) {
        try {
          // 重置分页状态
          this.seriesPage = 1
          this.booksPage = 1
          this.collectionsPage = 1
          this.readListsPage = 1

          // 重置计数状态
          this.seriesTotalElements = 0
          this.booksTotalElements = 0
          this.collectionsTotalElements = 0
          this.readListsTotalElements = 0
          this.seriesTotalPages = 1
          this.booksTotalPages = 1
          this.collectionsTotalPages = 1
          this.readListsTotalPages = 1

          // 重新设置loader（使用新的库过滤条件）
          this.setupLoaders(search)

          // 加载新的搜索结果（需要获取库搜索统计以更新标签页计数）
          this.loadResults(search, false, false)
        } catch (error) {
          // 即使出错也要确保UI状态正确
          this.loading = false
        }
      }
    },
    handleTabChange(newTab: string) {
      // 移动端标签页切换处理
      this.activeTab = newTab
    },
    async fetchLibrarySearchCounts(search: string) {
      if (!search) {
        this.librarySearchCounts = {}
        this.libraryCache = {}
        return
      }

      try {
        const libraries = this.$store.getters.getLibraries
        const counts: Record<string, number> = {}
        const cache: Record<string, any> = {}

        // 并行获取所有库的搜索结果统计和第一页数据
        const promises = libraries.map(async (library: any) => {
          try {
            const [seriesData, booksData, collectionsData, readListsData] = await Promise.all([
              this.$komgaSeries.getSeriesList({
                fullTextSearch: search,
                condition: new SearchConditionLibraryId(new SearchOperatorIs(library.id)),
              }, { page: 0, size: this.pageSize }).catch(() => ({ content: [], totalElements: 0, totalPages: 1 })),
              this.$komgaBooks.getBooksList({
                fullTextSearch: search,
                condition: new SearchConditionLibraryId(new SearchOperatorIs(library.id)),
              }, { page: 0, size: this.pageSize }).catch(() => ({ content: [], totalElements: 0, totalPages: 1 })),
              this.$komgaCollections.getCollections([library.id], { page: 0, size: this.pageSize }, search).catch(() => ({ content: [], totalElements: 0, totalPages: 1 })),
              this.$komgaReadLists.getReadLists([library.id], { page: 0, size: this.pageSize }, search).catch(() => ({ content: [], totalElements: 0, totalPages: 1 })),
            ])

            const totalCount = (seriesData.totalElements || 0) + (booksData.totalElements || 0) +
                              (collectionsData.totalElements || 0) + (readListsData.totalElements || 0)
            counts[library.id] = totalCount

            // 缓存第一页数据
            cache[library.id] = {
              series: {
                data: seriesData.content || [],
                totalPages: seriesData.totalPages || 1,
                totalElements: seriesData.totalElements || 0,
              },
              books: {
                data: booksData.content || [],
                totalPages: booksData.totalPages || 1,
                totalElements: booksData.totalElements || 0,
              },
              collections: {
                data: collectionsData.content || [],
                totalPages: collectionsData.totalPages || 1,
                totalElements: collectionsData.totalElements || 0,
              },
              readLists: {
                data: readListsData.content || [],
                totalPages: readListsData.totalPages || 1,
                totalElements: readListsData.totalElements || 0,
              },
            }
          } catch (error) {
            // eslint-disable-next-line no-console
            console.warn(`Failed to fetch search counts for library ${library.id}:`, error)
            counts[library.id] = 0
            cache[library.id] = {
              series: { data: [], totalPages: 1, totalElements: 0 },
              books: { data: [], totalPages: 1, totalElements: 0 },
              collections: { data: [], totalPages: 1, totalElements: 0 },
              readLists: { data: [], totalPages: 1, totalElements: 0 },
            }
          }
        })

        await Promise.all(promises)
        this.librarySearchCounts = counts
        this.libraryCache = cache

        // 同时缓存全库的数据（所有库的总和）
        const allLibrariesData = await Promise.all([
          this.$komgaSeries.getSeriesList({ fullTextSearch: search }, { page: 0, size: this.pageSize }).catch(() => ({ content: [], totalElements: 0, totalPages: 1 })),
          this.$komgaBooks.getBooksList({ fullTextSearch: search }, { page: 0, size: this.pageSize }).catch(() => ({ content: [], totalElements: 0, totalPages: 1 })),
          this.$komgaCollections.getCollections(undefined, { page: 0, size: this.pageSize }, search).catch(() => ({ content: [], totalElements: 0, totalPages: 1 })),
          this.$komgaReadLists.getReadLists(undefined, { page: 0, size: this.pageSize }, search).catch(() => ({ content: [], totalElements: 0, totalPages: 1 })),
        ])

        this.libraryCache[LIBRARIES_ALL] = {
          series: {
            data: allLibrariesData[0].content || [],
            totalPages: allLibrariesData[0].totalPages || 1,
            totalElements: allLibrariesData[0].totalElements || 0,
          },
          books: {
            data: allLibrariesData[1].content || [],
            totalPages: allLibrariesData[1].totalPages || 1,
            totalElements: allLibrariesData[1].totalElements || 0,
          },
          collections: {
            data: allLibrariesData[2].content || [],
            totalPages: allLibrariesData[2].totalPages || 1,
            totalElements: allLibrariesData[2].totalElements || 0,
          },
          readLists: {
            data: allLibrariesData[3].content || [],
            totalPages: allLibrariesData[3].totalPages || 1,
            totalElements: allLibrariesData[3].totalElements || 0,
          },
        }
      } catch (error) {
        // 即使出错也要设置一个空对象，确保UI不会崩溃
        this.librarySearchCounts = {}
        this.libraryCache = {}
      }
    },
    async loadSeriesPage(page: number) {
      const pageable = { page: page - 1, size: this.pageSize }
      const search = this.$route.query.q.toString()
      const searchParams: SeriesSearch = { fullTextSearch: search }
      if (this.selectedLibrary !== LIBRARIES_ALL) {
        searchParams.condition = new SearchConditionLibraryId(new SearchOperatorIs(this.selectedLibrary))
      }
      const pageData = await this.$komgaSeries.getSeriesList(searchParams, pageable)
      this.loaderSeries.items.splice(0, this.loaderSeries.items.length, ...pageData.content)
    },
    async loadBooksPage(page: number) {
      const pageable = { page: page - 1, size: this.pageSize }
      const search = this.$route.query.q.toString()
      const searchParams: BookSearch = { fullTextSearch: search }
      if (this.selectedLibrary !== LIBRARIES_ALL) {
        searchParams.condition = new SearchConditionLibraryId(new SearchOperatorIs(this.selectedLibrary))
      }
      const pageData = await this.$komgaBooks.getBooksList(searchParams, pageable)
      this.loaderBooks.items.splice(0, this.loaderBooks.items.length, ...pageData.content)
    },
    async loadCollectionsPage(page: number) {
      const pageable = { page: page - 1, size: this.pageSize }
      const search = this.$route.query.q.toString()
      const libraryIds = this.selectedLibrary !== LIBRARIES_ALL ? [this.selectedLibrary] : undefined
      const pageData = await this.$komgaCollections.getCollections(libraryIds, pageable, search)
      this.loaderCollections.items.splice(0, this.loaderCollections.items.length, ...pageData.content)
    },
    async loadReadListsPage(page: number) {
      const pageable = { page: page - 1, size: this.pageSize }
      const search = this.$route.query.q.toString()
      const libraryIds = this.selectedLibrary !== LIBRARIES_ALL ? [this.selectedLibrary] : undefined
      const pageData = await this.$komgaReadLists.getReadLists(libraryIds, pageable, search)
      this.loaderReadLists.items.splice(0, this.loaderReadLists.items.length, ...pageData.content)
    },
    useCachedData(libraryId: string) {
      // 使用缓存数据更新UI
      const cache = this.libraryCache[libraryId]
      if (cache) {
        // 更新分页信息
        this.seriesTotalPages = cache.series.totalPages
        this.booksTotalPages = cache.books.totalPages
        this.collectionsTotalPages = cache.collections.totalPages
        this.readListsTotalPages = cache.readLists.totalPages

        this.seriesTotalElements = cache.series.totalElements
        this.booksTotalElements = cache.books.totalElements
        this.collectionsTotalElements = cache.collections.totalElements
        this.readListsTotalElements = cache.readLists.totalElements

        // 更新loader数据，清理不再需要的内容
        if (this.loaderSeries) {
          if (cache.series.data.length > 0) {
            this.loaderSeries.items.splice(0, this.loaderSeries.items.length, ...cache.series.data)
          } else {
            // 如果没有数据，清理loader
            this.loaderSeries.items.splice(0, this.loaderSeries.items.length)
          }
        }
        if (this.loaderBooks) {
          if (cache.books.data.length > 0) {
            this.loaderBooks.items.splice(0, this.loaderBooks.items.length, ...cache.books.data)
          } else {
            // 如果没有数据，清理loader
            this.loaderBooks.items.splice(0, this.loaderBooks.items.length)
          }
        }
        if (this.loaderCollections) {
          if (cache.collections.data.length > 0) {
            this.loaderCollections.items.splice(0, this.loaderCollections.items.length, ...cache.collections.data)
          } else {
            // 如果没有数据，清理loader
            this.loaderCollections.items.splice(0, this.loaderCollections.items.length)
          }
        }
        if (this.loaderReadLists) {
          if (cache.readLists.data.length > 0) {
            this.loaderReadLists.items.splice(0, this.loaderReadLists.items.length, ...cache.readLists.data)
          } else {
            // 如果没有数据，清理loader
            this.loaderReadLists.items.splice(0, this.loaderReadLists.items.length)
          }
        }

        // 设置活动标签页（优先选择第一个有数据的标签页）
        this.$nextTick(() => {
          if (cache.series.data.length > 0) {
            this.activeTab = 'series'
          } else if (cache.books.data.length > 0) {
            this.activeTab = 'books'
          } else if (cache.collections.data.length > 0) {
            this.activeTab = 'collections'
          } else if (cache.readLists.data.length > 0) {
            this.activeTab = 'readlists'
          } else {
            // 如果都没有数据，默认选择系列标签页
            this.activeTab = 'series'
          }
        })
      }
    },
  },
})
</script>
<style scoped>
.tab-content {
  padding: 16px 0;
}

/* 深色主题优化 */
.theme--dark .tab-content {
  background-color: var(--v-contrast-1);
  border-radius: 4px;
  margin: 8px 0;
}

.theme--dark .v-tabs {
  background-color: var(--v-contrast-1);
  border-radius: 4px 4px 0 0;
  margin-bottom: 0;
  border-bottom: 1px solid var(--v-contrast-light-2);
}

.theme--dark .v-tabs .v-tab {
  background-color: transparent;
  color: var(--v-contrast-light-2);
  border-radius: 4px 4px 0 0;
}

.theme--dark .v-tabs .v-tab:hover {
  background-color: rgba(255, 255, 255, 0.05);
}

.theme--dark .v-tabs .v-tab--active {
  background-color: var(--v-secondary);
  color: var(--v-base);
  border-bottom: 2px solid var(--v-secondary);
}

.theme--dark .v-tabs-items {
  background-color: var(--v-contrast-1);
  border-radius: 0 0 4px 4px;
  margin-top: 0;
}

.theme--dark .v-pagination {
  background-color: transparent;
}

.theme--dark .v-select__selections {
  color: var(--v-contrast-light-2);
}

.theme--dark .v-chip {
  background-color: var(--v-secondary);
  color: var(--v-base);
  border: 1px solid var(--v-secondary);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
}

.theme--dark .v-chip.v-chip--outlined {
  background-color: transparent;
  color: var(--v-secondary);
  border-color: var(--v-secondary);
}

.theme--dark .v-chip:hover {
  background-color: var(--v-secondary);
  color: var(--v-base);
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.4);
}

/* 针对计数为0的芯片使用不同的样式 */
.theme--dark .v-chip[color="grey lighten-2"] {
  background-color: var(--v-contrast-light-2) !important;
  color: var(--v-base) !important;
  border-color: var(--v-contrast-light-2);
  opacity: 0.7;
}

.theme--dark .v-chip[color="grey lighten-2"]:hover {
  background-color: var(--v-contrast-light-2) !important;
  opacity: 1;
}

/* 确保小尺寸芯片有足够的对比度 */
.theme--dark .v-chip.v-size--small {
  font-weight: 500;
  letter-spacing: 0.5px;
}

/* 明亮主题保持原有样式 */
.theme--light .v-tabs {
  justify-content: center;
}

.v-tab {
  min-width: auto;
  padding: 0 16px;
}

.v-chip {
  font-size: 0.75rem;
  height: 20px;
}

.library-select {
  flex-shrink: 0;
}

/* 移动端样式优化 */
.search-controls-mobile {
  margin-bottom: 16px;
}

.search-controls-mobile .v-select {
  width: 100%;
}

/* 移动端分页组件样式 */
@media (max-width: 600px) {
  .v-pagination {
    flex-wrap: wrap;
    justify-content: center;
  }

  .v-pagination .v-pagination__item {
    margin: 2px;
    min-width: 32px;
    height: 32px;
  }

  .v-pagination .v-pagination__navigation {
    margin: 2px;
    min-width: 32px;
    height: 32px;
  }
}

/* 移动端卡片布局优化 */
@media (max-width: 600px) {
  .tab-content {
    padding: 8px 0;
  }

  /* 减少移动端卡片间距 */
  .v-item-group .my-2 {
    margin-top: 8px !important;
    margin-bottom: 8px !important;
  }

  .v-item-group .mx-2 {
    margin-left: 8px !important;
    margin-right: 8px !important;
  }
}

/* 移动端标签页样式 */
@media (max-width: 600px) {
  .v-tab {
    min-width: auto;
    padding: 0 8px;
    font-size: 0.875rem;
  }

  .v-tab .v-chip {
    font-size: 0.75rem;
    height: 18px;
  }
}

/* 移动端深色主题优化 */
@media (max-width: 600px) {
  .theme--dark .search-controls-mobile {
    background-color: var(--v-base);
    border-radius: 4px;
    padding: 12px;
    margin: 8px 0;
  }

  .theme--dark .v-select.v-text-field--outlined .v-input__control .v-input__slot {
    background-color: var(--v-contrast-1);
    border-color: var(--v-contrast-light-2);
  }

  .theme--dark .v-select .v-select__selection {
    color: var(--v-contrast-light-2);
  }

  /* 移动端分页组件深色主题 */
  .theme--dark .v-pagination .v-pagination__item {
    background-color: var(--v-contrast-1);
    color: var(--v-contrast-light-2);
    border: 1px solid var(--v-contrast-light-2);
  }

  .theme--dark .v-pagination .v-pagination__item--active {
    background-color: var(--v-secondary);
    color: var(--v-base);
    border-color: var(--v-secondary);
  }

  .theme--dark .v-pagination .v-pagination__navigation {
    background-color: var(--v-contrast-1);
    color: var(--v-contrast-light-2);
    border: 1px solid var(--v-contrast-light-2);
  }
}

/* 移动端触摸优化 */
@media (max-width: 600px) {
  .v-tab {
    min-height: 44px;
  }

  .v-select .v-input__control {
    min-height: 44px;
  }

  .v-pagination .v-pagination__item,
  .v-pagination .v-pagination__navigation {
    min-width: 44px;
    min-height: 44px;
  }
}

/* 深色主题下的下拉菜单样式 */
.theme--dark .v-menu__content {
  background-color: var(--v-contrast-1);
  border: 1px solid var(--v-contrast-light-2);
}

.theme--dark .v-list-item {
  color: var(--v-contrast-light-2);
}

.theme--dark .v-list-item:hover {
  background-color: var(--v-contrast-light-2);
  color: var(--v-base);
}

/* 深色主题下的分页组件样式 */
.theme--dark .v-pagination .v-pagination__item {
  background-color: transparent;
  color: var(--v-contrast-light-2);
}

.theme--dark .v-pagination .v-pagination__item--active {
  background-color: var(--v-secondary);
  color: var(--v-base);
}

/* 深色主题下的标签页样式 */
.theme--dark .v-tab {
  color: var(--v-contrast-light-2);
}

.theme--dark .v-tab--active {
  background-color: var(--v-secondary);
  color: var(--v-base);
}

.theme--dark .v-tab:hover {
  background-color: rgba(255, 255, 255, 0.1);
}

/* 深色主题下的选择器样式 */
.theme--dark .v-select .v-input__slot {
  background-color: var(--v-contrast-1);
  border-color: var(--v-contrast-light-2);
}

.theme--dark .v-select.v-text-field--outlined .v-input__control .v-input__slot {
  background-color: var(--v-contrast-1);
}

.theme--dark .v-select .v-select__selection {
  color: var(--v-contrast-light-2);
}

/* 深色主题下的空状态样式 */
.theme--dark .empty-state {
  background-color: var(--v-contrast-1);
  border-radius: 4px;
  padding: 16px;
}

/* 深色主题下的工具栏样式 */
.theme--dark .v-toolbar {
  background-color: var(--v-contrast-1);
  color: var(--v-contrast-light-2);
}

/* 深色主题下的多选栏样式 */
.theme--dark .multi-select-bar {
  background-color: var(--v-contrast-1);
  border-bottom: 1px solid var(--v-contrast-light-2);
}
</style>
