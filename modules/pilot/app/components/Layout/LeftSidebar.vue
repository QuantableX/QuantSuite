<script setup lang="ts">
/**
 * Sessions by context. "New" opens a terminal in a context (General or a
 * workspace) with a permission mode; which agent runs there is typed in
 * the terminal — `claude`, `codex`, `pi`… are wrappers on this row's
 * session. The list groups every session under its context. Clicking a
 * row is the link: it attaches to the running terminal, or opens it again
 * with the same wrappers, which then resume.
 */
import { computed, reactive, ref } from 'vue'
import { usePilotStore } from '#pilot/stores/pilot'
import { timeAgo } from '#pilot/utils/format'
import type { Mode, PilotContext, SessionMeta } from '#pilot/types'

const store = usePilotStore()

const showNew = ref(false)
const form = reactive<{ contextId: string; mode: Mode }>({
  contextId: 'general',
  mode: 'full',
})

function openNew() {
  form.mode = store.settings?.defaultMode ?? 'full'
  if (!store.contexts.some((c) => c.id === form.contextId)) form.contextId = 'general'
  showNew.value = !showNew.value
}

async function create() {
  const meta = await store.createSession({ contextId: form.contextId, mode: form.mode })
  if (meta) showNew.value = false
}

const found = computed(() => store.adapters.filter((a) => !store.status || a.found).map((a) => a.id))

const groups = computed(() => {
  const byContext = new Map<string, SessionMeta[]>()
  for (const s of store.sessions) {
    const list = byContext.get(s.contextId) ?? []
    list.push(s)
    byContext.set(s.contextId, list)
  }
  const out: { ctx: PilotContext | null; id: string; name: string; sessions: SessionMeta[] }[] = []
  for (const ctx of store.contexts) {
    const sessions = byContext.get(ctx.id) ?? []
    byContext.delete(ctx.id)
    out.push({ ctx, id: ctx.id, name: ctx.name, sessions })
  }
  // Sessions whose workspace was removed from the registry still exist.
  for (const [id, sessions] of byContext) {
    out.push({ ctx: null, id, name: sessions[0]?.contextName || 'Removed workspace', sessions })
  }
  return out
})

function stateOf(id: string) {
  return store.live[id]?.state ?? 'offline'
}

async function remove(s: SessionMeta) {
  if (!confirm(`Delete "${s.title || 'this session'}"? The row is removed from QuantPilot; the CLI's own transcript stays.`)) return
  await store.remove(s.id)
}
</script>

<template>
  <div class="qp-side">
    <div class="qp-side-head">
      <span class="qp-side-title">Sessions</span>
      <button class="qp-btn qp-btn-primary qp-btn-sm" @click="openNew">{{ showNew ? 'Close' : 'New' }}</button>
    </div>

    <form v-if="showNew" class="qp-new" @submit.prevent="create">
      <label class="qp-field">
        <span>Context</span>
        <select v-model="form.contextId" class="qp-select">
          <option v-for="c in store.contexts" :key="c.id" :value="c.id">{{ c.name }}{{ c.kind === 'general' ? ' (memory vault)' : '' }}</option>
        </select>
      </label>
      <label class="qp-field">
        <span>Permissions</span>
        <select v-model="form.mode" class="qp-select">
          <option value="ask">Ask before tools</option>
          <option value="auto">Auto (the agent's own judgement)</option>
          <option value="full">Full access</option>
        </select>
      </label>
      <button class="qp-btn qp-btn-primary" type="submit">Open terminal</button>
      <span class="qp-field-hint">
        Then type <template v-for="(id, i) in found" :key="id"><code>{{ id }}</code>{{ i < found.length - 1 ? ', ' : '' }}</template>
        — each runs as this session.
      </span>
      <span v-if="store.lastError" class="qp-field-warn">{{ store.lastError }}</span>
    </form>

    <div class="qp-side-list">
      <section v-for="g in groups" :key="g.id" class="qp-group">
        <div class="qp-group-head" :title="g.ctx?.path">
          <span class="qp-group-name">{{ g.name }}</span>
          <span class="qp-group-count">{{ g.sessions.length }}</span>
        </div>
        <div v-if="!g.sessions.length" class="qp-group-empty">no sessions yet</div>
        <button
          v-for="s in g.sessions"
          :key="s.id"
          class="qp-row"
          :class="{ active: store.activeId === s.id }"
          :title="store.live[s.id]?.detail || undefined"
          @click="store.openSession(s.id)"
        >
          <span class="qp-row-dot" :data-state="stateOf(s.id)" />
          <span class="qp-row-text">
            <span class="qp-row-title">{{ s.title || 'New session' }}</span>
            <span class="qp-row-sub">{{ store.adapterLabel(s.provider) }} · {{ timeAgo(s.updatedAt) }}</span>
          </span>
          <span class="qp-row-x" title="Delete session" @click.stop="remove(s)">×</span>
        </button>
      </section>
    </div>
  </div>
</template>

<style scoped>
.qp-side { display: flex; flex-direction: column; height: 100%; min-height: 0; }
.qp-side-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 12px 8px;
  border-bottom: 1px solid var(--qss-border-subtle, #35353d);
}
.qp-side-title { font-size: 12px; font-weight: 600; color: var(--qss-text-secondary, #9a9aa5); }
.qp-new {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 12px;
  border-bottom: 1px solid var(--qss-border-subtle, #35353d);
  background: var(--qss-bg-raised, #1f1f25);
}
.qp-field { display: flex; flex-direction: column; gap: 4px; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-field-warn { color: var(--qss-warning, #d29a3f); font-size: 11px; line-height: 1.4; }
.qp-field-hint { color: var(--qss-text-muted, #6e6e7a); font-size: 11px; line-height: 1.5; }
.qp-field-hint code { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 10.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-select {
  background: var(--qss-bg, #18181e);
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12px;
  padding: 5px 8px;
  outline: none;
}
.qp-side-list { flex: 1; min-height: 0; overflow-y: auto; padding: 6px 0 10px; }
.qp-group { padding: 4px 0 6px; }
.qp-group-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 12px 3px;
  font-size: 11px;
  color: var(--qss-text-muted, #6e6e7a);
}
.qp-group-name { font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-group-empty { padding: 2px 12px 4px; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); opacity: 0.7; }
.qp-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 12px;
  background: transparent;
  border: none;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.qp-row:hover { background: var(--qss-bg-hover, #313139); }
.qp-row.active { background: var(--qss-bg-card, #292930); }
.qp-row-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--qss-border, #47474f); flex-shrink: 0; }
.qp-row-dot[data-state='idle'] { background: var(--qss-text-secondary, #9a9aa5); }
.qp-row-dot[data-state='thinking'],
.qp-row-dot[data-state='working'] { background: var(--qss-success, #34d399); box-shadow: 0 0 5px var(--qss-success, #34d399); }
.qp-row-dot[data-state='waiting'] { background: var(--qss-warning, #d29a3f); box-shadow: 0 0 5px var(--qss-warning, #d29a3f); }
.qp-row-dot[data-state='error'] { background: var(--qss-error, #f87171); }
.qp-row-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
.qp-row-title { font-size: 12.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-row-sub { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-row-x { color: var(--qss-text-muted, #6e6e7a); font-size: 14px; line-height: 1; padding: 0 2px; opacity: 0; }
.qp-row:hover .qp-row-x { opacity: 1; }
.qp-row-x:hover { color: var(--qss-error, #f87171); }
</style>
