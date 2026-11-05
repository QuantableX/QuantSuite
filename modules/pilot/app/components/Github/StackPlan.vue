<script setup lang="ts">
/**
 * Updating a chain (round 4, card 6): each step shown first, with the exact
 * command, and nothing happens until it is confirmed. Afterwards each step says
 * how it went. A conflict changes nothing, and says which files.
 */
import { onMounted, ref } from 'vue'
import { useGithubStore } from '#pilot/stores/github'
import type { StackOutcome, StackPlan } from '#pilot/types'

const props = defineProps<{ contextId: string; root: number }>()
const emit = defineEmits<{ close: [] }>()

const github = useGithubStore()
const plan = ref<StackPlan | null>(null)
const outcome = ref<StackOutcome | null>(null)
const error = ref('')
const busy = ref(false)
const working = ref(false)
const MARK = { done: 'Done', failed: 'Failed', skipped: 'Not run', dropped: 'Not kept' } as const

async function look() {
  busy.value = true
  error.value = ''
  plan.value = null
  outcome.value = null
  try {
    plan.value = (await github.stackPlan(props.contextId, props.root)) ?? null
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}
onMounted(look)

async function run() {
  if (!plan.value || working.value) return
  working.value = true
  error.value = ''
  try {
    outcome.value = (await github.stackUpdate(props.contextId, props.root, plan.value.id)) ?? null
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    working.value = false
  }
}
</script>

<template>
  <div class="qp-sp" role="group" aria-label="Update this chain">
    <p v-if="busy" class="qp-sp-dim">Fetching and working out the steps…</p>
    <p v-if="error" class="qp-sp-bad" role="alert">{{ error }}</p>

    <!-- Afterwards -->
    <template v-if="outcome">
      <p :class="outcome.ok ? 'qp-sp-good' : 'qp-sp-bad'" role="status">{{ outcome.message }}</p>
      <ol class="qp-sp-steps">
        <li v-for="(s, i) in outcome.steps" :key="i" class="qp-sp-step" :data-status="s.status">
          <span class="qp-sp-mark" :class="s.status">{{ MARK[s.status] }}</span>
          <span class="qp-sp-text">{{ s.text }}</span>
          <span v-if="s.detail" class="qp-sp-detail">{{ s.detail }}</span>
        </li>
      </ol>
      <p v-for="n in outcome.notes" :key="n" class="qp-sp-dim">{{ n }}</p>
      <div class="qp-sp-actions">
        <button class="qp-btn qp-btn-sm" @click="emit('close')">Close</button>
      </div>
    </template>

    <!-- Before -->
    <template v-else-if="plan">
      <p v-if="plan.blocked" class="qp-sp-bad" role="alert">{{ plan.blocked }}</p>
      <p v-else-if="!plan.steps.length" class="qp-sp-good">Already up to date: nothing to do.</p>
      <template v-else>
        <p class="qp-sp-dim">These steps run in a separate place, so your checkout and branches stay as they are. A conflict stops it with nothing changed.</p>
        <ol class="qp-sp-steps">
          <li v-for="(s, i) in plan.steps" :key="i" class="qp-sp-step" :data-kind="s.kind">
            <span class="qp-sp-text">{{ s.text }}</span>
            <code class="qp-sp-code">{{ s.command }}</code>
          </li>
        </ol>
      </template>
      <div class="qp-sp-actions">
        <button class="qp-btn qp-btn-sm" :disabled="working" @click="emit('close')">{{ plan.steps.length && !plan.blocked ? 'Cancel' : 'Close' }}</button>
        <button v-if="plan.steps.length && !plan.blocked" class="qp-btn qp-btn-sm qp-btn-primary" :disabled="working" @click="run">
          {{ working ? 'Updating…' : `Run ${plan.steps.length} step${plan.steps.length === 1 ? '' : 's'}` }}
        </button>
      </div>
    </template>

    <div v-else-if="error" class="qp-sp-actions">
      <button class="qp-btn qp-btn-sm" @click="emit('close')">Close</button>
      <button class="qp-btn qp-btn-sm" @click="look">Try again</button>
    </div>
  </div>
</template>

<style scoped>
.qp-sp {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 2px 0 6px 10px;
  padding: 10px 12px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 8px;
  font-size: 12.5px;
  color: var(--qss-text, #d4d4d8);
}
.qp-sp p { margin: 0; line-height: 1.45; }
.qp-sp-dim { color: var(--qss-text-secondary, #9a9aa5); }
.qp-sp-good { color: var(--qss-success, #4ade80); }
.qp-sp-bad { color: var(--qss-error, #f87171); }
.qp-sp-steps { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 6px; }
.qp-sp-step { display: flex; flex-wrap: wrap; align-items: baseline; gap: 2px 8px; }
.qp-sp-text { min-width: 0; }
.qp-sp-code {
  flex-basis: 100%;
  min-width: 0;
  overflow-wrap: anywhere;
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 11.5px;
  color: var(--qss-text-muted, #6e6e7a);
}
.qp-sp-mark { flex-shrink: 0; width: 52px; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-sp-mark.done { color: var(--qss-success, #4ade80); }
.qp-sp-mark.failed { color: var(--qss-error, #f87171); }
.qp-sp-detail { flex-basis: 100%; padding-left: 60px; font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); overflow-wrap: anywhere; }
.qp-sp-actions { display: flex; justify-content: flex-end; gap: 6px; }
</style>
