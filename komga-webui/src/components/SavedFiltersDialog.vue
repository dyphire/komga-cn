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
        <v-icon left>mdi-bookmark-multiple</v-icon>
        {{ $t('filter.saved_filters') }}
        <v-spacer />
        <v-btn icon @click="dialog = false">
          <v-icon>mdi-close</v-icon>
        </v-btn>
      </v-card-title>

      <v-card-text class="pa-0">
        <div v-if="savedFilters.length === 0" class="empty-state">
          <v-icon size="64" color="grey lighten-2">mdi-bookmark-off-outline</v-icon>
          <h3 class="text-h6 grey--text text--darken-1 mt-4">{{ $t('filter.no_saved_filters') }}</h3>
          <p class="text-body-2 grey--text mt-2">{{ $t('filter.create_first_filter') }}</p>
        </div>
        <div v-else class="saved-filters-grid">
          <v-tooltip
            v-for="(filter, index) in savedFilters"
            :key="index"
            bottom
            max-width="300"
            :disabled="!isQueryTruncated(filter.query)"
            content-class="my-tooltip"
          >
            <template v-slot:activator="{ on }">
              <v-card
                class="saved-filter-card"
                elevation="2"
                @click="loadFilter(filter)"
                v-on="isQueryTruncated(filter.query) ? on : {}"
              >
                <v-card-text class="pa-2">
                  <div class="d-flex align-center justify-space-between mb-1">
                    <span class="filter-name text-caption font-weight-medium">
                      {{ filter.name || $t('filter.unnamed_filter') }}
                    </span>
                    <v-btn
                      icon
                      x-small
                      @click.stop="deleteFilter(index)"
                      class="delete-btn pa-0"
                    >
                      <v-icon size="10">mdi-delete</v-icon>
                    </v-btn>
                  </div>

                  <div class="filter-query">
                    <code class="query-text" :class="{ 'truncated': isQueryTruncated(filter.query) }">
                      {{ filter.query }}
                    </code>
                  </div>
                </v-card-text>
              </v-card>
            </template>
            <div class="tooltip-query">
              {{ filter.query }}
            </div>
          </v-tooltip>
        </div>
      </v-card-text>

      <v-card-actions>
        <v-spacer />
        <v-btn @click="dialog = false" text>
          {{ $t('common.close') }}
        </v-btn>
      </v-card-actions>
    </v-card>

    <!-- 删除确认对话框 -->
    <v-dialog
      v-model="showDeleteDialog"
      max-width="400px"
      persistent
    >
      <v-card>
        <v-card-title class="headline error white--text" style="background: linear-gradient(90deg, #f44336, #e53935);">
          <v-icon left color="white">mdi-delete-alert</v-icon>
          {{ $t('filter.confirm_delete') }}
        </v-card-title>

        <v-card-text class="pa-6">
          <div class="text-body-1 mb-4">
            {{ $t('filter.confirm_delete_message') }}
          </div>
          <div v-if="deleteIndex !== null" class="filter-preview pa-3">
            <div class="text-caption text--secondary mb-1">{{ $t('filter.filter_name') }}</div>
            <div class="font-weight-medium">{{ savedFilters[deleteIndex]?.name || $t('filter.unnamed_filter') }}</div>
            <div class="text-caption text--secondary mt-2 mb-1">{{ $t('filter.filter_query') }}</div>
            <code class="text-body-2 filter-code">
              {{ savedFilters[deleteIndex]?.query }}
            </code>
          </div>
        </v-card-text>

        <v-card-actions class="pa-6 pt-0">
          <v-spacer />
          <v-btn @click="cancelDelete" text color="grey">
            {{ $t('common.cancel') }}
          </v-btn>
          <v-btn @click="confirmDelete" color="error" depressed>
            <v-icon left>mdi-delete</v-icon>
            {{ $t('common.delete') }}
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>

    <!-- 保存筛选器命名对话框 -->
    <v-dialog
      v-model="showSaveDialog"
      max-width="500px"
      persistent
    >
      <v-card>
        <v-card-title class="headline primary white--text" style="background: linear-gradient(90deg, #1976d2, #42a5f5);">
          <v-icon left color="white">mdi-bookmark-plus</v-icon>
          {{ $t('filter.save_filter') }}
        </v-card-title>

        <v-card-text class="pa-6">
          <div class="text-body-1 mb-4">
            {{ $t('filter.save_filter_message') }}
          </div>

          <v-text-field
            v-model="saveFilterName"
            :label="$t('filter.filter_name')"
            :placeholder="$t('filter.filter_name_placeholder')"
            outlined
            dense
            autofocus
            :rules="[v => !!v || $t('filter.filter_name_required')]"
            @keyup.enter="confirmSave"
          />

          <div v-if="pendingFilterToSave" class="filter-preview mt-4 pa-3">
            <div class="text-caption text--secondary mb-1">{{ $t('filter.filter_query') }}</div>
            <code class="text-body-2 filter-code">
              {{ pendingFilterToSave.query }}
            </code>
          </div>
        </v-card-text>

        <v-card-actions class="pa-6 pt-0">
          <v-spacer />
          <v-btn @click="cancelSave" text color="grey">
            {{ $t('common.cancel') }}
          </v-btn>
          <v-btn @click="confirmSave" color="primary" depressed :disabled="!saveFilterName.trim()">
            <v-icon left>mdi-content-save</v-icon>
            {{ $t('common.save') }}
          </v-btn>
        </v-card-actions>
      </v-card>
    </v-dialog>
  </v-dialog>
