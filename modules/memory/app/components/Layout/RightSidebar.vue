<script setup lang="ts">
/**
 * Context panel. With a memory open: its metadata, outgoing links, backlinks
 * and connection suggestions — the "how does this fit the brain" view. On
 * the Vault: review shortcuts, recent notes and starred memories.
 */
import { useVaultStore } from '#memory/stores/vault'
import { useAppStore } from '#memory/stores/app'
import { timeAgo } from '#memory/utils/format'
import type { GraphEdge, GraphNode } from '#memory/types'
import QualityEditor from '#memory/components/QualityEditor.vue'

const vault = useVaultStore()
const app = useAppStore()
const route = useRoute()
const router = useRouter()

/** Depth-1 neighborhood of the open memory — built from the links the doc
 *  already carries, no extra backend query. */
const localGraph = computed<{ nodes: GraphNode[]; edges: GraphEdge[] } | null>(() => {
  const doc = vault.activeDoc
  if (!doc) return null
  const m = doc.meta
  const nodes = new Map<string, GraphNode>()
  const edges: GraphEdge[] = []
  const push = (n: GraphNode) => {
    if (!nodes.has(n.id)) nodes.set(n.id, n)
  }
  push({
    id: m.id,
    scope: m.scope,
    title: m.title,
    kind: m.kind,
    tags: m.tags,
    incoming: m.incomingLinks,
    outgoing: m.outgoingLinks,
    missing: false,
  })
  for (const link of doc.outgoing) {
    const id = link.targetId ?? `ghost:${m.scope}:${link.target.toLowerCase()}`
    push({
      id,
      scope: m.scope,
      title: link.resolvedTitle ?? link.target,
      kind: null,
      tags: [],
      incoming: 1,
      outgoing: 0,
      missing: !link.targetId,
    })
    edges.push({ source: m.id, target: id, count: 1 })
  }
  for (const b of doc.backlinks) {
    push({
      id: b.id,
      scope: m.scope,
      title: b.title,
      kind: null,
      tags: [],
      incoming: 0,
      outgoing: 1,
      missing: false,
    })
    edges.push({ source: b.id, target: m.id, count: b.count })
  }
  return { nodes: [...nodes.values()], edges }
})

function openGraphNode(id: string) {
  if (id.startsWith('ghost:')) {
    const title = localGraph.value?.nodes.find((n) => n.id === id)?.title
    if (title) app.startCreate(vault.activeDoc!.meta.scope, title)
    return
  }
  void router.push(`/memory/m/${id}`)
}

const docOpen = computed(() => route.path.startsWith('/memory/m/') && !!vault.activeDoc)
const meta = computed(() => vault.activeDoc?.meta ?? null)

const tagInput = ref('')

