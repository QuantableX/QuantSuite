<script setup lang="ts">
import { useAppStore } from '#finance/stores/app'
import { formatCents } from '#finance/utils/money'
import { layoutPie, type PieSlice } from '#finance/utils/pie'

const props = defineProps<{
  slices: PieSlice[]
  title: string
  description: string
  totalLabel: string
  emptyText: string
  selectedId?: string | null
}>()
const emit = defineEmits<{ select: [fundId: string] }>()
const app = useAppStore()
const hovered = ref<string | null>(null)
watch(() => props.slices, (slices) => {
  if (!slices.some((slice) => slice.id === hovered.value)) hovered.value = null
})
const chart = computed(() => layoutPie(props.slices))
const money = (cents: number) => formatCents(cents, app.settings.currency, app.settings.locale)
const percent = (cents: number) => new Intl.NumberFormat(app.settings.locale, {
  style: 'percent', maximumFractionDigits: 1,
}).format(chart.value.total > 0 ? cents / chart.value.total : 0)
const label = (slice: PieSlice) => `${slice.label}${slice.detail ? ` · ${slice.detail}` : ''}: ${money(slice.valueCents)} · ${percent(slice.valueCents)}`
function select(slice: PieSlice) {
  if (slice.fundId) emit('select', slice.fundId)
}
</script>

<template>
  <section class="qf-pie" :aria-label="title">
    <header class="qf-pie__header">
      <div><h2>{{ title }}</h2><p>{{ description }}</p></div>
      <div class="qf-pie__total"><span>{{ totalLabel }}</span><strong>{{ money(chart.total) }}</strong></div>
    </header>
    <div class="qf-pie__body">
      <svg v-if="chart.total > 0" class="qf-pie__svg" viewBox="0 0 200 200" role="group" :aria-label="title">
        <path
          v-for="slice in chart.segments"
          :key="slice.id"
          class="qf-pie__slice"
          :class="{ 'is-dim': hovered && hovered !== slice.id, 'is-selected': selectedId && selectedId === slice.fundId }"
          :d="slice.path"
          :fill="slice.color"
          :role="slice.fundId ? 'button' : undefined"
          :tabindex="slice.fundId ? 0 : undefined"
          :aria-label="slice.fundId ? `Open ${label(slice)}` : undefined"
          :aria-pressed="slice.fundId && selectedId !== undefined ? selectedId === slice.fundId : undefined"
          @pointerenter="hovered = slice.id"
          @pointerleave="hovered = null"
          @focus="hovered = slice.id"
          @blur="hovered = null"
          @click="select(slice)"
          @keydown.enter.prevent="select(slice)"
          @keydown.space.prevent="select(slice)"
        ><title>{{ label(slice) }}</title></path>
      </svg>
      <p v-else class="qf-pie__empty">{{ emptyText }}</p>
      <ul v-if="slices.length" class="qf-pie__legend" :aria-label="`${title} amounts and shares`">
        <li v-for="slice in slices" :key="slice.id">
          <component
            :is="slice.fundId ? 'button' : 'div'"
            :type="slice.fundId ? 'button' : undefined"
            class="qf-pie__row"
            :class="{ 'is-active': hovered === slice.id || (selectedId && selectedId === slice.fundId) }"
            :aria-label="slice.fundId ? `Open ${label(slice)}` : undefined"
            :aria-pressed="slice.fundId && selectedId !== undefined ? selectedId === slice.fundId : undefined"
            @pointerenter="hovered = slice.id"
            @pointerleave="hovered = null"
            @focus="hovered = slice.id"
            @blur="hovered = null"
            @click="select(slice)"
          >
            <span class="qf-pie__swatch" :style="{ background: slice.color }" aria-hidden="true" />
            <span class="qf-pie__name">{{ slice.label }}<small v-if="slice.detail">{{ slice.detail }}</small></span>
            <span class="qf-pie__amount">{{ money(slice.valueCents) }}<small>{{ percent(slice.valueCents) }}</small></span>
          </component>
        </li>
      </ul>
    </div>
  </section>
</template>

<style scoped>
.qf-pie { min-width: 0; container-type: inline-size; }
.qf-pie__header { display: flex; flex-wrap: wrap; align-items: flex-start; justify-content: space-between; gap: 12px 24px; padding-bottom: 18px; }
.qf-pie__header h2 { margin: 0; font-size: 14px; font-weight: 600; }
.qf-pie__header p { margin: 5px 0 0; max-width: 480px; color: var(--qf-text-secondary); font-size: 12px; }
.qf-pie__total { display: flex; flex-direction: column; gap: 3px; }
.qf-pie__total span { color: var(--qf-text-muted); font-size: 11px; }
.qf-pie__total strong { font-size: 21px; font-weight: 600; font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
.qf-pie__body { display: grid; grid-template-columns: minmax(180px, .9fr) minmax(220px, 1.1fr); align-items: center; gap: 24px; }
.qf-pie__svg { display: block; width: 100%; max-width: 320px; justify-self: center; overflow: visible; }
.qf-pie__slice { stroke: var(--qf-bg-raised); stroke-width: 1; stroke-linejoin: round; transition: opacity 120ms ease; }
.qf-pie__slice[role='button'] { cursor: pointer; }
.qf-pie__slice.is-dim { opacity: .4; }
.qf-pie__slice.is-selected, .qf-pie__slice:focus-visible { stroke: var(--qf-text); stroke-width: 1.5; }
.qf-pie__legend { list-style: none; margin: 0; padding: 0; max-height: 320px; overflow-y: auto; min-width: 0; }
.qf-pie__row { display: grid; grid-template-columns: 10px minmax(0, 1fr) auto; align-items: center; gap: 10px; width: 100%; padding: 9px 10px; text-align: left; border: 1px solid transparent; border-radius: var(--qf-radius); background: transparent; color: var(--qf-text); }
button.qf-pie__row { cursor: pointer; }
.qf-pie__row.is-active { background: var(--qf-bg-hover); border-color: var(--qf-border-subtle); }
.qf-pie__row:focus-visible { outline: 2px solid var(--qf-accent); outline-offset: -2px; }
.qf-pie__swatch { width: 10px; height: 10px; border-radius: 3px; }
.qf-pie__name { font-size: 12px; overflow-wrap: anywhere; }
.qf-pie__amount { text-align: right; font-size: 12px; font-variant-numeric: tabular-nums; }
.qf-pie__row small { display: block; margin-top: 2px; font-size: 11px; color: var(--qf-text-secondary); }
.qf-pie__empty { margin: 0; padding: 40px 16px; color: var(--qf-text-secondary); text-align: center; font-size: 12px; }
@container (max-width: 540px) {
  .qf-pie__body { grid-template-columns: minmax(0, 1fr); gap: 16px; }
  .qf-pie__svg { max-width: 240px; }
}
</style>
