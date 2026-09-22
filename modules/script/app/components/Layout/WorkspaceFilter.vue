<script setup lang="ts">
import { Check, ChevronDown } from 'lucide-vue-next'
import { useWorkbenchStore } from '#script/stores/workbench'
import { LIBRARY_FILTERS } from '#script/utils/library'

const wb = useWorkbenchStore()
const trigger = ref<HTMLButtonElement | null>(null)
const menu = ref<HTMLDivElement | null>(null)
const menuId = useId()
const open = ref(false)
const selected = computed(() => LIBRARY_FILTERS.find((f) => f.id === wb.libraryFilter) ?? LIBRARY_FILTERS[0])

function close(restoreFocus = false) {
  menu.value?.hidePopover()
  open.value = false
  if (restoreFocus) trigger.value?.focus()
}

function focusOption(index: number) {
  menu.value?.querySelectorAll<HTMLButtonElement>('[role="option"]')[index]?.focus()
}

function show() {
  if (!trigger.value || !menu.value) return
  const rect = trigger.value.getBoundingClientRect()
  // The top layer escapes the sidebar's scroll container, retaining its theme.
  const width = Math.min(Math.max(rect.width, 196), window.innerWidth - 16)
  const below = window.innerHeight - rect.bottom - 12
  const above = rect.top - 12
  const upwards = below < 146 && above > below
  Object.assign(menu.value.style, {
    left: `${Math.max(8, Math.min(rect.left, window.innerWidth - width - 8))}px`,
    top: upwards ? 'auto' : `${rect.bottom + 4}px`,
    bottom: upwards ? `${window.innerHeight - rect.top + 4}px` : 'auto',
    width: `${width}px`,
    maxHeight: `${Math.max(0, upwards ? above : below)}px`,
  })
  menu.value.showPopover()
  open.value = true
  focusOption(LIBRARY_FILTERS.findIndex((f) => f.id === selected.value.id))
}

function select(id: typeof wb.libraryFilter) {
  wb.libraryFilter = id
  close(true)
}

function onKeydown(event: KeyboardEvent) {
  const options = [...(menu.value?.querySelectorAll<HTMLButtonElement>('[role="option"]') ?? [])]
  const index = options.indexOf(document.activeElement as HTMLButtonElement)
  const last = options.length - 1
  let next: number
  switch (event.key) {
    case 'ArrowDown': next = (index + 1) % options.length; break
    case 'ArrowUp': next = (index + last) % options.length; break
    case 'Home': next = 0; break
    case 'End': next = last; break
    case 'Escape':
      event.preventDefault()
      event.stopPropagation()
      close(true)
      return
    case 'Tab': close(true); return
    default: return
  }
  event.preventDefault()
  event.stopPropagation()
  focusOption(next)
}

function onViewportChange(event: Event) {
  if (open.value && (!(event.target instanceof Node) || !menu.value?.contains(event.target))) close()
}

onMounted(() => {
  window.addEventListener('resize', onViewportChange)
  document.addEventListener('scroll', onViewportChange, true)
})
onBeforeUnmount(() => {
  window.removeEventListener('resize', onViewportChange)
  document.removeEventListener('scroll', onViewportChange, true)
})
onDeactivated(() => close())
</script>

<template>
  <div class="qsc-workspace-filter">
    <button
      ref="trigger"
      type="button"
      class="qsc-filter-trigger"
      aria-label="Filter workspace"
      aria-haspopup="listbox"
      :aria-expanded="open"
      :aria-controls="menuId"
      @click="open ? close() : show()"
      @keydown.down.prevent="show"
      @keydown.up.prevent="show"
    >
      <span class="qsc-filter-label">{{ selected.label }}</span>
      <span class="qsc-filter-count">{{ wb.filterCounts[selected.id] }}</span>
      <ChevronDown :size="13" :class="{ 'is-open': open }" />
    </button>
    <div
      :id="menuId"
      ref="menu"
      popover="auto"
      class="qsc-filter-menu"
      role="listbox"
      aria-label="Filter workspace"
      @toggle="open = menu?.matches(':popover-open') ?? false"
      @keydown="onKeydown"
    >
      <button
        v-for="f in LIBRARY_FILTERS"
        :key="f.id"
        type="button"
        class="qsc-filter-option"
        role="option"
        :aria-selected="wb.libraryFilter === f.id"
        tabindex="-1"
        @click="select(f.id)"
      >
        <Check :size="13" :class="{ 'is-hidden': wb.libraryFilter !== f.id }" />
        <span class="qsc-filter-label">{{ f.label }}</span>
        <span class="qsc-filter-count">{{ wb.filterCounts[f.id] }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.qsc-workspace-filter { flex: 1; min-width: 0; }
.qsc-filter-trigger {
  display: flex; align-items: center; gap: 7px; width: 100%; height: 30px;
  padding: 0 9px; border: 1px solid var(--qss-border); border-radius: 6px;
  background: var(--qss-bg); color: var(--qss-text); font-size: 11px; text-align: left; cursor: pointer;
}
.qsc-filter-trigger:hover, .qsc-filter-trigger[aria-expanded="true"] { background: var(--qss-bg-hover); border-color: var(--qss-text-muted); }
.qsc-filter-trigger:focus-visible { outline: 2px solid var(--qss-accent); outline-offset: 2px; }
.qsc-filter-trigger svg { flex-shrink: 0; color: var(--qss-text-secondary); }
.qsc-filter-trigger svg.is-open { transform: rotate(180deg); }
.qsc-filter-label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qsc-filter-count { color: var(--qss-text-secondary); font-variant-numeric: tabular-nums; font-size: 11px; }
.qsc-filter-menu {
  position: fixed; inset: auto; box-sizing: border-box; margin: 0; padding: 4px;
  border: 1px solid var(--qss-border); border-radius: 8px;
  background: var(--qss-bg-card); color: var(--qss-text); box-shadow: var(--qss-shadow-md);
  overflow-y: auto;
}
.qsc-filter-option {
  display: flex; align-items: center; gap: 8px; width: 100%; min-height: 32px; padding: 6px 8px;
  border: 0; border-radius: 4px; background: transparent; color: var(--qss-text);
  font-size: 12px; text-align: left; cursor: pointer;
}
.qsc-filter-option:hover, .qsc-filter-option:focus { outline: none; background: var(--qss-bg-hover); }
.qsc-filter-option[aria-selected="true"] { font-weight: 600; }
.qsc-filter-option svg { flex-shrink: 0; }
.qsc-filter-option .is-hidden { visibility: hidden; }
</style>
