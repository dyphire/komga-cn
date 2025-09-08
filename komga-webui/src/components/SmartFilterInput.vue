<template>
  <v-card flat class="pa-4">
    <v-card-title class="pa-0 pb-4">
      <span class="text-h6">{{ $t('filter.smart_filter') }}</span>
      <v-spacer />
      <v-btn icon small @click="clear">
        <v-icon>mdi-close</v-icon>
      </v-btn>
    </v-card-title>

    <v-text-field
      v-model="query"
      :label="$t('filter.smart_filter_placeholder')"
      :hint="$t('filter.smart_filter_hint')"
      persistent-hint
      outlined
      dense
      clearable
      @input="onQueryChange"
      @keyup.enter="applyFilter"
    />

    <v-card-text class="pa-0 pt-4">
      <div class="text-caption mb-2">{{ $t('filter.smart_filter_examples') }}</div>
      <div class="text-body-2 grey--text">
        <div><code>author:John</code> - {{ $t('filter.example_author') }}</div>
        <div><code>tag:action</code> - {{ $t('filter.example_tag') }}</div>
        <div><code>deleted:false</code> - {{ $t('filter.example_deleted') }}</div>
        <div><code>NOT tag:mystery</code> - {{ $t('filter.example_not') }}</div>
        <div><code>author:John AND tag:action</code> - {{ $t('filter.example_combined') }}</div>
      </div>
    </v-card-text>

    <v-card-actions class="pa-0 pt-4">
      <v-spacer />
      <v-btn
        color="primary"
        @click="applyFilter"
        :disabled="!query.trim()"
      >
        {{ $t('common.apply') }}
      </v-btn>
    </v-card-actions>
  </v-card>
</template>

<script lang="ts">
import Vue from 'vue'
import { parseSmartFilter } from '@/functions/smart-filter'

export default Vue.extend({
  name: 'SmartFilterInput',
  props: {
    value: {
      type: String,
      default: '',
    },
  },
  data: () => ({
    query: '',
  }),
  watch: {
    value: {
      handler(val) {
        this.query = val
      },
      immediate: true,
    },
  },
  methods: {
    onQueryChange() {
      this.$emit('input', this.query)
    },
    applyFilter() {
      if (!this.query.trim()) return

      const condition = parseSmartFilter(this.query.trim())
      if (condition) {
        this.$emit('apply', condition)
      } else {
        this.$emit('error', this.$t('filter.smart_filter_invalid'))
      }
    },
    clear() {
      this.query = ''
      this.$emit('input', '')
      this.$emit('clear')
    },
  },
})
</script>

<style scoped>
code {
  background-color: rgba(0, 0, 0, 0.1);
  padding: 2px 4px;
  border-radius: 3px;
  font-family: 'Courier New', monospace;
}
</style>
