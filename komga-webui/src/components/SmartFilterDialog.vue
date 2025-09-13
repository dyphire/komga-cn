<template>
  <v-dialog
    v-model="dialog"
    :max-width="$vuetify.breakpoint.xs ? '95vw' : '600px'"
    :width="$vuetify.breakpoint.xs ? '95vw' : '600px'"
    persistent
    :fullscreen="$vuetify.breakpoint.xs"
  >
    <v-card>
      <v-card-title class="headline">
        <v-icon left>mdi-filter-variant</v-icon>
        {{ $t('filter.smart_filter') }}
        <v-spacer />
        <v-btn icon @click="dialog = false">
          <v-icon>mdi-close</v-icon>
        </v-btn>
      </v-card-title>

      <v-card-text>
        <!-- Mode Toggle -->
        <v-tabs v-model="localMode" class="mb-4">
          <v-tab>{{ $t('filter.text_mode') }}</v-tab>
          <v-tab>{{ $t('filter.builder_mode') }}</v-tab>
        </v-tabs>

        <v-tabs-items v-model="localMode">
          <!-- Text Mode -->
          <v-tab-item>
            <v-text-field
              v-model="query"
              :label="$t('filter.smart_filter_placeholder')"
              :hint="$t('filter.smart_filter_hint')"
              persistent-hint
              outlined
              dense
              @input="onQueryInput"
              @keyup.enter="applyFilter"
            >
              <template v-slot:append>
                <v-btn
                  v-if="query.trim()"
                  icon
                  small
                  @click="query = ''"
                >
                  <v-icon small>mdi-close</v-icon>
                </v-btn>
              </template>
            </v-text-field>

            <v-divider class="my-4" />

            <div class="smart-filter-examples">
              <div class="examples-title">
                <v-icon class="examples-title-icon">mdi-lightbulb-on</v-icon>
                {{ $t('filter.smart_filter_examples') }}
              </div>

              <template v-if="$props.mode === 'series'">
                <div class="example-item">
                  <v-icon class="example-icon" small>mdi-account</v-icon>
                  <code class="mr-2">author:John</code>
                  <span class="example-description">{{ $t('filter.example_author') }}</span>
                </div>
                <div class="example-item">
                  <v-icon class="example-icon" small>mdi-tag</v-icon>
                  <code class="mr-2">tag:action</code>
                  <span class="example-description">{{ $t('filter.example_tag') }}</span>
                </div>
                <div class="example-item">
                  <v-icon class="example-icon" small>mdi-close-circle</v-icon>
                  <code class="mr-2">NOT tag:mystery</code>
                  <span class="example-description">{{ $t('filter.example_not') }}</span>
                </div>
                <div class="example-item">
                  <v-icon class="example-icon" small>mdi-movie</v-icon>
                  <code class="mr-2">genre:action</code>
                  <span class="example-description">{{ $t('filter.example_genre') }}</span>
                </div>
                <div class="example-item">
                  <v-icon class="example-icon" small>mdi-office-building</v-icon>
                  <code class="mr-2">publisher:Marvel</code>
                  <span class="example-description">{{ $t('filter.example_publisher') }}</span>
                </div>
                <div class="example-item">
                  <v-icon class="example-icon" small>mdi-link</v-icon>
                  <code class="mr-2">author:John AND tag:action</code>
                  <span class="example-description">{{ $t('filter.example_combined') }}</span>
                </div>
              </template>
              <template v-else>
                <div class="example-item">
                  <v-icon class="example-icon" small>mdi-account</v-icon>
                  <code class="mr-2">author:John</code>
                  <span class="example-description">{{ $t('filter.example_author') }}</span>
                </div>
                <div class="example-item">
                  <v-icon class="example-icon" small>mdi-tag</v-icon>
                  <code class="mr-2">tag:action</code>
                  <span class="example-description">{{ $t('filter.example_tag') }}</span>
                </div>
                <div class="example-item">
                  <v-icon class="example-icon" small>mdi-close-circle</v-icon>
                  <code class="mr-2">NOT tag:mystery</code>
                  <span class="example-description">{{ $t('filter.example_not') }}</span>
                </div>
                <div class="example-item">
                  <v-icon class="example-icon" small>mdi-link</v-icon>
                  <code class="mr-2">author:John AND tag:action</code>
                  <span class="example-description">{{ $t('filter.example_combined') }}</span>
                </div>
              </template>
            </div>
          </v-tab-item>

          <!-- Builder Mode -->
          <v-tab-item>
            <div class="text-subtitle-2 mb-3">{{ $t('filter.build_filter') }}</div>

            <!-- Filter Conditions -->
            <div v-for="(condition, index) in conditions" :key="condition._id" class="mb-3">
              <v-card outlined class="pa-3">
                <!-- Logic Operator (for conditions after the first) -->
                <v-row v-if="index > 0" dense class="mb-2">
                  <v-col cols="12">
                    <v-select
                      v-model="condition.logicOp"
                      :items="logicOperatorOptions"
                      :label="$t('filter.logic_operator')"
                      dense
                      outlined
                      hide-details
                      class="logic-operator"
                    />
                  </v-col>
                </v-row>

                <v-row dense>
                  <v-col cols="4">
                    <v-select
                      v-model="condition.field"
                      :items="fieldOptions"
                      :label="$t('filter.field')"
                      dense
                      outlined
                      hide-details
                      @change="onFieldChange(condition)"
                    />
                  </v-col>
                  <v-col cols="7">
                    <v-combobox
                      v-if="condition.field === 'genre'"
                      :key="`genre-${condition._id}`"
                      v-model="condition.value"
                      :items="genreSuggestions"
                      :label="$t('filter.value')"
                      :search-input.sync="condition.searchInput"
                      :loading="loadingStates.genre"
                      dense
                      outlined
                      hide-details
                      clearable
                      @input="onGenreInput"
                    />
                    <v-combobox
                      v-else-if="condition.field === 'tag'"
                      :key="`tag-${condition._id}`"
                      v-model="condition.value"
                      :items="tagSuggestions"
                      :label="$t('filter.value')"
                      :search-input.sync="condition.searchInput"
                      :loading="loadingStates.tag"
                      dense
                      outlined
                      hide-details
                      clearable
                      @input="onTagInput"
                    />
                    <v-combobox
                      v-else-if="condition.field === 'publisher'"
                      :key="`publisher-${condition._id}`"
                      v-model="condition.value"
                      :items="publisherSuggestions"
                      :label="$t('filter.value')"
                      :search-input.sync="condition.searchInput"
                      :loading="loadingStates.publisher"
                      dense
                      outlined
                      hide-details
                      clearable
                      @input="onPublisherInput"
                    />
                    <v-combobox
                      v-else-if="condition.field === 'language'"
                      :key="`language-${condition._id}`"
                      v-model="condition.value"
                      :items="languageSuggestions"
                      item-text="name"
                      item-value="value"
                      return-object
                      :label="$t('filter.value')"
                      :search-input.sync="condition.searchInput"
                      :loading="loadingStates.language"
                      dense
                      outlined
                      hide-details
                      clearable
                      @input="onLanguageInput"
                    />
                    <v-combobox
                      v-else-if="condition.field === 'age_rating'"
                      :key="`age_rating-${condition._id}`"
                      v-model="condition.value"
                      :items="ageRatingSuggestions"
                      :label="$t('filter.value')"
                      :search-input.sync="condition.searchInput"
                      :loading="loadingStates.ageRating"
                      dense
                      outlined
                      hide-details
                      clearable
                      @input="onAgeRatingInput"
                    />
                    <v-select
                      v-else-if="condition.field === 'status'"
                      :key="`status-${condition._id}`"
                      v-model="condition.value"
                      :items="statusOptions"
                      :label="$t('filter.value')"
                      dense
                      outlined
                      hide-details
                    />
                    <v-combobox
                      v-else-if="['author', 'writer', 'penciller', 'letterer', 'inker', 'editor', 'cover', 'colorist'].includes(condition.field)"
                      :key="`${condition.field}-${condition._id}`"
                      v-model="condition.value"
                      :items="authorSuggestions"
                      :label="$t('filter.value')"
                      :search-input.sync="condition.searchInput"
                      :loading="loadingStates.author"
                      dense
                      outlined
                      hide-details
                      clearable
                      @input="onAuthorInput(condition)"
                    />
                    <v-select
                      v-else-if="condition.field === 'oneshot'"
                      :key="`oneshot-${condition._id}`"
                      v-model="condition.value"
                      :items="booleanOptions"
                      :label="$t('filter.value')"
                      dense
                      outlined
                      hide-details
                    />
                    <v-text-field
                      v-else-if="condition.field && !isBooleanField(condition.field) && !['tag', 'author', 'writer', 'penciller', 'letterer', 'inker', 'editor', 'cover', 'colorist', 'genre', 'publisher', 'language', 'age_rating', 'status'].includes(condition.field)"
                      :key="`text-${condition._id}`"
                      v-model="condition.value"
                      :label="$t('filter.value')"
                      dense
                      outlined
                      hide-details
                    />
                    <v-select
                      v-else-if="condition.field === 'deleted'"
                      :key="`deleted-${condition._id}`"
                      v-model="condition.value"
                      :items="booleanOptions"
                      :label="$t('filter.value')"
                      dense
                      outlined
                      hide-details
                    />
                  </v-col>
                  <v-col cols="1">
                    <v-btn icon small @click="removeCondition(index)">
                      <v-icon small>mdi-delete</v-icon>
                    </v-btn>
                  </v-col>
                </v-row>
              </v-card>
            </div>

            <!-- Add Condition Button -->
            <v-btn
              color="primary"
              outlined
              small
              @click="addCondition"
              class="mb-3"
            >
              <v-icon left>mdi-plus</v-icon>
              {{ $t('filter.add_condition') }}
            </v-btn>

            <!-- Generated Query Preview -->
            <v-card v-if="conditions.length > 0" outlined class="pa-3 mb-3 query-preview-card">
              <div class="query-preview-title">
                <v-icon class="query-preview-icon" small>mdi-code-braces</v-icon>
                {{ $t('filter.generated_query') }}
              </div>
              <div class="query-preview-content">
                <code class="query-preview-code">{{ generateQueryFromConditions() }}</code>
              </div>
            </v-card>
          </v-tab-item>
        </v-tabs-items>
      </v-card-text>

      <v-card-actions>
        <v-btn @click="showSavedFiltersDialog = true" text>
          <v-icon left>mdi-bookmark-multiple</v-icon>
          {{ $t('filter.saved_filters') }}
        </v-btn>
        <v-spacer />
        <v-btn @click="clearFilter" text>
          {{ $t('common.clear') }}
        </v-btn>
        <v-btn @click="saveFilter" text :disabled="!query.trim()">
          <v-icon left>mdi-bookmark</v-icon>
          {{ $t('common.save') }}
        </v-btn>
        <v-btn
          color="primary"
          @click="applyFilter"
          :disabled="!canApplyFilter"
        >
          {{ $t('common.apply') }}
        </v-btn>
      </v-card-actions>

      <!-- Saved Filters Dialog -->
      <saved-filters-dialog
        v-model="showSavedFiltersDialog"
        :filter-to-save="filterToSave"
        :external-filter-to-save="externalFilterToSave"
        @load="loadFilter"
        @save="handleSaveFilter"
      />
    </v-card>
  </v-dialog>