</template>

<script lang="ts">
import Vue from 'vue'

export default Vue.extend({
  name: 'SavedFiltersDialog',
  props: {
    value: {
      type: Boolean,
      default: false,
    },
    filterToSave: {
      type: Object as () => {name: string, query: string} | null,
      default: null,
    },
    externalFilterToSave: {
      type: Object as () => {name: string, query: string} | null,
      default: null,
    },
  },
  data: () => ({
    savedFilters: [] as Array<{name: string, query: string}>,
    showDeleteDialog: false,
    deleteIndex: null as number | null,
    showSaveDialog: false,
    saveFilterName: '',
    pendingFilterToSave: null as {name: string, query: string} | null,
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
  },
  watch: {
    filterToSave: {
      handler(newFilter) {
        if (newFilter) {
          this.pendingFilterToSave = newFilter
          this.saveFilterName = ''
          this.showSaveDialog = true
        }
      },
      immediate: true,
    },
    externalFilterToSave: {
      handler(newFilter) {
        if (newFilter) {
          this.pendingFilterToSave = newFilter
          this.saveFilterName = ''
          this.showSaveDialog = true
        }
      },
      immediate: true,
    },
  },
  methods: {
    loadFilter(filter: {name: string, query: string}) {
      this.$emit('load', filter)
      this.dialog = false
    },
    deleteFilter(index: number) {
      this.showDeleteDialog = true
      this.deleteIndex = index
    },
    confirmDelete() {
      if (this.deleteIndex !== null) {
        this.savedFilters.splice(this.deleteIndex, 1)
        this.saveFiltersToStorage()
      }
      this.showDeleteDialog = false
      this.deleteIndex = null
    },
    cancelDelete() {
      this.showDeleteDialog = false
      this.deleteIndex = null
    },
    confirmSave() {
      if (this.pendingFilterToSave && this.saveFilterName.trim()) {
        const filterToSave = {
          name: this.saveFilterName.trim(),
          query: this.pendingFilterToSave.query,
        }
        this.saveFilter(filterToSave)
        this.showSaveDialog = false
        this.pendingFilterToSave = null
        this.saveFilterName = ''
        // Also reset external filter to save
        this.$emit('update:externalFilterToSave', null)
      }
    },
    cancelSave() {
      this.showSaveDialog = false
      this.pendingFilterToSave = null
      this.saveFilterName = ''
    },
    saveFilter(filter: {name: string, query: string}) {
      this.savedFilters.push(filter)
      this.saveFiltersToStorage()
      this.$emit('save', filter)
    },
    saveFiltersToStorage() {
      localStorage.setItem('komga_smart_filters', JSON.stringify(this.savedFilters))
    },
    loadFiltersFromStorage() {
      const saved = localStorage.getItem('komga_smart_filters')
      if (saved) {
        try {
          this.savedFilters = JSON.parse(saved)
        } catch (e) {
          this.savedFilters = []
        }
      }
    },
    isQueryTruncated(query: string): boolean {
      // 估算查询文本的显示宽度
      // 假设卡片宽度约140px，字体大小0.65rem，字符平均宽度约6px
      const estimatedWidth = query.length * 6
      const containerWidth = 120 // 卡片内容区域宽度估算
      return estimatedWidth > containerWidth
    },
  },
  mounted() {
    this.loadFiltersFromStorage()
  },
})
</script>

<style scoped>
/* 空状态样式 */
.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 64px 32px;
  text-align: center;
  background: linear-gradient(135deg, rgba(189, 189, 189, 0.1) 0%, rgba(189, 189, 189, 0.05) 100%);
  border-radius: 12px;
  margin: 16px;
  border: 2px dashed rgba(189, 189, 189, 0.3);
  position: relative;
  overflow: hidden;
}

