<script setup lang="ts">
/**
 * QuantScript's Collection settings: the catalog sources (a GitHub repository at
 * a branch, or a local catalog folder), the token of a private repository —
 * kept in the OS credential store, shown only as "token stored" — and a
 * connection test per source.
 */
import { useCatalogStore } from '#script/stores/catalog'
import type { CollectionSource, CollectionSourceInput } from '#script/types'

const store = useCatalogStore()
const editing = ref<CollectionSourceInput | null>(null)
const formError = ref<string | null>(null)
const saving = ref(false)
const tokenFor = ref<string | null>(null)
const token = ref('')
const tests = ref<Record<string, { ok: boolean; message: string } | 'running'>>({})

onMounted(() => {
  if (!store.sources.length) void store.loadSources()
})

function add(kind: 'github' | 'folder') {
  formError.value = null
  editing.value = kind === 'github'
    ? { kind, name: '', owner: '', repo: '', branch: 'main', path: '', enabled: true }
    : { kind, name: '', path: '', enabled: true }
}

function edit(s: CollectionSource) {
  formError.value = null
  editing.value = { id: s.id, kind: s.kind, name: s.name, owner: s.owner, repo: s.repo, branch: s.branch, path: s.path, enabled: s.enabled }
}

async function save() {
  if (!editing.value) return
  saving.value = true
  formError.value = null
  try {
    await store.saveSource(editing.value)
    editing.value = null
  } catch (err) {
    formError.value = String(err)
  } finally {
    saving.value = false
  }
}

async function remove(s: CollectionSource) {
  if (!confirm(`Remove the source "${s.name}"? Its stored token and cached files go too. Installed indicators stay in your library.`)) return
  try {
    await store.deleteSource(s.id)
  } catch (err) {
    tests.value[s.id] = { ok: false, message: String(err) }
  }
}

async function saveToken(s: CollectionSource) {
  const value = token.value
  token.value = ''
  try {
    await store.setToken(s.id, value)
    tokenFor.value = null
  } catch (err) {
    tests.value[s.id] = { ok: false, message: String(err) }
  }
}

async function clearToken(s: CollectionSource) {
  try {
    await store.clearToken(s.id)
  } catch (err) {
    tests.value[s.id] = { ok: false, message: String(err) }
  }
}

async function test(s: CollectionSource) {
  tests.value[s.id] = 'running'
  tests.value[s.id] = await store.testSource(s.id)
}

function where(s: CollectionSource): string {
  if (s.kind === 'folder') return s.path
  return `${s.owner}/${s.repo}@${s.branch}${s.path ? ` · ${s.path}` : ''}`
}
</script>