</template>

<script lang="ts">
import Vue from 'vue'
import { parseSmartFilter, parseSmartFilterForSeries } from '@/functions/smart-filter'
import SavedFiltersDialog from '@/components/SavedFiltersDialog.vue'
import { NameValue } from '@/types/filter'

export default Vue.extend({
  name: 'SmartFilterDialog',
  components: {
    SavedFiltersDialog,
  },
  props: {
    value: {
      type: Boolean,
      default: false,
    },
    currentQuery: {
      type: String,
      default: '',
    },
    libraryIds: {
      type: Array as () => string[],
      default: () => [],
    },
    mode: {
      type: String,
      default: 'books', // 'books' or 'series'
    },
    fromSearchBox: {
      type: Boolean,
      default: false,
    },
  },
  data: () => ({
    query: '',
    localMode: 0,
    showSavedFiltersDialog: false,
    filterToSave: null as {name: string, query: string} | null,
    externalFilterToSave: null as {name: string, query: string} | null,
    conditions: [] as Array<{_id: string, field: string, value: string, logicOp?: string, searchInput?: string}>,
    tagSuggestions: [] as string[],
    authorSuggestions: [] as string[],
    genreSuggestions: [] as string[],
    publisherSuggestions: [] as string[],
    languageSuggestions: [] as NameValue[],
    ageRatingSuggestions: [] as string[],
    // 性能优化相关数据
    loadingStates: {
      tag: false,
      author: false,
      genre: false,
      publisher: false,
      language: false,
      ageRating: false,
    } as Record<string, boolean>,
    debounceTimers: {} as Record<string, number>,
    suggestionsCache: {} as Record<string, any>,
    abortControllers: {} as Record<string, AbortController>,
  }),
  computed: {
    dialog: {
      get(): boolean {
        return this.value
      },
      set(value: boolean) {
        this.$emit('input', value)
      },
    },
    fieldOptions() {
      if (this.$props.mode === 'series') {
        return [
          { text: this.$t('filter.genre'), value: 'genre' },
          { text: this.$t('filter.tag'), value: 'tag' },
          { text: this.$t('filter.author'), value: 'author' },
          { text: this.$t('filter.publisher'), value: 'publisher' },
          { text: this.$t('filter.language'), value: 'language' },
          { text: this.$t('filter.age_rating'), value: 'age_rating' },
          { text: this.$t('filter.status'), value: 'status' },
          { text: this.$t('filter.oneshot'), value: 'oneshot' },
          { text: this.$t('filter.deleted'), value: 'deleted' },
        ]
      } else {
        return [
          { text: this.$t('filter.tag'), value: 'tag' },
          { text: this.$t('filter.author'), value: 'author' },
          { text: this.$t('filter.oneshot'), value: 'oneshot' },
          { text: this.$t('filter.deleted'), value: 'deleted' },
        ]
      }
    },
    booleanOptions() {
      return [
        { text: this.$t('common.true'), value: 'true' },
        { text: this.$t('common.false'), value: 'false' },
      ]
    },
    logicOperatorOptions() {
      return [
        { text: 'AND', value: 'AND' },
        { text: 'OR', value: 'OR' },
        { text: 'NOT', value: 'NOT' },
      ]
    },
    statusOptions() {
      return [
        { text: this.$t('enums.series_status.ONGOING'), value: 'ONGOING' },
        { text: this.$t('enums.series_status.ENDED'), value: 'ENDED' },
        { text: this.$t('enums.series_status.ABANDONED'), value: 'ABANDONED' },
        { text: this.$t('enums.series_status.HIATUS'), value: 'HIATUS' },
      ]
    },
    canApplyFilter() {
      if (this.localMode === 0) {
        // Text mode
        return this.query.trim().length > 0
      } else {
        // Builder mode
        const booleanFields = ['oneshot', 'deleted']
        return this.conditions.some(cond =>
          cond.field && (cond.value || booleanFields.includes(cond.field)),
        )
      }
    },
  },
  watch: {
    currentQuery: {
      handler(val) {
        this.query = val
      },
      immediate: true,
    },
    libraryIds: {
      handler() {
        // Reload suggestions when library IDs change
        this.loadTagSuggestions()
        this.loadAuthorSuggestions()
        this.loadGenreSuggestions()
        this.loadPublisherSuggestions()
        this.loadLanguageSuggestions()
        this.loadAgeRatingSuggestions()
      },
      deep: true,
    },
    conditions: {
      handler() {
        // Only sync from builder mode to text mode, not the other way around
        if (this.localMode === 1) { // Builder mode
          const generatedQuery = this.generateQueryFromConditions()
          if (generatedQuery !== this.query) {
            this.query = generatedQuery
          }
        }
      },
      deep: true,
    },
  },
  methods: {
    // 防抖处理方法
    debounce(func: Function, delay: number, key: string) {
      // 清除之前的定时器
      if (this.debounceTimers[key]) {
        clearTimeout(this.debounceTimers[key])
      }
      // 设置新的定时器
      this.debounceTimers[key] = window.setTimeout(() => {
        func()
        delete this.debounceTimers[key]
      }, delay)
    },

    // 取消请求方法
    cancelRequest(key: string) {
      if (this.abortControllers[key]) {
        this.abortControllers[key].abort()
        delete this.abortControllers[key]
      }
    },

    // 获取缓存的建议
    getCachedSuggestions(key: string) {
      return this.suggestionsCache[key]
    },

    // 设置缓存的建议
    setCachedSuggestions(key: string, data: any) {
      this.suggestionsCache[key] = data
    },

    applyFilter() {
      if (this.fromSearchBox) {
        // 从搜索框打开时，始终调用全文搜索
        if (!this.query.trim()) return
        this.$emit('full-text-search', this.query.trim())
        this.dialog = false
      } else {
        // 从其他地方打开时，使用智能筛选器逻辑
        if (this.localMode === 0) {
          // Text mode
          if (!this.query.trim()) return
          // 剥离"和'号
          const cleanedQuery = this.query.trim().replace(/["']/g, '')
          const parser = this.$props.mode === 'series' ? parseSmartFilterForSeries : parseSmartFilter
          const condition = parser(cleanedQuery)
          if (condition) {
            this.$emit('apply', condition, cleanedQuery)
            this.dialog = false
          } else {
            this.$emit('error', this.$t('filter.smart_filter_invalid'))
          }
        } else {
          // Builder mode
          const generatedQuery = this.generateQueryFromConditions()
          if (generatedQuery) {
            const parser = this.$props.mode === 'series' ? parseSmartFilterForSeries : parseSmartFilter
            const condition = parser(generatedQuery)
            if (condition) {
              this.$emit('apply', condition, generatedQuery)
              this.dialog = false
            } else {
              this.$emit('error', this.$t('filter.smart_filter_invalid'))
            }
          }
        }
      }
    },
    clearFilter() {
      this.query = ''
      this.conditions = []
      // Clear all suggestion caches to prevent stale data
      this.tagSuggestions = []
      this.authorSuggestions = []
      this.genreSuggestions = []
      this.publisherSuggestions = []
      this.languageSuggestions = []
      this.ageRatingSuggestions = []
      this.$emit('clear')
      // Don't close dialog automatically when clearing
    },
    saveFilter() {
      const currentQuery = this.localMode === 0 ? this.query.trim() : this.generateQueryFromConditions()
      if (!currentQuery) return

      // Set the filter to save - this will trigger the save dialog in SavedFiltersDialog
      this.externalFilterToSave = {
        name: '',
        query: currentQuery,
      }
      // Don't set showSavedFiltersDialog to true here - let SavedFiltersDialog handle it
    },
    loadFilter(filter: {name: string, query: string}) {
      this.query = filter.query
      this.localMode = 0 // Switch to text mode when loading
    },
    addCondition() {
      this.conditions.push({
        _id: Math.random().toString(36).substr(2, 9),
        field: '',
        value: '',
        logicOp: 'AND',
      })
    },
    removeCondition(index: number) {
      this.conditions.splice(index, 1)
      // Clear suggestion caches to prevent stale data from removed conditions
      this.tagSuggestions = []
      this.authorSuggestions = []
      this.genreSuggestions = []
      this.publisherSuggestions = []
      this.languageSuggestions = []
      this.ageRatingSuggestions = []
      // Reload suggestions for remaining conditions
      this.loadTagSuggestions()
      this.loadAuthorSuggestions()
      this.loadGenreSuggestions()
      this.loadPublisherSuggestions()
      this.loadLanguageSuggestions()
      this.loadAgeRatingSuggestions()
    },
    isBooleanField(field: string) {
      return ['oneshot', 'deleted'].includes(field)
    },
    generateQueryFromConditions() {
      if (this.conditions.length === 0) return ''

      const validConditions = this.conditions
        .filter(cond => cond.field && (cond.value || this.isBooleanField(cond.field)))

      if (validConditions.length === 0) return ''

      // 使用更高效的字符串构建方式
      const queryParts: string[] = []

      for (const cond of validConditions) {
        // Handle different value types
        let valueStr: string = ''
        if (cond.value === null || cond.value === undefined) {
          valueStr = ''
        } else if (typeof cond.value === 'object') {
          // For language field with return-object, extract the value property
          if (cond.field === 'language' && (cond.value as any).value) {
            valueStr = (cond.value as any).value
          } else {
            valueStr = String(cond.value)
          }
        } else {
          valueStr = String(cond.value)
        }

        const conditionStr = this.isBooleanField(cond.field)
          ? `${cond.field}:${valueStr}`
          : `${cond.field}:${valueStr}`

        queryParts.push(conditionStr)
      }

      // 使用 join 构建最终查询，比循环拼接更高效
      return queryParts.join(' AND ')
    },
    async loadTagSuggestions(search?: string) {
      const cacheKey = `tag_${this.$props.mode}_${this.libraryIds.join(',')}_${search || ''}`

      // 检查缓存
      const cached = this.getCachedSuggestions(cacheKey)
      if (cached && !search) {
        this.tagSuggestions = cached
        return
      }

      // 取消之前的请求
      this.cancelRequest('tag')

      // 创建新的 AbortController
      this.abortControllers['tag'] = new AbortController()

      try {
        this.loadingStates.tag = true

        // Choose the appropriate API based on mode
        let response: string[]
        if (this.$props.mode === 'books') {
          // For books mode, use getBookTags
          response = await this.$komgaReferential.getBookTags(undefined, undefined, this.libraryIds)
        } else {
          // For series mode, use getSeriesAndBookTags
          response = await this.$komgaReferential.getSeriesAndBookTags(this.libraryIds)
        }

        // 检查请求是否被取消
        if (this.abortControllers['tag']?.signal.aborted) {
          return
        }

        this.tagSuggestions = Array.isArray(response) ? response : []

        // 缓存结果（只缓存非搜索结果）
        if (!search) {
          this.setCachedSuggestions(cacheKey, this.tagSuggestions)
        }
      } catch (error) {
        // 检查是否是取消请求导致的错误
        if (error.name === 'AbortError') {
          return
        }
        // eslint-disable-next-line no-console
        console.error('Failed to load tag suggestions:', error)
        this.tagSuggestions = []
      } finally {
        this.loadingStates.tag = false
        delete this.abortControllers['tag']
      }
    },
    async loadAuthorSuggestions(role?: string) {
      const cacheKey = `author_${role || 'all'}_${this.libraryIds.join(',')}`

      // 检查缓存
      const cached = this.getCachedSuggestions(cacheKey)
      if (cached) {
        this.authorSuggestions = cached
        return
      }

      // 取消之前的请求
      this.cancelRequest('author')

      // 创建新的 AbortController
      this.abortControllers['author'] = new AbortController()

      try {
        this.loadingStates.author = true

        // Always reset suggestions first to prevent stale data
        this.authorSuggestions = []

        // Get all authors (search is always undefined)
        const response = await this.$komgaReferential.getAuthors(undefined, role, this.libraryIds, undefined, undefined, undefined, true)

        // 检查请求是否被取消
        if (this.abortControllers['author']?.signal.aborted) {
          return
        }

        this.authorSuggestions = response.content ? response.content.map((author: any) => author.name) : []

        // 缓存结果
        this.setCachedSuggestions(cacheKey, this.authorSuggestions)
      } catch (error) {
        // 检查是否是取消请求导致的错误
        if (error.name === 'AbortError') {
          return
        }
        // eslint-disable-next-line no-console
        console.error('Failed to load author suggestions:', error)
        this.authorSuggestions = []
      } finally {
        this.loadingStates.author = false
        delete this.abortControllers['author']
      }
    },
    async loadGenreSuggestions(search?: string) {
      const cacheKey = `genre_${this.libraryIds.join(',')}_${search || ''}`

      // 检查缓存
      const cached = this.getCachedSuggestions(cacheKey)
      if (cached && !search) {
        this.genreSuggestions = cached
        return
      }

      // 取消之前的请求
      this.cancelRequest('genre')

      // 创建新的 AbortController
      this.abortControllers['genre'] = new AbortController()

      try {
        this.loadingStates.genre = true

        // Get all genres
        const response = await this.$komgaReferential.getGenres(this.libraryIds)

        // 检查请求是否被取消
        if (this.abortControllers['genre']?.signal.aborted) {
          return
        }

        this.genreSuggestions = Array.isArray(response) ? response : []

        // 缓存结果（只缓存非搜索结果）
        if (!search) {
          this.setCachedSuggestions(cacheKey, this.genreSuggestions)
        }
      } catch (error) {
        // 检查是否是取消请求导致的错误
        if (error.name === 'AbortError') {
          return
        }
        // eslint-disable-next-line no-console
        console.error('Failed to load genre suggestions:', error)
        this.genreSuggestions = []
      } finally {
        this.loadingStates.genre = false
        delete this.abortControllers['genre']
      }
    },
    async loadPublisherSuggestions(search?: string) {
      const cacheKey = `publisher_${this.libraryIds.join(',')}_${search || ''}`

      // 检查缓存
      const cached = this.getCachedSuggestions(cacheKey)
      if (cached && !search) {
        this.publisherSuggestions = cached
        return
      }

      // 取消之前的请求
      this.cancelRequest('publisher')

      // 创建新的 AbortController
      this.abortControllers['publisher'] = new AbortController()

      try {
        this.loadingStates.publisher = true

        // Get all publishers
        const response = await this.$komgaReferential.getPublishers(this.libraryIds)

        // 检查请求是否被取消
        if (this.abortControllers['publisher']?.signal.aborted) {
          return
        }

        this.publisherSuggestions = Array.isArray(response) ? response : []

        // 缓存结果（只缓存非搜索结果）
        if (!search) {
          this.setCachedSuggestions(cacheKey, this.publisherSuggestions)
        }
      } catch (error) {
        // 检查是否是取消请求导致的错误
        if (error.name === 'AbortError') {
          return
        }
        // eslint-disable-next-line no-console
        console.error('Failed to load publisher suggestions:', error)
        this.publisherSuggestions = []
      } finally {
        this.loadingStates.publisher = false
        delete this.abortControllers['publisher']
      }
    },
    async loadLanguageSuggestions(search?: string) {
      const cacheKey = `language_${this.libraryIds.join(',')}_${search || ''}`

      // 检查缓存
      const cached = this.getCachedSuggestions(cacheKey)
      if (cached && !search) {
        this.languageSuggestions = cached
        return
      }

      // 取消之前的请求
      this.cancelRequest('language')

      // 创建新的 AbortController
      this.abortControllers['language'] = new AbortController()

      try {
        this.loadingStates.language = true

        // Get all languages
        const response = await this.$komgaReferential.getLanguages(this.libraryIds)

        // 检查请求是否被取消
        if (this.abortControllers['language']?.signal.aborted) {
          return
        }

        this.languageSuggestions = Array.isArray(response) ? response : []

        // 缓存结果（只缓存非搜索结果）
        if (!search) {
          this.setCachedSuggestions(cacheKey, this.languageSuggestions)
        }
      } catch (error) {
        // 检查是否是取消请求导致的错误
        if (error.name === 'AbortError') {
          return
        }
        // eslint-disable-next-line no-console
        console.error('Failed to load language suggestions:', error)
        this.languageSuggestions = []
      } finally {
        this.loadingStates.language = false
        delete this.abortControllers['language']
      }
    },
    async loadAgeRatingSuggestions(search?: string) {
      const cacheKey = `ageRating_${this.libraryIds.join(',')}_${search || ''}`

      // 检查缓存
      const cached = this.getCachedSuggestions(cacheKey)
      if (cached && !search) {
        this.ageRatingSuggestions = cached
        return
      }

      // 取消之前的请求
      this.cancelRequest('ageRating')

      // 创建新的 AbortController
      this.abortControllers['ageRating'] = new AbortController()

      try {
        this.loadingStates.ageRating = true

        // Get all age ratings
        const response = await this.$komgaReferential.getAgeRatings(this.libraryIds)

        // 检查请求是否被取消
        if (this.abortControllers['ageRating']?.signal.aborted) {
          return
        }

        this.ageRatingSuggestions = Array.isArray(response) ? response : []

        // 缓存结果（只缓存非搜索结果）
        if (!search) {
          this.setCachedSuggestions(cacheKey, this.ageRatingSuggestions)
        }
      } catch (error) {
        // 检查是否是取消请求导致的错误
        if (error.name === 'AbortError') {
          return
        }
        // eslint-disable-next-line no-console
        console.error('Failed to load age rating suggestions:', error)
        this.ageRatingSuggestions = []
      } finally {
        this.loadingStates.ageRating = false
        delete this.abortControllers['ageRating']
      }
    },
    onTagInput(condition: any) {
      // 使用防抖处理输入
      this.debounce(() => {
        if (condition.searchInput && condition.searchInput.length > 0) {
          this.loadTagSuggestions(condition.searchInput)
        } else {
          this.loadTagSuggestions()
        }
      }, 300, `tag_${condition._id}`)
    },
    onAuthorInput(condition?: any) {
      // 使用防抖处理输入
      this.debounce(() => {
        const role = condition?.field && ['author', 'writer', 'penciller', 'letterer', 'inker', 'editor', 'cover', 'colorist'].includes(condition.field)
          ? (condition.field === 'author' ? undefined : condition.field)
          : undefined

        // Always reload suggestions to prevent stale data
        this.loadAuthorSuggestions(role)
      }, 300, `author_${condition?._id || 'global'}`)
    },
    onGenreInput(condition?: any) {
      // 使用防抖处理输入
      this.debounce(() => {
        if (condition.searchInput && condition.searchInput.length > 0) {
          this.loadGenreSuggestions(condition.searchInput)
        } else {
          this.loadGenreSuggestions()
        }
      }, 300, `genre_${condition?._id || 'global'}`)
    },
    onPublisherInput(condition?: any) {
      // 使用防抖处理输入
      this.debounce(() => {
        if (condition.searchInput && condition.searchInput.length > 0) {
          this.loadPublisherSuggestions(condition.searchInput)
        } else {
          this.loadPublisherSuggestions()
        }
      }, 300, `publisher_${condition?._id || 'global'}`)
    },
    onLanguageInput(condition?: any) {
      // 使用防抖处理输入
      this.debounce(() => {
        if (condition.searchInput && condition.searchInput.length > 0) {
          this.loadLanguageSuggestions(condition.searchInput)
        } else {
          this.loadLanguageSuggestions()
        }
      }, 300, `language_${condition?._id || 'global'}`)
    },
    onAgeRatingInput(condition?: any) {
      // 使用防抖处理输入
      this.debounce(() => {
        if (condition.searchInput && condition.searchInput.length > 0) {
          this.loadAgeRatingSuggestions(condition.searchInput)
        } else {
          this.loadAgeRatingSuggestions()
        }
      }, 300, `ageRating_${condition?._id || 'global'}`)
    },
    onQueryInput(value: string) {
      // Handle query input changes
      // This method is called when user types or clears the text
      // We don't need to do anything special here, just let v-model handle it
    },
    onFieldChange(condition: any) {
      // Reset both search input and value when field changes to prevent stale data
      condition.searchInput = ''
      condition.value = ''

      // Clear all suggestion caches first to prevent any stale data
      this.tagSuggestions = []
      this.authorSuggestions = []
      this.genreSuggestions = []
      this.publisherSuggestions = []
      this.languageSuggestions = []
      this.ageRatingSuggestions = []

      // Reload suggestions when field changes
      if (['author', 'writer', 'penciller', 'letterer', 'inker', 'editor', 'cover', 'colorist'].includes(condition.field)) {
        const role = condition.field === 'author' ? undefined : condition.field
        this.loadAuthorSuggestions(role)
      } else if (condition.field === 'genre') {
        this.loadGenreSuggestions()
      } else if (condition.field === 'publisher') {
        this.loadPublisherSuggestions()
      } else if (condition.field === 'language') {
        this.loadLanguageSuggestions()
      } else if (condition.field === 'age_rating') {
        this.loadAgeRatingSuggestions()
      } else if (condition.field === 'tag') {
        this.loadTagSuggestions()
      }
    },
    handleSaveFilter(filter: {name: string, query: string}) {
      // Emit save-filter event to parent component
      this.$emit('save-filter', filter)
    },
  },
  async mounted() {
    // 使用并发控制加载初始建议，避免同时发起过多请求
    const loadTasks = [
      this.loadTagSuggestions(),
      this.loadAuthorSuggestions(),
    ]

    // 等待前两个任务完成后再加载其他任务
    await Promise.all(loadTasks)

    // 延迟加载其他建议
    setTimeout(() => {
      this.loadGenreSuggestions()
      this.loadPublisherSuggestions()
      this.loadLanguageSuggestions()
      this.loadAgeRatingSuggestions()
    }, 100)
  },
  beforeDestroy() {
    // 清理所有定时器
    Object.keys(this.debounceTimers).forEach(key => {
      if (this.debounceTimers[key]) {
        clearTimeout(this.debounceTimers[key])
      }
    })

    // 取消所有进行中的请求
    Object.keys(this.abortControllers).forEach(key => {
      if (this.abortControllers[key]) {
        this.abortControllers[key].abort()
      }
    })
  },
})
</script>

<style scoped>
/* 明亮主题样式 */
.smart-filter-examples {
  background: linear-gradient(135deg, rgba(0, 123, 255, 0.05) 0%, rgba(0, 123, 255, 0.02) 100%);
  border: 1px solid rgba(0, 123, 255, 0.1);
  border-radius: 8px;
  padding: 16px;
  margin: 16px 0;
}

.theme--light .smart-filter-examples {
  background: linear-gradient(135deg, rgba(0, 123, 255, 0.08) 0%, rgba(0, 123, 255, 0.03) 100%);
  border: 1px solid rgba(0, 123, 255, 0.15);
  box-shadow: 0 2px 8px rgba(0, 123, 255, 0.1);
}

.theme--dark .smart-filter-examples {
  background: linear-gradient(135deg, rgba(33, 150, 243, 0.08) 0%, rgba(33, 150, 243, 0.03) 100%);
  border: 1px solid rgba(33, 150, 243, 0.2);
  box-shadow: 0 2px 8px rgba(33, 150, 243, 0.15);
}

.example-item {
  display: flex;
  align-items: center;
  margin-bottom: 8px;
  padding: 8px 12px;
  border-radius: 6px;
  transition: all 0.2s ease;
}

.theme--light .example-item:hover {
  background-color: rgba(0, 123, 255, 0.05);
}

.theme--dark .example-item:hover {
  background-color: rgba(33, 150, 243, 0.08);
}

.example-icon {
  margin-right: 8px;
  color: #2196f3;
  font-size: 16px;
}

.theme--dark .example-icon {
  color: #64b5f6;
}

code {
  background: linear-gradient(135deg, rgba(0, 0, 0, 0.08) 0%, rgba(0, 0, 0, 0.05) 100%);
  color: #2c3e50;
  padding: 4px 8px;
  border-radius: 6px;
  font-family: 'SF Mono', 'Monaco', 'Inconsolata', 'Roboto Mono', 'Courier New', monospace;
  font-size: 0.85em;
  font-weight: 500;
  border: 1px solid rgba(0, 0, 0, 0.1);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  transition: all 0.2s ease;
}

.theme--light code:hover {
  background: linear-gradient(135deg, rgba(0, 0, 0, 0.12) 0%, rgba(0, 0, 0, 0.08) 100%);
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.15);
}

.theme--dark code {
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.12) 0%, rgba(255, 255, 255, 0.08) 100%);
  color: #e3f2fd;
  border: 1px solid rgba(255, 255, 255, 0.2);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
}

