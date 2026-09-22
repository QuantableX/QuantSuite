<script setup lang="ts">
import type { IndicatorOption, TrendKind } from '#systems/types'

const props = withDefaults(defineProps<{
  modelValue: TrendKind[]
  options: IndicatorOption[]
  /** The run's own indicator — it is compared against, never with itself. */
  exclude?: TrendKind
  disabled?: boolean
  /** Small settings button inside the compact TOTAL controls. */
  iconOnly?: boolean
  /** Trigger text; the picker also serves as the aggregate's member list. */
  label?: string
  title?: string
  emptyHint?: string
  /** What one picked row is, for the foot line ("2 members"). */
  unit?: string
  unitPlural?: string
}>(), {
  label: 'Compare',
  title: 'Run further indicators on the same backtest and compare them',
  emptyHint: 'Pick indicators to run alongside',
  unit: 'extra run per backtest',
  unitPlural: 'extra runs per backtest',
})

const emit = defineEmits<{ 'update:modelValue': [value: TrendKind[]] }>()

const open = ref(false)
const root = ref<HTMLElement | null>(null)

const rows = computed(() => props.options.filter(o => o.value !== props.exclude))
const picked = computed(() => new Set(props.modelValue))

function toggleRow(option: IndicatorOption) {
  const next = props.modelValue.filter(v => v !== option.value)
  if (next.length === props.modelValue.length)
    next.push(option.value)
  // Keep catalog order (best first) so the chart and table read top-down.
  const order = new Map(props.options.map((o, i) => [o.value, i]))
  next.sort((a, b) => (order.get(a) ?? 99) - (order.get(b) ?? 99))
  emit('update:modelValue', next)
}

function clear() {
  emit('update:modelValue', [])
}

function close() {
  open.value = false
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape')
    close()
}

function onDocumentPointerDown(e: PointerEvent) {
  if (root.value && !root.value.contains(e.target as Node))
    close()
}

onMounted(() => document.addEventListener('pointerdown', onDocumentPointerDown))
onBeforeUnmount(() => document.removeEventListener('pointerdown', onDocumentPointerDown))
watch(() => props.disabled, disabled => { if (disabled) close() })

function gradeClass(grade: string | null): string {
  if (!grade)
    return ''
  if (grade === 'S' || grade === 'A')
    return 'qs-cmp__grade--top'
  if (grade === 'F')
    return 'qs-cmp__grade--fail'
  if (grade === 'C' || grade === 'D')
    return 'qs-cmp__grade--weak'
  return ''
}
</script>

<template>
  <div ref="root" class="qs-cmp" @keydown="onKeydown">
    <button
      type="button"
      class="qs-cmp__trigger"
      :class="{ 'qs-cmp__trigger--active': modelValue.length, 'qs-cmp__trigger--icon-only': iconOnly }"
      :aria-label="iconOnly ? `${label} (${modelValue.length} selected)` : undefined"
      :aria-expanded="open"
      aria-haspopup="listbox"
      :disabled="disabled"
      :title="title"
      @click="open = !open"
    >
      <svg v-if="iconOnly" width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
        <path d="M2 4h3m3 0h6M2 12h6m3 0h3M5 2v4m6 4v4" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
      </svg>
      <template v-else>
        <span class="qs-cmp__label">{{ label }}</span>
        <span class="qs-cmp__count mono" :class="{ 'qs-cmp__count--empty': !modelValue.length }">{{ modelValue.length }}</span>
        <svg class="qs-cmp__chevron" :class="{ 'qs-cmp__chevron--open': open }"
          width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true">
          <path d="M2.5 4.5L6 8L9.5 4.5" stroke="currentColor" stroke-width="1.5"
            stroke-linecap="round" stroke-linejoin="round" />
        </svg>
      </template>
    </button>

    <Transition name="fade">
      <div v-if="open" class="qs-cmp__panel" role="listbox" aria-multiselectable="true">
        <div class="qs-cmp__row qs-cmp__row--head" aria-hidden="true">
          <span />
          <span>Indicator</span>
          <span class="qs-cmp__col-score">Score</span>
          <span class="qs-cmp__col-grade">Grade</span>
        </div>
        <div class="qs-cmp__list">
          <button
            v-for="option in rows"
            :key="option.value"
            type="button"
            role="option"
            class="qs-cmp__row"
            :class="{ 'qs-cmp__row--on': picked.has(option.value) }"
            :aria-selected="picked.has(option.value)"
            :title="option.tag"
            @click="toggleRow(option)"
          >
            <span class="qs-cmp__check" :class="{ 'qs-cmp__check--on': picked.has(option.value) }">
              <svg v-if="picked.has(option.value)" width="10" height="10" viewBox="0 0 10 10" fill="none" aria-hidden="true">
                <path d="M2 5.2L4.2 7.4L8 3" stroke="currentColor" stroke-width="1.6"
                  stroke-linecap="round" stroke-linejoin="round" />
              </svg>
            </span>
            <span class="qs-cmp__name">{{ option.name }}</span>
            <span class="qs-cmp__col-score mono">{{ option.score ?? '—' }}</span>
            <span class="qs-cmp__col-grade">
              <span v-if="option.grade" class="qs-cmp__grade" :class="gradeClass(option.grade)">{{ option.grade }}</span>
              <span v-else class="qs-cmp__grade qs-cmp__grade--none">—</span>
            </span>
          </button>
        </div>
        <div class="qs-cmp__foot">
          <span class="qs-cmp__hint">
            {{ modelValue.length ? `${modelValue.length} ${modelValue.length === 1 ? unit : unitPlural}` : emptyHint }}
          </span>
          <button v-if="modelValue.length" type="button" class="qs-cmp__clear" @click="clear">Clear</button>
        </div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.qs-cmp {
  position: relative;
}

