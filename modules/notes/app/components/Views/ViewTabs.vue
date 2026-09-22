<script setup lang="ts">
/**
 * The view switcher — one tab per saved view, plus "add view".
 *
 * Tabs, not a dropdown: switching between how you look at the same notes is
 * the module's main gesture, and it should cost one click.
 */
import { useSchemaStore } from '#notes/stores/schema'
import type { ViewKind } from '#notes/types'

const schema = useSchemaStore()

const adding = ref(false)
const root = ref<HTMLElement | null>(null)

/** 24×24 stroke paths, per PLAN-V3 §3 — no emoji, no glyph characters. */
const KIND_ICONS: Record<ViewKind, string> = {
  table: 'M3 5h18v14H3z M3 10h18 M3 15h18 M9 5v14',
  board: 'M4 4h4v16H4z M10 4h4v11h-4z M16 4h4v7h-4z',
  list: 'M4 6h16 M4 12h16 M4 18h11',
  gallery: 'M4 4h7v7H4z M13 4h7v7h-7z M4 13h7v7H4z M13 13h7v7h-7z',
  calendar: 'M4 6h16v14H4z M4 10h16 M8 3v4 M16 3v4',
}

const KIND_LABELS: Array<{ kind: ViewKind; label: string }> = [
  { kind: 'table', label: 'Table' },
  { kind: 'board', label: 'Board' },
  { kind: 'list', label: 'List' },
  { kind: 'gallery', label: 'Gallery' },
  { kind: 'calendar', label: 'Calendar' },
]

async function addView(kind: ViewKind) {
  adding.value = false
  const label = KIND_LABELS.find((k) => k.kind === kind)?.label ?? 'View'
  // A new board needs something to group by or it renders one column; the
  // first select property is the only sensible guess.
  const groupBy = schema.properties.find((p) => p.kind === 'select')?.id
  const dateBy = schema.properties.find((p) => p.kind === 'date')?.id
  const config =
    kind === 'board' ? { groupBy } : kind === 'calendar' ? { dateBy } : {}
  await schema.createView(label, kind, config)
}

function onPointerDown(event: PointerEvent) {
  if (!root.value?.contains(event.target as Node)) adding.value = false
}

watch(adding, (open) => {
  if (open) document.addEventListener('pointerdown', onPointerDown)
  else document.removeEventListener('pointerdown', onPointerDown)
})

onBeforeUnmount(() => document.removeEventListener('pointerdown', onPointerDown))
</script>

<template>
  <div ref="root" class="qn-tabs">
    <button
      v-for="view in schema.views"
      :key="view.id"
      class="qn-tabs__tab"
      :class="{ 'is-active': view.id === schema.activeView?.id }"
      @click="schema.setActiveView(view.id)"
    >
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path :d="KIND_ICONS[view.kind]" />
      </svg>
      <span>{{ view.name }}</span>
    </button>

    <div class="qn-tabs__add-wrap">
      <button class="qn-tabs__add" aria-label="Add view" @click="adding = !adding">+</button>
      <div v-if="adding" class="qn-tabs__menu">
        <button v-for="entry in KIND_LABELS" :key="entry.kind" class="qn-tabs__menu-item" @click="addView(entry.kind)">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path :d="KIND_ICONS[entry.kind]" />
          </svg>
          <span>{{ entry.label }}</span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qn-tabs {
  display: flex;
  align-items: center;
  gap: 2px;
  min-width: 0;
  overflow-x: auto;
  scrollbar-width: none;
}

.qn-tabs::-webkit-scrollbar {
  display: none;
}

.qn-tabs__tab {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  padding: 5px 10px;
  border: none;
  border-bottom: 2px solid transparent;
  border-radius: 6px 6px 0 0;
  background: transparent;
  color: var(--qn-text-muted);
  font-size: 13px;
  cursor: pointer;
  transition: color var(--qss-dur-fast, 120ms) ease, border-color var(--qss-dur-fast, 120ms) ease;
}

.qn-tabs__tab:hover {
  color: var(--qn-text-secondary);
  background: var(--qn-bg-hover);
}

.qn-tabs__tab.is-active {
  color: var(--qn-text);
  border-bottom-color: var(--qn-accent);
}

.qn-tabs__add-wrap {
  position: relative;
  flex-shrink: 0;
}

.qn-tabs__add {
  width: 24px;
  height: 24px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qn-text-muted);
  font-size: 15px;
  line-height: 1;
  cursor: pointer;
}

.qn-tabs__add:hover {
  background: var(--qn-bg-hover);
  color: var(--qn-text);
}

.qn-tabs__menu {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  z-index: 40;
  width: 160px;
  padding: 4px;
  border: 1px solid var(--qn-border);
  border-radius: 10px;
  background: var(--qn-bg-sidebar);
  box-shadow: 0 16px 36px rgba(0, 0, 0, 0.35);
}

.qn-tabs__menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 8px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qn-text-secondary);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}

.qn-tabs__menu-item:hover {
  background: var(--qn-bg-hover);
  color: var(--qn-text);
}
</style>
