<script setup lang="ts">
import { getInvoke } from '#memory/utils/invoke'
import { useVaultStore } from '#memory/stores/vault'
const vault = useVaultStore()
interface Issue { id: string; title: string; scope: string; reasons: string[]; relatedIds: string[] }
const issues = ref<Issue[]>([])
const error = ref('')
const busy = ref(false)
let request = 0
async function load() {
  const ticket = ++request; busy.value = true; error.value = ''
  try {
    const invoke = await getInvoke()
    const result = await invoke('plugin:memory|get_memory_review_queue', { scope: vault.viewScope })
    if (ticket === request) issues.value = result ?? []
  } catch (e) { if (ticket === request) { error.value = String(e); issues.value = [] } }
  finally { if (ticket === request) busy.value = false }
}
watch(() => vault.viewScope, load, { immediate: true })
onBeforeUnmount(() => { request++ })
</script>

<template>
  <section class="qm-review" aria-label="Memory review queue">
    <div class="qm-review-head"><span>{{ issues.length }} memories to review</span><button :disabled="busy" @click="load">Refresh</button></div>
    <p v-if="error" role="alert">{{ error }}</p><p v-else-if="busy" role="status">Loading review queue…</p>
    <article v-for="issue in issues" v-else :key="issue.id">
      <NuxtLink :to="`/memory/m/${encodeURIComponent(issue.id)}`">{{ issue.title }}</NuxtLink>
      <p>{{ vault.scopeName(issue.scope) }} · {{ issue.reasons.join(' · ') }}</p>
      <div v-if="issue.relatedIds.length" class="qm-review-related">Related: <NuxtLink v-for="id in issue.relatedIds" :key="id" :to="`/memory/m/${encodeURIComponent(id)}`">{{ vault.memories.find(m => m.id === id)?.title ?? id }}</NuxtLink></div>
    </article>
    <p v-if="!busy && !error && !issues.length">No pending issues.</p>
  </section>
</template>

<style scoped>
.qm-review { padding: 18px 0; font-size: 12px; }
.qm-review-head { display: flex; justify-content: space-between; align-items: center; color: var(--qm-text-muted); }
button { padding: 6px 10px; border: 1px solid var(--qm-border); border-radius: 4px; background: var(--qm-bg); color: var(--qm-text); }
article { padding: 18px 0; border-bottom: 1px solid var(--qm-border); }
article > a { font-size: 14px; color: var(--qm-text); }
p, .qm-review-related { color: var(--qm-text-muted); line-height: 1.6; }
.qm-review-related { display: flex; flex-wrap: wrap; gap: 8px; }
a { color: var(--qm-text-secondary); }
</style>
