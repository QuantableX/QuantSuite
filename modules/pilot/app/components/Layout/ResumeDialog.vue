<script setup lang="ts">
/**
 * After a restart (round 2, card 12; Orca's resume-on-restart): the threads
 * that were working when the app closed, grouped by project — resume all,
 * or the ones still ticked. Offered once per start; "Not now" leaves each
 * thread's own Resume button in its composer.
 */
import { computed, onMounted, ref } from 'vue'
import { RotateCcw } from 'lucide-vue-next'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'
import { timeAgo } from '#pilot/utils/format'

const store = usePilotStore()
const threads = useThreadStore()
const picked = ref<Set<string>>(new Set(store.restartOffer.map((t) => t.id)))
const busy = ref(false)
const groups = computed(() => {
  const by = new Map<string, typeof store.restartOffer>()
  for (const t of store.restartOffer) by.set(t.contextName || 'No project', [...(by.get(t.contextName || 'No project') ?? []), t])
  return [...by.entries()]
})
function toggle(id: string) {
  const next = new Set(picked.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  picked.value = next
}
function close() {
  store.restartOffer = []
}
async function resume() {
  busy.value = true
  const ids = store.restartOffer.filter((t) => picked.value.has(t.id)).map((t) => t.id)
  close()
  for (const id of ids) await threads.resumeTurn(id)
  busy.value = false
}
const root = ref<HTMLElement | null>(null)
onMounted(() => root.value?.querySelector<HTMLButtonElement>('.qp-rd-primary')?.focus())
function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    close()
  }
}
</script>

<template>
  <div class="qp-rd-veil" @pointerdown.self="close">
    <div ref="root" class="qp-rd" role="dialog" aria-modal="true" aria-labelledby="qp-rd-title" @keydown="onKey">
      <h3 id="qp-rd-title" class="qp-rd-title"><RotateCcw :size="14" /> Resume what was working?</h3>
      <p class="qp-rd-desc">These threads were working when QuantSuite closed. Their turn stopped; resuming continues where each left off.</p>
      <div class="qp-rd-list">
        <section v-for="[project, list] in groups" :key="project" class="qp-rd-group">
          <h4 class="qp-rd-project">{{ project }}</h4>
          <label v-for="t in list" :key="t.id" class="qp-rd-row">
            <input type="checkbox" :checked="picked.has(t.id)" @change="toggle(t.id)" />
            <PilotAgentIcon :agent="t.provider || null" :size="14" decorative />
            <span class="qp-rd-name">{{ t.title || 'New thread' }}</span>
            <span class="qp-rd-when">{{ timeAgo(t.lastActivityAt || t.updatedAt) }}</span>
          </label>
        </section>
      </div>
      <div class="qp-rd-actions">
        <button class="qp-rd-btn" @click="close">Not now</button>
        <button class="qp-rd-btn qp-rd-primary" :disabled="busy || !picked.size" @click="resume">
          {{ picked.size === store.restartOffer.length ? `Resume all (${picked.size})` : `Resume ${picked.size}` }}
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qp-rd-veil { position: fixed; inset: 0; z-index: 1100; display: flex; align-items: center; justify-content: center; background: rgba(0, 0, 0, 0.35); }
.qp-rd {
  width: 440px;
  max-width: calc(100% - 32px);
  max-height: calc(100% - 48px);
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 16px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 12px;
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 16px 40px rgba(0, 0, 0, 0.5);
  color: var(--qss-text, #d4d4d8);
}
.qp-rd-title { display: flex; align-items: center; gap: 8px; margin: 0; font-size: 14px; font-weight: 600; }
.qp-rd-desc { margin: 0; font-size: 12px; line-height: 1.5; color: var(--qss-text-secondary, #9a9aa5); }
.qp-rd-list { min-height: 0; overflow-y: auto; display: flex; flex-direction: column; gap: 8px; }
.qp-rd-project { margin: 0 0 2px; font-size: 11px; font-weight: 600; color: var(--qss-text-muted, #6e6e7a); }
.qp-rd-row { display: flex; align-items: center; gap: 8px; padding: 5px 6px; border-radius: 6px; font-size: 12.5px; cursor: pointer; }
.qp-rd-row:hover { background: var(--qss-bg-hover, #313139); }
.qp-rd-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-rd-when { flex-shrink: 0; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-rd-actions { display: flex; justify-content: flex-end; gap: 8px; }
.qp-rd-btn { padding: 6px 12px; border: 1px solid var(--qss-border, #47474f); border-radius: 7px; background: transparent; color: var(--qss-text, #d4d4d8); font: inherit; font-size: 12.5px; cursor: pointer; }
.qp-rd-btn:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); }
.qp-rd-primary { border-color: var(--qss-accent, #7aa2f7); }
.qp-rd-btn:disabled { opacity: 0.5; cursor: default; }
</style>
