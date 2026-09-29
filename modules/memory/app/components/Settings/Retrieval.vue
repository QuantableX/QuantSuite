<script setup lang="ts">
import { getInvoke } from '#memory/utils/invoke'
import { useVaultStore } from '#memory/stores/vault'
interface EngineModel { id: string; label: string; size: number; dims: number; license: string; gpuRecommended: boolean; installed: boolean }
interface EngineStatus {
  supported: boolean; nvidia: boolean; missingBytes: number; running: boolean; ready: boolean
  model: string | null; device: string | null; gpu: string | null; loadMs: number | null
  lastError: string | null; lastWarning: string | null; models: EngineModel[]
  setup: { busy: boolean; phase: string; file: string; done: number; total: number; error: string | null }
}
interface Status { engine: EngineStatus; pending: number; indexerError: string | null }
const vault = useVaultStore()
const config = reactive({ enabled: false, provider: 'builtin', port: 11434, model: '', revision: '1', builtinModel: 'qwen3-embedding-0.6b', device: 'auto', idleMinutes: 5 })
const loaded = ref(false)
const busy = ref(false)
const savedEnabled = ref(false)
const notice = ref('')
const scope = ref(vault.createScope)
const status = ref<Status | null>(null)
let timer: ReturnType<typeof setTimeout> | null = null
let alive = true
const engine = computed(() => status.value?.engine ?? null)
const downloading = computed(() => engine.value?.setup.busy ?? false)
const builtin = computed(() => config.provider === 'builtin')
const gb = (bytes: number) => `${(bytes / 1e9).toFixed(bytes < 1e9 ? 2 : 1)} GB`
const draft = () => ({ ...config, port: Number(config.port), idleMinutes: Number(config.idleMinutes) })
async function refresh() {
  try {
    const invoke = await getInvoke()
    status.value = await invoke('plugin:memory|embedding_engine_status', { draft: builtin.value ? draft() : null })
  } catch { /* the form still works without the status line */ }
}
async function poll() {
  if (!alive) return
  await refresh()
  if (alive) timer = setTimeout(poll, downloading.value ? 700 : 4000)
}
onMounted(async () => {
  try { const invoke = await getInvoke(); Object.assign(config, await invoke('plugin:memory|get_embedding_config')); savedEnabled.value = config.enabled; loaded.value = true }
  catch (e) { notice.value = String(e) }
  void poll()
})
onBeforeUnmount(() => { alive = false; if (timer) clearTimeout(timer) })
watch(() => [config.provider, config.builtinModel, config.device], () => { void refresh() })
watch(downloading, (now, before) => {
  if (before && !now) {
    const error = engine.value?.setup.error
    if (error) notice.value = error
    else { config.enabled = true; savedEnabled.value = true; notice.value = 'Downloaded. Local embeddings are on; memories are indexed in the background.' }
  }
})
async function save() {
  busy.value = true; notice.value = ''
  try {
    const invoke = await getInvoke()
    await invoke('plugin:memory|set_embedding_config', { config: draft() })
    savedEnabled.value = config.enabled
    notice.value = config.enabled ? (builtin.value ? 'Local embeddings enabled; memories are indexed in the background.' : 'Local embeddings enabled. Index a workspace below.') : 'Embeddings disabled; lexical recall remains available.'
    await refresh()
  } catch (e) { notice.value = String(e) }
  finally { busy.value = false }
}
async function download() {
  busy.value = true; notice.value = ''
  try {
    const invoke = await getInvoke()
    await invoke('plugin:memory|embedding_engine_setup', { model: config.builtinModel, device: config.device, idleMinutes: Number(config.idleMinutes) })
    await refresh()
  } catch (e) { notice.value = String(e) }
  finally { busy.value = false }
}
async function indexBatch() {
  busy.value = true; notice.value = 'Indexing up to 10 memories…'
  try {
    const invoke = await getInvoke()
    const result = await invoke('plugin:memory|index_embeddings', { scope: scope.value })
    notice.value = `${result.indexed} memories indexed. ${result.hasMore ? 'More remain; run another batch.' : 'This workspace is up to date.'}`
  } catch (e) { notice.value = String(e) }
  finally { busy.value = false }
}
const engineLine = computed(() => {
  const e = engine.value
  if (!e) return ''
  if (!e.supported) return 'The built-in engine ships for Windows x64 so far; use Ollama on this system.'
  if (e.running && e.ready) return `Running on ${e.device === 'cuda' ? `the GPU (${e.gpu})` : 'the CPU'} · ${e.model} · loaded in ${((e.loadMs ?? 0) / 1000).toFixed(1)} s`
  if (e.running) return 'Loading the model…'
  return e.missingBytes > 0 ? `Not downloaded yet: ${gb(e.missingBytes)} to download once.` : 'Stopped; starts with the next recall or indexing.'
})
const progress = computed(() => {
  const s = engine.value?.setup
  if (!s?.busy) return ''
  if (s.phase === 'extract') return 'Unpacking the runtime…'
  return `Downloading ${s.file} · ${gb(s.done)} of ${gb(s.total)}`
})
</script>
<template>
  <section class="qm-embedding" aria-label="Local memory embeddings">
    <h3>Local semantic recall</h3>
    <p>A multilingual embedding model finds memories across languages. The built-in engine downloads llama.cpp and the model once (GitHub, Hugging Face) and then runs offline on this machine; Ollama uses a model you already run at 127.0.0.1. Enabling this sends memory text and recall queries to that local engine only.</p>
    <label class="qm-embedding-provider">Engine<select v-model="config.provider" :disabled="!loaded || busy || downloading"><option value="builtin">Built-in (llama.cpp)</option><option value="ollama">Ollama</option></select></label>
    <template v-if="builtin">
      <div class="qm-embedding-fields"><label>Model<select v-model="config.builtinModel" :disabled="!loaded || busy || downloading"><option v-for="m in engine?.models ?? []" :key="m.id" :value="m.id">{{ m.label }} · {{ gb(m.size) }}{{ m.gpuRecommended ? ' · GPU' : '' }}{{ m.installed ? ' · downloaded' : '' }}</option></select></label><label>Device<select v-model="config.device" :disabled="!loaded || busy || downloading"><option value="auto">Auto (GPU first)</option><option value="gpu">GPU</option><option value="cpu">CPU</option></select></label><label>Unload after (min)<input v-model.number="config.idleMinutes" type="number" min="1" max="240" :disabled="!loaded || busy || downloading" /></label></div>
      <p v-if="engineLine" role="status">{{ engineLine }}</p>
      <p v-if="engine?.lastWarning">{{ engine.lastWarning }}</p>
      <p v-if="engine?.lastError && !engine.running">{{ engine.lastError }}</p>
      <template v-if="downloading"><progress :value="engine?.setup.done ?? 0" :max="engine?.setup.total || 1" /><p>{{ progress }}</p></template>
      <button v-else-if="engine && engine.supported && engine.missingBytes > 0" :disabled="!loaded || busy" @click="download">Download and enable ({{ gb(engine.missingBytes) }})</button>
    </template>
    <template v-else>
      <div class="qm-embedding-fields"><label>Installed model<input v-model="config.model" placeholder="Model name" :disabled="!loaded || busy" /></label><label>Port<input v-model.number="config.port" type="number" min="1" max="65535" :disabled="!loaded || busy" /></label><label>Model revision<input v-model="config.revision" :disabled="!loaded || busy" /></label></div>
      <p>Change the revision after replacing a model under the same name. This invalidates cached vectors.</p>
    </template>
    <label class="qm-embedding-check"><input v-model="config.enabled" type="checkbox" :disabled="!loaded || busy || downloading || (builtin && !config.enabled && (engine?.missingBytes ?? 0) > 0)" /> Enable local embeddings</label>
    <button :disabled="!loaded || busy || downloading" @click="save">Save retrieval settings</button>
    <p v-if="savedEnabled && status && (status.pending > 0 || status.indexerError)">{{ status.pending }} memories waiting for vectors.{{ status.indexerError ? ` ${status.indexerError}` : '' }}</p>
    <div class="qm-embedding-actions"><select v-model="scope" aria-label="Workspace to embed" :disabled="busy"><option v-for="s in vault.scopes" :key="s.scope" :value="s.scope">{{ s.name }}</option></select><button :disabled="busy || !savedEnabled" @click="indexBatch">Index next batch</button></div>
    <p v-if="notice" role="status">{{ notice }}</p>
  </section>
</template>
<style scoped>
.qm-embedding { display: grid; gap: 10px; }
h3 { font-size: 12px; font-weight: 600; text-transform: uppercase; letter-spacing: .05em; color: var(--qss-text-muted); margin: 0; }
p { font-size: 12px; color: var(--qss-text-secondary); margin: 0; line-height: 1.6; overflow-wrap: anywhere; }
label { display: grid; gap: 6px; font-size: 12px; color: var(--qss-text-secondary); }
.qm-embedding-check, .qm-embedding-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.qm-embedding-provider { justify-self: start; }
.qm-embedding-fields { display: grid; grid-template-columns: minmax(0, 2fr) minmax(70px, 1fr) minmax(70px, 1fr); gap: 8px; }
input:not([type=checkbox]), select, button { min-width: 0; padding: 6px 10px; border: 1px solid var(--qss-border); border-radius: 4px; background: var(--qss-bg); color: var(--qss-text); }
button { justify-self: start; }
progress { width: 100%; height: 6px; accent-color: var(--qss-accent); }
:disabled { opacity: .5; }
</style>