.theme--dark code:hover {
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.18) 0%, rgba(255, 255, 255, 0.12) 100%);
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.4);
}

.example-description {
  color: rgba(0, 0, 0, 0.7);
  font-size: 0.9em;
}

.theme--dark .example-description {
  color: rgba(255, 255, 255, 0.8);
}

.examples-title {
  color: #1976d2;
  font-weight: 600;
  margin-bottom: 12px;
  display: flex;
  align-items: center;
}

.theme--dark .examples-title {
  color: #64b5f6;
}

.examples-title-icon {
  margin-right: 8px;
  font-size: 18px;
}

/* 深色主题整体支持 */
.theme--dark .v-card {
  background-color: #1e1e1e;
  color: rgba(255, 255, 255, 0.87);
  border: 1px solid rgba(255, 255, 255, 0.1);
}

.theme--dark .v-card-title {
  color: rgba(255, 255, 255, 0.87);
}

.theme--dark .v-card-text {
  color: rgba(255, 255, 255, 0.87);
}

.theme--dark .text-body-2.grey--text {
  color: rgba(255, 255, 255, 0.6) !important;
}

.theme--dark .text-subtitle-2 {
  color: rgba(255, 255, 255, 0.8);
}

/* 标签页样式优化 */
.theme--light .v-tabs .v-tab {
  color: rgba(0, 0, 0, 0.7);
}