.theme--light .empty-state {
  background: linear-gradient(135deg, rgba(189, 189, 189, 0.12) 0%, rgba(189, 189, 189, 0.06) 100%);
  border: 2px dashed rgba(189, 189, 189, 0.4);
}

.theme--dark .empty-state {
  background: linear-gradient(135deg, rgba(66, 66, 66, 0.15) 0%, rgba(66, 66, 66, 0.08) 100%);
  border: 2px dashed rgba(189, 189, 189, 0.5);
}

.empty-state::before {
  content: '';
  position: absolute;
  top: -50%;
  left: -50%;
  width: 200%;
  height: 200%;
  background: radial-gradient(circle, rgba(25, 118, 210, 0.03) 0%, transparent 70%);
  animation: float 6s ease-in-out infinite;
}

@keyframes float {
  0%, 100% { transform: translate(-50%, -50%) rotate(0deg); }
  50% { transform: translate(-50%, -50%) rotate(180deg); }
}

.empty-state-icon {
  margin-bottom: 16px;
  opacity: 0.6;
  filter: drop-shadow(0 2px 4px rgba(0, 0, 0, 0.1));
}

.theme--dark .empty-state-icon {
  opacity: 0.7;
  filter: drop-shadow(0 2px 4px rgba(0, 0, 0, 0.3));
}

.saved-filters-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
  gap: 12px;
  padding: 20px;
}

.saved-filter-card {
  transition: all 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  border-radius: 12px;
  overflow: hidden;
  position: relative;
  height: 100%;
  cursor: pointer;
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.9) 0%, rgba(255, 255, 255, 0.95) 100%);
  border: 1px solid rgba(0, 0, 0, 0.08);
}

.theme--light .saved-filter-card {
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.95) 0%, rgba(248, 250, 252, 1) 100%);
  border: 1px solid rgba(0, 0, 0, 0.1);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.08);
}

.theme--dark .saved-filter-card {
  background: linear-gradient(135deg, rgba(30, 30, 30, 0.95) 0%, rgba(45, 45, 45, 1) 100%);
  border: 1px solid rgba(255, 255, 255, 0.1);
  box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
}

.saved-filter-card:hover {
  transform: translateY(-2px) scale(1.02);
  box-shadow: 0 8px 25px rgba(0, 0, 0, 0.15) !important;
}

.theme--dark .saved-filter-card:hover {
  box-shadow: 0 8px 25px rgba(0, 0, 0, 0.4) !important;
}

