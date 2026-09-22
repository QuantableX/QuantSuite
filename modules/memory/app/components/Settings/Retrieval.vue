<script setup lang="ts">
import { getInvoke } from '#memory/utils/invoke'
import { useVaultStore } from '#memory/stores/vault'
const vault = useVaultStore()
const config = reactive({ enabled: false, port: 11434, model: '', revision: '1' })
const loaded = ref(false)
const busy = ref(false)
const savedEnabled = ref(false)
const notice = ref('')
const scope = ref(vault.createScope)
onMounted(async () => {
  try { const invoke = await getInvoke(); Object.assign(config, await invoke('plugin:memory|get_embedding_config')); savedEnabled.value = config.enabled; loaded.value = true }
  catch (e) { notice.value = String(e) }
})
async function save() {
  busy.value = true; notice.value = ''
  try {
    const invoke = await getInvoke()
    await invoke('plugin:memory|set_embedding_config', { config: { ...config, port: Number(config.port) } })
    savedEnabled.value = config.enabled
    notice.value = config.enabled ? 'Local embeddings enabled. Index a workspace below.' : 'Embeddings disabled; lexical recall remains available.'
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
</script>
<template>
  <section class="qm-embedding" aria-label="Local memory embeddings">
    <h3>Local semantic recall</h3>
    <p>Optional Ollama embeddings at 127.0.0.1. Uses an already-installed model; no downloads or remote endpoints. Enabling this sends selected memory text and recall queries to that local service.</p>
    <label class="qm-embedding-check"><input v-model="config.enabled" type="checkbox" :disabled="!loaded || busy" /> Enable local embeddings</label>
    <div class="qm-embedding-fields"><label>Installed model<input v-model="config.model" placeholder="Model name" :disabled="!loaded || busy" /></label><label>Port<input v-model.number="config.port" type="number" min="1" max="65535" :disabled="!loaded || busy" /></label><label>Model revision<input v-model="config.revision" :disabled="!loaded || busy" /></label></div>
    <p>Change the revision after replacing a model under the same name. This invalidates cached vectors.</p>
    <button :disabled="!loaded || busy" @click="save">Save retrieval settings</button>
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
.qm-embedding-fields { display: grid; grid-template-columns: minmax(0, 2fr) minmax(70px, 1fr) minmax(70px, 1fr); gap: 8px; }
input:not([type=checkbox]), select, button { min-width: 0; padding: 6px 10px; border: 1px solid var(--qss-border); border-radius: 4px; background: var(--qss-bg); color: var(--qss-text); }
button { justify-self: start; }
:disabled { opacity: .5; }
</style>
