<script setup lang="ts">
/**
 * A kanban card on the canvas (round 2, card 09): dragged out of a Kanban
 * window, or kept from an earlier session. Shows the card as the board
 * has it now (column, priority, the agent holding it) and opens its thread
 * — the linked one, or a new one with the card's prompt in its box — and
 * that thread's diff, each as a connected window.
 */
import { computed, ref } from 'vue'
import type { CanvasWindow } from '../../../shared/types'
import { useCardsStore } from '../../../stores/cards'
import { useCanvasStore } from '../../../stores/canvas'
import { usePilotStore } from '#pilot/stores/pilot'
import { useCanvasActions } from '../../utils/canvasLinks'
import { cardState } from '../../utils/edges'

const props = defineProps<{ window: CanvasWindow }>()
const cards = useCardsStore()
const canvas = useCanvasStore()
const pilotStore = usePilotStore()
const actions = useCanvasActions()

const cfg = computed(() => props.window.cardConfig ?? null)
const card = computed(() => (cfg.value ? cards.card(cfg.value.boardId, cfg.value.cardId) : null))
const state = computed(() => cardState(card.value))
const COLUMN: Record<string, string> = { plan: 'Plan', work: 'Work', review: 'Review', done: 'Done' }
const busy = ref(false)
const error = ref('')

/** The chat window this card is connected to, if any. */
const chat = computed(() => {
  const st = canvas.activeCanvasState
  const ids = new Set((st?.edges ?? []).filter((e) => e.from === props.window.id || e.to === props.window.id).flatMap((e) => [e.from, e.to]))
  return st?.windows.find((w) => w.type === 'chat' && ids.has(w.id)) ?? null
})
const thread = computed(() => (chat.value?.chatConfig?.threadId ? pilotStore.sessions.find((s) => s.id === chat.value!.chatConfig!.threadId) ?? null : null))

async function openThread() {
  busy.value = true
  error.value = ''
  try {
    await actions.cardThread(props.window)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}
function openDiff() {
  if (chat.value) actions.threadDiff(chat.value)
}
</script>

<template>
  <div class="cv-card" :class="`s-${state}`">
    <div class="cv-card-title" :title="card?.title || cfg?.title">{{ card?.title || cfg?.title }}</div>
    <div class="cv-card-meta">
      <span class="cv-card-col">{{ card ? COLUMN[card.column] ?? card.column : 'Card' }}</span>
      <span v-if="card?.priority" class="cv-card-prio">{{ card.priority }}</span>
      <span v-if="card?.agent_id" class="cv-card-agent">agent · {{ card.agent_id }}</span>
      <span v-else-if="thread" class="cv-card-agent">thread · {{ thread.title || 'new' }}</span>
    </div>
    <div class="cv-card-actions">
      <button :disabled="busy" :title="chat ? 'Show its thread' : 'Open its thread — the linked one, or a new one with the card in its box'" @click.stop="openThread">{{ busy ? 'Opening…' : chat ? 'Thread' : 'Open thread' }}</button>
      <button v-if="chat" title="The thread's changes, connected" @click.stop="openDiff">Diff</button>
    </div>
    <p v-if="error" class="cv-card-err">{{ error }}</p>
  </div>
</template>

<style scoped>
.cv-card { height: 100%; display: flex; flex-direction: column; gap: 6px; padding: 10px 12px; border-left: 3px solid #8a8a95; color: var(--qc-text, #d4d4d8); }
.cv-card.s-working { border-left-color: #7aa2f7; }
.cv-card.s-waiting { border-left-color: #d29a3f; }
.cv-card.s-done { border-left-color: #34d399; }
.cv-card.s-error { border-left-color: #f87171; }
.cv-card-title { font-size: 13px; font-weight: 600; line-height: 1.35; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; }
.cv-card-meta { display: flex; flex-wrap: wrap; gap: 6px; font-size: 11px; color: var(--qc-text-muted, #8a8a95); }
.cv-card-col { color: var(--qc-text, #d4d4d8); }
.cv-card-actions { margin-top: auto; display: flex; gap: 6px; }
.cv-card-actions button { padding: 3px 10px; border: 1px solid var(--qc-border, #3f3f46); border-radius: 6px; background: var(--qc-bg-surface, #232329); color: var(--qc-text, #d4d4d8); font-size: 11.5px; cursor: pointer; }
.cv-card-actions button:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); }
.cv-card-actions button:disabled { opacity: 0.5; cursor: default; }
.cv-card-err { margin: 0; font-size: 11px; color: #f87171; }
</style>
