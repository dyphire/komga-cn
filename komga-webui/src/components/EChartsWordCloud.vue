<template>
  <div class="echarts-word-cloud">
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
  TitleComponent,
  TooltipComponent,
} from 'echarts/components'
import 'echarts-wordcloud'

// 注册必需的组件
use([
  CanvasRenderer,
  TitleComponent,
  TooltipComponent,
])

interface WordData {
  text: string
  weight: number
}

export default Vue.extend({
  name: 'EChartsWordCloud',
  components: {
    VChart,
  },
  props: {
    words: {
      type: Array as () => WordData[],
      default: () => [],
    },
    width: {
      type: Number,
      default: 260,
    },
    height: {
      type: Number,
      default: 260,
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
          show: true,
          backgroundColor: 'rgba(50, 50, 50, 0.9)',
          borderColor: 'rgba(50, 50, 50, 0.9)',
          textStyle: {
            color: '#fff',
            fontSize: 12,
          },
          formatter: (params: any) => {
            return `${params.data.name}: ${params.data.value}`
          },
        },
        series: [{
          type: 'wordCloud',
          shape: 'circle',
          left: 'center',
          top: 'center',
          width: '95%',
          height: '95%',
          right: null,
          bottom: null,
          sizeRange: [10, 40],
          rotationRange: [-30, 30],
          rotationStep: 15,
          gridSize: 4,
          drawOutOfBound: false,
          shrinkToFit: true,
          layoutAnimation: true,
          keepAspectRatio: false,
          autoSize: {
            enable: true,
            minSize: 8,
          },
          textPadding: 2,
          emphasis: {
            focus: 'self',
            textStyle: {
              shadowBlur: 5,
              shadowColor: '#333',
            },
          },
          textStyle: {
            fontFamily: 'sans-serif',
            fontWeight: 'bold',
            color: (params: any) => {
              const colors = [
                '#1976d2', '#388e3c', '#f57c00', '#d32f2f',
                '#7b1fa2', '#0097a7', '#689f38', '#fbc02d',
                '#e91e63', '#9c27b0', '#673ab7', '#3f51b5',
              ]
              return colors[params.dataIndex % colors.length]
            },
          },
          data: this.words.map(word => ({
            name: word.text,
            value: word.weight,
            textStyle: {
              color: this.getRandomColor(),
            },
          })),
        }],
      }
    },
  },
  methods: {
    getRandomColor(): string {
      const colors = [
        '#1976d2', '#388e3c', '#f57c00', '#d32f2f',
        '#7b1fa2', '#0097a7', '#689f38', '#fbc02d',
        '#e91e63', '#9c27b0', '#673ab7', '#3f51b5',
        '#2196f3', '#4caf50', '#ff9800', '#f44336',
      ]
      return colors[Math.floor(Math.random() * colors.length)]
    },
  },
})
</script>

<style scoped>
.echarts-word-cloud {
  width: 100%;
  height: 100%;
}

.chart-container {
  width: 100%;
  height: 260px;
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
