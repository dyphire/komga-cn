<template>
  <div class="echarts-line-chart">
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
  LineChart,
  BarChart,
  PieChart,
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
  LineChart,
  BarChart,
  PieChart,
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
  name: 'EChartsLineChart',
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
    maxValue(): number {
      return Math.max(...this.data.map(item => item.value || 0), 1)
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
            type: 'cross',
            lineStyle: {
              color: '#fff',
              width: 1,
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
          boundaryGap: false,
          data: this.data.map(item => item.name),
          axisLine: {
            lineStyle: {
              color: this.$vuetify.theme.dark ? 'rgba(255, 255, 255, 0.3)' : 'rgba(0, 0, 0, 0.3)',
            },
          },
          axisLabel: {
            color: this.$vuetify.theme.dark ? 'rgba(255, 255, 255, 0.8)' : 'rgba(0, 0, 0, 0.8)',
            fontSize: 11,
            rotate: this.data.length > 8 ? 45 : 0,
            interval: this.data.length > 12 ? 'auto' : 0,
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
          name: this.yAxisTitle || '阅读时长',
          type: 'line',
          smooth: true,
          symbol: 'circle',
          symbolSize: 6,
          lineStyle: {
            color: '#1976d2',
            width: 3,
          },
          itemStyle: {
            color: '#1976d2',
          },
          areaStyle: {
            color: {
              type: 'linear',
              x: 0,
              y: 0,
              x2: 0,
              y2: 1,
              colorStops: [{
                offset: 0, color: 'rgba(25, 118, 210, 0.3)',
              }, {
                offset: 1, color: 'rgba(25, 118, 210, 0.05)',
              }],
            },
          },
          data: this.data.map(item => item.value),
        }],
      }
    },
  },
})
</script>

<style scoped>
.echarts-line-chart {
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
