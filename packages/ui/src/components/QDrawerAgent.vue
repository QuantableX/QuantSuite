<script setup lang="ts">
/**
 * The drawer's Agent tab (PLAN-V2 E4) — the agent layer's control surface.
 *
 * What is real today: the full tool catalogue every agent will see
 * (`quantsuite.<module>.<name>` from the MCP bridge), the approval mode, and
 * the gate's *actual* decision per tool — queried live from the Rust gate,
 * not re-derived here, so the policy stays inspectable rather than folklore.
 * `external` capabilities are never auto-approved in any mode; the badge
 * column makes that visible.
 *
 * What is still missing is the transport (the hook in QuantMCP's MCP server
 * that lets an external agent actually call these) — the card→agent flow and
 * the live approval queue arrive with it. This tab says so instead of
 * pretending.
 *
 * Status (docs/PLAN-AGENT-STATUS.md) is the tab's first and default view:
 * every agent's progress bars and shell calls, so nobody has to ask how far
 * the work is. It renders the ONE board template (../progress/board.js) that
 * QuantMCP's /progress page renders too, polled once a second while shown.
 * Strict and Relaxed stay the approval-mode switch and show the tool table;
 * the active mode stays marked while Status is shown, and Status never
 * changes the mode.
 */
import { nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { qs, type AgentCapability, type AgentDecision, type PendingAgentCall } from '@quantsuite/core'
import { updateBoard, type ProgressBoard } from '../progress/board.js'
import '../progress/board.css'

const props = defineProps<{
  /** Prompted calls awaiting the user, oldest first. */
  queue: PendingAgentCall[]
}>()
const emit = defineEmits<{
  (e: 'approve', call: PendingAgentCall): void
  (e: 'reject', call: PendingAgentCall): void
}>()

const tools = ref<AgentCapability[]>([])
const decisions = ref<Record<string, AgentDecision>>({})
const mode = ref<'strict' | 'relaxed'>('strict')
const available = ref(false)

async function loadDecisions() {
  // Concurrently: the catalogue is one round-trip per tool, and serially that
  // is the whole tab's open (and every mode toggle) waiting on N of them.
  const entries = await Promise.all(
    tools.value.map(async (t) => [t.tool, await qs.core.agentToolDecision(t.tool, mode.value)] as const)
  )
  decisions.value = Object.fromEntries(entries)
}

async function setMode(m: 'strict' | 'relaxed') {
  mode.value = m
  try {
    await qs.core.setSetting('core', 'agent.approvalMode', m)
  } catch {
    // browser — nothing to persist against
  }
  if (available.value) await loadDecisions()
}

const view = ref<'status' | 'tools'>('status')

function showStatus() {
  view.value = 'status'
}

async function pickMode(m: 'strict' | 'relaxed') {
  view.value = 'tools'
  if (m !== mode.value) await setMode(m)
}

const board = ref<ProgressBoard | null>(null)
const boardRoot = ref<HTMLElement | null>(null)
const boardLive = ref(true)
let boardTimer: ReturnType<typeof setInterval> | undefined
let boardPending = false
let boardEpoch = 0

const BOARD_EMPTY =
  'No agent is reporting work right now.'

async function loadBoard() {
  if (boardPending) return
  if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) {
    boardLive.value = false // plain browser — the board lives in QuantMCP
    return
  }
  boardPending = true
  const epoch = boardEpoch
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    const next = await invoke<ProgressBoard>('plugin:mcp|progress_board')
    if (epoch !== boardEpoch) return
    board.value = next
    boardLive.value = true
    await nextTick()
    if (epoch === boardEpoch && boardRoot.value) updateBoard(boardRoot.value, next, { empty: BOARD_EMPTY })
  } catch {
    if (epoch === boardEpoch) boardLive.value = false
  } finally {
    boardPending = false
  }
}

function startBoard() {
  if (boardTimer) return
  void loadBoard()
  boardTimer = setInterval(loadBoard, 1000)
}

function stopBoard() {
  boardEpoch++
  if (boardTimer) clearInterval(boardTimer)
  boardTimer = undefined
}

watch(view, (v) => (v === 'status' ? startBoard() : stopBoard()))
onMounted(() => {
  if (view.value === 'status') startBoard()
})
onUnmounted(stopBoard)

onMounted(async () => {
  try {
    tools.value = await qs.core.agentTools()
    mode.value = (await qs.core.getSetting<'strict' | 'relaxed'>('core', 'agent.approvalMode')) ?? 'strict'
    available.value = true
    await loadDecisions()
  } catch {
    available.value = false // plain browser — the catalogue lives in Rust
  }
})
</script>

