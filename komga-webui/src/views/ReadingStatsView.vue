<template>
  <v-container fluid class="pa-6">
    <!-- 加载进度条 -->
    <v-row v-if="loading || processingStats || processingCharts || processingDimensions">
      <v-col cols="12">
        <v-card class="pa-4">
          <div class="d-flex align-center mb-2">
            <v-icon class="mr-2" color="primary">mdi-chart-line</v-icon>
            <span class="text-h6">{{ loadingMessage || $t('reading_stats.loading_statistics') }}</span>
          </div>
          <v-progress-linear
            :value="loadingProgress"
            color="primary"
            height="8"
            rounded
            class="mb-2"
          ></v-progress-linear>
          <div class="text-caption text-center">{{ loadingProgress }}% {{ $t('reading_stats.completed') }}</div>
        </v-card>
      </v-col>
    </v-row>

    <v-row>
      <v-col cols="12">
        <div class="d-flex align-center justify-space-between mb-6">
          <h1 class="text-h4">{{ $t('reading_stats.title') }}</h1>
          <v-select
            v-model="selectedLibraryId"
            :items="libraryOptions"
            item-text="name"
            item-value="id"
            :label="$t('reading_stats.select_library')"
            dense
            outlined
            hide-details
            class="library-select"
            style="max-width: 250px;"
            :disabled="loading || processingStats || processingCharts || processingDimensions"
          ></v-select>
        </div>
      </v-col>
    </v-row>

    <!-- 概览统计卡片 -->
    <v-row class="stats-overview-row">
      <v-col cols="12" sm="6" md="4" xl="2">
        <v-card class="stats-card" height="140">
          <v-card-title class="text-h6">{{ $t('reading_stats.books_started_reading') }}</v-card-title>
          <v-card-text class="pa-4">
            <div class="text-h3 font-weight-bold primary--text">{{ stats.booksStartedReading }}</div>
            <div class="text-caption">{{ $t('reading_stats.out_of_total', { total: stats.totalBooks }) }}</div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" sm="6" md="4" xl="2">
        <v-card class="stats-card" height="140">
          <v-card-title class="text-h6">{{ $t('reading_stats.books_completed_reading') }}</v-card-title>
          <v-card-text class="pa-4">
            <div class="text-h3 font-weight-bold success--text">{{ stats.booksCompletedReading }}</div>
            <div class="text-caption">{{ $t('reading_stats.out_of_total', { total: stats.totalBooks }) }}</div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" sm="6" md="4" xl="2">
        <v-card class="stats-card" height="140">
          <v-card-title class="text-h6">{{ $t('reading_stats.total_pages_read') }}</v-card-title>
          <v-card-text class="pa-4">
            <div class="text-h3 font-weight-bold success--text">{{ stats.totalPagesRead }}</div>
            <div class="text-caption">{{ $t('reading_stats.average_per_book', { avg: stats.averagePagesPerBook }) }}</div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" sm="6" md="4" xl="2">
        <v-card class="stats-card" height="140">
          <v-card-title class="text-h6">{{ $t('reading_stats.reading_streak') }}</v-card-title>
          <v-card-text class="pa-4">
            <div class="text-h3 font-weight-bold orange--text">{{ stats.readingStreak }}</div>
            <div class="text-caption">{{ $t('reading_stats.days') }}</div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" sm="6" md="4" xl="2">
        <v-card class="stats-card" height="140">
          <v-card-title class="text-h6">{{ $t('reading_stats.estimated_reading_time') }}</v-card-title>
          <v-card-text class="pa-4">
            <div class="text-h3 font-weight-bold purple--text">{{ stats.estimatedReadingTime }}</div>
            <div class="text-caption">{{ $t('reading_stats.hours') }}</div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" sm="6" md="4" xl="2">
        <v-card class="stats-card" height="140">
          <v-card-title class="text-h6">{{ $t('reading_stats.last_read_time') }}</v-card-title>
          <v-card-text class="pa-4">
            <div v-if="lastReadDate" class="text-h3 font-weight-bold teal--text">
              {{ formatTimeAgo(lastReadDate) }}
            </div>
            <div v-else class="text-h3 font-weight-bold grey--text">
              {{ $t('reading_stats.no_reading_record') }}
            </div>
            <div class="text-caption">{{ $t('reading_stats.since_last_read') }}</div>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>

    <!-- 图表区域 -->
    <v-row class="mt-6">
      <v-col cols="12" md="8">
        <v-card>
          <v-card-title class="d-flex align-center justify-space-between">
            <span>{{ $t('reading_stats.reading_time_chart') }}</span>
            <v-select
              v-model="timeRange"
              :items="timeRangeOptions"
              item-text="text"
              item-value="value"
              dense
              outlined
              hide-details
              class="time-range-select"
              style="max-width: 200px;"
            ></v-select>
          </v-card-title>
          <v-card-text>
            <div class="chart-info mb-4">
              <v-chip small color="primary" class="mr-2">
                {{ $t('reading_stats.total_reading_time') }}: {{ formatReadingTime(totalReadingTime) }}
              </v-chip>
              <v-chip small color="success">
                {{ $t('reading_stats.average_daily') }}: {{ formatReadingTime(averageDailyTime) }}
              </v-chip>
            </div>
            <e-charts-line-chart
              v-if="chartData.length > 0"
              :key="chartKey"
              :data="chartData"
              :x-axis-title="$t('reading_stats.time_short')"
              :y-axis-title="$t('reading_stats.time_short')"
            />
            <div v-else class="text-center py-8">
              <v-icon size="64" color="grey">mdi-chart-line</v-icon>
              <div class="mt-2">{{ $t('reading_stats.no_data_available') }}</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" md="4">
        <v-card>
          <v-card-title>{{ $t('reading_stats.reading_status') }}</v-card-title>
          <v-card-text>
            <e-charts-pie-chart v-if="statusData.length > 0" :data="statusData" :legend="true"/>
            <div v-else class="text-center py-8">
              <v-icon size="64" color="grey">mdi-chart-pie</v-icon>
              <div class="mt-2">{{ $t('reading_stats.no_data_available') }}</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>


    <!-- 阅读习惯分析 -->
    <v-row class="mt-6">
      <v-col cols="12" md="6">
        <v-card>
          <v-card-title>{{ $t('reading_stats.reading_by_day') }}</v-card-title>
          <v-card-text>
            <e-charts-bar-chart
              v-if="dailyData.length > 0"
              :data="dailyData"
              :x-axis-title="$t('reading_stats.weekday_short')"
              :y-axis-title="$t('reading_stats.count_short')"
              :filter-zero-values="true"
            />
            <div v-else class="text-center py-8">
              <v-icon size="64" color="grey">mdi-calendar</v-icon>
              <div class="mt-2">{{ $t('reading_stats.no_data_available') }}</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" md="6">
        <v-card>
          <v-card-title>{{ $t('reading_stats.reading_by_hour') }}</v-card-title>
          <v-card-text>
            <e-charts-bar-chart
              v-if="hourlyData.length > 0"
              :data="hourlyData"
              :x-axis-title="$t('reading_stats.hour_short')"
              :y-axis-title="$t('reading_stats.count_short')"
              :filter-zero-values="true"
            />
            <div v-else class="text-center py-8">
              <v-icon size="64" color="grey">mdi-clock</v-icon>
              <div class="mt-2">{{ $t('reading_stats.no_data_available') }}</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>

    <!-- 内容维度统计 -->
    <v-row class="mt-6">
      <v-col cols="12">
        <h2 class="text-h5 mb-4">{{ $t('reading_stats.content_dimensions') }}</h2>
      </v-col>


      <!-- 偏好作者 -->
      <v-col cols="12" sm="6" md="4" lg="4" xl="4">
        <v-card class="word-cloud-card" height="350">
          <v-card-title class="word-cloud-title">{{ $t('reading_stats.preferred_authors') }}</v-card-title>
          <v-card-text class="pa-2">
            <div class="word-cloud-container">
              <e-charts-word-cloud
                v-if="authorWords.length > 0"
                :words="authorWords"
                :width="260"
                :height="260"
              />
              <div v-else class="text-center py-8">
                <v-icon size="48" color="grey">mdi-account</v-icon>
                <div class="mt-2">{{ $t('reading_stats.no_data_available') }}</div>
              </div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <!-- 偏好类型 -->
      <v-col cols="12" sm="6" md="4" lg="4" xl="4">
        <v-card class="word-cloud-card" height="350">
          <v-card-title class="word-cloud-title">{{ $t('reading_stats.preferred_genres') }}</v-card-title>
          <v-card-text class="pa-2">
            <div class="word-cloud-container">
              <e-charts-word-cloud
                v-if="genreWords.length > 0"
                :words="genreWords"
                :width="260"
                :height="260"
              />
              <div v-else class="text-center py-8">
                <v-icon size="48" color="grey">mdi-tag</v-icon>
                <div class="mt-2">{{ $t('reading_stats.no_data_available') }}</div>
              </div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <!-- 偏好标签 -->
      <v-col cols="12" sm="6" md="4" lg="4" xl="4">
        <v-card class="word-cloud-card" height="350">
          <v-card-title class="word-cloud-title">{{ $t('reading_stats.preferred_tags') }}</v-card-title>
          <v-card-text class="pa-2">
            <div class="word-cloud-container">
              <e-charts-word-cloud
                v-if="tagWords.length > 0"
                :words="tagWords"
                :width="260"
                :height="260"
              />
              <div v-else class="text-center py-8">
                <v-icon size="48" color="grey">mdi-tag-multiple</v-icon>
                <div class="mt-2">{{ $t('reading_stats.no_data_available') }}</div>
              </div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>

    <!-- 类型和标签的详细图表 -->
    <v-row class="mt-6">
      <v-col cols="12" md="6">
        <v-card>
          <v-card-title>{{ $t('reading_stats.genres_distribution') }}</v-card-title>
          <v-card-text>
            <e-charts-pie-chart v-if="genreChartData.length > 0" :data="genreChartData" :legend="true"/>
            <div v-else class="text-center py-8">
              <v-icon size="64" color="grey">mdi-chart-pie</v-icon>
              <div class="mt-2">{{ $t('reading_stats.no_data_available') }}</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>

      <v-col cols="12" md="6">
        <v-card>
          <v-card-title>{{ $t('reading_stats.tags_distribution') }}</v-card-title>
          <v-card-text>
            <e-charts-pie-chart v-if="tagChartData.length > 0" :data="tagChartData" :legend="true"/>
            <div v-else class="text-center py-8">
              <v-icon size="64" color="grey">mdi-chart-pie</v-icon>
              <div class="mt-2">{{ $t('reading_stats.no_data_available') }}</div>
            </div>
          </v-card-text>
        </v-card>
      </v-col>
    </v-row>
  </v-container>
