<script setup lang="ts">
/**
 * One property value — the shared cell every view renders.
 *
 * Display and editing live in the same component on purpose: a table cell, a
 * board card chip and the editor's property panel must agree on what "empty"
 * looks like and on what clicking does, and three components drift.
 *
 * `readonly` renders the value only (cards, calendar chips); the default is
 * click-to-edit in place.
 */
import { useSchemaStore } from '#notes/stores/schema'
import { useNotesStore } from '#notes/stores/notes'
import type { OptionColor, Property, PropertyValue, SelectOption } from '#notes/types'

const props = withDefaults(
  defineProps<{
    noteId: string
    property: Property
    value: PropertyValue | undefined
    readonly?: boolean
    /** Hides the empty placeholder — for cards, where a blank chip is noise. */
    hideEmpty?: boolean
  }>(),
  { readonly: false, hideEmpty: false },
)

const schema = useSchemaStore()
const notes = useNotesStore()

const editing = ref(false)
const draft = ref('')
const menuOpen = ref(false)
const root = ref<HTMLElement | null>(null)

const options = computed(() => props.property.config.options ?? [])

const selected = computed<SelectOption[]>(() => {
  if (props.property.kind === 'select') {
    const found = options.value.find((o) => o.id === props.value)
    return found ? [found] : []
  }
  if (props.property.kind === 'multi_select') {
    const ids = Array.isArray(props.value) ? props.value : []
    return ids.map((id) => options.value.find((o) => o.id === id)).filter((o): o is SelectOption => !!o)
  }
  return []
})

const isEmpty = computed(() => {
  const v = props.value
  if (v === null || v === undefined || v === '') return true
  if (Array.isArray(v)) return v.length === 0
  return false
})

const display = computed(() => {
  const v = props.value
  if (isEmpty.value) return ''
  if (props.property.kind === 'date') {
    // Rendered from the parts, not `new Date(v)`: a bare `YYYY-MM-DD` parses
    // as UTC midnight and shows as the day before in negative offsets.
    const [y, m, d] = String(v).split('-').map(Number)
    if (!y || !m || !d) return String(v)
    return new Date(y, m - 1, d).toLocaleDateString(undefined, {
      month: 'short',
      day: 'numeric',
      year: y === new Date().getFullYear() ? undefined : 'numeric',
    })
  }
  return String(v)
})

async function commit(value: PropertyValue) {
  await notes.setProperty(props.noteId, props.property.id, value)
}

function startEdit() {
  if (props.readonly) return
  if (props.property.kind === 'select' || props.property.kind === 'multi_select') {
    menuOpen.value = true
    return
  }
  if (props.property.kind === 'checkbox') {
    void commit(!props.value)
    return
  }
  draft.value = props.value === null || props.value === undefined ? '' : String(props.value)
  editing.value = true
  nextTick(() => root.value?.querySelector('input')?.focus())
}

async function commitInput() {
  editing.value = false
  const raw = draft.value.trim()
  if (raw === '') {
    await commit(null)
    return
  }
  if (props.property.kind === 'number') {
    const n = Number(raw)
    await commit(Number.isFinite(n) ? n : null)
    return
  }
  await commit(raw)
}

async function pickOption(option: SelectOption | null) {
  if (props.property.kind === 'select') {
    await commit(option ? option.id : null)
    menuOpen.value = false
    return
  }
  const ids = Array.isArray(props.value) ? [...props.value] : []
  if (!option) {
    await commit(null)
    menuOpen.value = false
    return
  }
  const at = ids.indexOf(option.id)
  if (at === -1) ids.push(option.id)
  else ids.splice(at, 1)
  await commit(ids.length ? ids : null)
}

const newOptionName = ref('')
const PALETTE: OptionColor[] = ['slate', 'blue', 'green', 'amber', 'red', 'purple', 'pink']

async function createOption() {
  const name = newOptionName.value.trim()
  if (!name) return
  // Cycle the palette by option count so a fresh list is not seven greys.
  const color = PALETTE[options.value.length % PALETTE.length]!
  const option = await schema.addOption(props.property.id, name, color)
  newOptionName.value = ''
  await pickOption(option)
}

function onPointerDown(event: PointerEvent) {
  if (!root.value?.contains(event.target as Node)) menuOpen.value = false
}

watch(menuOpen, (open) => {
  if (open) document.addEventListener('pointerdown', onPointerDown)
  else document.removeEventListener('pointerdown', onPointerDown)
})

onBeforeUnmount(() => document.removeEventListener('pointerdown', onPointerDown))
</script>