<template>
  <div class="daw">
    <header class="daw-head">
      <div v-if="view === 'status'">
        <h3>Agent status</h3>
        <p>
          Current work, progress and latest updates.
          <template v-if="board?.pageUrl">
            Also in a browser: <span class="daw-url">{{ board.pageUrl }}</span>
          </template>
        </p>
      </div>
      <div v-else>
        <h3>Agent tools</h3>
        <p>
          What an agent may call, and what the gate does about it.
          <strong>external</strong> is never auto-approved, in any mode.
        </p>
      </div>

      <div class="daw-mode" role="group" aria-label="Agent status and approval mode">
        <button :class="{ 'is-on': view === 'status' }" :aria-pressed="view === 'status'" @click="showStatus">Status</button>
        <button
          :class="{ 'is-on': view === 'tools' && mode === 'strict', 'is-mode': view === 'status' && mode === 'strict' }"
          :aria-pressed="view === 'tools' && mode === 'strict'"
          :title="mode === 'strict' ? 'Approval mode: Strict (active)' : 'Switch the approval mode to Strict'"
          @click="pickMode('strict')"
        >
          Strict
        </button>
        <button
          :class="{ 'is-on': view === 'tools' && mode === 'relaxed', 'is-mode': view === 'status' && mode === 'relaxed' }"
          :aria-pressed="view === 'tools' && mode === 'relaxed'"
          :title="mode === 'relaxed' ? 'Approval mode: Relaxed (active)' : 'Switch the approval mode to Relaxed'"
          @click="pickMode('relaxed')"
        >
          Relaxed
        </button>
      </div>
    </header>

    <!-- The approval queue: prompted calls block their agent until answered. -->
    <section v-if="props.queue.length" class="daw-queue">
      <h4>Waiting for your approval</h4>
      <article v-for="call in props.queue" :key="call.callId" class="daw-call">
        <div class="daw-call-main">
          <span class="daw-tool">{{ call.tool }}</span>
          <span v-if="call.reason" class="daw-call-reason">{{ call.reason }}</span>
          <code v-if="Object.keys(call.args ?? {}).length" class="daw-call-args">{{ JSON.stringify(call.args) }}</code>
        </div>
        <div class="daw-call-actions">
          <button class="is-approve" @click="emit('approve', call)">Approve</button>
          <button class="is-reject" @click="emit('reject', call)">Reject</button>
        </div>
      </article>
    </section>

    <template v-if="view === 'status'">
      <p v-if="!boardLive" class="daw-empty">
        {{ board ? 'Connection interrupted. Showing the last update; reconnecting…' : 'Progress is temporarily unavailable. Reconnecting…' }}
      </p>
      <!-- updateBoard escapes values and preserves expanded details across polls. -->
      <div ref="boardRoot" class="daw-board" />
    </template>

    <p v-else-if="!available" class="daw-empty">
      The tool catalogue lives in the Rust gate — open QuantSuite itself to inspect it.
    </p>

    <table v-else class="daw-table">
      <thead>
        <tr><th>Tool</th><th>Effects</th><th>Gate ({{ mode }})</th></tr>
      </thead>
      <tbody>
        <tr v-for="t in tools" :key="t.tool">
          <td>
            <span class="daw-tool">{{ t.tool }}</span>
            <span v-if="t.description" class="daw-desc">{{ t.description }}</span>
          </td>
          <td><span class="daw-badge" :class="`is-${t.sideEffects}`">{{ t.sideEffects }}</span></td>
          <td>
            <span v-if="decisions[t.tool]" class="daw-gate" :class="`is-${decisions[t.tool]!.decision}`">
              {{ decisions[t.tool]!.decision }}
            </span>
          </td>
        </tr>
      </tbody>
    </table>

    <footer v-if="view === 'tools'" class="daw-foot">
      Agents reach these tools through QuantMCP's server; every call passes the gate above, and
      prompted calls wait in this queue. The card→agent flow is the remaining E4 work.
    </footer>
  </div>
</template>

<style scoped>
.daw {
  height: 100%;
  overflow-y: auto;
  padding: 12px 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.daw-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
}
.daw-head h3 {
  margin: 0 0 3px;
  font: 600 12.5px/1 var(--qss-font-sans);
  color: var(--qss-text);
}
.daw-head p {
  margin: 0;
  color: var(--qss-text-muted);
  font-size: 11.5px;
}