</template>

<script lang="ts">
import Vue from 'vue'
import { BookDto } from '@/types/komga-books'
import { ReadStatus } from '@/types/enum-books'
import { SearchConditionReadStatus, SearchConditionLibraryId, SearchOperatorIsNot, SearchOperatorIs } from '@/types/komga-search'
import EChartsLineChart from '@/components/EChartsLineChart.vue'
import EChartsBarChart from '@/components/EChartsBarChart.vue'
import EChartsPieChart from '@/components/EChartsPieChart.vue'
import EChartsWordCloud from '@/components/EChartsWordCloud.vue'
import _ from 'lodash'

interface ReadingStats {
  totalBooks: number
  booksStartedReading: number
  booksCompletedReading: number
  totalPagesRead: number
  averagePagesPerBook: number
  readingStreak: number
  estimatedReadingTime: number
}

export default Vue.extend({
  name: 'ReadingStatsView',
  components: {
    EChartsLineChart,
    EChartsBarChart,
    EChartsPieChart,
    EChartsWordCloud,
  },
  data: () => ({
    loading: true,
    processingStats: false,
    processingCharts: false,
    processingDimensions: false,
    timeRange: 'last30days' as string,
    selectedLibraryId: 'all' as string, // 默认全库统计
    books: [] as BookDto[],
    stats: {
      totalBooks: 0,
      booksStartedReading: 0,
      booksCompletedReading: 0,
      totalPagesRead: 0,
      averagePagesPerBook: 0,
      readingStreak: 0,
      estimatedReadingTime: 0,
    } as ReadingStats,
    statusData: [] as any[],
    dailyData: [] as any[],
    hourlyData: [] as any[],
    topAuthors: [] as any[],
    topGenres: [] as any[],
    topTags: [] as any[],
    genreChartData: [] as any[],
    tagChartData: [] as any[],
    authorWords: [] as any[],
    genreWords: [] as any[],
    tagWords: [] as any[],
    timeRangeOptions: [] as any[],
    totalReadingTime: 0,
    averageDailyTime: 0,
    lastReadDate: null as Date | null,
    loadingProgress: 0,
    loadingMessage: '',

    // 性能优化缓存
    seriesMap: new Map<string, any>(),
    processedBooks: [] as any[],
    chartDataCache: [] as any[],
    lastTimeRange: '' as string,
    lastLibraryId: '' as string,
    chartKey: 0,
  }),
  computed: {
    chartData(): any[] {
      // 使用缓存优化，避免每次都重新计算
      if (!this.chartDataCache.length ||
          this.lastTimeRange !== this.timeRange ||
          this.lastLibraryId !== this.selectedLibraryId) {
        this.updateChartDataCache()
      }
      return this.chartDataCache
    },
    libraryOptions(): any[] {
      const libraries = this.$store.getters.getLibraries.map((lib: any) => ({
        id: lib.id,
        name: lib.name,
      }))
      // 在开头添加"全库统计"选项
      return [
        { id: 'all', name: this.$t('common.all_libraries').toString() },
        ...libraries,
      ]
    },
    filteredBooks(): BookDto[] {
      if (this.selectedLibraryId === 'all') {
        return this.books
      }
      return this.books.filter(book => book.libraryId === this.selectedLibraryId)
    },
  },
  created() {
    this.initializeTimeRangeOptions()
  },
  mounted() {
    this.loadData()
  },
  watch: {
    timeRange() {
      // 当时间范围改变时，重新生成图表数据
      if (this.books.length > 0) {
        this.generateTimeRangeData()
      }
    },
    selectedLibraryId: _.debounce(function() {
      // 当选择的库改变时，重新计算所有统计数据，添加防抖避免频繁触发
      if (this.books.length > 0) {
        this.clearCache() // 清除旧缓存
        this.preprocessBooks() // 预处理新数据
        this.calculateStatsAsync()
        this.generateChartDataAsync()
        this.generateDimensionDataAsync()
        this.generateTimeRangeData()
      }
    }, 300),
  },
  methods: {
    // 系列查找方法，直接从内存查找，无需API调用
    getSeriesCached(seriesId: string) {
      return this.seriesMap.get(seriesId) || null
    },

    // 预处理书籍数据，提高后续计算性能
    preprocessBooks() {
      this.processedBooks = this.filteredBooks
        .filter(book => book.readProgress?.completed && book.readProgress.readDate)
        .map(book => ({
          ...book,
          _processedDate: this.parseDate(book.readProgress!.readDate),
          _readingTimeHours: (book.readProgress?.page || 0) * 2 / 60,
        }))
    },

    // 解析日期的辅助方法
    parseDate(dateInput: string | Date): Date {
      if (typeof dateInput === 'string') {
        const dateStr = dateInput as string
        return new Date(dateStr.includes('T') ? dateStr : `${dateStr}T00:00:00`)
      } else {
        return new Date(dateInput)
      }
    },

    // 更新图表数据缓存
    updateChartDataCache() {
      this.chartDataCache = this.generateTimeRangeData()
      this.lastTimeRange = this.timeRange
      this.lastLibraryId = this.selectedLibraryId
      this.chartKey++ // 强制更新图表
    },

    // 清理缓存，释放内存
    clearCache() {
      this.processedBooks = []
      this.chartDataCache = []
      this.lastTimeRange = ''
      this.lastLibraryId = ''
    },

    async loadData() {
      this.loading = true
      this.loadingProgress = 0
      this.loadingMessage = this.$t('reading_stats.loading_books').toString()

      try {
        // 步骤1: 获取有阅读记录的书籍数据 (15%)
        const readBooksCondition = new SearchConditionReadStatus(new SearchOperatorIsNot(ReadStatus.UNREAD))
        const booksSearch = {
          condition: readBooksCondition,
        }
        const allBooksResponse = await this.$komgaBooks.getBooksList(booksSearch, { unpaged: true })
        this.books = allBooksResponse.content
        this.loadingProgress = 15
        this.loadingMessage = this.$t('reading_stats.loading_read_books').toString()

        // 步骤2: 获取有阅读记录的系列数据 (25%)
        const readStatusCondition = new SearchConditionReadStatus(new SearchOperatorIsNot(ReadStatus.UNREAD))
        const seriesSearch = {
          condition: readStatusCondition,
        }
        const allSeriesResponse = await this.$komgaSeries.getSeriesList(seriesSearch, { unpaged: true })
        this.seriesMap = new Map(allSeriesResponse.content.map(series => [series.id, series]))
        this.loadingProgress = 25
        this.loadingMessage = this.$t('reading_stats.loading_read_series').toString()

        // 步骤3: 计算基础统计数据 (35%)
        this.processingStats = true
        await this.calculateStatsAsync()
        this.processingStats = false
        this.loadingProgress = 35
        this.loadingMessage = this.$t('reading_stats.generating_charts').toString()

        // 步骤4: 生成图表数据 (55%)
        this.processingCharts = true
        await this.generateChartDataAsync()
        this.processingCharts = false
        this.loadingProgress = 55
        this.loadingMessage = this.$t('reading_stats.analyzing_dimensions').toString()

        // 步骤5: 生成维度统计数据 (75%)
        this.processingDimensions = true
        await this.generateDimensionDataAsync()
        this.processingDimensions = false
        this.loadingProgress = 75
        this.loadingMessage = this.$t('reading_stats.finalizing').toString()

        // 预处理书籍数据，提高后续性能
        this.preprocessBooks()

        // 最后生成时间范围图表数据并更新缓存
        this.updateChartDataCache()
        this.loadingProgress = 100
        this.loadingMessage = this.$t('reading_stats.completed').toString()

      } catch (error) {
      } finally {
        this.loading = false
        this.loadingProgress = 0
        this.loadingMessage = ''
      }
    },

    async calculateStatsAsync() {
      // 使用 nextTick 确保UI更新
      await this.$nextTick()

      // 获取当前库的总书籍数
      this.stats.totalBooks = await this.getLibraryTotalBooks()

      const readBooks = this.filteredBooks.filter(book =>
        book.readProgress && book.readProgress.completed,
      )

      // 已读过书籍数（已阅读+阅读中的书籍）
      const booksWithProgress = this.filteredBooks.filter(book =>
        book.readProgress !== undefined && book.readProgress !== null,
      )

      // 已读完书籍数（完成阅读的书籍）
      const completedBooks = this.filteredBooks.filter(book =>
        book.readProgress && book.readProgress.completed,
      )

      this.stats.booksStartedReading = booksWithProgress.length
      this.stats.booksCompletedReading = completedBooks.length
      this.stats.totalPagesRead = readBooks.reduce((total, book) =>
        total + (book.readProgress?.page || 0), 0,
      )
      this.stats.averagePagesPerBook = readBooks.length > 0
        ? Math.round(this.stats.totalPagesRead / readBooks.length)
        : 0

      // 估算阅读时间（假设每分钟阅读2页）
      this.stats.estimatedReadingTime = Math.round(this.stats.totalPagesRead / 2 / 60)

      // 计算最后阅读日期
      this.calculateLastReadDate(readBooks)

      // 计算阅读天数
      this.calculateReadingStreak(readBooks)

      // 添加小延迟以显示处理效果
      await new Promise(resolve => setTimeout(resolve, 100))
    },

    calculateLastReadDate(readBooks: BookDto[]) {
      let latest: Date | null = null

      for (const book of readBooks) {
        const dateStr = book.readProgress?.readDate
        if (dateStr) {
          const date = new Date(dateStr)
          if (!latest || date > latest) {
            latest = date
          }
        }
      }

      this.lastReadDate = latest
    },

    calculateReadingStreak(readBooks: BookDto[]) {
      if (readBooks.length === 0) {
        this.stats.readingStreak = 0
        return
      }

      // 计算总阅读天数（有阅读记录的不同日期数量）
      const uniqueDates = new Set<string>()

      readBooks
        .filter(book => book.readProgress?.readDate)
        .forEach(book => {
          const readDate = new Date(book.readProgress!.readDate)
          const dateKey = readDate.toISOString().split('T')[0] // YYYY-MM-DD格式
          uniqueDates.add(dateKey)
        })

      this.stats.readingStreak = uniqueDates.size
    },

    async generateChartDataAsync() {
      await this.$nextTick()

      // 阅读状态分布
      this.generateStatusData()

      // 每日阅读习惯
      this.generateDailyData()

      // 每小时阅读习惯
      this.generateHourlyData()

      // 添加小延迟以显示处理效果
      await new Promise(resolve => setTimeout(resolve, 150))
    },

    generateStatusData() {
      const statusCounts = {
        [ReadStatus.UNREAD]: 0,
        [ReadStatus.IN_PROGRESS]: 0,
        [ReadStatus.READ]: 0,
      }

      // 计算有阅读记录的书籍状态
      this.filteredBooks.forEach(book => {
        if (!book.readProgress) {
          statusCounts[ReadStatus.UNREAD]++
        } else if (book.readProgress.completed) {
          statusCounts[ReadStatus.READ]++
        } else {
          statusCounts[ReadStatus.IN_PROGRESS]++
        }
      })

      // 计算未阅读的书籍数：总书籍数减去有阅读记录的书籍数
      const readBooksCount = this.filteredBooks.length
      statusCounts[ReadStatus.UNREAD] = this.stats.totalBooks - readBooksCount

      this.statusData = Object.entries(statusCounts)
        .filter(([, count]) => count > 0)
        .map(([status, count]) => ({
          name: this.$t(`enums.read_status.${status}`).toString(),
          value: count,
        }))
    },

    generateDailyData() {
      const dayNames = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday']
      const dailyMap = new Map<string, number>()

      this.filteredBooks
        .filter(book => book.readProgress?.completed && book.readProgress.readDate)
        .forEach(book => {
          const date = new Date(book.readProgress!.readDate)
          const dayName = dayNames[date.getDay()]
          dailyMap.set(dayName, (dailyMap.get(dayName) || 0) + 1)
        })

      this.dailyData = dayNames.map(dayName => ({
        name: this.$t(`common.day.${dayName.toLowerCase()}`).toString(),
        value: dailyMap.get(dayName) || 0,
      }))
    },

    generateHourlyData() {
      const hourlyMap = new Map<number, number>()

      this.filteredBooks
        .filter(book => book.readProgress?.completed && book.readProgress.readDate)
        .forEach(book => {
          const date = new Date(book.readProgress!.readDate)
          const hour = date.getHours()
          hourlyMap.set(hour, (hourlyMap.get(hour) || 0) + 1)
        })

      this.hourlyData = Array.from({ length: 24 }, (_, hour) => ({
        name: `${hour}:00`,
        value: hourlyMap.get(hour) || 0,
      }))
    },


    async generateDimensionDataAsync() {
      await this.$nextTick()

      await this.generateAuthorData()
      await this.generateGenreData()
      await this.generateTagData()

      // 生成去重后的词云数据
      this.generateWordCloudData()

      // 添加小延迟以显示处理效果
      await new Promise(resolve => setTimeout(resolve, 200))
    },

    async generateAuthorData() {
      const authorMap = new Map<string, number>()

      // 获取所有相关的系列信息，使用缓存优化
      const seriesIds = new Set(
        this.filteredBooks
          .filter(book => book.readProgress?.completed)
          .map(book => book.seriesId),
      )

      const seriesPromises = Array.from(seriesIds).map(seriesId => this.getSeriesCached(seriesId))
      const seriesList = await Promise.all(seriesPromises)
      const seriesMap = new Map<string, any>()

      seriesList.forEach(series => {
        if (series) {
          seriesMap.set(series.id, series)
        }
      })

      // 统计作者数据（按系列去重，避免同一系列重复统计）
      const processedSeries = new Set<string>()

      this.filteredBooks
        .filter(book => book.readProgress?.completed)
        .forEach(book => {
          // 跳过已处理的系列
          if (processedSeries.has(book.seriesId)) {
            return
          }

          const series = seriesMap.get(book.seriesId)

          // 收集该系列的所有作者（书籍作者 + 系列作者），去重
          const seriesAuthors = new Set<string>()

          // 添加系列作者
          if (series?.metadata?.authors) {
            series.metadata.authors.forEach((author: any) => seriesAuthors.add(author.name))
          }

          // 添加该系列下所有书籍的作者
          const seriesBooks = this.filteredBooks.filter(b =>
            b.seriesId === book.seriesId && b.readProgress?.completed,
          )
          seriesBooks.forEach(seriesBook => {
            if (seriesBook.metadata.authors) {
              seriesBook.metadata.authors.forEach(author => seriesAuthors.add(author.name))
            }
          })

          // 统计该系列的作者（每个系列只统计一次）
          seriesAuthors.forEach(author => {
            authorMap.set(author, (authorMap.get(author) || 0) + 1)
          })

          // 标记该系列已处理
          processedSeries.add(book.seriesId)
        })

      this.topAuthors = Array.from(authorMap.entries())
        .map(([name, count]) => ({ name, count }))
        .sort((a, b) => b.count - a.count)
    },

    async generateGenreData() {
      const genreMap = new Map<string, number>()

      // 获取所有相关的系列信息，使用缓存优化
      const seriesIds = new Set(
        this.filteredBooks
          .filter(book => book.readProgress?.completed)
          .map(book => book.seriesId),
      )

      const seriesPromises = Array.from(seriesIds).map(seriesId => this.getSeriesCached(seriesId))
      const seriesList = await Promise.all(seriesPromises)
      const seriesMap = new Map<string, any>()

      seriesList.forEach(series => {
        if (series) {
          seriesMap.set(series.id, series)
        }
      })

      // 统计类型数据（按系列去重，避免同一系列重复统计）
      const processedSeries = new Set<string>()

      this.filteredBooks
        .filter(book => book.readProgress?.completed)
        .forEach(book => {
          // 跳过已处理的系列
          if (processedSeries.has(book.seriesId)) {
            return
          }

          const series = seriesMap.get(book.seriesId)
          if (series?.metadata?.genres) {
            series.metadata.genres.forEach((genre: string) => {
              genreMap.set(genre, (genreMap.get(genre) || 0) + 1)
            })
          }

          // 标记该系列已处理
          processedSeries.add(book.seriesId)
        })

      this.topGenres = Array.from(genreMap.entries())
        .map(([name, count]) => ({ name, count }))
        .sort((a, b) => b.count - a.count)

      // 生成饼图数据（只显示前17个，其余归为"其他"）
      const topGenresForChart = this.topGenres.slice(0, 17)
      const otherGenres = this.topGenres.slice(17)
      const otherCount = otherGenres.reduce((sum, genre) => sum + genre.count, 0)

      this.genreChartData = topGenresForChart.map(genre => ({
        name: genre.name,
        value: genre.count,
      }))

      if (otherCount > 0) {
        this.genreChartData.push({
          name: this.$t('reading_stats.other').toString(),
          value: otherCount,
        })
      }

      // 确保总共不超过18个项目（17个具体项目 + 1个其他）
      if (this.genreChartData.length > 18) {
        this.genreChartData = this.genreChartData.slice(0, 18)
      }
    },

    async generateTagData() {
      const tagMap = new Map<string, number>()

      // 获取所有相关的系列信息，使用缓存优化
      const seriesIds = new Set(
        this.filteredBooks
          .filter(book => book.readProgress?.completed)
          .map(book => book.seriesId),
      )

      const seriesPromises = Array.from(seriesIds).map(seriesId => this.getSeriesCached(seriesId))
      const seriesList = await Promise.all(seriesPromises)
      const seriesMap = new Map<string, any>()

      seriesList.forEach(series => {
        if (series) {
          seriesMap.set(series.id, series)
        }
      })

      // 统计标签数据（按系列去重，避免同一系列重复统计）
      const processedSeries = new Set<string>()

      this.filteredBooks
        .filter(book => book.readProgress?.completed)
        .forEach(book => {
          // 跳过已处理的系列
          if (processedSeries.has(book.seriesId)) {
            return
          }

          const series = seriesMap.get(book.seriesId)

          // 收集该系列的所有标签（书籍标签 + 系列标签），去重
          const seriesTags = new Set<string>()

          // 添加系列标签
          if (series?.metadata?.tags) {
            series.metadata.tags.forEach((tag: string) => seriesTags.add(tag))
          }

          // 添加该系列下所有书籍的标签
          const seriesBooks = this.filteredBooks.filter(b =>
            b.seriesId === book.seriesId && b.readProgress?.completed,
          )
          seriesBooks.forEach(seriesBook => {
            if (seriesBook.metadata.tags) {
              seriesBook.metadata.tags.forEach(tag => seriesTags.add(tag))
            }
          })

          // 统计该系列的标签（每个系列只统计一次）
          seriesTags.forEach(tag => {
            tagMap.set(tag, (tagMap.get(tag) || 0) + 1)
          })

          // 标记该系列已处理
          processedSeries.add(book.seriesId)
        })

      this.topTags = Array.from(tagMap.entries())
        .map(([name, count]) => ({ name, count }))
        .sort((a, b) => b.count - a.count)

      // 生成饼图数据（只显示前17个，其余归为"其他"）
      const topTagsForChart = this.topTags.slice(0, 17)
      const otherTags = this.topTags.slice(17)
      const otherCount = otherTags.reduce((sum, tag) => sum + tag.count, 0)

      this.tagChartData = topTagsForChart.map(tag => ({
        name: tag.name,
        value: tag.count,
      }))

      if (otherCount > 0) {
        this.tagChartData.push({
          name: this.$t('reading_stats.other').toString(),
          value: otherCount,
        })
      }

      // 确保总共不超过18个项目（17个具体项目 + 1个其他）
      if (this.tagChartData.length > 18) {
        this.tagChartData = this.tagChartData.slice(0, 18)
      }
    },

    generateWordCloudData() {
      // 生成作者词云数据，确保文本唯一性
      const authorTextSet = new Set<string>()
      this.authorWords = this.topAuthors
        .filter(author => {
          if (authorTextSet.has(author.name)) {
            return false
          }
          authorTextSet.add(author.name)
          return true
        })
        .map(author => ({ text: author.name, weight: author.count }))

      // 生成类型词云数据，确保文本唯一性
      const genreTextSet = new Set<string>()
      this.genreWords = this.topGenres
        .filter(genre => {
          if (genreTextSet.has(genre.name)) {
            return false
          }
          genreTextSet.add(genre.name)
          return true
        })
        .map(genre => ({ text: genre.name, weight: genre.count }))

      // 生成标签词云数据，确保文本唯一性
      const tagTextSet = new Set<string>()
      this.tagWords = this.topTags
        .filter(tag => {
          if (tagTextSet.has(tag.name)) {
            return false
          }
          tagTextSet.add(tag.name)
          return true
        })
        .map(tag => ({ text: tag.name, weight: tag.count }))
    },

    formatDate(date: Date | undefined): string {
      if (!date) return ''
      return new Intl.DateTimeFormat(this.$i18n.locale, {
        year: 'numeric',
        month: 'short',
        day: 'numeric',
      }).format(new Date(date))
    },

    formatTimeAgo(date: Date): string {
      const now = new Date()
      const diffMs = now.getTime() - date.getTime()
      const diffMinutes = Math.floor(diffMs / (1000 * 60))
      const diffHours = Math.floor(diffMs / (1000 * 60 * 60))
      const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24))

      if (diffMinutes < 1) {
        return this.$t('reading_stats.just_now').toString()
      } else if (diffMinutes < 60) {
        return this.$t('reading_stats.minutes_ago', { count: diffMinutes }).toString()
      } else if (diffHours < 24) {
        return this.$t('reading_stats.hours_ago', { count: diffHours }).toString()
      } else if (diffDays < 30) {
        return this.$t('reading_stats.days_ago', { count: diffDays }).toString()
      } else {
        const months = Math.floor(diffDays / 30)
        if (months < 12) {
          return this.$t('reading_stats.months_ago', { count: months }).toString()
        } else {
          const years = Math.floor(months / 12)
          return this.$t('reading_stats.years_ago', { count: years }).toString()
        }
      }
    },

    initializeTimeRangeOptions() {
      this.timeRangeOptions = [
        { text: this.$t('reading_stats.this_week').toString(), value: 'thisWeek' },
        { text: this.$t('reading_stats.last_7_days').toString(), value: 'last7days' },
        { text: this.$t('reading_stats.last_30_days').toString(), value: 'last30days' },
        { text: this.$t('reading_stats.last_90_days').toString(), value: 'last90days' },
        { text: this.$t('reading_stats.last_6_months').toString(), value: 'last6months' },
        { text: this.$t('reading_stats.last_year').toString(), value: 'lastYear' },
        { text: this.$t('reading_stats.all_time').toString(), value: 'all' },
      ]
    },

    formatReadingTime(hours: number): string {
      if (hours < 1) {
        const minutes = Math.round(hours * 60)
        return `${minutes} ${this.$t('reading_stats.minutes').toString()}`
      } else {
        const wholeHours = Math.floor(hours)
        const remainingMinutes = Math.round((hours % 1) * 60)
        if (remainingMinutes === 0) {
          return `${wholeHours} ${this.$t('reading_stats.hours').toString()}`
        } else {
          return `${wholeHours} ${this.$t('reading_stats.hours').toString()} ${remainingMinutes} ${this.$t('reading_stats.minutes').toString()}`
        }
      }
    },

    formatReadingTimeForBook(book: any): string {
      // 估算单本书的阅读时长（假设每页阅读时间为2分钟）
      const readingTimeMinutes = (book.readProgress?.page || 0) * 2
      const readingTimeHours = readingTimeMinutes / 60
      return this.formatReadingTime(readingTimeHours)
    },

    generateTimeRangeData(): any[] {
      const now = new Date()

      let startDate: Date = new Date(now)
      let endDate: Date = new Date(now)
      let groupBy: 'day' | 'month' | 'year' = 'day'

      // 设置endDate为今天的结束时间，确保包含今天的所有数据
      endDate.setUTCHours(23, 59, 59, 999)

      // 根据时间范围设置开始日期和分组方式
      switch (this.timeRange) {
        case 'thisWeek':
          startDate = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() - now.getUTCDay()))
          groupBy = 'day'
          break
        case 'last7days':
          startDate = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() - 6))
          groupBy = 'day'
          break
        case 'last30days':
          startDate = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate() - 29))
          groupBy = 'day'
          break
        case 'last90days':
          startDate = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth() - 2, 1))
          groupBy = 'month'
          break
        case 'last6months':
          startDate = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth() - 5, 1))
          groupBy = 'month'
          break
        case 'lastYear':
          startDate = new Date(Date.UTC(now.getUTCFullYear() - 1, now.getUTCMonth() + 1, 1))
          groupBy = 'month'
          break
        case 'all':
        default:
          // 找到最早的阅读日期
          const earliestDate = this.filteredBooks
            .filter(book => book.readProgress?.completed && book.readProgress.readDate)
            .reduce((earliest, book) => {
              const readDate = new Date(book.readProgress!.readDate)
              return readDate < earliest ? readDate : earliest
            }, new Date())

          // 从最早阅读日期开始
          startDate = new Date(Date.UTC(earliestDate.getUTCFullYear(), earliestDate.getUTCMonth(), earliestDate.getUTCDate()))
          groupBy = 'year'
          break
      }

      // 使用预处理的书籍数据，提高性能
      const filteredBooks = this.processedBooks.filter(book => {
        const readDate = book._processedDate
        // 确保只比较日期部分，不比较时间（使用UTC避免时区问题）
        const readDateUTC = new Date(readDate.getUTCFullYear(), readDate.getUTCMonth(), readDate.getUTCDate())
        const startDateUTC = new Date(startDate.getUTCFullYear(), startDate.getUTCMonth(), startDate.getUTCDate())
        const endDateUTC = new Date(endDate.getUTCFullYear(), endDate.getUTCMonth(), endDate.getUTCDate())

        const isInRange = readDateUTC >= startDateUTC && readDateUTC <= endDateUTC
        return isInRange
      })

      // 按时间分组并计算阅读时长，使用预处理的阅读时间
      const timeMap = new Map<string, number>()

      filteredBooks.forEach(book => {
        const readDate = book._processedDate
        let key: string

        switch (groupBy) {
          case 'day':
            key = readDate.toISOString().split('T')[0] // YYYY-MM-DD
            break
          case 'month':
            key = `${readDate.getFullYear()}-${String(readDate.getMonth() + 1).padStart(2, '0')}` // YYYY-MM
            break
          case 'year':
            key = readDate.getFullYear().toString() // YYYY
            break
        }

        // 使用预处理的阅读时间数据
        timeMap.set(key, (timeMap.get(key) || 0) + book._readingTimeHours)
      })

      // 生成连续的时间序列，确保包含所有日期
      const result: any[] = []
      const current = new Date(startDate)
      const end = new Date(now)
      // 确保endDate包含今天 - 使用UTC避免时区问题
      const endDateUTC = new Date(Date.UTC(end.getUTCFullYear(), end.getUTCMonth(), end.getUTCDate()))

      // 强制确保今天的数据点被包含（对于所有时间范围）
      const today = new Date(now)
      const todayKey = today.toISOString().split('T')[0]
      if (!timeMap.has(todayKey)) {
        timeMap.set(todayKey, 0)
      }

      // 如果startDate晚于今天，确保至少包含今天
      if (startDate > endDateUTC) {
        const todayDataPoint = {
          name: this.formatAxisLabel(todayKey, 'day'),
          value: 0,
        }
        result.push(todayDataPoint)
        return result
      }

      while (current <= endDateUTC) {
        let key: string

        switch (groupBy) {
          case 'day':
            key = current.toISOString().split('T')[0]
            current.setUTCDate(current.getUTCDate() + 1)
            break
          case 'month':
            key = `${current.getUTCFullYear()}-${String(current.getUTCMonth() + 1).padStart(2, '0')}`
            current.setUTCMonth(current.getUTCMonth() + 1)
            break
          case 'year':
            key = current.getUTCFullYear().toString()
            current.setUTCFullYear(current.getUTCFullYear() + 1)
            break
        }

        const value = timeMap.get(key) || 0
        result.push({
          name: this.formatAxisLabel(key, groupBy),
          value: Math.round(value * 100) / 100, // 保留两位小数的小时
        })
      }

      // 确保今天的数据点在结果中（额外保险）
      const hasToday = result.some(item => {
        if (groupBy === 'day') {
          return item.name === this.formatAxisLabel(todayKey, 'day')
        }
        // 对于月度和年度视图，今天应该已经包含在当前月份/年份中
        return true
      })

      if (!hasToday && groupBy === 'day') {
        result.push({
          name: this.formatAxisLabel(todayKey, 'day'),
          value: 0,
        })
      }

      // 计算总阅读时长和日均阅读时长
      this.totalReadingTime = result.reduce((sum, item) => sum + item.value, 0)

      // 计算实际的天数，而不是数据点的数量
      const actualDaysCount = Math.ceil((end.getTime() - startDate.getTime()) / (1000 * 60 * 60 * 24)) + 1
      this.averageDailyTime = actualDaysCount > 0 ? this.totalReadingTime / actualDaysCount : 0

      return result
    },

    formatAxisLabel(key: string, groupBy: 'day' | 'month' | 'year'): string {
      switch (groupBy) {
        case 'day':
          const date = new Date(key + 'T00:00:00')
          return date.toLocaleDateString(this.$i18n.locale, { month: 'short', day: 'numeric' })
        case 'month':
          const [year, month] = key.split('-')
          const monthDate = new Date(parseInt(year), parseInt(month) - 1, 1)
          return monthDate.toLocaleDateString(this.$i18n.locale, { year: 'numeric', month: 'short' })
        case 'year':
          return key
        default:
          return key
      }
    },

    // 获取当前库的总书籍数
    async getLibraryTotalBooks(): Promise<number> {
      try {
        if (this.selectedLibraryId === 'all') {
          // 全库统计：获取所有书籍数
          const response = await this.$komgaBooks.getBooksList({}, { page: 0, size: 1 })
          return response.totalElements
        } else {
          // 特定库：获取该库的所有书籍数
          const libraryCondition = new SearchConditionLibraryId(new SearchOperatorIs(this.selectedLibraryId))
          const search = {
            condition: libraryCondition,
          }
          const response = await this.$komgaBooks.getBooksList(search, { page: 0, size: 1 })
          return response.totalElements
        }
      } catch (error) {
        // 出错时返回当前过滤书籍的数量作为fallback
        return this.filteredBooks.length
      }
    },
  },
})
</script>
<style scoped>
/* 统计卡片统一高度和样式 */
.stats-overview-row {
  margin-bottom: 24px;
}