.qs-cmp__trigger {
  box-sizing: border-box;
  width: 100%;
  height: 32px;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  border: 1px solid var(--qs-border);
  border-radius: var(--qs-radius);
  background: var(--qs-bg-input);
  color: var(--qs-text);
  padding: 5px 10px;
  font-size: 12px;
  cursor: pointer;
  outline: none;
  transition: border-color var(--qs-transition);
}

.qs-cmp__trigger:focus,
.qs-cmp__trigger:hover {
  border-color: var(--qs-accent);
}

.qs-cmp__trigger--icon-only {
  justify-content: center;
  padding: 0;
}

.qs-cmp__label {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qs-cmp__trigger:disabled {
  opacity: 0.6;
  cursor: default;
  border-color: var(--qs-border);
}

.qs-cmp__trigger--active {
  border-color: color-mix(in srgb, var(--qs-accent) 60%, var(--qs-border));
}

.qs-cmp__count {
  box-sizing: border-box;
  flex: 0 0 24px;
  width: 24px;
  margin-left: auto;
  border-radius: 999px;
  padding: 0 3px;
  font-size: 11px;
  text-align: center;
  color: var(--qs-bg);
  background: var(--qs-accent);
}

.qs-cmp__count--empty {
  color: var(--qs-text-muted);
  background: var(--qs-bg-hover);
}

.qs-cmp__chevron {
  flex-shrink: 0;
  color: var(--qs-text-muted);
  transition: transform var(--qs-transition);
}

.qs-cmp__chevron--open {
  transform: rotate(180deg);
}

.qs-cmp__panel {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  z-index: 40;
  min-width: 360px;
  background: var(--qs-bg-card);
  border: 1px solid var(--qs-border);
  border-radius: var(--qs-radius);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
  padding: 4px;
}

.qs-cmp__list {
  max-height: 320px;
  overflow-y: auto;
}

.qs-cmp__row {
  display: grid;
  grid-template-columns: 16px 1fr 52px 52px;
  align-items: center;
  gap: 10px;
  width: 100%;
  border: none;
  border-radius: calc(var(--qs-radius) - 3px);
  background: transparent;
  color: var(--qs-text);
  padding: 7px 10px;
  font-size: 13px;
  text-align: left;
  cursor: pointer;
}

.qs-cmp__row:hover {
  background: var(--qs-bg-hover);
}

.qs-cmp__row--on {
  color: var(--qs-accent-hover);
}

.qs-cmp__row--head {
  cursor: default;
  padding-top: 5px;
  padding-bottom: 5px;
  font-size: 10px;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--qs-text-muted);
  border-bottom: 1px solid var(--qs-border-subtle);
  border-radius: 0;
  margin-bottom: 4px;
}

.qs-cmp__row--head:hover {
  background: transparent;
}

.qs-cmp__check {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 14px;
  height: 14px;
  border: 1px solid var(--qs-border);
  border-radius: 3px;
  color: var(--qs-bg);
  background: var(--qs-bg-input);
}

.qs-cmp__check--on {
  border-color: var(--qs-accent);
  background: var(--qs-accent);
}

.qs-cmp__name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qs-cmp__col-score {
  text-align: right;
  font-size: 12px;
  color: var(--qs-text-secondary);
}

.qs-cmp__col-grade {
  text-align: center;
}

.qs-cmp__grade {
  display: inline-block;
  min-width: 22px;
  border: 1px solid var(--qs-border-subtle);
  border-radius: 4px;
  padding: 0 5px;
  font-size: 11px;
  font-weight: 600;
  text-align: center;
  color: var(--qs-text-secondary);
}

.qs-cmp__grade--top {
  color: var(--qs-success);
  border-color: color-mix(in srgb, var(--qs-success) 45%, transparent);
}

.qs-cmp__grade--weak {
  color: var(--qs-warning);
  border-color: color-mix(in srgb, var(--qs-warning) 45%, transparent);
}

.qs-cmp__grade--fail {
  color: var(--qs-error);
  border-color: color-mix(in srgb, var(--qs-error) 45%, transparent);
}

.qs-cmp__grade--none {
  border-color: transparent;
  color: var(--qs-text-muted);
}

.qs-cmp__foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-top: 4px;
  padding: 6px 10px 4px;
  border-top: 1px solid var(--qs-border-subtle);
}

.qs-cmp__hint {
  font-size: 11px;
  color: var(--qs-text-muted);
}

.qs-cmp__clear {
  border: none;
  background: transparent;
  color: var(--qs-text-secondary);
  font-size: 11px;
  cursor: pointer;
  padding: 2px 4px;
}

.qs-cmp__clear:hover {
  color: var(--qs-text);
}
</style>
