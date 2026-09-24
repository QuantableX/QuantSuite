<script setup lang="ts">
/**
 * The confirmation of an install, update or removal. An install first
 * shows store_plan — what is new, what is replaced, which requirements come
 * with it and what conflicts — and a conflict needs the explicit overwrite
 * box. The install itself stages your library and has the engine check it
 * before anything is written.
 */
import { useCatalogStore } from '#script/stores/catalog'
import { PLAN_ACTION_LABELS, planSummary } from '#script/utils/catalog'
import type { StoreCatalogItem, StorePlan } from '#script/types'

const props = defineProps<{ item: StoreCatalogItem; mode: 'install' | 'update' | 'remove' }>()
const emit = defineEmits<{ close: []; done: [message: string] }>()

const store = useCatalogStore()
const plan = ref<StorePlan | null>(null)
const planError = ref<string | null>(null)
const error = ref<string | null>(null)
const overwrite = ref(false)
const dialog = ref<HTMLElement | null>(null)
const previousFocus = typeof document !== 'undefined' ? document.activeElement as HTMLElement | null : null

const summary = computed(() => (plan.value ? planSummary(plan.value) : null))
const busy = computed(() => store.busy !== null)
const title = computed(() => ({ install: `Install ${props.item.name}`, update: `Update ${props.item.name}`, remove: `Remove ${props.item.name}` })[props.mode])
const canConfirm = computed(() => {
  if (busy.value) return false
  if (props.mode === 'remove') return true
  if (!plan.value || summary.value?.empty) return false
  return plan.value.conflicts === 0 || overwrite.value
})

onMounted(async () => {
  dialog.value?.focus()
  if (props.mode === 'remove') return
  try {
    plan.value = await store.plan([props.item.key])
  } catch (err) {
    planError.value = String(err)
  }
})
onUnmounted(() => previousFocus?.focus())

async function confirm() {
  if (!canConfirm.value) return
  error.value = null
  try {
    if (props.mode === 'remove') {
      const removed = await store.remove(props.item.key)
      emit('done', `Removed ${removed.key} — ${removed.files.length} file(s). The script is kept in the history.`)
      return
    }
    const outcome = await store.install([props.item.key], overwrite.value)
    const scripts = outcome.written.length
    emit('done', `${props.mode === 'update' ? 'Updated' : 'Installed'} ${props.item.name} ${props.item.version}${scripts > 1 ? ` with ${scripts - 1} requirement script(s)` : ''}.`)
  } catch (err) {
    error.value = String(err)
  }
}

function close() {
  if (!busy.value) emit('close')
}
</script>

<template>
  <div class="qsc-modal-overlay" @mousedown.self="close">
    <div ref="dialog" class="qsc-modal qst-modal" role="dialog" aria-modal="true" :aria-label="title" tabindex="-1" @keydown.esc.stop="close">
      <header class="qsc-modal-head">
        <h2 class="qsc-modal-title">{{ title }}</h2>
        <button class="qsc-icon-btn" aria-label="Close dialog" title="Close" :disabled="busy" @click="close">
          <svg width="12" height="12" viewBox="0 0 14 14" fill="none" aria-hidden="true">
            <path d="M3 3l8 8M11 3l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
          </svg>
        </button>
      </header>

      <template v-if="mode === 'remove'">
        <p class="qst-modal-text">
          Removes <span class="mono">{{ item.key }}.py</span> and its version files from your library. Its last content stays in
          the history, so Restore brings it back. The Store refuses while another installed indicator requires it.
        </p>
      </template>
      <template v-else>
        <p v-if="!plan && !planError" class="muted qsc-pulse">Planning…</p>
        <div v-if="planError" class="qsc-note is-error" role="alert">{{ planError }}</div>
        <template v-if="plan && summary">
          <p class="qst-modal-text">
            <template v-if="summary.empty">Everything is already in your library — nothing to write.</template>
            <template v-else>
              <span class="mono">{{ summary.create }}</span> new · <span class="mono">{{ summary.replace }}</span> replaced
              <template v-if="summary.dependencies"> · <span class="mono">{{ summary.dependencies }}</span> requirement(s) come with it</template>
              <template v-if="summary.conflict"> · <span class="mono">{{ summary.conflict }}</span> conflict(s)</template>
            </template>
          </p>
          <ul class="qst-plan">
            <li v-for="p in plan.items" :key="p.key" :class="`is-${p.action}`">
              <span class="qst-plan-action">{{ PLAN_ACTION_LABELS[p.action] }}</span>
              <span class="qst-plan-name">{{ p.name }} <span class="mono muted">{{ p.key }} {{ p.version }}</span></span>
              <span class="muted qst-plan-why">
                {{ p.requested ? '' : 'requirement' }}{{ p.reason ? (p.requested ? '' : ' · ') + p.reason : '' }}
                <template v-if="p.installed_version && p.action === 'replace'"> · from {{ p.installed_version }}</template>
              </span>
            </li>
          </ul>
          <label v-if="plan.conflicts" class="qsc-check qst-overwrite">
            <input v-model="overwrite" type="checkbox" :disabled="busy" />
            Replace the conflicting files — their current content stays in the history
          </label>
        </template>
      </template>

      <div v-if="error" class="qsc-note is-error" role="alert">{{ error }}</div>
      <p v-if="busy" class="muted qsc-pulse" role="status">
        {{ mode === 'remove' ? 'Removing…' : 'Checking the new files with your library in a sandbox, then writing them…' }}
      </p>

      <footer class="qsc-modal-foot">
        <button class="qsc-btn" :disabled="busy" @click="close">Cancel</button>
        <button class="qsc-btn" :class="mode === 'remove' ? 'is-danger' : 'is-primary'" :disabled="!canConfirm" @click="confirm">
          {{ mode === 'remove' ? 'Remove' : mode === 'update' ? 'Update' : 'Install' }}
        </button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.qst-modal {
  width: 540px;
  max-width: 100%;
}
.qst-modal-text {
  font-size: 12.5px;
  line-height: 1.5;
  color: var(--qss-text-secondary);
}
.qst-plan {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 280px;
  overflow-y: auto;
  padding: 6px 8px;
  border: 1px solid var(--qss-border-subtle);
  border-radius: 8px;
  font-size: 12px;
}
.qst-plan li {
  display: grid;
  grid-template-columns: 78px minmax(0, 1fr);
  column-gap: 8px;
}
.qst-plan-action {
  font-size: 11px;
  color: var(--qss-text-muted);
}
.qst-plan li.is-create .qst-plan-action {
  color: var(--qss-success);
}
.qst-plan li.is-replace .qst-plan-action,
.qst-plan li.is-conflict .qst-plan-action {
  color: var(--qss-warning);
}
.qst-plan-name {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--qss-text);
}
.qst-plan-why {
  grid-column: 2;
  font-size: 11px;
}
.qst-overwrite {
  font-size: 12px;
}
</style>
