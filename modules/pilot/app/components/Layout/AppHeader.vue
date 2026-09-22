<script setup lang="ts">
/**
 * Header center: the open session's title (click to rename), its context,
 * the agent running in its terminal (or "terminal"), the model it reported,
 * the permission mode the wrappers pass on, and End while the terminal
 * lives. The chrome (logo cap, selector, settings) is QModuleHeader's.
 */
import { computed, nextTick, ref } from 'vue'
import { usePilotStore } from '#pilot/stores/pilot'
import { modeLabel } from '#pilot/utils/format'

const store = usePilotStore()
const session = computed(() => store.active)
const live = computed(() => store.activeLive)

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
  if (!confirm('End this terminal? The session stays and its agent resumes on the next open.')) return
  await store.stop(session.value.id)
}
</script>

<template>
  <QModuleHeader module-id="pilot">
    <div class="qp-header-center">
      <template v-if="session">
        <form v-if="editing" class="qp-title-form" @submit.prevent="commitEdit">
          <input ref="titleInput" v-model="draft" class="qp-title-input" @blur="commitEdit" @keydown.esc="editing = false" />
        </form>
        <button v-else class="qp-title" :title="session.title || 'Untitled — click to rename'" @click="startEdit">
          {{ session.title || 'New session' }}
        </button>
        <span class="qp-head-chip" :title="session.cwd">{{ session.contextName }}</span>
        <span class="qp-head-chip is-provider" :title="live.display || undefined">{{ store.adapterLabel(session.provider) }}</span>
        <span v-if="model" class="qp-head-chip" :title="`Model: ${model}`">{{ model }}</span>
        <span class="qp-head-chip" :title="'Permissions the wrappers pass on'">{{ modeLabel(session.mode) }}</span>
        <div class="qp-header-spacer" />
        <button v-if="live.alive" class="qp-btn qp-btn-sm qp-btn-danger" title="End the agent process" @click="end">End</button>
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
.qp-header-spacer { flex: 1; }
.qp-head-hint { font-size: 12px; color: var(--qss-text-muted, #6e6e7a); }
</style>