<template>
  <!-- The settings modal is teleported outside the module: scope it so the
       module's buttons and inputs apply. -->
  <div class="qsc-settings" data-module="script">
    <section class="qsc-set-block">
      <h3 class="qsc-set-title">Catalog sources</h3>
      <p class="qsc-set-hint">
        The Collection reads indicator catalogs from these sources and installs single indicators with their requirements
        and all their versions into your script folder. A private GitHub repository needs a token that can read it
        (a fine-grained token with read access to its contents); it is kept in the system's credential store, never
        in a file of QuantSuite.
      </p>
      <p v-if="store.sourcesError" class="qsc-note is-error">{{ store.sourcesError }}</p>
      <div v-for="s in store.sources" :key="s.id" class="qst-source-card">
        <div class="qst-source-top">
          <div class="qst-source-name">
            <strong>{{ s.name }}</strong>
            <span class="qsc-chip">{{ s.kind === 'github' ? 'GitHub' : 'Folder' }}</span>
            <span v-if="!s.enabled" class="qsc-chip is-warn">off</span>
            <span v-if="s.kind === 'github'" class="qsc-chip" :class="{ 'is-ok': s.has_token }">{{ s.has_token ? 'token stored' : 'no token' }}</span>
          </div>
          <div class="qst-source-actions">
            <button class="qsc-btn is-sm" :disabled="tests[s.id] === 'running'" @click="test(s)">{{ tests[s.id] === 'running' ? 'Testing…' : 'Test connection' }}</button>
            <button class="qsc-btn is-sm" @click="edit(s)">Edit</button>
            <button class="qsc-btn is-sm is-danger" @click="remove(s)">Remove</button>
          </div>
        </div>
        <p class="mono qst-source-where" :title="where(s)">{{ where(s) }}</p>
        <p v-if="tests[s.id] && tests[s.id] !== 'running'" class="qst-test" :class="(tests[s.id] as { ok: boolean }).ok ? 'is-ok' : 'is-bad'">
          {{ (tests[s.id] as { ok: boolean }).ok ? 'Connected — ' : '' }}{{ (tests[s.id] as { message: string }).message }}
        </p>
        <div v-if="s.kind === 'github'" class="qst-token">
          <template v-if="tokenFor === s.id">
            <input v-model="token" class="qsc-input mono" type="password" autocomplete="off" spellcheck="false" placeholder="github_pat_… or ghp_…" aria-label="GitHub token" @keydown.enter="saveToken(s)" />
            <button class="qsc-btn is-sm is-primary" :disabled="!token.trim()" @click="saveToken(s)">Save token</button>
            <button class="qsc-btn is-sm" @click="tokenFor = null; token = ''">Cancel</button>
          </template>
          <template v-else>
            <button class="qsc-btn is-sm" @click="tokenFor = s.id">{{ s.has_token ? 'Replace token' : 'Set token' }}</button>
            <button v-if="s.has_token" class="qsc-btn is-sm" @click="clearToken(s)">Clear token</button>
          </template>
        </div>
      </div>
      <p v-if="!store.sources.length && !store.sourcesLoading" class="qsc-set-hint">No source yet.</p>
      <div class="qsc-set-row">
        <button class="qsc-btn" @click="add('github')">Add GitHub source</button>
        <button class="qsc-btn" @click="add('folder')">Add local folder</button>
      </div>
    </section>

    <section v-if="editing" class="qsc-set-block qst-form">
      <h3 class="qsc-set-title">{{ editing.id ? 'Edit source' : editing.kind === 'github' ? 'New GitHub source' : 'New folder source' }}</h3>
      <label class="qst-field"><span>Name</span><input v-model="editing.name" class="qsc-input" placeholder="My indicators" /></label>
      <template v-if="editing.kind === 'github'">
        <label class="qst-field"><span>Owner</span><input v-model="editing.owner" class="qsc-input mono" spellcheck="false" placeholder="QuantableX" /></label>
        <label class="qst-field"><span>Repository</span><input v-model="editing.repo" class="qsc-input mono" spellcheck="false" placeholder="QuantScript-Collection-Public" /></label>
        <label class="qst-field"><span>Branch</span><input v-model="editing.branch" class="qsc-input mono" spellcheck="false" placeholder="main" /></label>
        <label class="qst-field"><span>Folder</span><input v-model="editing.path" class="qsc-input mono" spellcheck="false" placeholder="(the repository root)" /></label>
      </template>
      <label v-else class="qst-field"><span>Folder</span><input v-model="editing.path" class="qsc-input mono" spellcheck="false" placeholder="C:\Projects\QuantScript-Collection-Public" /></label>
      <label class="qsc-check"><input v-model="editing.enabled" type="checkbox" /> Enabled</label>
      <p v-if="formError" class="qsc-note is-error">{{ formError }}</p>
      <div class="qsc-set-row">
        <button class="qsc-btn is-primary" :disabled="saving" @click="save">{{ saving ? 'Saving…' : 'Save source' }}</button>
        <button class="qsc-btn" :disabled="saving" @click="editing = null">Cancel</button>
      </div>
    </section>
  </div>
</template>

<style scoped>
.qsc-settings {
  display: flex;
  flex-direction: column;
  gap: 20px;
  background: transparent;
}
.qsc-set-block {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.qsc-set-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--qss-text-muted);
}
.qsc-set-hint {
  font-size: 12px;
  line-height: 1.5;
  color: var(--qss-text-secondary);
}
.qsc-set-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.qst-source-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 10px 12px;
  border: 1px solid var(--qss-border);
  border-radius: 8px;
}
.qst-source-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-wrap: wrap;
}
.qst-source-name {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12.5px;
}
.qst-source-actions,
.qst-token {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}
.qst-token input {
  width: 260px;
  max-width: 100%;
}
.qst-source-where {
  font-size: 11.5px;
  color: var(--qss-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.qst-test {
  font-size: 12px;
}
.qst-test.is-ok {
  color: var(--qss-success);
}
.qst-test.is-bad {
  color: var(--qss-error);
}
.qst-field {
  display: grid;
  grid-template-columns: 90px minmax(0, 1fr);
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--qss-text-muted);
}
</style>
