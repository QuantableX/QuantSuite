<script setup lang="ts">
/**
 * Header center: Conversation or Board for the main area (card 21), then
 * the open session's title (click to rename), its context,
 * the agent running in its terminal (or "terminal"), the model it reported,
 * the permission mode the wrappers pass on, its pull requests, the git
 * actions (card 12), and End while the terminal lives. A thread started
 * from a kanban card names the card; a click shows it on the board. The
 * chrome (logo cap, selector, settings) is QModuleHeader's.
 */
import { computed, nextTick, ref } from 'vue'
import { usePilotStore } from '#pilot/stores/pilot'
import { useGitStore } from '#pilot/stores/git'
import { useDrawerStore } from '#pilot/stores/drawer'
import { SquareKanban, SquareTerminal } from 'lucide-vue-next'
import { modeLabel } from '#pilot/utils/format'
import { useKeysStore } from '#pilot/stores/keys'
import { useAppStore } from '#pilot/stores/app'
import { cardIdOf } from '#pilot/utils/board'
import type { ThreadLink } from '#pilot/types'

const app = useAppStore()
const store = usePilotStore()
const git = useGitStore()
const drawer = useDrawerStore()
const keys = useKeysStore()
const prs = computed(() => (session.value ? (git.links[session.value.id] ?? []).filter((l) => l.kind === 'pr') : []))
const session = computed(() => store.active)
const live = computed(() => store.activeLive)
const cards = computed(() => (session.value ? (git.links[session.value.id] ?? []).filter((l) => l.kind === 'card') : []))

/** The card on its board, with this thread open beside it. */
function showCard(link: ThreadLink) {
  const s = session.value
  if (!s) return
  app.setBoard(s.contextId)
  app.boardCard = cardIdOf(link)
  app.boardPanel = s.id
  app.setView('board')
}

const editing = ref(false)
const draft = ref('')
const titleInput = ref<HTMLInputElement | null>(null)

function startEdit() {
  if (!session.value) return
  draft.value = session.value.title
  editing.value = true
  void nextTick(() => titleInput.value?.select())
}
async function commitEdit() {
  editing.value = false
  const s = session.value
  if (!s) return
  const title = draft.value.trim()
  if (title && title !== s.title) await store.update(s.id, { title })
}

const model = computed(() => live.value.vitals.model || session.value?.model || '')

async function end() {
  if (!session.value) return
  if (!confirm('End this terminal? The thread stays and its agent resumes on the next message.')) return
  await store.stop(session.value.id)
}
</script>

<template>
  <QModuleHeader module-id="pilot">
    <div class="qp-header-center">
      <div class="qp-view-switch" role="group" aria-label="Main area">
        <button :class="{ on: app.view === 'conversation' }" :aria-pressed="app.view === 'conversation'" @click="app.setView('conversation')">Conversation</button>
        <button :class="{ on: app.view === 'board' }" :aria-pressed="app.view === 'board'" title="The kanban board of a workspace or General" @click="app.setView('board')">Board</button>
      </div>
      <template v-if="app.view === 'board'" />
      <template v-else-if="session">
        <form v-if="editing" class="qp-title-form" @submit.prevent="commitEdit">
          <input ref="titleInput" v-model="draft" class="qp-title-input" @blur="commitEdit" @keydown.esc="editing = false" />
        </form>
        <button v-else class="qp-title" :title="session.title || 'Untitled — click to rename'" @click="startEdit">
          {{ session.title || 'New thread' }}
        </button>
        <span class="qp-head-chip" :title="session.cwd">{{ session.contextName }}</span>
        <span class="qp-head-chip is-provider" :title="live.display || undefined">{{ store.adapterLabel(session.provider) }}</span>
        <span v-if="model" class="qp-head-chip" :title="`Model: ${model}`">{{ model }}</span>
        <span class="qp-head-chip" title="Permission mode">{{ modeLabel(session.mode) }}</span>
        <PilotGitPrChip v-for="l in prs" :key="l.url" :link="l" />
        <button v-for="l in cards" :key="l.url" class="qp-head-chip is-card" :title="`Kanban card: ${l.title} — show it on the board`" @click="showCard(l)">
          <SquareKanban :size="11" />
          <span>{{ l.title || 'Kanban card' }}</span>
        </button>
        <div class="qp-header-spacer" />
        <PilotDrawerActions :thread-id="session.id" />
        <button class="qp-head-icon" :class="{ on: drawer.open[session.id] }" :title="keys.hint('terminal.toggle', 'Terminal drawer')" aria-label="Terminal drawer" @click="drawer.toggle(session.id)">
          <SquareTerminal :size="14" />
        </button>
        <PilotGitActions :thread-id="session.id" />
        <button
          v-if="!store.terminalShown[session.id]"
          class="qp-btn qp-btn-sm"
          title="Open this thread's terminal (the session's shell, with the agent CLIs as commands)"
          @click="store.openTerminal(session.id)"
        >
          Terminal
        </button>
        <button v-if="live.alive" class="qp-btn qp-btn-sm qp-btn-danger" title="End the terminal" @click="end">End terminal</button>
      </template>
      <span v-else class="qp-head-hint">QuantPilot</span>
    </div>
  </QModuleHeader>
</template>

<style scoped>
.qp-header-center {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  flex: 1;
  /* Both caps' borders sit right against this box: the title and End get
     the same breathing room the logo cap gives the brand. */
  padding: 0 14px;
}
.qp-title {
  background: transparent;
  border: none;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  padding: 2px 6px;
  border-radius: 5px;
  max-width: 320px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  cursor: text;
}
.qp-title:hover { background: var(--qss-bg-hover, #313139); }
.qp-title-form { display: flex; }
.qp-title-input {
  background: var(--qss-bg, #18181e);
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 5px;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 13px;
  font-weight: 600;
  padding: 2px 6px;
  width: 280px;
  outline: none;
}
.qp-head-chip {
  font-size: 11px;
  color: var(--qss-text-secondary, #9a9aa5);
  border: 1px solid var(--qss-border-subtle, #35353d);
  border-radius: 999px;
  padding: 1px 8px;
  max-width: 200px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.qp-head-chip.is-provider { color: var(--qss-text, #d4d4d8); }
.qp-head-chip.is-card {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  background: transparent;
  font-family: inherit;
  cursor: pointer;
}
.qp-head-chip.is-card span { overflow: hidden; text-overflow: ellipsis; }
.qp-head-chip.is-card:hover { color: var(--qss-text, #d4d4d8); border-color: var(--qss-border, #47474f); }
.qp-view-switch {
  flex-shrink: 0;
  display: inline-flex;
  padding: 2px;
  gap: 2px;
  border: 1px solid var(--qss-border-subtle, #35353d);
  border-radius: 7px;
  background: var(--qss-bg, #18181e);
}
.qp-view-switch button {
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text-muted, #6e6e7a);
  font: inherit;
  font-size: 11.5px;
  padding: 2px 9px;
  cursor: pointer;
}
.qp-view-switch button:hover { color: var(--qss-text, #d4d4d8); }
.qp-view-switch button.on { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-header-spacer { flex: 1; }
.qp-head-icon { display: inline-flex; padding: 4px; border: 1px solid transparent; border-radius: 6px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-head-icon:hover,
.qp-head-icon.on { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-head-hint { font-size: 12px; color: var(--qss-text-muted, #6e6e7a); }
</style>