.saved-filter-card::before {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 4px;
  background: linear-gradient(90deg, #1976d2, #42a5f5);
  border-radius: 12px 12px 0 0;
}

.theme--dark .saved-filter-card::before {
  background: linear-gradient(90deg, #2196f3, #64b5f6);
}

.filter-name {
  color: rgba(0, 0, 0, 0.87);
  font-weight: 600;
  font-size: 0.85rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
  line-height: 1.2;
  margin-bottom: 6px;
  display: flex;
  align-items: center;
}

.theme--dark .filter-name {
  color: rgba(255, 255, 255, 0.9);
}

.filter-name::before {
  content: '🏷️';
  margin-right: 4px;
  font-size: 0.8rem;
}

.delete-btn {
  opacity: 0.5;
  transition: all 0.2s ease;
  width: 20px !important;
  height: 20px !important;
  border-radius: 50%;
  background-color: rgba(244, 67, 54, 0.1);
  margin-left: 4px;
}

.theme--light .delete-btn {
  background-color: rgba(244, 67, 54, 0.08);
}

.theme--dark .delete-btn {
  background-color: rgba(244, 67, 54, 0.15);
}

.delete-btn:hover {
  opacity: 1;
  background-color: rgba(244, 67, 54, 0.2);
  transform: scale(1.1);
}

.theme--dark .delete-btn:hover {
  background-color: rgba(244, 67, 54, 0.3);
}

.query-text {
  font-family: 'JetBrains Mono', 'Fira Code', 'SF Mono', 'Monaco', 'Inconsolata', 'Roboto Mono', 'Courier New', monospace;
  font-size: 0.7rem;
  font-weight: 500;
  background: linear-gradient(135deg, rgba(0, 0, 0, 0.08) 0%, rgba(0, 0, 0, 0.05) 100%);
  color: rgba(0, 0, 0, 0.75);
  padding: 6px 8px;
  border-radius: 6px;
  border: 1px solid rgba(0, 0, 0, 0.1);
  display: block;
  line-height: 1.3;
  word-break: break-word;
  box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.05);
  transition: all 0.2s ease;
}

.theme--light .query-text {
  background: linear-gradient(135deg, rgba(0, 0, 0, 0.08) 0%, rgba(0, 0, 0, 0.05) 100%);
  color: rgba(0, 0, 0, 0.75);
  border: 1px solid rgba(0, 0, 0, 0.1);
}

.theme--dark .query-text {
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.12) 0%, rgba(255, 255, 255, 0.08) 100%);
  color: rgba(255, 255, 255, 0.8);
  border: 1px solid rgba(255, 255, 255, 0.15);
  box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.2);
}

.query-text:hover {
  background: linear-gradient(135deg, rgba(0, 0, 0, 0.12) 0%, rgba(0, 0, 0, 0.08) 100%);
}

.theme--dark .query-text:hover {
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.18) 0%, rgba(255, 255, 255, 0.12) 100%);
}

