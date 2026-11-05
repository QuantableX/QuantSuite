<script setup lang="ts">
import { getInvoke } from '#memory/utils/invoke'
import { useVaultStore } from '#memory/stores/vault'
import type { MemoryQuality } from '#memory/types'
const vault = useVaultStore()
const query = ref('')
const scope = ref(vault.scope)
const includeGeneral = ref(false)
const reviewedOnly = ref(false)
const busy = ref(false)
const error = ref('')
interface Context { mode: string; device?: string; embeddingModel?: string; elapsedMs: number; excerptChars: number; warnings: string[]; sources: { id: string; title: string; citation: string; scope: string; path: string; excerpt: string; reasons: string[]; quality: MemoryQuality }[] }
const result = ref<Context | null>(null)
let request = 0
watch([query, scope, includeGeneral, reviewedOnly], () => { request++; result.value = null; error.value = ''; busy.value = false })
watch(() => vault.scope, value => { scope.value = value })
onBeforeUnmount(() => { request++ })
async function recall() {
  const ticket = ++request; busy.value = true; error.value = ''
  try {
    const invoke = await getInvoke()
    const data = await invoke('plugin:memory|memory_context', { request: { query: query.value, scope: scope.value, includeGeneral: includeGeneral.value, reviewedOnly: reviewedOnly.value, maxChars: 8000, limit: 8 } })
    if (ticket === request) result.value = data
  } catch (e) { if (ticket === request) error.value = String(e) }
  finally { if (ticket === request) busy.value = false }
}
</script>
<template>
  <section class="qm-recall" aria-label="Scoped memory recall">
    <form @submit.prevent="recall"><label>Question<input v-model="query" placeholder="What should this task remember?" required maxlength="2000" /></label>
      <div class="qm-recall-options"><select v-model="scope" aria-label="Recall workspace"><option v-for="s in vault.scopes" :key="s.scope" :value="s.scope">{{ s.name }}</option></select>
        <label><input v-model="includeGeneral" type="checkbox" :disabled="scope === 'general'" /> Include General</label>
        <label><input v-model="reviewedOnly" type="checkbox" /> Reviewed only</label>
        <button :disabled="busy || !query.trim()">{{ busy ? 'Retrieving…' : 'Recall' }}</button></div>
    </form>
    <p v-if="error" role="alert">{{ error }}</p>
    <template v-if="result"><p>{{ result.mode }}<template v-if="result.device"> · {{ result.device }} · {{ result.embeddingModel }}</template> · {{ result.elapsedMs }} ms · {{ result.excerptChars }} excerpt characters</p>
      <p v-for="warning in result.warnings" :key="warning" role="status">{{ warning }}</p>
      <article v-for="source in result.sources" :key="source.id"><NuxtLink :to="`/memory/m/${encodeURIComponent(source.id)}`">{{ source.title }}</NuxtLink>
        <p>{{ vault.scopeName(source.scope) }} / {{ source.path }} · {{ source.quality.reviewed ? 'Reviewed' : 'Unverified claim' }}</p>
        <blockquote>{{ source.excerpt }}</blockquote><code>{{ source.citation }}</code><p>{{ source.reasons.join(' · ') }}</p>
      </article>
    </template>
  </section>
</template>
<style scoped>
.qm-recall { flex: 1 1 0; min-height: 0; overflow-y: auto; font-size: var(--qss-text-label); container-type: inline-size; }
/* The question and how to ask it share one row. */
form { display: flex; flex-wrap: wrap; align-items: center; gap: var(--qss-gap-tight) var(--qss-gap); }
form > label { flex: 1 1 320px; min-width: 0; display: flex; align-items: center; gap: var(--qss-gap-tight); }
form > label input { flex: 1; }
input:not([type=checkbox]), select { min-width: 0; height: var(--qss-ctl-h); border: 1px solid var(--qm-border-subtle); background: var(--qm-bg-raised); color: var(--qm-text); border-radius: 8px; padding: 0 8px; }
select { max-width: 200px; }
.qm-recall-options { display: flex; flex-wrap: wrap; gap: var(--qss-gap-tight) var(--qss-gap); align-items: center; }
.qm-recall-options label { display: flex; gap: 6px; align-items: center; white-space: nowrap; }
button { height: var(--qss-ctl-h); border: 1px solid var(--qm-border-subtle); border-radius: 8px; background: var(--qm-bg-raised); color: var(--qm-text); padding: 0 12px; cursor: pointer; }
button:hover:not(:disabled) { background: var(--qm-bg-hover); }
button:disabled { opacity: .5; cursor: default; }
article { border-top: 1px solid var(--qm-border-subtle); padding: var(--qss-gap-tight) 0; }
article > a { color: var(--qm-text); font-size: var(--qss-text-section); font-weight: 600; }
p { margin: 0; color: var(--qm-text-muted); line-height: 1.5; }
.qm-recall > p { margin: var(--qss-gap-tight) 0; font-size: var(--qss-text-meta); font-variant-numeric: tabular-nums; }
article > p { margin: 2px 0; font-size: var(--qss-text-meta); }
blockquote { max-width: 80ch; white-space: pre-wrap; overflow-wrap: anywhere; margin: var(--qss-gap-tight) 0; padding: 0 0 0 var(--qss-gap); border-left: 2px solid var(--qm-border); color: var(--qm-text-secondary); line-height: 1.5; }
code { color: var(--qm-text-muted); font-family: var(--qss-font-mono); font-size: 11px; }
</style>