.theme--light .v-tabs .v-tab--active {
  color: #1976d2;
  font-weight: 500;
}

.theme--dark .v-tabs .v-tab {
  color: rgba(255, 255, 255, 0.7);
}

.theme--dark .v-tabs .v-tab--active {
  color: #64b5f6;
  font-weight: 500;
}

/* 输入框样式优化 */
.theme--light .v-text-field .v-input__slot {
  border-radius: 8px;
}

.theme--dark .v-text-field .v-input__slot {
  border-radius: 8px;
  background-color: rgba(255, 255, 255, 0.05);
}

/* 按钮样式优化 */
.theme--light .v-btn {
  border-radius: 6px;
  font-weight: 500;
}

.theme--dark .v-btn {
  border-radius: 6px;
  font-weight: 500;
}

/* 分割线样式 */
.theme--light .v-divider {
  border-color: rgba(0, 0, 0, 0.1);
}

.theme--dark .v-divider {
  border-color: rgba(255, 255, 255, 0.1);
}

/* 查询预览样式 */
.query-preview-card {
  background: linear-gradient(135deg, rgba(76, 175, 80, 0.05) 0%, rgba(76, 175, 80, 0.02) 100%);
  border: 1px solid rgba(76, 175, 80, 0.2);
  border-radius: 8px;
}