.stats-card {
  display: flex;
  flex-direction: column;
  transition: all 0.3s ease;
  border-radius: 12px;
  overflow: hidden;
}

.stats-card:hover {
  transform: translateY(-2px);
  box-shadow: 0 8px 25px rgba(0, 0, 0, 0.15);
}

.stats-card .v-card__title {
  padding: 16px 16px 8px;
  margin-bottom: 0;
}

.stats-card .v-card__text {
  flex-grow: 1;
  display: flex;
  flex-direction: column;
  justify-content: center;
  padding: 16px !important;
}

/* 响应式布局优化 */
@media (min-width: 1904px) { /* xl */
  .stats-overview-row .v-col {
    flex: 0 0 16.666%;
    max-width: 16.666%;
  }
}

@media (max-width: 1264px) { /* lg */
  .stats-overview-row .v-col {
    flex: 0 0 20%;
    max-width: 20%;
  }
}

@media (max-width: 960px) { /* md */
  .stats-overview-row .v-col {
    flex: 0 0 25%;
    max-width: 25%;
  }
}

@media (max-width: 600px) { /* sm */
  .stats-overview-row .v-col {
    flex: 0 0 50%;
    max-width: 50%;
  }

  .stats-card {
    height: 120px !important;
  }

  .stats-card .v-card__text {
    padding: 12px !important;
  }
}

