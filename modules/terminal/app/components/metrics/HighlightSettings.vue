<script setup lang="ts">
import { Highlighter, SlidersHorizontal } from 'lucide-vue-next'
import { useMetricsStore } from '#terminal/stores/metrics'
import { formatValue, type Band } from '#terminal/utils/metrics'
import {
  defaultHighlightRule,
  highlightBands,
  sameHighlightRule,
  type HighlightMode,
  type HighlightResult,
  type HighlightRule,
  type HighlightSide,
} from '#terminal/utils/metric-highlight'

const props = defineProps<{
  title: string
  unit: string
  result: HighlightResult
}>()
const store = useMetricsStore()
const root = ref<HTMLElement | null>(null)
const open = ref(false)
const rule = computed(() => store.highlightRule(props.title))
const bands = computed(() => highlightBands(props.title))
const isDefault = computed(() =>
  sameHighlightRule(rule.value, defaultHighlightRule(props.title)),
)
const modes: { value: HighlightMode; label: string }[] = [
  { value: 'bands', label: 'Bands' },
  { value: 'value', label: 'Value' },
  { value: 'sigma', label: 'Std dev' },
]
const sides: { value: HighlightSide; label: string }[] = [
  { value: 'both', label: 'Both sides' },
  { value: 'above', label: 'Above' },
  { value: 'below', label: 'Below' },
]

/** Choosing a range is a request to see it, so editing switches the highlight on. */
function update(change: Partial<HighlightRule>) {
  store.setHighlightRule(props.title, { ...rule.value, ...change })
  store.highlight = true
}
function toggleBand(label: string) {
  const chosen = rule.value.bands
  update({
    bands: chosen.includes(label) ? chosen.filter((l) => l !== label) : [...chosen, label],
  })
}
function numberFrom(event: Event) {
  const raw = (event.target as HTMLInputElement).value.trim()
  return raw === '' || !Number.isFinite(Number(raw)) ? null : Number(raw)
}
function setSigma(event: Event) {
  const value = numberFrom(event)
  if (value != null && value > 0) update({ sigma: value })
  else (event.target as HTMLInputElement).value = String(rule.value.sigma)
}
const bandRange = (band: Band) =>
  !Number.isFinite(band.min)
    ? `< ${formatValue(band.max, props.unit)}`
    : band === bands.value.at(-1)
      ? `≥ ${formatValue(band.min, props.unit)}`
      : `${formatValue(band.min, props.unit)} – ${formatValue(band.max, props.unit)}`

// The chart clips its overflow and its toolbar wraps on narrow windows, so the
// popover lives on <body> and is placed against the viewport.
const trigger = ref<HTMLElement | null>(null)
const popover = ref<HTMLElement | null>(null)
const placement = ref<Record<string, string>>({})
const WIDTH = 300,
  GAP = 6,
  MARGIN = 8
function place() {
  const rect = trigger.value?.getBoundingClientRect()
  if (!rect) return
  const left = Math.max(MARGIN, Math.min(rect.right - WIDTH, window.innerWidth - WIDTH - MARGIN))
  const below = window.innerHeight - rect.bottom - GAP - MARGIN,
    above = rect.top - GAP - MARGIN
  placement.value =
    below >= 280 || below >= above
      ? { left: `${left}px`, top: `${rect.bottom + GAP}px`, maxHeight: `${below}px` }
      : {
          left: `${left}px`,
          bottom: `${window.innerHeight - rect.top + GAP}px`,
          maxHeight: `${above}px`,
        }
}
function toggleOpen() {
  if (!open.value) place()
  open.value = !open.value
}
const inside = (target: EventTarget | null) =>
  !!target && (root.value?.contains(target as Node) || popover.value?.contains(target as Node))
function outside(event: Event) {
  if (open.value && !inside(event.target)) open.value = false
}
function close() {
  open.value = false
}
onMounted(() => {
  document.addEventListener('pointerdown', outside)
  window.addEventListener('scroll', outside, true)
  window.addEventListener('resize', close)
})
// The page is kept alive between visits; a <body> popover must not outlive it.
onDeactivated(close)
onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', outside)
  window.removeEventListener('scroll', outside, true)
  window.removeEventListener('resize', close)
})
</script>