.query-text.truncated {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 响应式设计 */
@media (max-width: 600px) {
  .saved-filters-grid {
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    padding: 6px;
    gap: 4px;
  }

  .saved-filter-card {
    margin: 0;
  }
}

@media (min-width: 601px) and (max-width: 960px) {
  .saved-filters-grid {
    grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
  }
}

/* 深色主题支持 */
.theme--dark .saved-filter-card {
  background-color: #1e1e1e;
}

.theme--dark .saved-filter-card::before {
  background: linear-gradient(90deg, #2196f3, #64b5f6);
}

.theme--dark .filter-name {
  color: rgba(255, 255, 255, 0.87);
}

.theme--dark .query-text {
  background: rgba(255, 255, 255, 0.1);
  color: rgba(255, 255, 255, 0.7);
}

.theme--dark .empty-state h3 {
  color: rgba(255, 255, 255, 0.7) !important;
}

.theme--dark .empty-state p {
  color: rgba(255, 255, 255, 0.5) !important;
}

/* 对话框样式优化 */
.v-card-title {
  background: linear-gradient(135deg, #1976d2 0%, #42a5f5 100%);
  color: white;
  border-radius: 8px 8px 0 0;
  padding: 16px 24px;
  font-weight: 600;
  font-size: 1.1rem;
  box-shadow: 0 2px 8px rgba(25, 118, 210, 0.3);
}

.theme--dark .v-card-title {
  background: linear-gradient(135deg, #2196f3 0%, #64b5f6 100%);
  box-shadow: 0 2px 8px rgba(33, 150, 243, 0.4);
}

/* 删除确认对话框特殊样式 */
.v-card-title.error {
  background: linear-gradient(135deg, #f44336 0%, #e53935 100%) !important;
  box-shadow: 0 2px 8px rgba(244, 67, 54, 0.3) !important;
}

.theme--dark .v-card-title.error {
  background: linear-gradient(135deg, #ef5350 0%, #f44336 100%) !important;
  box-shadow: 0 2px 8px rgba(244, 67, 54, 0.4) !important;
}

/* 保存筛选器对话框样式 */
.v-card-title.primary {
  background: linear-gradient(135deg, #4caf50 0%, #81c784 100%) !important;
  box-shadow: 0 2px 8px rgba(76, 175, 80, 0.3) !important;
}

.theme--dark .v-card-title.primary {
  background: linear-gradient(135deg, #66bb6a 0%, #a5d6a7 100%) !important;
  box-shadow: 0 2px 8px rgba(102, 187, 106, 0.4) !important;
}

/* 对话框内容样式 */
.v-card-text {
  padding: 24px;
}

.theme--light .v-card {
  background-color: #ffffff;
  border: 1px solid rgba(0, 0, 0, 0.08);
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.08);
}

.theme--dark .v-card {
  background-color: #1e1e1e;
  color: rgba(255, 255, 255, 0.87);
  border: 1px solid rgba(255, 255, 255, 0.08);
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3);
}

.theme--dark .v-card-text {
  color: rgba(255, 255, 255, 0.87);
}

/* 筛选器预览样式 */
.filter-preview {
  background: linear-gradient(135deg, rgba(245, 245, 245, 0.8) 0%, rgba(250, 250, 250, 0.9) 100%);
  border-radius: 10px;
  border: 1px solid rgba(0, 0, 0, 0.08);
  box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.05);
  position: relative;
  overflow: hidden;
}

.theme--light .filter-preview {
  background: linear-gradient(135deg, rgba(245, 245, 245, 0.9) 0%, rgba(250, 250, 250, 1) 100%);
  border: 1px solid rgba(0, 0, 0, 0.1);
}

.theme--dark .filter-preview {
  background: linear-gradient(135deg, rgba(66, 66, 66, 0.8) 0%, rgba(55, 55, 55, 0.9) 100%);
  border: 1px solid rgba(255, 255, 255, 0.1);
  box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.2);
}

.filter-preview::before {
  content: '';
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 2px;
  background: linear-gradient(90deg, #4caf50, #81c784);
}

.theme--dark .filter-preview::before {
  background: linear-gradient(90deg, #66bb6a, #a5d6a7);
}

.filter-code {
  background: linear-gradient(135deg, rgba(0, 0, 0, 0.08) 0%, rgba(0, 0, 0, 0.05) 100%);
  color: rgba(0, 0, 0, 0.8);
  padding: 8px 12px;
  border-radius: 6px;
  border: 1px solid rgba(0, 0, 0, 0.1);
  word-break: break-word;
  font-family: 'JetBrains Mono', 'Fira Code', 'SF Mono', 'Monaco', 'Inconsolata', 'Roboto Mono', 'Courier New', monospace;
  font-size: 0.85rem;
  font-weight: 500;
  box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.05);
  transition: all 0.2s ease;
}

.theme--light .filter-code {
  background: linear-gradient(135deg, rgba(0, 0, 0, 0.08) 0%, rgba(0, 0, 0, 0.05) 100%);
  color: rgba(0, 0, 0, 0.8);
  border: 1px solid rgba(0, 0, 0, 0.1);
}

.theme--dark .filter-code {
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.12) 0%, rgba(255, 255, 255, 0.08) 100%);
  color: rgba(255, 255, 255, 0.9);
  border: 1px solid rgba(255, 255, 255, 0.15);
  box-shadow: inset 0 1px 2px rgba(0, 0, 0, 0.2);
}

.filter-code:hover {
  background: linear-gradient(135deg, rgba(0, 0, 0, 0.12) 0%, rgba(0, 0, 0, 0.08) 100%);
}

.theme--dark .filter-code:hover {
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.18) 0%, rgba(255, 255, 255, 0.12) 100%);
}

/* 工具提示样式 */
.my-tooltip {
  background: linear-gradient(135deg, rgba(33, 33, 33, 0.95) 0%, rgba(45, 45, 45, 0.98) 100%) !important;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3) !important;
  padding: 12px 16px !important;
  border-radius: 8px !important;
  max-width: 320px !important;
  font-size: 0.8rem !important;
  color: white !important;
  white-space: pre-wrap;
  line-height: 1.5;
  border: 1px solid rgba(255, 255, 255, 0.1) !important;
  backdrop-filter: blur(8px);
}

