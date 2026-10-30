<script setup lang="ts">
/**
 * The board view (card 21): the kanban board of a workspace or General in
 * the main area. It is the suite's one board (QDrawerKanban — the columns,
 * pointer drag and drop under the board's approval mode, the
 * QKanbanCardModal editor, refreshes on kanban events only) with what
 * QuantPilot adds: each card's threads, Start thread, and a thread open
 * beside the board. Start thread creates the thread in the card's workspace
 * with its defaults, prefills its first message with the card and links
 * the two (pilot.db `links`, kind `card`).
 */
import { computed, onMounted, ref } from 'vue'
import type { KanbanCard } from '@quantsuite/core'
import { usePilotStore } from '#pilot/stores/pilot'
import { useAppStore } from '#pilot/stores/app'
import { useGitStore } from '#pilot/stores/git'
import { cardPrompt, GENERAL_BOARD, threadsByCard } from '#pilot/utils/board'
import { pilot } from '#pilot/utils/invoke'

const store = usePilotStore()
const app = useAppStore()
const git = useGitStore()

const boards = computed(() => (store.contexts.length ? store.contexts : [{ id: GENERAL_BOARD, name: 'General' }]))
/** The remembered board, else the open thread's workspace; a forgotten workspace falls back to General. */
const boardId = computed(() => {
  const id = app.boardId || store.active?.contextId || GENERAL_BOARD
  return !store.contexts.length || store.contexts.some((c) => c.id === id) ? id : GENERAL_BOARD
})
const boardLabel = computed(() => boards.value.find((b) => b.id === boardId.value)?.name ?? 'General')
const byCard = computed(() => threadsByCard(git.links))
const panelId = computed(() => app.boardPanel)

function onBoard(e: Event) {
  app.setBoard((e.target as HTMLSelectElement).value)
  app.boardPanel = null
}

function openPanel(threadId: string) {
  app.boardPanel = threadId
  if (store.activeId !== threadId) void store.openSession(threadId)
}

/** A click selects the card; one with threads opens its latest beside the board. */
function onSelect(card: KanbanCard) {
  app.boardCard = card.id
  const ids = byCard.value[card.id]
  if (ids?.length) openPanel(ids[ids.length - 1]!)
}

// The composer's drafts (Composer.vue keeps them under the same key).
const drafts = useState<Record<string, string>>('pilot-drafts', () => {
  try {
    return JSON.parse(localStorage.getItem('pilot.drafts.v1') ?? '{}') as Record<string, string>
  } catch {
    return {}
  }
})

const starting = ref<string | null>(null)
const error = ref('')
async function start(card: KanbanCard) {
  if (starting.value) return
  starting.value = card.id
  error.value = ''
  try {
    const meta = await store.createThread({ contextId: card.workspace_id, title: card.title })
    if (!meta) {
      error.value = store.lastError || 'The thread could not be created.'
      return
    }
    drafts.value[meta.id] = cardPrompt(card, boards.value.find((b) => b.id === card.workspace_id)?.name ?? boardLabel.value)
    try { localStorage.setItem('pilot.drafts.v1', JSON.stringify(drafts.value)) } catch { /* the draft lives for this session */ }
    await pilot('plugin:pilot|pilot_card_link', { id: meta.id, cardId: card.id, title: card.title })
    await git.loadLinks()
    app.boardCard = card.id
    app.boardPanel = meta.id
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    starting.value = null
  }
}

// The thread beside the board is the open one: its composer's keys and links follow it.
onMounted(() => {
  if (app.boardPanel && store.sessions.some((s) => s.id === app.boardPanel)) openPanel(app.boardPanel)
})
</script>

<template>
  <div class="qp-board">
    <div class="qp-board-main">
      <p v-if="error" class="qp-board-error" role="alert">
        <span>{{ error }}</span>
        <button class="qp-btn qp-btn-sm" @click="error = ''">Dismiss</button>
      </p>
      <QDrawerKanban class="qp-board-kanban" :board-id="boardId" :selected-card-id="app.boardCard" @select="onSelect">
        <template #board>
          <select class="qp-board-select" :value="boardId" aria-label="Board" @change="onBoard">
            <option v-for="b in boards" :key="b.id" :value="b.id">{{ b.name }}</option>
          </select>
        </template>
        <template #card-extra="{ card }">
          <PilotBoardCardExtra
            :card="card"
            :thread-ids="byCard[card.id] ?? []"
            :open-id="panelId"
            :starting="starting === card.id"
            @open="(id: string) => { app.boardCard = card.id; openPanel(id) }"
            @start="start(card)"
          />
        </template>
      </QDrawerKanban>
    </div>
    <PilotBoardPanel v-if="panelId" :key="panelId" :thread-id="panelId" @close="app.boardPanel = null" />
  </div>
</template>

<style scoped>
.qp-board { flex: 1; min-height: 0; display: flex; min-width: 0; }
.qp-board-main { flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; }
.qp-board-kanban { flex: 1; min-height: 0; height: auto; }
.qp-board-error {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 8px 14px 0;
  padding: 4px 6px 4px 10px;
  border: 1px solid color-mix(in srgb, var(--qss-error, #f87171) 40%, transparent);
  border-radius: 7px;
  background: color-mix(in srgb, var(--qss-error, #f87171) 9%, transparent);
  color: var(--qss-text, #d4d4d8);
  font-size: 11.5px;
}
.qp-board-error span { flex: 1; min-width: 0; }
.qp-board-select {
  min-width: 0;
  width: 100%;
  padding: 4px 9px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 7px;
  background: var(--qss-bg-raised, #1f1f25);
  color: var(--qss-text, #d4d4d8);
  font: 500 11.5px/1.5 var(--qss-font-sans, system-ui, sans-serif);
  cursor: pointer;
}
.qp-board-select:focus { outline: none; border-color: color-mix(in srgb, var(--qss-accent, #7c8cff) 50%, var(--qss-border, #47474f)); }
</style>
