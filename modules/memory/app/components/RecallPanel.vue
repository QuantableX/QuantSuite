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
interface Context { mode: string; elapsedMs: number; excerptChars: number; warnings: string[]; sources: { id: string; title: string; citation: string; scope: string; path: string; excerpt: string; reasons: string[]; quality: MemoryQuality }[] }
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
    <template v-if="result"><p>{{ result.mode }} · {{ result.elapsedMs }} ms · {{ result.excerptChars }} excerpt characters</p>
      <p v-for="warning in result.warnings" :key="warning" role="status">{{ warning }}</p>
      <article v-for="source in result.sources" :key="source.id"><NuxtLink :to="`/memory/m/${encodeURIComponent(source.id)}`">{{ source.title }}</NuxtLink>
        <p>{{ vault.scopeName(source.scope) }} / {{ source.path }} · {{ source.quality.reviewed ? 'Reviewed' : 'Unverified claim' }}</p>
        <blockquote>{{ source.excerpt }}</blockquote><code>{{ source.citation }}</code><p>{{ source.reasons.join(' · ') }}</p>
      </article>
    </template>
  </section>
</template>
<style scoped>
.qm-recall { padding: 18px 0; font-size: 12px; }
form > label { display: grid; gap: 8px; }
input:not([type=checkbox]), select { min-width: 0; border: 1px solid var(--qm-border); background: var(--qm-bg); color: var(--qm-text); border-radius: 4px; padding: 8px; }
.qm-recall-options { display: flex; flex-wrap: wrap; gap: 12px; align-items: center; margin-top: 12px; }
.qm-recall-options label { display: flex; gap: 6px; align-items: center; }
button { border: 1px solid var(--qm-border); border-radius: 4px; background: var(--qm-bg-raised); color: var(--qm-text); padding: 8px 14px; }
article { border-bottom: 1px solid var(--qm-border); padding: 18px 0; }
article > a { color: var(--qm-text); font-size: 14px; }
p { color: var(--qm-text-muted); line-height: 1.6; }
.qm-recall > p { margin: 12px 0; }
article > p { margin: 6px 0; }
blockquote { white-space: pre-wrap; overflow-wrap: anywhere; margin: 12px 0; padding: 12px; border-left: 2px solid var(--qm-border); color: var(--qm-text-secondary); line-height: 1.6; }
code { color: var(--qm-text-muted); font-size: 11px; }
</style>