<template>
  <div ref="root" class="highlight-control" @keydown.esc.stop="open = false">
    <button
      class="highlight-toggle"
      :class="{ active: store.highlight }"
      :aria-pressed="store.highlight"
      :title="
        store.highlight
          ? 'Show the whole chart in colour'
          : 'Grey out the days outside the chosen indicator range'
      "
      @click="store.highlight = !store.highlight"
    >
      <Highlighter :size="13" />Highlight<span
        v-if="store.highlight && result.ok"
        class="match-count"
        >{{ result.periodMatches }}d</span
      >
    </button>
    <button
      ref="trigger"
      class="highlight-settings"
      :class="{ active: open }"
      :aria-expanded="open"
      aria-haspopup="dialog"
      aria-label="Highlight ranges"
      title="Choose the highlighted range for this indicator"
      @click="toggleOpen"
    >
      <SlidersHorizontal :size="13" />
    </button>
    <Teleport to="body"
      ><div
        v-if="open"
        ref="popover"
        data-module="terminal"
        class="highlight-popover"
        role="dialog"
        aria-label="Highlight ranges"
        :style="placement"
        @keydown.esc.stop="open = false"
      >
      <header>
        <strong>Highlight ranges</strong><span>{{ title }}</span>
      </header>
      <p class="hint">
        Days in this range keep their colour; the rest of the price and indicator chart is greyed
        out.
      </p>
      <div class="modes" role="radiogroup" aria-label="Range type">
        <button
          v-for="mode in modes"
          :key="mode.value"
          role="radio"
          :class="{ active: rule.mode === mode.value }"
          :aria-checked="rule.mode === mode.value"
          :disabled="mode.value === 'bands' && !bands.length"
          :title="
            mode.value === 'bands' && !bands.length
              ? 'This indicator has no named bands'
              : undefined
          "
          @click="update({ mode: mode.value })"
        >
          {{ mode.label }}
        </button>
      </div>

      <div v-if="rule.mode === 'bands'" class="band-list">
        <label v-for="band in bands" :key="band.label"
          ><input
            type="checkbox"
            :checked="rule.bands.includes(band.label)"
            @change="toggleBand(band.label)" /><i :style="{ background: band.color }" /><span>{{
            band.label
          }}</span
          ><small>{{ bandRange(band) }}</small></label
        >
      </div>

      <div v-else-if="rule.mode === 'value'" class="value-range">
        <label
          >From<input
            type="number"
            step="any"
            placeholder="No limit"
            :value="rule.min ?? ''"
            aria-label="Highlight minimum"
            @change="update({ min: numberFrom($event) })" /></label
        ><span>–</span
        ><label
          >To<input
            type="number"
            step="any"
            placeholder="No limit"
            :value="rule.max ?? ''"
            aria-label="Highlight maximum"
            @change="update({ max: numberFrom($event) })"
        /></label>
        <small>{{ unit }} · leave one side empty for no limit</small>
      </div>

      <div v-else class="sigma-range">
        <div class="sigma-row">
          <label
            >Beyond<input
              type="number"
              min="0.5"
              max="10"
              step="0.5"
              :value="rule.sigma"
              aria-label="Standard deviations"
              @change="setSigma" />σ</label
          >
          <div class="presets">
            <button
              v-for="k in [1, 2, 3]"
              :key="k"
              :class="{ active: rule.sigma === k }"
              :aria-pressed="rule.sigma === k"
              @click="update({ sigma: k })"
            >
              {{ k }}σ
            </button>
          </div>
        </div>
        <div class="modes" role="radiogroup" aria-label="Side">
          <button
            v-for="side in sides"
            :key="side.value"
            role="radio"
            :class="{ active: rule.side === side.value }"
            :aria-checked="rule.side === side.value"
            @click="update({ side: side.value })"
          >
            {{ side.label }}
          </button>
        </div>
        <small v-if="result.ok && result.stats"
          >Mean {{ formatValue(result.stats.mean, unit) }} · σ
          {{ formatValue(result.stats.deviation, unit) }} over the selected period</small
        >
      </div>

      <footer>
        <span v-if="result.ok"
          ><strong>{{ result.periodMatches }}</strong> of {{ result.periodDays }} days in the
          selected period</span
        ><span v-else class="invalid">{{ result.reason }}</span
        ><button :disabled="isDefault" @click="store.resetHighlightRule(title)">
          Reset to default
        </button>
      </footer>
    </div></Teleport
    >
  </div>