async function addTag() {
  const m = meta.value
  const tag = tagInput.value.trim().replace(/^#/, '')
  if (!m || !tag) return
  tagInput.value = ''
  await vault.setMeta(m.id, { tags: [...new Set([...m.tags, tag])] })
}

async function removeTag(tag: string) {
  const m = meta.value
  if (!m) return
  await vault.setMeta(m.id, { tags: m.tags.filter((t) => t !== tag) })
}

async function setKind(event: Event) {
  const m = meta.value
  if (!m) return
  await vault.setMeta(m.id, { kind: (event.target as HTMLInputElement).value.trim() })
}

function openLink(link: { targetId: string | null; target: string }) {
  if (link.targetId) {
    void router.push(`/memory/m/${link.targetId}`)
  } else if (vault.activeDoc) {
    app.startCreate(vault.activeDoc.meta.scope, link.target)
  }
}

</script>

<template>
  <div class="qm-side">
    <!-- ── Memory context ────────────────────────────────────────────── -->
    <template v-if="docOpen && meta">
      <QualityEditor v-if="vault.activeDoc" :key="meta.id" :doc="vault.activeDoc" />
      <section class="qm-panel">
        <h3 class="qm-panel-title">Metadata</h3>
        <div class="qm-meta-row">
          <span class="qm-meta-label">scope</span>
          <span class="qm-meta-value">{{ vault.scopeName(meta.scope) }}</span>
        </div>
        <div class="qm-meta-row">
          <span class="qm-meta-label">type</span>
          <input
            class="qm-meta-input"
            :value="meta.kind ?? ''"
            list="qm-kinds"
            placeholder="—"
            @change="setKind"
          />
          <datalist id="qm-kinds">
            <option v-for="k in ['user', 'project', 'reference', 'decision', 'insight']" :key="k" :value="k" />
          </datalist>
        </div>
        <div class="qm-meta-row">
          <span class="qm-meta-label">author</span>
          <span class="qm-meta-value">{{ meta.author ?? '—' }}</span>
        </div>
        <div class="qm-meta-row">
          <span class="qm-meta-label">updated</span>
          <span class="qm-meta-value">{{ timeAgo(meta.updatedAt) }}</span>
        </div>
        <div class="qm-meta-row">
          <span class="qm-meta-label">words</span>
          <span class="qm-meta-value">{{ meta.wordCount }}</span>
        </div>
        <div class="qm-tags">
          <button v-for="tag in meta.tags" :key="tag" class="qm-tag" :title="`Remove #${tag}`" @click="removeTag(tag)">
            #{{ tag }} ✕
          </button>
          <input
            v-model="tagInput"
            class="qm-tag-input"
            placeholder="+ tag"
            @keydown.enter.prevent="addTag"
          />
        </div>
      </section>

      <section v-if="localGraph && localGraph.nodes.length > 1" class="qm-panel">
        <h3 class="qm-panel-title">Local graph</h3>
        <div class="qm-local-graph">
          <MemoryGraphCanvas
            :nodes="localGraph.nodes"
            :edges="localGraph.edges"
            :focus-id="meta.id"
            :label-threshold="0"
            @open="openGraphNode"
          />
        </div>
      </section>

      <section class="qm-panel">
        <h3 class="qm-panel-title">Links ({{ vault.activeDoc!.outgoing.length }})</h3>
        <button
          v-for="(link, i) in vault.activeDoc!.outgoing"
          :key="i"
          class="qm-link-row"
          :class="{ 'is-broken': !link.targetId }"
          @click="openLink(link)"
        >
          {{ link.resolvedTitle ?? link.target }}
        </button>
        <p v-if="!vault.activeDoc!.outgoing.length" class="qm-panel-empty">No outgoing links.</p>
      </section>

      <section class="qm-panel">
        <h3 class="qm-panel-title">Backlinks ({{ vault.activeDoc!.backlinks.length }})</h3>
        <button
          v-for="b in vault.activeDoc!.backlinks"
          :key="b.id"
          class="qm-link-row"
          @click="router.push(`/memory/m/${b.id}`)"
        >
          {{ b.title }} <span v-if="b.count > 1" class="qm-link-count">×{{ b.count }}</span>
        </button>
        <p v-if="!vault.activeDoc!.backlinks.length" class="qm-panel-empty">Nothing links here yet.</p>
      </section>

      <section v-if="vault.suggestions.length" class="qm-panel">
        <h3 class="qm-panel-title">Might connect</h3>
        <button
          v-for="s in vault.suggestions"
          :key="s.id"
          class="qm-link-row"
          :title="`Shared: ${s.sharedTerms.join(', ')}`"
          @click="router.push(`/memory/m/${s.id}`)"
        >
          {{ s.title }}
        </button>
      </section>
    </template>

    <MemoryLayoutVaultOverview v-else />
  </div>
</template>

<style scoped>
.qm-side {
  height: 100%;
  min-height: 0;
  overflow-y: auto;
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.qm-panel {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.qm-panel-title {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--qm-text-muted);
  margin-bottom: 2px;
}

.qm-panel-empty {
  font-size: 12px;
  color: var(--qm-text-muted);
}

.qm-meta-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
}

.qm-meta-label {
  width: 62px;
  flex-shrink: 0;
  color: var(--qm-text-muted);
}

.qm-meta-value {
  color: var(--qm-text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qm-meta-input {
  flex: 1;
  min-width: 0;
  padding: 2px 6px;
  border: 1px solid transparent;
  border-radius: 4px;
  background: transparent;
  color: var(--qm-text-secondary);
  outline: none;
}
.qm-meta-input:hover,
.qm-meta-input:focus {
  border-color: var(--qm-border-subtle);
  background: var(--qm-bg);
}

.qm-tags {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 4px;
  margin-top: 4px;
}

.qm-tag-input {
  width: 64px;
  padding: 1px 7px;
  border: 1px dashed var(--qm-border-subtle);
  border-radius: 999px;
  background: transparent;
  color: var(--qm-text-secondary);
  font-size: 11px;
  outline: none;
}

.qm-link-row {
  display: block;
  width: 100%;
  padding: 4px 7px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qm-link);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.qm-link-row:hover {
  background: var(--qm-bg-hover);
}
.qm-link-row.is-broken {
  color: var(--qm-link-broken);
}

.qm-link-count {
  color: var(--qm-text-muted);
  font-size: 11px;
}

.qm-local-graph {
  height: 190px;
  border: 1px solid var(--qm-border-subtle);
  border-radius: var(--qm-radius);
  background: var(--qm-bg);
  overflow: hidden;
}
</style>
