<template>
  <div class="echarts-bar-chart">
    <v-chart
      :option="chartOption"
      :autoresize="true"
      class="chart-container"
    />
  </div>
</template>

<script lang="ts">
import Vue from 'vue'
import VChart from 'vue-echarts'
import { use } from 'echarts/core'
import {
  CanvasRenderer,
} from 'echarts/renderers'
import {
  BarChart,
} from 'echarts/charts'
import {
  TitleComponent,
  TooltipComponent,
  LegendComponent,
  GridComponent,
} from 'echarts/components'

// 注册必需的组件
use([
  CanvasRenderer,
  BarChart,
  TitleComponent,
  TooltipComponent,
  LegendComponent,
  GridComponent,
])

interface ChartData {
  name: string
  value: number
}

export default Vue.extend({
  name: 'EChartsBarChart',
  components: {
    VChart,
  },
  props: {
    data: {
      type: Array as () => ChartData[],
      default: () => [],
    },
    xAxisTitle: {
      type: String,
      default: '',
    },
    yAxisTitle: {
      type: String,
      default: '',
    },
    filterZeroValues: {
      type: Boolean,
      default: true,
    },
    width: {
      type: Number,
      default: 400,
    },
    height: {
      type: Number,
      default: 250,
    },
  },
  computed: {
    filteredData(): ChartData[] {
      if (!this.filterZeroValues) return this.data
      return this.data.filter(item => item.value > 0)
    },
    maxValue(): number {
      return Math.max(...this.filteredData.map(item => item.value || 0), 1)
    },
    chartOption(): any {
      return {
        backgroundColor: 'transparent',
        title: {
          show: false,
        },
        tooltip: {
          trigger: 'axis',
          backgroundColor: 'rgba(50, 50, 50, 0.9)',
          borderColor: 'rgba(50, 50, 50, 0.9)',
          textStyle: {
            color: '#fff',
            fontSize: 12,
          },
          axisPointer: {
            type: 'shadow',
            shadowStyle: {
              color: 'rgba(150, 150, 150, 0.1)',
            },
          },
        },
        legend: {
          show: false,
        },
        grid: {
          left: '3%',
          right: '4%',
          bottom: '15%',
          top: '10%',
          containLabel: true,
        },
        xAxis: {
          type: 'category',
          data: this.filteredData.map(item => item.name),
          axisLine: {
            lineStyle: {
              color: this.$vuetify.theme.dark ? 'rgba(255, 255, 255, 0.3)' : 'rgba(0, 0, 0, 0.3)',
            },
          },
          axisLabel: {
            color: this.$vuetify.theme.dark ? 'rgba(255, 255, 255, 0.8)' : 'rgba(0, 0, 0, 0.8)',
            fontSize: 11,
            rotate: this.filteredData.length > 8 ? 45 : 0,
            interval: this.filteredData.length > 12 ? 'auto' : 0,
          },
          axisTick: {
            show: false,
          },
        },
        yAxis: {
          type: 'value',
          axisLine: {
            lineStyle: {
              color: this.$vuetify.theme.dark ? 'rgba(255, 255, 255, 0.3)' : 'rgba(0, 0, 0, 0.3)',
            },
          },
          axisLabel: {
            color: this.$vuetify.theme.dark ? 'rgba(255, 255, 255, 0.8)' : 'rgba(0, 0, 0, 0.8)',
            fontSize: 11,
            formatter: (value: number) => {
              if (value >= 1000) {
                return `${(value / 1000).toFixed(1)}k`
              }
              return value.toString()
            },
          },
          splitLine: {
            show: false,
          },
          axisTick: {
            show: false,
          },
        },
        series: [{
          name: this.yAxisTitle || '数量',
          type: 'bar',
          barWidth: this.filteredData.length > 10 ? '60%' : '40%',
          itemStyle: {
            color: (params: any) => {
              const colors = [
                '#1976d2', '#388e3c', '#f57c00', '#d32f2f',
                '#7b1fa2', '#0097a7', '#689f38', '#fbc02d',
              ]
              return colors[params.dataIndex % colors.length]
            },
            borderRadius: [4, 4, 0, 0],
          },
          emphasis: {
            itemStyle: {
              shadowBlur: 10,
              shadowOffsetX: 0,
              shadowColor: 'rgba(0, 0, 0, 0.5)',
            },
          },
          label: {
            show: true,
            position: 'top',
            color: this.$vuetify.theme.dark ? 'rgba(255, 255, 255, 0.9)' : 'rgba(0, 0, 0, 0.8)',
            fontSize: 11,
            fontWeight: 'bold',
            formatter: (params: any) => {
              const value = params.value
              if (value >= 1000) {
                return `${(value / 1000).toFixed(1)}k`
              }
              return value.toString()
            },
          },
          data: this.filteredData.map(item => item.value),
        }],
      }
    },
  },
})
</script>

<style scoped>
.echarts-bar-chart {
  width: 100%;
  height: 100%;
}

.chart-container {
  width: 100%;
  height: 250px;
}

/* 深色模式适配 */
.theme--dark .chart-container {
  /* ECharts 会自动处理深色模式 */
}

/* 响应式设计 */
@media (max-width: 600px) {
  .chart-container {
    height: 200px;
  }
}
</style>
