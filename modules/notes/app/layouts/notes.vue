<script setup lang="ts">
/**
 * QuantNotes module shell.
 *
 * Loads the collection and its schema once and holds them for the module's
 * lifetime — every view renders from the same in-memory list, so switching
 * between table, board and calendar costs no IPC.
 *
 * Ctrl+K opens QuantNotes' own palette. It calls preventDefault(), which is
 * the signal the shell uses to stand down — see apps/shell/app/app.vue.
 */
import { useAppStore } from '#notes/stores/app'
import { useNotesStore } from '#notes/stores/notes'
import { useSchemaStore } from '#notes/stores/schema'
import { useShortcuts, useTauriEvent } from '@quantsuite/core'

const app = useAppStore()
const notes = useNotesStore()
const schema = useSchemaStore()
const router = useRouter()

onMounted(async () => {
  await app.loadSettings()
  await Promise.all([notes.loadList(), schema.load()])
})

// Another window (or the agent broker) can write a note; the module's own
// writes already patch the store, so a reload here is only for the others.
useTauriEvent('note:created', () => void notes.loadList())
useTauriEvent('note:deleted', () => void notes.loadList())
useTauriEvent('schema:changed', () => void schema.load())

useShortcuts([
  // An unspecified modifier is a wildcard and the first match wins, so the
  // Ctrl+Shift+B binding has to come before the plain Ctrl+B one.
  {
    key: 'B',
    ctrl: true,
    shift: true,
    handler: (e) => {
      e.preventDefault()
      app.toggleSidebar('right')
    },
  },
  {
    key: 'b',
    ctrl: true,
    handler: (e) => {
      e.preventDefault()
      app.toggleSidebar('left')
    },
  },
  {
    key: '\\',
    ctrl: true,
    handler: (e) => {
      e.preventDefault()
      app.enterFocusMode()
    },
  },
  {
    key: 'k',
    ctrl: true,
    handler: (e) => {
      e.preventDefault()
      app.commandPaletteOpen = true
    },
  },
  {
    key: 'p',
    ctrl: true,
    handler: (e) => {
      e.preventDefault()
      app.commandPaletteOpen = true
    },
  },
  {
    key: 'n',
    ctrl: true,
    handler: async (e) => {
      e.preventDefault()
      const note = await notes.create({})
      await router.push(`/notes/n/${note.id}`)
    },
  },
  {
    key: ',',
    ctrl: true,
    handler: (e) => {
      e.preventDefault()
      router.push('/notes/settings')
    },
  },
])
</script>

<template>
  <div class="qn-shell">
    <NotesLayoutAppHeader />
    <div class="qn-body">
      <!-- V3: shared sidebar shells — 220px, in-flow, collapse via store. -->
      <QSidebar :model-value="app.sidebarLeftOpen && !app.focusMode" resizable storage-key="notes.left">
        <NotesLayoutLeftSidebar />
      </QSidebar>
      <main class="qn-main">
        <slot />
      </main>
      <QRightPanel :model-value="app.sidebarRightOpen && !app.focusMode" storage-key="notes.right">
        <NotesLayoutRightSidebar />
      </QRightPanel>
    </div>
    <NotesLayoutCommandPalette />
  </div>
</template>

<style scoped>
.qn-shell {
  display: flex;
  flex-direction: column;
  /* The module fills its container, never the viewport (ARCHITECTURE.md §9). */
  width: 100%;
  height: 100%;
  overflow: hidden;
}

.qn-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.qn-main {
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  background: var(--qn-bg);
}
</style>
