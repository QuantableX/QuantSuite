<script setup lang="ts">
import type { MemoryDoc, MemoryQuality } from '#memory/types'
import { useVaultStore } from '#memory/stores/vault'
import { getInvoke } from '#memory/utils/invoke'
import { qualityFromDraft } from '#memory/utils/quality'

const props = defineProps<{ doc: MemoryDoc }>()
const vault = useVaultStore()
const basis = ref<MemoryQuality['basis']>('unverified')
const confidence = ref<string | number>('')
const sources = ref('')
const conflicts = ref('')
const replacement = ref('')
const revision = ref<string | null>(null)
const busy = ref(false)
const notice = ref('')
const candidates = computed(() => vault.memories.filter(m => m.id !== props.doc.meta.id && (m.scope === props.doc.meta.scope || m.scope === 'general')))
function reset() {
  const q = props.doc.meta.quality
  basis.value = q?.basis ?? 'unverified'
  confidence.value = q?.confidence == null ? '' : String(q.confidence)
  sources.value = q?.sources.join('\n') ?? ''
  conflicts.value = q?.conflictsWith.join('\n') ?? ''
  replacement.value = q?.supersededBy ?? ''
  revision.value = props.doc.revision ?? null
  notice.value = ''
}
watch(() => props.doc.meta.id, reset, { immediate: true })
async function reload() {
  try { await vault.refreshActive(); reset() }
  catch (e) { notice.value = String(e) }
}
async function save(reviewed: boolean) {
  const id = props.doc.meta.id
  busy.value = true; notice.value = ''
  try {
    if (!revision.value) throw new Error('Reload this memory before reviewing it.')
    const quality = qualityFromDraft({ basis: basis.value, confidence: confidence.value, sources: sources.value,
      conflicts: conflicts.value, replacement: replacement.value, lastVerified: props.doc.meta.quality?.lastVerified ?? null })
    const invoke = await getInvoke()
    await invoke(`plugin:memory|${reviewed ? 'review_memory_quality' : 'set_memory_quality'}`, { identifier: id, quality, revision: revision.value })
    if (props.doc.meta.id !== id) return
    await vault.refreshActive()
    reset()
    notice.value = reviewed ? 'Review recorded.' : 'Saved for review.'
  } catch (error) { if (props.doc.meta.id === id) notice.value = String(error) }
  finally { busy.value = false }
}
</script>

<template>
  <section class="qm-quality" aria-label="Memory provenance">
    <h3>Provenance <span>{{ doc.meta.quality?.reviewed ? 'Reviewed' : 'Pending review' }}</span></h3>
    <label>Basis<select v-model="basis" aria-label="Basis"><option value="unverified">Unverified</option><option value="observed">Observed</option><option value="inferred">Inferred</option></select></label>
    <label>Confidence (0–1)<input v-model="confidence" type="number" min="0" max="1" step="0.05" placeholder="Not assessed" /></label>
    <label>Sources<textarea v-model="sources" rows="3" placeholder="One source or evidence reference per line" /></label>
    <label>Replaced by<select v-model="replacement" aria-label="Replaced by"><option value="">Current memory</option><option v-for="m in candidates" :key="m.id" :value="m.id">{{ m.title }}</option></select></label>
    <label>Conflicting memory IDs<textarea v-model="conflicts" rows="2" placeholder="One ID per line" /></label>
    <p v-if="doc.meta.quality?.lastVerified">Last verified: {{ new Date(doc.meta.quality.lastVerified).toLocaleString() }}</p>
    <div class="qm-quality-actions"><button :disabled="busy" @click="save(false)">Save</button><button :disabled="busy" @click="save(true)">Mark reviewed</button><button :disabled="busy" @click="reload">Reload</button></div>
    <p v-if="notice" role="status">{{ notice }}</p>
  </section>
</template>

<style scoped>
.qm-quality { display: grid; gap: 10px; padding: 16px; border-bottom: 1px solid var(--qm-border); }
h3 { display: flex; flex-wrap: wrap; gap: 8px; font-size: 12px; margin: 0; }
h3 span, p { color: var(--qm-text-muted); font-size: 11px; font-weight: normal; margin: 0; overflow-wrap: anywhere; }
label { display: grid; gap: 5px; font-size: 11px; color: var(--qm-text-secondary); }
input, select, textarea { width: 100%; min-width: 0; border: 1px solid var(--qm-border); background: var(--qm-bg); color: var(--qm-text); border-radius: 4px; padding: 6px; font: inherit; }
textarea { resize: vertical; }
.qm-quality-actions { display: flex; flex-wrap: wrap; gap: 6px; }
button { border: 1px solid var(--qm-border); background: var(--qm-bg-raised); color: var(--qm-text); border-radius: 4px; padding: 5px 7px; font-size: 11px; }
button:disabled { opacity: .5; }
</style>
