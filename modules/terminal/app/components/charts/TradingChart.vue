<script setup lang="ts">
import { useSettingsStore } from '#terminal/stores/settings'
const props = withDefaults(defineProps<{
  symbol?: string
}>(), {
  symbol: 'BTC/USDT',
})

const colorMode = useColorMode()
const settings = useSettingsStore()
const container = ref<HTMLElement>()

onMounted(() => {
  settings.loadFromStorage()
  createWidget()
})

function tvSymbol(symbol: string): string {
  return 'BINANCE:' + symbol.replace('/', '')
}

function createWidget() {
  if (!container.value) return

  // Clear previous widget
  container.value.innerHTML = ''

  const widgetDiv = document.createElement('div')
  widgetDiv.className = 'tradingview-widget-container__widget'
  widgetDiv.style.height = '100%'
  widgetDiv.style.width = '100%'
  container.value.appendChild(widgetDiv)

  const isDark = colorMode.value === 'dark'
  const tv = settings.tvSettings

  const config: Record<string, unknown> = {
    autosize: true,
    symbol: tvSymbol(props.symbol),
    interval: tv.interval,
    timezone: tv.timezone,
    theme: isDark ? 'dark' : 'light',
    style: tv.style,
    locale: tv.locale,
    backgroundColor: isDark ? tv.backgroundColor.dark : tv.backgroundColor.light,
    gridColor: isDark ? tv.gridColor.dark : tv.gridColor.light,
    hide_top_toolbar: tv.hideTopToolbar,
    hide_side_toolbar: tv.hideSideToolbar,
    allow_symbol_change: tv.allowSymbolChange,
    calendar: tv.calendar,
    hide_volume: tv.hideVolume,
    withdateranges: tv.withdateranges,
    details: tv.details,
    hotlist: tv.hotlist,
    show_popup_button: tv.showPopupButton,
    support_host: 'https://www.tradingview.com',
  }

  if (tv.studies.length > 0) {
    config.studies = tv.studies
  }

  if (tv.range) {
    config.range = tv.range
  }

  const script = document.createElement('script')
  script.src = 'https://s3.tradingview.com/external-embedding/embed-widget-advanced-chart.js'
  script.type = 'text/javascript'
  script.async = true
  script.innerHTML = JSON.stringify(config)
  container.value.appendChild(script)
}

watch(() => colorMode.value, () => createWidget())
watch(() => props.symbol, () => createWidget())
watch(() => settings.tvSettings, () => createWidget(), { deep: true })
</script>

<template>
  <div ref="container" class="tradingview-widget-container" />
</template>

<style scoped>
/*
  TradingView's embed injects its own stylesheet for
  `.tradingview-widget-container`, and it wins: the element carried an inline
  `height: 100%` and still computed to 150px inside a 418px box, which is the
  widget's fallback height. Only `!important` beats an inline declaration, so
  that is what this needs — the scoped `[data-v-…]` attribute adds the
  specificity to stay ahead of the injected rule.

  Absolute positioning pins it to the parent's box regardless of what the
  widget does to its own layout afterwards.
*/
.tradingview-widget-container {
  position: absolute;
  inset: 0;
  height: 100% !important;
  width: 100% !important;
}
.tradingview-widget-container :deep(iframe) {
  height: 100% !important;
  width: 100% !important;
}
</style>
