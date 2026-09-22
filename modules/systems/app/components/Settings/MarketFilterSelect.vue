<script setup lang="ts">
import { useIndicatorOptions } from '#systems/composables/useIndicatorOptions'
import type { MarketIndicatorConfig, RunConfig, TotalBreakoutConfig, TrendKind } from '#systems/types'

const props = defineProps<{ config: RunConfig; compact?: boolean; disabled?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: MarketIndicatorConfig | null] }>()
const selection = computed(() => props.config.marketIndicator)
const standard = computed(() => selection.value?.trend !== 'total_breakout' ? selection.value : null)
const breakout = computed(() => selection.value?.trend === 'total_breakout' ? selection.value : null)
const { options } = useIndicatorOptions(() => ({
  ...props.config, indicator: standard.value ?? props.config.indicator,
}))
const selectedTrend = computed(() => selection.value?.trend ?? 'same')

function setTrend(value: string) {
  if (value === 'same') {
    emit('update:modelValue', null)
    return
  }
  if (value === 'total_breakout') {
    emit('update:modelValue', { trend: 'total_breakout', trendLength: 100, entryLength: 5, exitLength: 5 })
    return
  }
  emit('update:modelValue', {
    trend: value as TrendKind,
    emaCross: standard.value?.emaCross ?? { src: 'close', fastLength: 12, slowLength: 21 },
    aggregate: standard.value?.aggregate ?? [],
  })
}

function setMembers(aggregate: TrendKind[]) {
  if (standard.value) emit('update:modelValue', { ...standard.value, aggregate })
}

function setLength(key: 'fastLength' | 'slowLength', event: Event) {
  if (!standard.value) return
  const length = Math.max(1, Math.trunc(Number((event.target as HTMLInputElement).value) || 1))
  emit('update:modelValue', {
    ...standard.value, emaCross: { ...standard.value.emaCross, [key]: length },
  })
}

function setBreakout(key: keyof Omit<TotalBreakoutConfig, 'trend'>, event: Event) {
  if (!breakout.value) return
  const value = Math.min(5000, Math.max(2, Math.trunc(Number((event.target as HTMLInputElement).value) || 2)))
  emit('update:modelValue', { ...breakout.value, [key]: value })
}
</script>

<template>
  <div class="qs-market-select" :class="{ 'qs-market-select--compact': compact }">
    <select class="select" aria-label="TOTAL market filter indicator" :value="selectedTrend"
      :disabled="disabled" @change="setTrend(($event.target as HTMLSelectElement).value)">
      <option value="same">TOTAL · Same as ranking</option>
      <option value="total_breakout">TOTAL · Breakout (research)</option>
      <option value="aggregate">TOTAL · Aggregate</option>
      <option v-for="option in options" :key="option.value" :value="option.value">TOTAL · {{ option.name }}</option>
    </select>
    <SystemsBacktestCompareSelect v-if="compact || selectedTrend === 'aggregate'"
      class="qs-market-select__members"
      :icon-only="compact"
      label="TOTAL members"
      :title="selectedTrend === 'aggregate' ? 'Independent members for the TOTAL market signal' : 'TOTAL members are available for the Aggregate market filter'"
      empty-hint="Pick TOTAL indicators" unit="member" unit-plural="members"
      :disabled="disabled || selectedTrend !== 'aggregate'"
      :model-value="selectedTrend === 'aggregate' ? standard?.aggregate ?? [] : []" :options="options"
      @update:model-value="setMembers" />
    <div v-if="!compact && standard && (selectedTrend === 'ema_cross' || (selectedTrend === 'aggregate' && standard.aggregate.includes('ema_cross')))"
      class="qs-market-select__lengths">
      <label>TOTAL EMA fast
        <input class="input mono" type="number" min="1" :disabled="disabled"
          :value="standard.emaCross.fastLength" @change="setLength('fastLength', $event)" />
      </label>
      <label>TOTAL EMA slow
        <input class="input mono" type="number" min="1" :disabled="disabled"
          :value="standard.emaCross.slowLength" @change="setLength('slowLength', $event)" />
      </label>
    </div>
    <template v-if="!compact && breakout">
      <div class="qs-market-select__lengths">
        <label>Trend EMA
          <input class="input mono" type="number" min="2" max="5000" :disabled="disabled"
            :value="breakout.trendLength" @change="setBreakout('trendLength', $event)" />
        </label>
        <label>Breakout bars
          <input class="input mono" type="number" min="2" max="5000" :disabled="disabled"
            :value="breakout.entryLength" @change="setBreakout('entryLength', $event)" />
        </label>
        <label>Exit EMA
          <input class="input mono" type="number" min="2" max="5000" :disabled="disabled"
            :value="breakout.exitLength" @change="setBreakout('exitLength', $event)" />
        </label>
      </div>
      <p class="qs-market-select__hint">Enter after a new closing high above a rising trend EMA; return to USD after a close below either EMA.
        Research option: the tested LCES setup still exceeded 30% drawdown.</p>
    </template>
  </div>
</template>

<style scoped>
.qs-market-select { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; min-width: 0; }
.qs-market-select > .select { max-width: 100%; }
.qs-market-select--compact { flex-wrap: nowrap; }
.qs-market-select--compact > .select {
  box-sizing: border-box;
  flex: 1 1 225px;
  min-width: 0;
  width: 225px;
  height: 32px;
  padding: 5px 8px;
  font-size: 12px;
}
.qs-market-select--compact .qs-market-select__members { flex: 0 0 32px; width: 32px; }
.qs-market-select__lengths { display: flex; gap: 10px; min-width: 0; max-width: 100%; }
.qs-market-select__lengths label { display: grid; gap: 4px; min-width: 0; font-size: 12px; color: var(--qs-text-secondary); }
.qs-market-select__lengths input { width: 90px; max-width: 100%; min-width: 0; }
.qs-market-select__hint { width: 100%; margin: 0; font-size: 12px; color: var(--qs-text-muted); }
</style>