.theme--light .query-preview-card {
  background: linear-gradient(135deg, rgba(76, 175, 80, 0.08) 0%, rgba(76, 175, 80, 0.03) 100%);
  border: 1px solid rgba(76, 175, 80, 0.25);
  box-shadow: 0 2px 8px rgba(76, 175, 80, 0.1);
}

.theme--dark .query-preview-card {
  background: linear-gradient(135deg, rgba(129, 199, 132, 0.08) 0%, rgba(129, 199, 132, 0.03) 100%);
  border: 1px solid rgba(129, 199, 132, 0.25);
  box-shadow: 0 2px 8px rgba(129, 199, 132, 0.15);
}

.query-preview-title {
  color: #388e3c;
  font-weight: 600;
  margin-bottom: 8px;
  display: flex;
  align-items: center;
}

.theme--dark .query-preview-title {
  color: #81c784;
}

.query-preview-icon {
  margin-right: 8px;
  font-size: 18px;
}

.query-preview-content {
  background-color: rgba(0, 0, 0, 0.03);
  border-radius: 6px;
  padding: 12px;
  border: 1px solid rgba(0, 0, 0, 0.08);
}

.theme--light .query-preview-content {
  background-color: rgba(0, 0, 0, 0.03);
  border: 1px solid rgba(0, 0, 0, 0.08);
}