.tooltip-query {
  font-family: 'JetBrains Mono', 'Fira Code', 'SF Mono', 'Monaco', 'Inconsolata', 'Roboto Mono', 'Courier New', monospace;
  color: #64b5f6;
  background: linear-gradient(135deg, rgba(0, 0, 0, 0.6) 0%, rgba(0, 0, 0, 0.7) 100%) !important;
  padding: 10px 14px !important;
  border-radius: 6px;
  border: 1px solid rgba(255, 255, 255, 0.3) !important;
  word-break: break-word;
  white-space: pre-wrap;
  overflow-wrap: break-word;
  letter-spacing: 0.02em;
  font-size: 0.75rem;
  font-weight: 500;
  box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.2);
  margin-top: 8px;
}

/* 明亮主题工具提示 */
.theme--light .my-tooltip {
  background: linear-gradient(135deg, rgba(255, 255, 255, 0.98) 0%, rgba(248, 250, 252, 0.95) 100%) !important;
  color: #222 !important;
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.15) !important;
  border: 1px solid rgba(0, 0, 0, 0.1) !important;
}

.theme--light .tooltip-query {
  background: linear-gradient(135deg, rgba(0, 0, 0, 0.08) 0%, rgba(0, 0, 0, 0.05) 100%) !important;
  border: 1px solid rgba(0, 0, 0, 0.2) !important;
  color: #1565c0;
  box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.1);
}

/* 深色主题工具提示 */
.theme--dark .my-tooltip {
  background: linear-gradient(135deg, rgba(33, 33, 33, 0.98) 0%, rgba(45, 45, 45, 0.95) 100%) !important;
  border: 1px solid rgba(255, 255, 255, 0.15) !important;
}

.theme--dark .tooltip-query {
  background: linear-gradient(135deg, rgba(0, 0, 0, 0.7) 0%, rgba(0, 0, 0, 0.8) 100%) !important;
  border: 1px solid rgba(255, 255, 255, 0.4) !important;
  color: #4fc3f7;
  box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.3);
}

/* 按钮样式优化 */
.v-btn {
  border-radius: 8px;
  font-weight: 500;
  text-transform: none;
  letter-spacing: 0.02em;
  transition: all 0.2s ease;
}

.theme--light .v-btn {
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
}

.theme--dark .v-btn {
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.3);
}

.v-btn:hover {
  transform: translateY(-1px);
}

.theme--light .v-btn:hover {
  box-shadow: 0 4px 8px rgba(0, 0, 0, 0.15);
}

.theme--dark .v-btn:hover {
  box-shadow: 0 4px 8px rgba(0, 0, 0, 0.4);
}

