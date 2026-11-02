<script setup lang="ts">
/**
 * Settings → Storage (parity 19): what QuantPilot keeps, by category, and
 * cleanup rules — all off by default. Preview shows exactly what a cleanup
 * would remove and what stays (and why); Clean up removes only what the
 * preview showed. Turning on the daily run is a separate, explicit switch.
 */
import { computed, onMounted, ref, watch } from 'vue'
import { usePilotStore } from '#pilot/stores/pilot'
import { pilot } from '#pilot/utils/invoke'
import { draftPaths } from '#pilot/utils/storage'
import type { StorageCategory, StorageCleanResult, StoragePlan, StoragePolicy } from '#pilot/types'

const store = usePilotStore()
const inventory = ref<StorageCategory[] | null>(null)
const plan = ref<StoragePlan | null>(null)
const result = ref<StorageCleanResult | null>(null)
const busy = ref(false)
const confirming = ref(false)
const notice = ref('')

const saved = computed<StoragePolicy>(() => ({ patchDays: null, unsentDays: null, documentDays: null, orphans: true, auto: false, lastRun: null, ...(store.settings?.storage ?? {}) }))
const policy = ref<StoragePolicy>({ ...saved.value })
watch(saved, (s) => (policy.value = { ...s }))
const changed = computed(() => JSON.stringify({ ...policy.value, lastRun: null }) !== JSON.stringify({ ...saved.value, lastRun: null }))

const PATCH = [null, 30, 90, 180, 365]
const UNSENT = [null, 7, 30, 90]
const DOCS = [null, 30, 90]
const label = (d: number | null) => (d === null ? 'Keep' : `After ${d} days`)

function size(n: number): string {
  if (n >= 1024 ** 3) return `${(n / 1024 ** 3).toFixed(1)} GB`
  if (n >= 1024 ** 2) return `${(n / 1024 ** 2).toFixed(1)} MB`
  if (n >= 1024) return `${Math.round(n / 1024)} KB`
  return `${n} B`
}
const CATEGORY: Record<string, string> = { orphans: 'Deleted threads', unsent: 'Unsent attachments', documents: 'Document text', patches: 'Saved patches' }
const groups = computed(() => {
  const m = new Map<string, { count: number; bytes: number; items: StoragePlan['items'] }>()
  for (const i of plan.value?.items ?? []) {
    const g = m.get(i.category) ?? { count: 0, bytes: 0, items: [] }
    g.count++
    g.bytes += i.bytes
    if (g.items.length < 50) g.items.push(i)
    m.set(i.category, g)
  }
  return [...m.entries()]
})

async function loadInventory() {
  inventory.value = (await pilot<StorageCategory[]>('plugin:pilot|pilot_storage_inventory').catch(() => [])) ?? []
}
async function preview() {
  busy.value = true
  result.value = null
  confirming.value = false
  try {
    plan.value = await pilot<StoragePlan>('plugin:pilot|pilot_storage_plan', { policy: policy.value, drafts: draftPaths() })
  } catch (e) {
    notice.value = String(e)
  } finally {
    busy.value = false
  }
}
async function clean() {
  if (!plan.value) return
  busy.value = true
  try {
    result.value = await pilot<StorageCleanResult>('plugin:pilot|pilot_storage_clean', { policy: policy.value, drafts: draftPaths(), shown: plan.value.items.map((i) => i.target), daily: false })
    confirming.value = false
    plan.value = null
    await loadInventory()
  } catch (e) {
    notice.value = String(e)
  } finally {
    busy.value = false
  }
}
async function save() {
  await store.saveSettings({ storage: { ...policy.value, lastRun: saved.value.lastRun } }, { quiet: true })
  notice.value = policy.value.auto ? 'Saved — these rules now run once a day.' : 'Saved.'
}
watch(policy, () => (plan.value = null), { deep: true })
onMounted(loadInventory)
</script>