.theme--dark .query-preview-content {
  background-color: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
}

.query-preview-code {
  background: linear-gradient(135deg, rgba(0, 0, 0, 0.08) 0%, rgba(0, 0, 0, 0.05) 100%);
  color: #2c3e50;
  padding: 6px 10px;
  border-radius: 4px;
  font-family: 'SF Mono', 'Monaco', 'Inconsolata', 'Roboto Mono', 'Courier New', monospace;
  font-size: 0.9em;
  font-weight: 500;
  border: 1px solid rgba(0, 0, 0, 0.1);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  display: block;
  margin: 0;
  word-break: break-all;
}

.theme--light .query-preview-code {
  background: linear-gradient(135deg, rgba(0, 0, 0, 0.08) 0%, rgba(0, 0, 0, 0.05) 100%);
  color: #2c3e50;
  border: 1px solid rgba(0, 0, 0, 0.1);
}

.theme--dark .query-preview-code {
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.12) 0%, rgba(255, 255, 255, 0.08) 100%);
  color: #e8f5e8;
  border: 1px solid rgba(255, 255, 255, 0.2);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
}

/* 逻辑操作符样式 */
.logic-operator .v-select__selection {
  font-weight: 600;
  color: #f57c00;
}

.theme--dark .logic-operator .v-select__selection {
  color: #ffb74d;
}

