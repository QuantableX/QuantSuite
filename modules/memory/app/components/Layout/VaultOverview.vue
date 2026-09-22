<script setup lang="ts">
import { ArrowUpRight, Clock3, Link2Off, Network, ShieldCheck, Star } from 'lucide-vue-next'
import { useVaultStore } from '#memory/stores/vault'
import { useAppStore } from '#memory/stores/app'
import { timeAgo } from '#memory/utils/format'

const vault = useVaultStore()
const app = useAppStore()
const router = useRouter()
const recent = computed(() => [...vault.scopeMemories].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)).slice(0, 5))
const starred = computed(() => vault.scopeMemories.filter(m => app.starred.includes(m.id)).slice(0, 4))
const unlinked = computed(() => vault.scopeMemories.filter(m => !m.incomingLinks && !m.outgoingLinks).length)
const unreviewed = computed(() => vault.scopeMemories.filter(m => !m.quality?.reviewed && !m.quality?.supersededBy).length)
function browse(view: string) {
  vault.query = ''; vault.filterTag = null; vault.filterKind = null
  void router.push({ path: '/memory', query: { view } })
}
</script>

<template>
  <div class="qm-vault-context" aria-label="Vault sidebar">
    <section class="qm-vault-summary"><h2>{{ vault.scope === 'general' ? 'Your knowledge' : vault.scopeName(vault.scope) }}</h2><div><span><strong>{{ vault.scopeMemories.length }}</strong>memories</span><span><strong>{{ vault.stats?.resolvedLinks ?? '—' }}</strong>connections</span></div></section>
    <section class="qm-vault-attention"><h2>Needs attention</h2>
      <button @click="browse('review')"><ShieldCheck :size="15" /><span>Not reviewed</span><small>{{ unreviewed }}</small><ArrowUpRight :size="13" /></button>
      <button @click="browse('unlinked')"><Network :size="15" /><span>Unlinked memories</span><small>{{ unlinked }}</small><ArrowUpRight :size="13" /></button>
      <button @click="browse('broken')"><Link2Off :size="15" /><span>Missing targets</span><small>{{ vault.unresolved.length }}</small><ArrowUpRight :size="13" /></button>
    </section>
    <section v-if="starred.length" class="qm-vault-starred"><h2><Star :size="14" /> Starred</h2><NuxtLink v-for="memory in starred" :key="memory.id" :to="'/memory/m/' + encodeURIComponent(memory.id)"><span>{{ memory.title }}</span><ArrowUpRight :size="12" /></NuxtLink><button class="qm-context-more" @click="browse('starred')">View all starred</button></section>
    <section class="qm-vault-recent"><h2><Clock3 :size="14" /> Recently updated</h2><NuxtLink v-for="memory in recent" :key="memory.id" :to="'/memory/m/' + encodeURIComponent(memory.id)"><span>{{ memory.title }}</span><small>{{ timeAgo(memory.updatedAt) }} · {{ vault.scopeName(memory.scope) }}</small></NuxtLink><p v-if="!recent.length">New memories will appear here.</p></section>
  </div>
</template>

<style scoped>
.qm-vault-context { padding: 8px 4px; color: var(--qm-text-secondary); font-size: 12px; }.qm-vault-context section + section { border-top: 1px solid var(--qm-border-subtle); padding-top: 19px; margin-top: 22px; }
.qm-vault-context h2 { display: flex; align-items: center; gap: 7px; margin: 0 0 14px; color: var(--qm-text); font-size: 13px; line-height: 1.5; font-weight: 600; overflow-wrap: anywhere; }.qm-vault-context h2 svg { color: var(--qm-text-muted); }
.qm-vault-summary > div { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }.qm-vault-summary > div > span { color: var(--qm-text-muted); font-size: 11px; }.qm-vault-summary strong { display: block; margin-bottom: 5px; color: var(--qm-text); font-size: 25px; font-weight: 500; letter-spacing: -.7px; }
.qm-vault-context button, .qm-vault-context a { cursor: pointer; }.qm-vault-context button:focus-visible, .qm-vault-context a:focus-visible { outline: 2px solid var(--qm-link); outline-offset: 2px; }
.qm-vault-attention button { display: flex; align-items: center; gap: 6px; width: 100%; padding: 10px 0; border: 0; border-radius: 5px; color: var(--qm-text-secondary); background: none; font-size: 11px; text-align: left; }.qm-vault-attention button:hover { background: var(--qm-bg-hover); color: var(--qm-text); }.qm-vault-attention button > span { flex: 1; }.qm-vault-attention svg { flex-shrink: 0; color: #b2a3d8; }.qm-vault-attention small { min-width: 16px; text-align: center; font-size: 11px; color: var(--qm-text-muted); }.qm-vault-attention button > svg:last-child { color: var(--qm-text-muted); }
.qm-vault-context a { display: block; padding: 9px 3px; border-radius: 5px; color: var(--qm-text-secondary); text-decoration: none; }.qm-vault-context a:hover { background: var(--qm-bg-hover); color: var(--qm-text); }.qm-vault-context a span { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }.qm-vault-context a small { display: block; margin-top: 5px; color: var(--qm-text-muted); font-size: 10px; }.qm-vault-starred a { display: flex; align-items: center; gap: 6px; }.qm-vault-starred a span { flex: 1; }.qm-vault-starred a svg { flex-shrink: 0; }
.qm-context-more { border: 0; padding: 9px 3px 0; background: none; color: var(--qm-link); font-size: 11px; }.qm-vault-recent p { color: var(--qm-text-muted); font-size: 12px; line-height: 1.6; }
</style>
