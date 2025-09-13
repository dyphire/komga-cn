<template>
  <div class="echarts-pie-chart">
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
  PieChart,
} from 'echarts/charts'
import {
  TitleComponent,
  TooltipComponent,
  LegendComponent,
} from 'echarts/components'

// 注册必需的组件
use([
  CanvasRenderer,
  PieChart,
  TitleComponent,
  TooltipComponent,
  LegendComponent,
])

interface ChartData {
  name: string
  value: number
}

export default Vue.extend({
  name: 'EChartsPieChart',
  components: {
    VChart,
  },
  props: {
    data: {
      type: Array as () => ChartData[],
      default: () => [],
    },
    legend: {
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
    chartOption(): any {
      return {
        backgroundColor: 'transparent',
        title: {
          show: false,
        },
        tooltip: {
          trigger: 'item',
          backgroundColor: 'rgba(50, 50, 50, 0.9)',
          borderColor: 'rgba(50, 50, 50, 0.9)',
          textStyle: {
            color: '#fff',
            fontSize: 12,
          },
          formatter: (params: any) => {
            const percent = ((params.value / this.totalValue) * 100).toFixed(1)
            return `${params.name}: ${params.value} (${percent}%)`
          },
        },
        legend: {
          show: this.legend,
          orient: 'vertical',
          left: 'right',
          top: 'center',
          textStyle: {
            color: this.$vuetify.theme.dark ? 'rgba(255, 255, 255, 0.8)' : 'rgba(0, 0, 0, 0.8)',
            fontSize: 11,
          },
          itemWidth: 12,
          itemHeight: 12,
          itemGap: 8,
        },
        series: [{
          name: '数据',
          type: 'pie',
          radius: this.legend ? ['40%', '70%'] : ['50%', '80%'],
          center: this.legend ? ['35%', '50%'] : ['50%', '50%'],
          avoidLabelOverlap: true,
          emphasis: {
            itemStyle: {
              shadowBlur: 10,
              shadowOffsetX: 0,
              shadowColor: 'rgba(0, 0, 0, 0.5)',
            },
          },
          label: {
            show: false,
          },
          labelLine: {
            show: false,
          },
          data: this.data.map((item, index) => ({
            name: item.name,
            value: item.value,
            itemStyle: {
              color: this.getColor(index),
            },
          })),
        }],
      }
    },
    totalValue(): number {
      return this.data.reduce((sum, item) => sum + item.value, 0)
    },
  },
  methods: {
    getColor(index: number): string {
      const colors = [
        '#1976d2', '#388e3c', '#f57c00', '#d32f2f',
        '#7b1fa2', '#0097a7', '#689f38', '#fbc02d',
        '#e91e63', '#9c27b0', '#673ab7', '#3f51b5',
      ]
      return colors[index % colors.length]
    },
  },
})
</script>

<style scoped>
.echarts-pie-chart {
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
