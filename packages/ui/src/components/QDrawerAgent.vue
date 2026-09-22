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
 */
import { onMounted, ref } from 'vue'
import { qs, type AgentCapability, type AgentDecision, type PendingAgentCall } from '@quantsuite/core'

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
      <div>
        <h3>Agent tools</h3>
        <p>
          What an agent may call, and what the gate does about it.
          <strong>external</strong> is never auto-approved, in any mode.
        </p>
      </div>

      <div class="daw-mode" role="radiogroup" aria-label="Approval mode">
        <button :class="{ 'is-on': mode === 'strict' }" @click="setMode('strict')">Strict</button>
        <button :class="{ 'is-on': mode === 'relaxed' }" @click="setMode('relaxed')">Relaxed</button>
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

    <p v-if="!available" class="daw-empty">
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

    <footer class="daw-foot">
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
  max-width: 60ch;
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
