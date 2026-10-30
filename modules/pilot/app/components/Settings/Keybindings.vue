<script setup lang="ts">
/**
 * Settings → Keybindings (card 17): every command with its keys, searchable
 * by name, id or key. Record a shortcut (Escape cancels), remove it, or go
 * back to the default; what else answers the same key — another command,
 * the suite itself, a project action — shows as a warning. The rules live
 * in keybindings.json in the pilot data dir; the file's invalid rules are
 * named here and skipped.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { useKeysStore } from '#pilot/stores/keys'
import { usePilotStore } from '#pilot/stores/pilot'
import { chordOf, COMMANDS, conflicts, keysOf, rebind, resetBinding, unbind, usableKey, type CommandInfo } from '#pilot/utils/keybindings'

const keys = useKeysStore()
const store = usePilotStore()
const search = ref('')
const recording = ref<string | null>(null)
const notice = ref('')
const busy = ref(false)

onMounted(async () => {
  if (!keys.loaded) await keys.load()
  if (!store.status) void store.refreshStatus()
})
onBeforeUnmount(() => (keys.recording = false))

const filePath = computed(() => (store.status?.dataDir ? `${store.status.dataDir}\\keybindings.json` : 'keybindings.json in the QuantPilot data folder'))

/** Project actions' keys answer in the terminal drawer's threads too. */
const actionKeys = computed(() => {
  const out: Record<string, string> = {}
  for (const list of Object.values(store.settings?.actions ?? {})) for (const a of list) if (a.key) out[a.key] = `Project action "${a.name || a.command}"`
  return out
})

interface Row {
  info: CommandInfo
  keys: string[]
  when: string
  custom: boolean
  warnings: string[]
}

const rows = computed<Row[]>(() => {
  const q = search.value.trim().toLowerCase()
  return COMMANDS.map((info) => {
    const k = keysOf(keys.rules, info.id)
    const rule = [...keys.rules].reverse().find((r) => r.command === info.id)
    const warnings = k.flatMap((key) => conflicts(keys.rules, info.id, key, rule?.when, actionKeys.value).map((w) => `${key} also: ${w}`))
    return { info, keys: k, when: rule?.when ?? '', custom: keys.user.some((r) => r.command === info.id || r.command === `-${info.id}`), warnings }
  }).filter((r) => !q || `${r.info.title} ${r.info.id} ${r.keys.join(' ')}`.toLowerCase().includes(q))
})
const groups = computed(() => ['General', 'Threads', 'Panels', 'Composer'].map((g) => ({ name: g, rows: rows.value.filter((r) => r.info.group === g) })).filter((g) => g.rows.length))

function startRecording(id: string) {
  recording.value = id
  keys.recording = true
  notice.value = ''
}
function stopRecording() {
  recording.value = null
  keys.recording = false
}

async function apply(next: typeof keys.user, done: string) {
  busy.value = true
  try {
    await keys.save(next)
    notice.value = done
  } catch (e) {
    notice.value = e instanceof Error ? e.message : String(e)
  } finally {
    busy.value = false
  }
}

function onRecordKey(e: KeyboardEvent, id: string) {
  if (recording.value !== id || e.key === 'Tab') return
  e.preventDefault()
  e.stopPropagation()
  if (e.key === 'Escape') return stopRecording()
  const chord = chordOf(e)
  if (!chord) return
  if (!usableKey(chord)) {
    notice.value = 'Use Ctrl or Alt with a key, or a function key.'
    return
  }
  stopRecording()
  const title = COMMANDS.find((c) => c.id === id)?.title ?? id
  void apply(rebind(keys.user, id, chord), `${title}: ${chord}`)
}
</script>

