<script setup lang="ts">
import { useMarketStore } from '#terminal/stores/market'
definePageMeta({ layout: 'terminal' })

const marketStore = useMarketStore()
</script>

<template>
  <div class="h-full flex flex-col overflow-hidden">
    <!-- `relative`: the chart pins itself with `position: absolute` to survive
         TradingView's injected stylesheet (see TradingChart.vue). -->
    <div class="flex-1 min-h-0 relative">
      <TerminalChartsTradingChart :symbol="marketStore.selectedSymbol" />
    </div>
  </div>
</template>