<template>
  <div class="qp-settings" data-module="pilot">
    <h3 class="qp-settings-title">What QuantPilot keeps</h3>
    <table class="qp-st-table">
      <tbody>
        <tr v-for="c in inventory ?? []" :key="c.key">
          <td class="qp-st-name">{{ c.label }}</td>
          <td class="qp-st-num">{{ c.key === 'worktrees' ? `${c.items} threads` : size(c.bytes) }}</td>
          <td class="qp-st-note">{{ c.note }}</td>
        </tr>
      </tbody>
    </table>
    <p v-if="!inventory" class="qp-settings-desc">Measuring…</p>

    <h3 class="qp-settings-title">Cleanup rules</h3>
    <p class="qp-settings-desc">Nothing is removed until you clean up — or turn on the daily run. Kept always: message text and history, browser profiles, the skill library, worktrees (Worktree cleanup), and anything a message, queued message or draft uses or a working thread has.</p>
    <label class="qp-st-rule">
      <span>Saved patches of old turns<small>The text goes; the file list and line counts stay.</small></span>
      <select v-model="policy.patchDays"><option v-for="d in PATCH" :key="String(d)" :value="d">{{ label(d) }}</option></select>
    </label>
    <label class="qp-st-rule">
      <span>Attachments never sent<small>Screenshots, pastes and files in no message or draft.</small></span>
      <select v-model="policy.unsentDays"><option v-for="d in UNSENT" :key="String(d)" :value="d">{{ label(d) }}</option></select>
    </label>
    <label class="qp-st-rule">
      <span>Extracted document text<small>Read again from the document when needed.</small></span>
      <select v-model="policy.documentDays"><option v-for="d in DOCS" :key="String(d)" :value="d">{{ label(d) }}</option></select>
    </label>
    <label class="qp-st-check"><input v-model="policy.orphans" type="checkbox" /> Files left by deleted threads</label>
    <label class="qp-st-check"><input v-model="policy.auto" type="checkbox" /> Run these rules once a day by themselves</label>
    <div class="qp-row">
      <button class="qp-btn qp-btn-sm" :disabled="busy" @click="preview">Preview cleanup</button>
      <button class="qp-btn qp-btn-sm" :disabled="!changed" @click="save">Save rules</button>
      <span class="qp-notice">{{ notice }}</span>
    </div>
    <p v-if="saved.lastRun" class="qp-settings-desc">Last daily run: {{ new Date(saved.lastRun).toLocaleString('en-US', { hour12: false }) }}</p>

    <template v-if="plan">
      <h3 class="qp-settings-title">{{ plan.items.length ? `Would remove ${plan.items.length} item${plan.items.length === 1 ? '' : 's'} · ${size(plan.bytes)}` : 'Nothing to remove' }}</h3>
      <div v-for="[cat, g] in groups" :key="cat" class="qp-st-group">
        <div class="qp-st-ghead">{{ CATEGORY[cat] ?? cat }} · {{ g.count }} · {{ size(g.bytes) }}</div>
        <div v-for="i in g.items" :key="i.target" class="qp-st-item" :title="i.target">
          <span class="qp-st-target">{{ i.thread ? `${i.thread} — ` : '' }}{{ i.target.split(/[\\/]/).pop() }}</span>
          <span class="qp-st-why">{{ i.reason }}</span>
          <span class="qp-st-num">{{ size(i.bytes) }}</span>
        </div>
        <div v-if="g.count > g.items.length" class="qp-st-why">and {{ g.count - g.items.length }} more</div>
      </div>
      <template v-if="plan.kept.length">
        <div class="qp-st-ghead">Stays</div>
        <div v-for="k in plan.kept" :key="k.category + k.reason" class="qp-st-item">
          <span class="qp-st-target">{{ CATEGORY[k.category] ?? k.category }}</span>
          <span class="qp-st-why">{{ k.reason }} · {{ k.count }}</span>
          <span class="qp-st-num">{{ size(k.bytes) }}</span>
        </div>
      </template>
      <div v-if="plan.items.length" class="qp-row">
        <template v-if="confirming">
          <span class="qp-settings-desc">Remove these {{ plan.items.length }} items? This cannot be undone.</span>
          <button class="qp-btn qp-btn-sm" :disabled="busy" @click="confirming = false">Cancel</button>
          <button class="qp-btn qp-btn-sm qp-btn-primary" :disabled="busy" @click="clean">Remove</button>
        </template>
        <button v-else class="qp-btn qp-btn-sm qp-btn-primary" :disabled="busy" @click="confirming = true">Clean up now</button>
      </div>
    </template>
    <p v-if="result" class="qp-settings-desc" role="status">
      Removed {{ result.removed }} item{{ result.removed === 1 ? '' : 's' }} · {{ size(result.bytes) }}.<template v-if="result.failed.length"> {{ result.failed.length }} could not be removed (in use?) — they are tried again next time: {{ result.failed.map((f) => f.target.split(/[\\/]/).pop() + ': ' + f.error).join('; ') }}</template>
    </p>
  </div>
</template>

<style scoped>
.qp-settings { display: flex; flex-direction: column; gap: 10px; font-size: 12.5px; color: var(--qss-text, #d4d4d8); }
.qp-settings-title { margin: 6px 0 0; font-size: 13px; font-weight: 600; }
.qp-settings-desc { margin: 0; color: var(--qss-text-secondary, #9a9aa5); font-size: 12px; line-height: 1.5; }
.qp-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.qp-notice { font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-st-table { border-collapse: collapse; width: 100%; }
.qp-st-table td { padding: 4px 8px 4px 0; vertical-align: top; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-st-name { white-space: nowrap; }
.qp-st-num { white-space: nowrap; text-align: right; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-st-note { color: var(--qss-text-muted, #6e6e7a); font-size: 11.5px; }
.qp-st-rule { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.qp-st-rule span { display: flex; flex-direction: column; }
.qp-st-rule small { color: var(--qss-text-muted, #6e6e7a); font-size: 11.5px; }
.qp-st-rule select { min-width: 140px; height: 26px; padding: 0 6px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: var(--qss-bg, #18181e); color: var(--qss-text, #d4d4d8); font: inherit; }
.qp-st-check { display: flex; align-items: center; gap: 8px; }
.qp-st-group { display: flex; flex-direction: column; gap: 2px; }
.qp-st-ghead { margin-top: 4px; font-size: 12px; font-weight: 600; color: var(--qss-text-secondary, #9a9aa5); }
.qp-st-item { display: grid; grid-template-columns: minmax(0, 1.2fr) minmax(0, 2fr) auto; gap: 10px; padding: 2px 0; }
.qp-st-target { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-st-why { min-width: 0; color: var(--qss-text-muted, #6e6e7a); font-size: 11.5px; }
</style>