/* 主要按钮特殊样式 */
.v-btn.primary {
  background: linear-gradient(135deg, #1976d2 0%, #42a5f5 100%);
  border: none;
}

.theme--dark .v-btn.primary {
  background: linear-gradient(135deg, #2196f3 0%, #64b5f6 100%);
}

/* 错误按钮样式 */
.v-btn.error {
  background: linear-gradient(135deg, #f44336 0%, #e53935 100%);
  border: none;
}

.theme--dark .v-btn.error {
  background: linear-gradient(135deg, #ef5350 0%, #f44336 100%);
}

/* 输入框样式优化 */
.v-text-field .v-input__slot {
  border-radius: 8px;
  transition: all 0.2s ease;
}

.theme--light .v-text-field .v-input__slot {
  background-color: rgba(0, 0, 0, 0.02);
  border: 1px solid rgba(0, 0, 0, 0.1);
}

.theme--dark .v-text-field .v-input__slot {
  background-color: rgba(255, 255, 255, 0.05);
  border: 1px solid rgba(255, 255, 255, 0.1);
}

.v-text-field .v-input__slot:hover {
  background-color: rgba(0, 0, 0, 0.04);
}

.theme--dark .v-text-field .v-input__slot:hover {
  background-color: rgba(255, 255, 255, 0.08);
}

.v-text-field .v-input__slot:focus-within {
  background-color: rgba(25, 118, 210, 0.04);
  border-color: #1976d2;
}

.theme--dark .v-text-field .v-input__slot:focus-within {
  background-color: rgba(33, 150, 243, 0.08);
  border-color: #2196f3;
}

/* 动画效果 */
.v-tooltip__content {
  animation: tooltipFadeIn 0.2s ease-out;
}

@keyframes tooltipFadeIn {
  from {
    opacity: 0;
    transform: translateY(4px) scale(0.95);
  }
  to {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}

/* 卡片悬停动画 */
.saved-filter-card {
  animation: cardFadeIn 0.3s ease-out;
  animation-fill-mode: both;
}

.saved-filter-card:nth-child(1) { animation-delay: 0.05s; }
.saved-filter-card:nth-child(2) { animation-delay: 0.1s; }
.saved-filter-card:nth-child(3) { animation-delay: 0.15s; }
.saved-filter-card:nth-child(4) { animation-delay: 0.2s; }
.saved-filter-card:nth-child(5) { animation-delay: 0.25s; }

@keyframes cardFadeIn {
  from {
    opacity: 0;
    transform: translateY(10px);
  }
  to {
    opacity: 1;
    transform: translateY(0);
  }
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

  /* 移动端空状态样式 */
  .v-dialog--fullscreen .empty-state {
    padding: 32px 16px !important;
  }

  .v-dialog--fullscreen .empty-state .v-icon {
    font-size: 48px !important;
  }

  .v-dialog--fullscreen .empty-state h3 {
    font-size: 1.1rem !important;
    margin: 12px 0 !important;
  }

  .v-dialog--fullscreen .empty-state p {
    font-size: 0.9rem !important;
    margin: 8px 0 !important;
  }

  /* 移动端网格样式 */
  .v-dialog--fullscreen .saved-filters-grid {
    grid-template-columns: repeat(auto-fill, minmax(100px, 1fr)) !important;
    gap: 6px !important;
    padding: 8px !important;
  }

  .v-dialog--fullscreen .saved-filter-card {
    min-height: 80px !important;
  }

  .v-dialog--fullscreen .saved-filter-card .v-card-text {
    padding: 8px !important;
  }

  .v-dialog--fullscreen .filter-name {
    font-size: 0.75rem !important;
    margin-bottom: 4px !important;
  }

  .v-dialog--fullscreen .query-text {
    font-size: 0.65rem !important;
    padding: 4px 6px !important;
  }

  .v-dialog--fullscreen .delete-btn {
    width: 16px !important;
    height: 16px !important;
    margin-left: 2px !important;
  }

  .v-dialog--fullscreen .delete-btn .v-icon {
    font-size: 12px !important;
  }

  /* 移动端工具提示样式 */
  .v-dialog--fullscreen .my-tooltip {
    max-width: 280px !important;
    font-size: 0.75rem !important;
    padding: 8px 12px !important;
  }

  .v-dialog--fullscreen .tooltip-query {
    font-size: 0.7rem !important;
    padding: 6px 8px !important;
  }

  /* 移动端按钮样式 */
  .v-dialog--fullscreen .v-btn {
    height: 44px !important;
    font-size: 0.9rem !important;
  }

  .v-dialog--fullscreen .v-btn .v-icon {
    margin-right: 8px !important;
  }

  /* 移动端图标样式 */
  .v-dialog--fullscreen .v-icon {
    font-size: 18px !important;
  }

  /* 移动端悬停效果调整 */
  .v-dialog--fullscreen .saved-filter-card:hover {
    transform: translateY(-1px) scale(1.02) !important;
  }

  /* 移动端删除确认对话框样式 */
  .v-dialog--fullscreen .v-dialog .v-card {
    margin: 16px !important;
    max-height: calc(100vh - 32px) !important;
  }

  .v-dialog--fullscreen .v-dialog .v-card-title {
    padding: 12px 16px !important;
    font-size: 0.95rem !important;
  }

  .v-dialog--fullscreen .v-dialog .v-card-text {
    padding: 16px !important;
  }

  .v-dialog--fullscreen .v-dialog .v-card-actions {
    padding: 16px !important;
    flex-direction: column !important;
    gap: 8px !important;
  }

  .v-dialog--fullscreen .v-dialog .v-card-actions .v-btn {
    width: 100% !important;
    margin: 0 !important;
  }

  /* 移动端保存对话框样式 */
  .v-dialog--fullscreen .v-dialog--active .v-card {
    margin: 16px !important;
    max-height: calc(100vh - 32px) !important;
  }

  .v-dialog--fullscreen .filter-preview {
    margin: 12px 0 !important;
    padding: 12px !important;
  }

  .v-dialog--fullscreen .filter-code {
    font-size: 0.8rem !important;
    padding: 8px 10px !important;
  }

  /* 移动端输入框样式 */
  .v-dialog--fullscreen .v-text-field {
    margin-bottom: 12px !important;
  }

  .v-dialog--fullscreen .v-text-field .v-input__slot {
    font-size: 0.875rem !important;
  }

  .v-dialog--fullscreen .v-text-field .v-label {
    font-size: 0.875rem !important;
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

  /* 平板网格样式 */
  .saved-filters-grid {
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr)) !important;
    gap: 8px !important;
    padding: 16px !important;
  }

  .saved-filter-card {
    min-height: 90px !important;
  }

  .saved-filter-card .v-card-text {
    padding: 10px !important;
  }

  .filter-name {
    font-size: 0.8rem !important;
    margin-bottom: 6px !important;
  }

  .query-text {
    font-size: 0.7rem !important;
    padding: 6px 8px !important;
  }

  .delete-btn {
    width: 18px !important;
    height: 18px !important;
    margin-left: 3px !important;
  }

  .delete-btn .v-icon {
    font-size: 11px !important;
  }

  /* 平板工具提示样式 */
  .my-tooltip {
    max-width: 300px !important;
    font-size: 0.8rem !important;
    padding: 10px 14px !important;
  }

  .tooltip-query {
    font-size: 0.75rem !important;
    padding: 8px 10px !important;
  }

  /* 平板按钮样式 */
  .v-btn {
    height: 40px !important;
    font-size: 0.9rem !important;
  }

  /* 平板空状态样式 */
  .empty-state {
    padding: 48px 24px !important;
  }

  .empty-state .v-icon {
    font-size: 56px !important;
  }

  .empty-state h3 {
    font-size: 1.2rem !important;
    margin: 16px 0 !important;
  }

  .empty-state p {
    font-size: 0.95rem !important;
    margin: 12px 0 !important;
  }
}
</style>