</template>

<style scoped>
.highlight-control {
  position: relative;
  display: flex;
  gap: 1px;
}
button {
  display: flex;
  align-items: center;
  gap: 5px;
  border: 1px solid transparent;
  background: transparent;
  color: var(--text-secondary);
  border-radius: 4px;
  padding: 5px 7px;
  font-size: 12px;
  cursor: pointer;
}
button:hover,
button.active {
  background: var(--surface-2);
  border-color: var(--border);
  color: var(--text-primary);
}
button:disabled {
  opacity: 0.4;
  cursor: default;
}
button:focus-visible,
input:focus-visible {
  outline: 2px solid #70b5e8;
  outline-offset: 2px;
}
.match-count {
  font-family: var(--qss-font-mono);
  font-size: 11px;
  color: var(--text-secondary);
}
.highlight-popover {
  /* Above the teleported Focus workspace (z-index 200). */
  position: fixed;
  z-index: 300;
  width: 300px;
  overflow: auto;
  display: grid;
  gap: 10px;
  padding: 12px;
  background: var(--surface-1);
  border: 1px solid var(--border);
  border-radius: 6px;
  box-shadow: 0 10px 28px rgb(0 0 0 / 0.32);
  font-size: 12px;
  color: var(--text-secondary);
}
header {
  display: grid;
  gap: 2px;
}
header strong {
  color: var(--text-primary);
  font-size: 13px;
  font-weight: 600;
}
header span,
.hint,
small {
  font-size: 11px;
  color: var(--muted);
}
.hint {
  margin: 0;
  line-height: 1.4;
}
.modes {
  display: flex;
  gap: 2px;
  padding: 2px;
  border: 1px solid var(--border);
  border-radius: 5px;
}
.modes button {
  flex: 1;
  justify-content: center;
  padding: 4px 6px;
}
.band-list {
  display: grid;
  gap: 2px;
}
.band-list label {
  display: grid;
  grid-template-columns: auto auto 1fr auto;
  align-items: center;
  gap: 7px;
  padding: 4px 6px;
  border-radius: 4px;
  cursor: pointer;
  color: var(--text-primary);
}
.band-list label:hover {
  background: var(--surface-2);
}
.band-list i {
  width: 8px;
  height: 8px;
  border-radius: 50%;
}
.band-list small {
  font-family: var(--qss-font-mono);
}
input[type='checkbox'] {
  margin: 0;
  accent-color: #70b5e8;
}
.value-range {
  display: flex;
  flex-wrap: wrap;
  align-items: end;
  gap: 8px;
}
.value-range label,
.sigma-row label {
  font-size: 11px;
  color: var(--muted);
}
.value-range label {
  flex: 1;
  min-width: 0;
}
.value-range > span {
  margin-bottom: 7px;
  color: var(--muted);
}
.value-range small {
  flex-basis: 100%;
}
input[type='number'] {
  display: block;
  width: 100%;
  min-width: 0;
  margin-top: 4px;
  padding: 6px 7px;
  background: var(--surface-0);
  color: var(--text-primary);
  border: 1px solid var(--border);
  border-radius: 4px;
  font-family: var(--qss-font-mono);
  font-size: 11px;
}
.sigma-range {
  display: grid;
  gap: 8px;
}
.sigma-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.sigma-row label {
  display: flex;
  align-items: center;
  gap: 6px;
}
.sigma-row input {
  width: 58px;
  margin-top: 0;
}
.presets {
  display: flex;
  gap: 2px;
}
footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding-top: 8px;
  border-top: 1px solid var(--border);
  font-size: 11px;
}
footer strong {
  font-family: var(--qss-font-mono);
  color: var(--text-primary);
}
.invalid {
  color: var(--negative);
}
footer button {
  flex-shrink: 0;
  font-size: 11px;
  padding: 4px 6px;
}
</style>