<template>
  <div class="qp-settings">
    <p class="qp-settings-desc">
      Click a shortcut and press the new keys. The last rule that matches wins; the terminal keeps its own keys. The rules are stored in
      <code>{{ filePath }}</code> — edit it by hand if you like: a JSON list of <code>{ "key", "command", "when" }</code>.
    </p>
    <div v-if="keys.problems.length" class="qp-kb-problems" role="alert">
      <strong>Skipped in keybindings.json:</strong>
      <span v-for="p in keys.problems" :key="p">{{ p }}</span>
    </div>
    <input v-model="search" class="qp-input" placeholder="Search commands or keys" spellcheck="false" />

    <section v-for="g in groups" :key="g.name" class="qp-kb-group">
      <h3 class="qp-settings-title">{{ g.name }}</h3>
      <div v-for="r in g.rows" :key="r.info.id" class="qp-kb-row" :class="{ warn: r.warnings.length }">
        <div class="qp-kb-name">
          <span class="qp-kb-title">{{ r.info.title }}</span>
          <span class="qp-kb-id">{{ r.info.id }}<template v-if="r.when"> · when {{ r.when }}</template></span>
          <span v-for="w in r.warnings" :key="w" class="qp-kb-warn">{{ w }}</span>
        </div>
        <button
          class="qp-input qp-kb-key"
          :class="{ recording: recording === r.info.id }"
          :title="recording === r.info.id ? 'Press the keys — Escape cancels' : 'Click to record a shortcut'"
          :disabled="busy"
          @click="startRecording(r.info.id)"
          @keydown="onRecordKey($event, r.info.id)"
          @blur="recording === r.info.id && stopRecording()"
        >
          {{ recording === r.info.id ? 'Press keys…' : r.keys.join(' · ') || 'No shortcut' }}
        </button>
        <button class="qp-kb-btn" :disabled="busy || !r.keys.length" title="No shortcut for this command" @click="apply(unbind(keys.user, r.info.id), `${r.info.title}: no shortcut`)">Remove</button>
        <button class="qp-kb-btn" :disabled="busy || !r.custom" title="Back to the default" @click="apply(resetBinding(keys.user, r.info.id), `${r.info.title}: default`)">Reset</button>
      </div>
    </section>
    <div v-if="!groups.length" class="qp-kb-empty">No command matches.</div>
    <div class="qp-kb-notice" aria-live="polite">{{ notice }}</div>
  </div>
</template>

<style scoped>
.qp-settings { display: flex; flex-direction: column; gap: 10px; font-size: 12.5px; color: var(--qss-text, #d4d4d8); }
.qp-settings-title { margin: 6px 0 2px; font-size: 13px; font-weight: 600; }
.qp-settings-desc { margin: 0; color: var(--qss-text-secondary, #9a9aa5); line-height: 1.5; }
.qp-settings-desc code { font-size: 11.5px; overflow-wrap: anywhere; }
.qp-input {
  padding: 5px 8px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: var(--qss-radius, 6px);
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12.5px;
  outline: none;
}
.qp-input:focus { border-color: var(--qss-accent, #a0a0a8); }
.qp-kb-problems { display: flex; flex-direction: column; gap: 2px; padding: 8px 10px; border-radius: 8px; border: 1px solid color-mix(in srgb, var(--qss-warning, #d29a3f) 45%, var(--qss-border, #47474f)); color: var(--qss-text-secondary, #9a9aa5); font-size: 12px; }
.qp-kb-problems strong { color: var(--qss-warning, #d29a3f); font-weight: 600; }
.qp-kb-group { display: flex; flex-direction: column; gap: 2px; }
.qp-kb-row { display: flex; align-items: center; gap: 6px; padding: 4px 6px; border-radius: 6px; }
.qp-kb-row:hover { background: var(--qss-bg-hover, #313139); }
.qp-kb-name { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
.qp-kb-title { font-size: 12.5px; }
.qp-kb-id { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-kb-warn { font-size: 11px; color: var(--qss-warning, #d29a3f); }
.qp-kb-key { min-width: 150px; text-align: left; color: var(--qss-text-secondary, #9a9aa5); cursor: pointer; font-size: 12px; }
.qp-kb-key.recording { border-color: var(--qss-accent, #7aa2f7); color: var(--qss-text, #d4d4d8); }
.qp-kb-btn { padding: 3px 8px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: 6px; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; font-size: 11.5px; cursor: pointer; }
.qp-kb-btn:hover:not(:disabled) { background: var(--qss-bg-card, #292930); color: var(--qss-text, #d4d4d8); }
.qp-kb-btn:disabled { opacity: 0.4; cursor: default; }
.qp-kb-empty,
.qp-kb-notice { font-size: 12px; color: var(--qss-text-muted, #6e6e7a); min-height: 16px; }
</style>