<template>
  <div ref="root" class="qn-cell" :class="{ 'qn-cell--readonly': readonly }">
    <!-- Checkbox reads as a control, not as a value, so it never swaps to an input. -->
    <button
      v-if="property.kind === 'checkbox'"
      class="qn-cell__check"
      :class="{ 'is-on': !!value }"
      :disabled="readonly"
      :aria-label="property.name"
      @click.stop="startEdit"
    >
      <svg v-if="value" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M20 6 9 17l-5-5" />
      </svg>
    </button>

    <input
      v-else-if="editing"
      v-model="draft"
      class="qn-cell__input"
      :type="property.kind === 'number' ? 'number' : property.kind === 'date' ? 'date' : 'text'"
      @blur="commitInput"
      @keydown.enter.prevent="commitInput"
      @keydown.esc.prevent="editing = false"
    />

    <button v-else class="qn-cell__value" :disabled="readonly" @click.stop="startEdit">
      <template v-if="selected.length">
        <span v-for="option in selected" :key="option.id" class="qn-tag" :data-color="option.color">
          {{ option.name }}
        </span>
      </template>
      <a
        v-else-if="property.kind === 'url' && !isEmpty"
        class="qn-cell__link"
        :href="String(value)"
        target="_blank"
        rel="noreferrer"
        @click.stop
      >{{ display }}</a>
      <span v-else-if="!isEmpty" class="qn-cell__text">{{ display }}</span>
      <span v-else-if="!hideEmpty" class="qn-cell__empty">Empty</span>
    </button>

    <!-- Select / multi-select picker -->
    <div v-if="menuOpen" class="qn-cell__menu">
      <div class="qn-cell__menu-head">
        <input
          v-model="newOptionName"
          class="qn-cell__menu-input"
          placeholder="Search or create…"
          @keydown.enter.prevent="createOption"
        />
      </div>
      <div class="qn-cell__menu-list">
        <button
          v-for="option in options.filter((o) => o.name.toLowerCase().includes(newOptionName.toLowerCase()))"
          :key="option.id"
          class="qn-cell__menu-item"
          :class="{ 'is-selected': selected.some((s) => s.id === option.id) }"
          @click="pickOption(option)"
        >
          <span class="qn-tag" :data-color="option.color">{{ option.name }}</span>
        </button>
        <button v-if="newOptionName.trim()" class="qn-cell__menu-item qn-cell__menu-create" @click="createOption">
          Create “{{ newOptionName.trim() }}”
        </button>
        <button v-if="selected.length" class="qn-cell__menu-item qn-cell__menu-clear" @click="pickOption(null)">
          Clear
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qn-cell {
  position: relative;
  min-width: 0;
  display: flex;
  align-items: center;
}

.qn-cell__value {
  min-width: 0;
  width: 100%;
  display: flex;
  align-items: center;
  gap: 4px;
  flex-wrap: wrap;
  padding: 4px 6px;
  border: 1px solid transparent;
  border-radius: 6px;
  background: transparent;
  color: var(--qn-text);
  font-size: 13px;
  text-align: left;
  cursor: pointer;
}

.qn-cell__value:hover:not(:disabled) {
  background: var(--qn-bg-hover);
}

.qn-cell--readonly .qn-cell__value {
  cursor: default;
  padding: 0;
}

.qn-cell__text,
.qn-cell__link {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qn-cell__link {
  color: var(--qn-accent);
  text-decoration: underline;
  text-underline-offset: 2px;
}

.qn-cell__empty {
  color: var(--qn-text-muted);
  font-size: 12px;
}

.qn-cell__input {
  width: 100%;
  padding: 3px 6px;
  border: 1px solid var(--qn-accent);
  border-radius: 6px;
  background: var(--qn-bg-input);
  color: var(--qn-text);
  font-size: 13px;
  font-family: inherit;
  outline: none;
}

.qn-cell__check {
  display: grid;
  place-items: center;
  width: 16px;
  height: 16px;
  border: 1px solid var(--qn-border);
  border-radius: 4px;
  background: var(--qn-bg-input);
  color: var(--qn-bg);
  cursor: pointer;
}

.qn-cell__check.is-on {
  border-color: var(--qn-accent);
  background: var(--qn-accent);
}

/* Absolute, never fixed: the module panel is the containing block
   (ARCHITECTURE.md §9 / PLAN-V3 §3). */
.qn-cell__menu {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  z-index: 40;
  width: 220px;
  border: 1px solid var(--qn-border);
  border-radius: 10px;
  background: var(--qn-bg-sidebar);
  box-shadow: 0 16px 36px rgba(0, 0, 0, 0.35);
  overflow: hidden;
}

.qn-cell__menu-head {
  padding: 8px;
  border-bottom: 1px solid var(--qn-border);
}

.qn-cell__menu-input {
  width: 100%;
  padding: 5px 8px;
  border: 1px solid var(--qn-border);
  border-radius: 6px;
  background: var(--qn-bg-input);
  color: var(--qn-text);
  font-size: 12px;
  font-family: inherit;
  outline: none;
}

.qn-cell__menu-list {
  max-height: 240px;
  overflow: auto;
  padding: 4px;
}

.qn-cell__menu-item {
  display: flex;
  align-items: center;
  width: 100%;
  padding: 5px 6px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qn-text-secondary);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}

.qn-cell__menu-item:hover {
  background: var(--qn-bg-hover);
}

.qn-cell__menu-item.is-selected {
  background: var(--qn-bg-card);
}

.qn-cell__menu-create,
.qn-cell__menu-clear {
  color: var(--qn-text-muted);
}
</style>
