<script setup lang="ts">
/**
 * One memory: Monaco (markdown) and/or the rendered preview, Obsidian-style.
 *
 * The editor buffer is local state; saves are debounced through the store
 * (`update_memory` keeps the frontmatter). Wikilinks complete on `[[` and
 * are clickable in the preview — a broken one offers to create its target,
 * which is how a second brain grows.
 */
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { onBeforeRouteLeave, onBeforeRouteUpdate } from 'vue-router'
import { useAppStore } from '#memory/stores/app'
import { useVaultStore } from '#memory/stores/vault'
import {
  ensureWikilinkCompletion,
  renderWikilinks,
  wikilinkFromClick,
} from '#memory/composables/useWikilinks'

definePageMeta({ layout: 'memory' })

const app = useAppStore()
const vault = useVaultStore()
const route = useRoute()
const router = useRouter()

const body = ref('')
const dirty = ref(false)
const missing = ref(false)
const renaming = ref(false)
const renameValue = ref('')
const loadedId = ref<string | null>(null)
const saveError = ref('')
const loadError = ref('')
const saving = ref(false)
let loadRequest = 0

const meta = computed(() => vault.activeDoc?.meta ?? null)
const identifier = computed(() => String(route.params.id ?? ''))

/** Monaco model identity — the real file path (per the memory's scope
 *  vault), so QuantCode/QuantCanvas opening the same file would share one
 *  buffer. */
const modelPath = computed(() => {
  const m = meta.value
  if (!m) return `qm:/${identifier.value}`
  const dir = vault.scopes.find((s) => s.scope === m.scope)?.vaultDir
  return dir ? `${dir}/${m.relPath}` : `qm:/${m.scope}/${m.relPath}`
})

async function load(ident: string) {
  if (!ident) return
  const request = ++loadRequest
  if (!await flushSave()) return
  loadError.value = ''
  try {
    const doc = await vault.open(ident)
    if (request !== loadRequest) return
    missing.value = !doc
    if (doc) {
      loadedId.value = doc.meta.id
      body.value = doc.body
      dirty.value = false
    }
  } catch (error) { if (request === loadRequest) loadError.value = String(error) }
}

watch(identifier, (ident) => void load(ident), { immediate: false })
onMounted(() => {
  if (inActiveKeepAliveTree()) void load(identifier.value)
  void ensureWikilinkCompletion(vault)
})
onActivated(() => {
  // Agents write while the user is elsewhere — refresh unless mid-edit.
  if (!dirty.value) void load(identifier.value)
})

// ── Saving ───────────────────────────────────────────────────────────────

let saveTimer: ReturnType<typeof setTimeout> | null = null

function onEdit(value: string) {
  body.value = value
  dirty.value = true
  if (saveTimer) clearTimeout(saveTimer)
  saveTimer = setTimeout(() => void doSave(), 900)
}

let saveQueue: Promise<boolean> = Promise.resolve(true)
function doSave(): Promise<boolean> {
  saveQueue = saveQueue.then(async () => {
    if (!dirty.value || !loadedId.value) return true
    const value = body.value
    const id = loadedId.value
    saving.value = true
    saveError.value = ''
    try {
      const saved = await vault.saveBody(id, value)
      if (!saved) throw new Error('Memory was not saved.')
      if (loadedId.value === id && body.value === value) dirty.value = false
      return !dirty.value
    } catch (error) { saveError.value = String(error); return false }
    finally { saving.value = false }
  })
  return saveQueue
}

async function flushSave() {
  if (saveTimer) {
    clearTimeout(saveTimer)
    saveTimer = null
  }
  return doSave()
}

onBeforeRouteLeave(() => flushSave())
onBeforeRouteUpdate(() => flushSave())

onDeactivated(() => void flushSave())
onBeforeUnmount(() => void flushSave())

// ── Preview ──────────────────────────────────────────────────────────────

const previewSource = computed(() =>
  renderWikilinks(body.value, (target) => !!vault.byIdentifier(target, meta.value?.scope))
)

async function onPreviewClick(event: MouseEvent) {
  const link = wikilinkFromClick(event)
  if (!link) return
  event.preventDefault()
  const existing = vault.byIdentifier(link.target, meta.value?.scope)
  if (existing) {
    void router.push(`/memory/m/${existing.id}`)
    return
  }
  app.startCreate(meta.value?.scope ?? vault.createScope, link.target)
}

// ── Title / delete ───────────────────────────────────────────────────────

function startRename() {
  if (!meta.value) return
  renameValue.value = meta.value.title
  renaming.value = true
}

async function submitRename() {
  const m = meta.value
  const title = renameValue.value.trim()
  renaming.value = false
  if (!m || !title || title === m.title) return
  if (!await flushSave()) return
  const renamed = await vault.rename(m.id, title)
  if (renamed) {
    body.value = vault.activeDoc?.body ?? body.value
    void router.replace(`/memory/m/${renamed.id}`)
  }
}