/* 条件卡片样式 */
.theme--light .v-card {
  border: 1px solid rgba(0, 0, 0, 0.08);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.08);
}

.theme--dark .v-card {
  border: 1px solid rgba(255, 255, 255, 0.08);
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
}

/* 悬停效果 */
.theme--light .v-card:hover {
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.15);
}

.theme--dark .v-card:hover {
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
}

/* 移动端适配样式 */
@media (max-width: 600px) {
  .v-dialog--fullscreen .v-card {
    border-radius: 0 !important;
  }

  .v-dialog--fullscreen .v-card-title {
    border-radius: 0 !important;
    padding: 12px 16px !important;
    font-size: 1rem !important;
  }

  .v-dialog--fullscreen .v-card-text {
    padding: 16px !important;
  }

  .v-dialog--fullscreen .v-card-actions {
    padding: 16px !important;
    flex-direction: column !important;
    gap: 8px !important;
  }

  .v-dialog--fullscreen .v-card-actions .v-btn {
    width: 100% !important;
    margin: 0 !important;
  }

  /* 移动端标签页样式 */
  .v-dialog--fullscreen .v-tabs {
    margin-bottom: 8px !important;
  }

  .v-dialog--fullscreen .v-tab {
    min-width: auto !important;
    padding: 0 12px !important;
    font-size: 0.875rem !important;
  }

  /* 移动端输入框样式 */
  .v-dialog--fullscreen .v-text-field {
    margin-bottom: 8px !important;
  }

  /* 移动端示例区域样式 */
  .v-dialog--fullscreen .smart-filter-examples {
    padding: 12px !important;
    margin: 12px 0 !important;
  }

  .v-dialog--fullscreen .example-item {
    padding: 6px 8px !important;
    margin-bottom: 6px !important;
  }

  .v-dialog--fullscreen .example-icon {
    margin-right: 6px !important;
    font-size: 14px !important;
  }

  /* 移动端构建器模式样式 */
  .v-dialog--fullscreen .v-row {
    margin-bottom: 8px !important;
  }

  .v-dialog--fullscreen .v-col {
    padding: 4px !important;
  }

  /* 移动端逻辑操作符样式 */
  .v-dialog--fullscreen .logic-operator {
    margin-bottom: 4px !important;
  }

  /* 移动端查询预览样式 */
  .v-dialog--fullscreen .query-preview-card {
    margin: 12px 0 !important;
    padding: 12px !important;
  }

  .v-dialog--fullscreen .query-preview-code {
    font-size: 0.8rem !important;
    padding: 8px 10px !important;
  }

  /* 移动端按钮样式 */
  .v-dialog--fullscreen .v-btn {
    height: 44px !important;
    font-size: 0.9rem !important;
  }

  .v-dialog--fullscreen .v-btn .v-icon {
    margin-right: 8px !important;
  }

  /* 移动端卡片样式 */
  .v-dialog--fullscreen .v-card {
    margin-bottom: 8px !important;
  }

  .v-dialog--fullscreen .v-card .v-card-text {
    padding: 12px !important;
  }

  /* 移动端选择器样式 */
  .v-dialog--fullscreen .v-select {
    font-size: 0.875rem !important;
  }

  .v-dialog--fullscreen .v-select .v-select__selection {
    font-size: 0.875rem !important;
  }

  /* 移动端文本字段样式 */
  .v-dialog--fullscreen .v-text-field .v-input__slot {
    font-size: 0.875rem !important;
  }

  .v-dialog--fullscreen .v-text-field .v-label {
    font-size: 0.875rem !important;
  }

  /* 移动端图标样式 */
  .v-dialog--fullscreen .v-icon {
    font-size: 18px !important;
  }

  /* 移动端分割线样式 */
  .v-dialog--fullscreen .v-divider {
    margin: 12px 0 !important;
  }

  /* 移动端悬停效果调整 */
  .v-dialog--fullscreen .example-item:hover {
    background-color: transparent !important;
  }

  .v-dialog--fullscreen .v-card:hover {
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1) !important;
  }

  .theme--dark .v-dialog--fullscreen .v-card:hover {
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3) !important;
  }
}

/* 平板适配 */
@media (min-width: 601px) and (max-width: 960px) {
  .v-dialog .v-card {
    max-width: 90vw !important;
  }

  .v-dialog .v-card-text {
    padding: 20px !important;
  }

  .v-dialog .v-card-actions {
    padding: 20px !important;
  }

  /* 平板标签页样式 */
  .v-tabs .v-tab {
    font-size: 0.9rem !important;
    padding: 0 16px !important;
  }

  /* 平板输入框样式 */
  .v-text-field {
    margin-bottom: 12px !important;
  }

  /* 平板示例区域样式 */
  .smart-filter-examples {
    padding: 16px !important;
    margin: 16px 0 !important;
  }

  /* 平板构建器模式样式 */
  .v-row {
    margin-bottom: 12px !important;
  }

  .v-col {
    padding: 6px !important;
  }

  /* 平板查询预览样式 */
  .query-preview-card {
    margin: 16px 0 !important;
    padding: 16px !important;
  }
}
</style>