/* 图表区域样式 */
.chart-section {
  margin-bottom: 24px;
}

/* 词云图卡片样式 */
.word-cloud-card {
  display: flex;
  flex-direction: column;
  transition: all 0.3s ease;
  border-radius: 12px;
  overflow: hidden;
}

.word-cloud-card:hover {
  transform: translateY(-2px);
  box-shadow: 0 8px 25px rgba(0, 0, 0, 0.15);
}

.word-cloud-title {
  padding: 16px 16px 8px;
  margin-bottom: 0;
  font-size: 1rem;
  font-weight: 500;
}

.word-cloud-container {
  display: flex;
  justify-content: center;
  align-items: center;
  width: 100%;
  height: 100%;
  border-radius: 8px;
  background: var(--v-card-base);
  transition: background-color 0.3s ease;
}

/* 时间范围选择器样式 */
.time-range-select {
  min-width: 180px;
}

.time-range-select .v-select__selection {
  font-size: 0.875rem;
}

/* 深色模式适配 */
.theme--dark .word-cloud-container {
  background: rgba(255, 255, 255, 0.03);
}

/* 响应式设计 */
@media (max-width: 600px) {
  .stats-card {
    height: 100px !important;
  }

  .stats-card .v-card__title {
    font-size: 0.9rem !important;
    padding: 12px 12px 6px;
  }

  .stats-card .v-card__text {
    padding: 8px !important;
  }

  .stats-card .text-h3 {
    font-size: 1.5rem !important;
  }
}
</style>