.daw-mode {
  display: flex;
  flex-shrink: 0;
  border: 1px solid var(--qss-border);
  border-radius: 7px;
  overflow: hidden;
}
.daw-mode button {
  border: none;
  background: none;
  padding: 5px 11px;
  color: var(--qss-text-muted);
  font: 500 11px/1 var(--qss-font-sans);
  cursor: pointer;
}
.daw-mode button.is-on {
  background: var(--qss-bg-card);
  color: var(--qss-text);
}
/* The active approval mode while Status is shown: a dot, not a second
   selection. */
.daw-mode button.is-mode {
  color: var(--qss-text-secondary);
}
.daw-mode button.is-mode::before {
  content: '';
  display: inline-block;
  width: 5px;
  height: 5px;
  margin: 0 5px 1px 0;
  border-radius: 50%;
  background: var(--qss-text-secondary);
  vertical-align: middle;
}

.daw-url {
  color: var(--qss-text-secondary);
  user-select: all;
}

.daw-empty {
  margin: 24px auto;
  color: var(--qss-text-muted);
  font-size: 12px;
}

.daw-queue {
  border: 1px solid color-mix(in srgb, var(--qss-warning) 45%, var(--qss-border));
  border-radius: 9px;
  background: color-mix(in srgb, var(--qss-warning) 6%, transparent);
  padding: 10px 12px;
}
.daw-queue h4 {
  margin: 0 0 8px;
  font: 600 11px/1 var(--qss-font-sans);
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--qss-warning);
}
.daw-call {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 7px 0;
}
.daw-call + .daw-call {
  border-top: 1px solid color-mix(in srgb, var(--qss-border-subtle) 60%, transparent);
}
.daw-call-main {
  flex: 1;
  min-width: 0;
}
.daw-call-reason {
  display: block;
  margin-top: 2px;
  color: var(--qss-text-secondary);
  font-size: 11px;
}
.daw-call-args {
  display: block;
  margin-top: 3px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--qss-text-muted);
  font: 400 10.5px/1.4 var(--qss-font-mono);
}
.daw-call-actions {
  display: flex;
  gap: 6px;
  flex-shrink: 0;
}
.daw-call-actions button {
  padding: 5px 12px;
  border-radius: 7px;
  border: 1px solid var(--qss-border);
  background: none;
  font: 500 11.5px/1 var(--qss-font-sans);
  cursor: pointer;
}
.daw-call-actions .is-approve {
  color: var(--qss-success);
  border-color: color-mix(in srgb, var(--qss-success) 50%, var(--qss-border));
}
.daw-call-actions .is-approve:hover {
  background: color-mix(in srgb, var(--qss-success) 14%, transparent);
}
.daw-call-actions .is-reject {
  color: var(--qss-text-muted);
}
.daw-call-actions .is-reject:hover {
  color: var(--qss-error);
  border-color: color-mix(in srgb, var(--qss-error) 50%, var(--qss-border));
}

.daw-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 11.5px;
}
.daw-table th {
  text-align: left;
  padding: 4px 10px 4px 0;
  color: var(--qss-text-muted);
  font: 600 10px/1 var(--qss-font-sans);
  letter-spacing: 0.05em;
  text-transform: uppercase;
  border-bottom: 1px solid var(--qss-border-subtle);
}
.daw-table td {
  padding: 6px 10px 6px 0;
  border-bottom: 1px solid color-mix(in srgb, var(--qss-border-subtle) 45%, transparent);
  vertical-align: top;
}

.daw-tool {
  display: block;
  font: 500 11.5px/1.3 var(--qss-font-mono);
  color: var(--qss-text);
}
.daw-desc {
  display: block;
  margin-top: 2px;
  color: var(--qss-text-muted);
  font-size: 11px;
}

.daw-badge,
.daw-gate {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 999px;
  border: 1px solid var(--qss-border);
  font: 500 10px/1.4 var(--qss-font-mono);
  color: var(--qss-text-secondary);
}
.daw-badge.is-external {
  color: var(--qss-error);
  border-color: color-mix(in srgb, var(--qss-error) 45%, var(--qss-border));
}
.daw-badge.is-write {
  color: var(--qss-warning);
  border-color: color-mix(in srgb, var(--qss-warning) 45%, var(--qss-border));
}
.daw-gate.is-allow {
  color: var(--qss-success);
  border-color: color-mix(in srgb, var(--qss-success) 45%, var(--qss-border));
}
.daw-gate.is-prompt {
  color: var(--qss-warning);
  border-color: color-mix(in srgb, var(--qss-warning) 45%, var(--qss-border));
}
.daw-gate.is-deny {
  color: var(--qss-error);
  border-color: color-mix(in srgb, var(--qss-error) 45%, var(--qss-border));
}

.daw-foot {
  margin-top: auto;
  color: var(--qss-text-muted);
  font-size: 10.5px;
}
</style>
