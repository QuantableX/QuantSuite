<script setup lang="ts">
/**
 * QuantNotes' header — the shared QModuleHeader (V3) with a breadcrumb and the
 * search trigger in the center slot.
 *
 * The workspace switcher and the tool chip that used to sit here are gone with
 * the single-collection rewrite: there is one workspace, and "which tool" was
 * replaced by the view tabs above the collection.
 */
import { useAppStore } from '#notes/stores/app'
import { useNotesStore } from '#notes/stores/notes'

const app = useAppStore()
const notes = useNotesStore()
const route = useRoute()

/** `All notes` on the collection, `All notes / <title>` on an open note. */
const crumb = computed(() => {
  if (route.path === '/notes/trash') return 'Trash'
  if (route.path === '/notes/suite') return 'Suite notes'
  if (route.path === '/notes/settings') return 'Settings'
  const open = notes.openNote
  return open ? open.title || 'Untitled' : 'All notes'
})

const nested = computed(() => !!notes.openNote && route.path.startsWith('/notes/n/'))
</script>

<template>
  <QModuleHeader module-id="notes">
    <div class="qn-hd">
      <nav class="qn-hd__crumbs">
        <NuxtLink v-if="nested" to="/notes" class="qn-hd__crumb">All notes</NuxtLink>
        <span v-if="nested" class="qn-hd__sep">/</span>
        <span class="qn-hd__crumb qn-hd__crumb--current">{{ crumb }}</span>
      </nav>

      <button class="qn-hd__search" @click="app.commandPaletteOpen = true">
        <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="11" cy="11" r="7" /><path d="m20 20-3.5-3.5" />
        </svg>
        <span>Search</span>
        <span class="qn-hd__kbd">Ctrl+K</span>
      </button>
    </div>
  </QModuleHeader>
</template>

<style scoped>
.qn-hd {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 0 20px;
}

.qn-hd__crumbs {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 7px;
}

.qn-hd__crumb {
  color: var(--qn-text-muted);
  font-size: 12px;
  text-decoration: none;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qn-hd__crumb:hover {
  color: var(--qn-text-secondary);
}

.qn-hd__crumb--current {
  color: var(--qn-text-secondary);
}

.qn-hd__sep {
  color: var(--qn-text-muted);
  font-size: 12px;
}

.qn-hd__search {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
  padding: 5px 10px;
  border: 1px solid var(--qn-border);
  border-radius: 7px;
  background: var(--qn-bg-card);
  color: var(--qn-text-muted);
  font-size: 12px;
  cursor: pointer;
  transition: border-color 150ms ease, color 150ms ease;
}

.qn-hd__search:hover {
  border-color: var(--qn-accent);
  color: var(--qn-text);
}

.qn-hd__kbd {
  padding: 1px 5px;
  border: 1px solid var(--qn-border);
  border-radius: 4px;
  font-family: var(--qss-font-mono, monospace);
  font-size: 10px;
}
</style>
