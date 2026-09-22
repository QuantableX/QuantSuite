<script setup lang="ts">
import type { IndicatorOption } from '#systems/types'

const props = defineProps<{
  modelValue: string
  options: IndicatorOption[]
  /** Toolbar size: a tighter trigger, the list anchored to its right edge. */
  compact?: boolean
  disabled?: boolean
}>()

const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

const open = ref(false)
const highlighted = ref(-1)
const root = ref<HTMLElement | null>(null)

const selected = computed(
  () => props.options.find(o => o.value === props.modelValue) ?? props.options[0],
)

function toggle() {
  open.value = !open.value
  if (open.value)
    highlighted.value = props.options.findIndex(o => o.value === props.modelValue)
}

function close() {
  open.value = false
  highlighted.value = -1
}

function pick(option: IndicatorOption) {
  emit('update:modelValue', option.value)
  close()
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    close()
    return
  }
  if (e.key === 'Enter' || e.key === ' ') {
    e.preventDefault()
    if (open.value && highlighted.value >= 0)
      pick(props.options[highlighted.value]!)
    else
      toggle()
    return
  }
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    if (!open.value) {
      toggle()
      return
    }
    const delta = e.key === 'ArrowDown' ? 1 : -1
    const n = props.options.length
    highlighted.value = (highlighted.value + delta + n) % n
  }
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
    return 'qs-isel__grade--top'
  if (grade === 'F')
    return 'qs-isel__grade--fail'
  if (grade === 'C' || grade === 'D')
    return 'qs-isel__grade--weak'
  return ''
}
</script>

<template>
  <div ref="root" class="qs-isel" :class="{ 'qs-isel--compact': compact }">
    <button
      type="button"
      class="qs-isel__trigger"
      :title="selected?.name"
      :aria-expanded="open"
      aria-haspopup="listbox"
      :disabled="disabled"
      @click="toggle"
      @keydown="onKeydown"
    >
      <span class="qs-isel__trigger-name">{{ selected?.name }}</span>
      <span v-if="selected?.score != null" class="qs-isel__trigger-meta mono">
        {{ selected.score }}
        <span class="qs-isel__grade" :class="gradeClass(selected.grade)">{{ selected.grade }}</span>
      </span>
      <span v-else-if="selected?.tag" class="qs-isel__trigger-meta">{{ selected.tag }}</span>
      <svg class="qs-isel__chevron" :class="{ 'qs-isel__chevron--open': open }"
        width="12" height="12" viewBox="0 0 12 12" fill="none" aria-hidden="true">
        <path d="M2.5 4.5L6 8L9.5 4.5" stroke="currentColor" stroke-width="1.5"
          stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </button>

    <Transition name="fade">
      <div v-if="open" class="qs-isel__panel" role="listbox">
        <div class="qs-isel__row qs-isel__row--head" aria-hidden="true">
          <span>Indicator</span>
          <span class="qs-isel__col-score">Score</span>
          <span class="qs-isel__col-grade">Grade</span>
        </div>
        <button
          v-for="(option, i) in options"
          :key="option.value"
          type="button"
          role="option"
          class="qs-isel__row"
          :class="{
            'qs-isel__row--selected': option.value === modelValue,
            'qs-isel__row--highlighted': i === highlighted,
          }"
          :aria-selected="option.value === modelValue"
          :title="option.tag ? `${option.name} — ${option.tag}` : option.name"
          @pointerenter="highlighted = i"
          @click="pick(option)"
        >
          <span class="qs-isel__name">{{ option.name }}</span>
          <span class="qs-isel__col-score mono">{{ option.score ?? '—' }}</span>
          <span class="qs-isel__col-grade">
            <span v-if="option.grade" class="qs-isel__grade" :class="gradeClass(option.grade)">
              {{ option.grade }}
            </span>
            <span v-else class="qs-isel__grade qs-isel__grade--none">—</span>
          </span>
        </button>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.qs-isel {
  position: relative;
  width: 100%;
  min-width: 0;
}

.qs-isel__trigger {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  border: 1px solid var(--qs-border);
  border-radius: var(--qs-radius);
  background: var(--qs-bg-input);
  color: var(--qs-text);
  padding: 8px 12px;
  cursor: pointer;
  text-align: left;
  outline: none;
  transition: border-color var(--qs-transition);
}

.qs-isel__trigger:focus,
.qs-isel__trigger:hover {
  border-color: var(--qs-accent);
}

.qs-isel__trigger:disabled {
  opacity: 0.6;
  cursor: default;
  border-color: var(--qs-border);
}

/* Toolbar variant: the trigger shrinks to the row's button height and the
   list, wider than the trigger, hangs off its right edge. */
.qs-isel--compact .qs-isel__trigger {
  box-sizing: border-box;
  height: 32px;
  padding: 5px 10px;
  font-size: 12px;
  gap: 6px;
}

.qs-isel--compact .qs-isel__panel {
  left: auto;
  right: 0;
  min-width: 340px;
}

.qs-isel__trigger-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qs-isel:not(.qs-isel--compact) .qs-isel__trigger-name {
  white-space: normal;
  overflow-wrap: anywhere;
}

.qs-isel__trigger-meta {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--qs-text-secondary);
}

.qs-isel__chevron {
  flex-shrink: 0;
  color: var(--qs-text-muted);
  transition: transform var(--qs-transition);
}

.qs-isel__chevron--open {
  transform: rotate(180deg);
}

.qs-isel__panel {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  z-index: 40;
  background: var(--qs-bg-card);
  border: 1px solid var(--qs-border);
  border-radius: var(--qs-radius);
  box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
  max-height: 360px;
  overflow-y: auto;
  padding: 4px;
}

.qs-isel__row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 52px 52px;
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

.qs-isel__row--head {
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

.qs-isel__row--highlighted {
  background: var(--qs-bg-hover);
}

.qs-isel__row--selected {
  color: var(--qs-accent-hover);
}

.qs-isel__name {
  display: block;
  min-width: 0;
  white-space: normal;
  overflow-wrap: anywhere;
}

.qs-isel__col-score {
  text-align: right;
  font-size: 12px;
  color: var(--qs-text-secondary);
}

.qs-isel__col-grade {
  text-align: center;
}

.qs-isel__grade {
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

.qs-isel__grade--top {
  color: var(--qs-success);
  border-color: color-mix(in srgb, var(--qs-success) 45%, transparent);
}

.qs-isel__grade--weak {
  color: var(--qs-warning);
  border-color: color-mix(in srgb, var(--qs-warning) 45%, transparent);
}

.qs-isel__grade--fail {
  color: var(--qs-error);
  border-color: color-mix(in srgb, var(--qs-error) 45%, transparent);
}

.qs-isel__grade--none {
  border-color: transparent;
  color: var(--qs-text-muted);
}
</style>