async function removeMemory() {
  const m = meta.value
  if (!m) return
  if (!confirm(`Move "${m.title}" to the vault trash?`)) return
  if (!await flushSave()) return
  await vault.remove(m.id)
  void router.push('/memory')
}
</script>

<template>
  <div class="qm-editor-page">
    <p v-if="loadError" class="qm-editor-error" role="alert">{{ loadError }} <button @click="load(identifier)">Retry</button></p>
    <p v-if="saveError" class="qm-editor-error" role="alert">Not saved: {{ saveError }} <button @click="flushSave">Retry save</button></p>
    <div v-if="missing" class="qm-missing">
      <p>No memory matches "{{ identifier }}".</p>
      <button class="qm-btn" @click="router.push('/memory')">Back to the vault</button>
    </div>

    <template v-else-if="meta">
      <div class="qm-editor-bar">
        <form v-if="renaming" class="qm-rename" @submit.prevent="submitRename">
          <input
            v-model="renameValue"
            class="qm-rename-input"
            @keydown.esc="renaming = false"
            @blur="submitRename"
          />
        </form>
        <button v-else class="qm-title" title="Rename" @click="startRename">
          {{ meta.title }}
        </button>

        <span class="qm-dirty" role="status">{{ saving ? 'Saving…' : dirty ? 'Unsaved' : 'Saved' }}</span>

        <div class="qm-bar-spacer" />

        <div class="qm-view-toggle">
          <button
            v-for="mode in ['edit', 'split', 'preview'] as const"
            :key="mode"
            class="qm-view-btn"
            :class="{ 'is-active': app.viewMode === mode }"
            @click="app.setViewMode(mode)"
          >
            {{ mode }}
          </button>
        </div>

        <button class="qm-icon-btn" title="Move to trash" @click="removeMemory">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" aria-hidden="true">
            <path d="M3 6h18M8 6V4h8v2M6 6l1 14h10l1-14M10 11v6M14 11v6" />
          </svg>
        </button>
      </div>

      <div class="qm-editor-split" :data-mode="app.viewMode">
        <div v-show="app.viewMode !== 'preview'" class="qm-pane">
          <QCodeEditor
            :model-value="body"
            :path="modelPath"
            language="markdown"
            :minimap="false"
            word-wrap
            @update:model-value="onEdit"
            @save="() => void flushSave()"
          />
        </div>
        <div
          v-show="app.viewMode !== 'edit'"
          class="qm-pane qm-pane-preview"
          @click="onPreviewClick"
        >
          <QMarkdownPreview :source="previewSource" />
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.qm-editor-error { padding: 10px 16px; color: var(--qss-error); font-size: 12px; }
.qm-editor-error button { color: var(--qm-text); text-decoration: underline; }
.qm-editor-page {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.qm-missing {
  margin: auto;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  color: var(--qm-text-secondary);
}

.qm-btn {
  padding: 5px 12px;
  border: 1px solid var(--qm-border);
  border-radius: 6px;
  background: var(--qm-bg-card);
  color: var(--qm-text);
  cursor: pointer;
}

.qm-editor-bar {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 14px;
  border-bottom: 1px solid var(--qm-border-subtle);
  background: var(--qm-bg-raised);
}

.qm-title {
  padding: 3px 8px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qm-text);
  font-size: 14px;
  font-weight: 600;
  cursor: text;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.qm-title:hover {
  background: var(--qm-bg-hover);
}

.qm-rename {
  flex: 1;
  min-width: 0;
  display: flex;
}

.qm-rename-input {
  flex: 1;
  min-width: 0;
  padding: 3px 8px;
  border: 1px solid var(--qm-link);
  border-radius: 6px;
  background: var(--qm-bg);
  color: var(--qm-text);
  font-size: 14px;
  font-weight: 600;
  outline: none;
}

.qm-dirty {
  color: var(--qm-link-broken);
  font-size: 10px;
}

.qm-bar-spacer {
  flex: 1;
}

.qm-view-toggle {
  display: flex;
  gap: 2px;
  padding: 2px;
  border-radius: 7px;
  background: var(--qm-bg);
}

.qm-view-btn {
  padding: 3px 10px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qm-text-muted);
  font-size: 11px;
  cursor: pointer;
  text-transform: capitalize;
}
.qm-view-btn.is-active {
  background: var(--qm-bg-card);
  color: var(--qm-text);
}

.qm-icon-btn {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qm-text-secondary);
  cursor: pointer;
}
.qm-icon-btn:hover {
  background: var(--qm-bg-hover);
  color: var(--qm-text);
}

.qm-editor-split {
  flex: 1;
  min-height: 0;
  display: flex;
}

.qm-pane {
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
}

.qm-editor-split[data-mode='split'] .qm-pane-preview {
  border-left: 1px solid var(--qm-border-subtle);
}
</style>
