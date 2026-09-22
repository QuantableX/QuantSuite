<script setup lang="ts">
/**
 * The script tree — the module's left sidebar body.
 *
 * Four sections: the indicator scripts (one row per file, expandable to the
 * registered indicators it holds, each with the registry's one score), the
 * registry (`indicators/__init__.py`), the read-only reference (the
 * contract), and the archive — scripts deleted here, kept as versions, one
 * click from coming back. A row carries what needs attention: unsaved
 * changes, a change made outside QuantScript, a syntax error, a script that
 * cannot register.
 */
import { useWorkbenchStore } from '#script/stores/workbench'
import { Library, Star, X } from 'lucide-vue-next'
import { gradeClass, timeAgo } from '#script/utils/format'
import type { ArchivedScript, ScriptClass, ScriptEntry } from '#script/types'

const wb = useWorkbenchStore()
const router = useRouter()

const indicatorScripts = computed(() => wb.filtered.filter((s) => s.kind === 'script'))
const registryScripts = computed(() => wb.filtered.filter((s) => s.kind === 'registry'))
const groups = computed(() => [
  { label: 'Indicator scripts', entries: indicatorScripts.value.filter((s) => s.registration === 'discovered' || s.registration === 'explicit') },
  { label: 'Utilities', entries: indicatorScripts.value.filter((s) => s.registration !== 'explicit' && s.registration !== 'discovered') },
].filter((group) => group.entries.length))

function library() {
  wb.showLibrary()
  void router.push('/script')
}

function registered(entry: ScriptEntry): ScriptClass[] {
  return entry.classes.filter((c) => c.key)
}

function isExpanded(entry: ScriptEntry): boolean {
  const state = wb.expanded[entry.file]
  // A filter that matched an indicator opens its file, so the hit is visible.
  if (wb.search.trim()) return state !== false
  return !!state
}

function scoreOf(c: ScriptClass): string {
  const cert = c.certification
  if (!cert) return '—'
  return `${cert.score}${cert.grade ? ' ' + cert.grade : ''}`
}

function scoreTitle(c: ScriptClass): string {
  const cert = c.certification
  if (!cert) return `${c.name ?? c.class_name}: no gauntlet verdict yet`
  const tracks = cert.tracks_run != null && cert.tracks != null ? ` · ${cert.tracks_run}/${cert.tracks} tracks` : ''
  return `${c.name ?? c.class_name}: ${cert.score}/100 ${cert.grade}${cert.certified ? ' · certified on every track' : ''}${tracks}${cert.source === 'historical' ? ' · historical run' : ''}`
}

/** Opening a script from the tree always lands in the editor, wherever the
 *  module was (the forge page shares this sidebar). */
function openFile(entry: ScriptEntry) {
  void wb.openScript(entry.file)
  if (!router.currentRoute.value.path.startsWith('/script/forge')) return
  void router.push('/script')
}

function openClass(entry: ScriptEntry, c: ScriptClass) {
  void wb.openScript(entry.file, c.line)
  if (router.currentRoute.value.path.startsWith('/script/forge')) void router.push('/script')
}

async function restoreArchived(entry: ArchivedScript) {
  if (!confirm(`Bring ${entry.file} back? Its last recorded content becomes the file again (a new version, checked first).`)) return
  await wb.restoreArchived(entry)
  if (router.currentRoute.value.path.startsWith('/script/forge')) void router.push('/script')
}
</script>

<template>
  <div class="qsc-tree">
    <button class="qsc-library-nav" :class="{ 'is-active': wb.libraryOpen && !router.currentRoute.value.path.startsWith('/script/forge') }" @click="library"><Library :size="15" /><span>Indicator library</span><span class="muted">{{ wb.libraryScripts.length }}</span></button>
    <div class="qsc-tree-filter">
      <ScriptLayoutWorkspaceFilter />
      <button v-if="wb.libraryFilter !== 'all' || wb.search" class="qsc-icon-btn" aria-label="Clear workspace filters" @click="wb.libraryFilter = 'all'; wb.search = ''"><X :size="12" /></button>
    </div>
    <div v-if="wb.listingError" class="qsc-note is-error qsc-tree-note">{{ wb.listingError }}</div>
    <div v-else-if="wb.listing?.registry_error" class="qsc-note is-warn qsc-tree-note" :title="wb.listing.registry_error">
      The registry could not be read — files only, no verdicts. {{ wb.listing.registry_error }}
    </div>
    <div v-else-if="wb.listing && !wb.listing.python.ok" class="qsc-note is-warn qsc-tree-note" :title="wb.listing.python.error ?? ''">
      No Python with numpy and pandas — files only, checks off.
    </div>

    <p v-if="!wb.listing && wb.listingLoading" class="qsc-tree-empty qsc-pulse">Reading the registry…</p>
    <p v-else-if="wb.listing && !indicatorScripts.length" class="qsc-tree-empty">No scripts in this view. Try another filter.</p>

    <template v-if="wb.listing">
      <div v-for="group in groups" :key="group.label" class="qsc-section">
        <div class="qsc-section-title">
          <span>{{ group.label }}</span>
          <span class="qsc-section-count mono">{{ group.entries.length }}</span>
        </div>
        <div v-for="entry in group.entries" :key="entry.file" class="qsc-node">
          <div
            class="qsc-row"
            :class="{ 'is-active': wb.activeFile === entry.file && !wb.libraryOpen, 'is-open': wb.open.some((o) => o.file === entry.file) }"
            :title="entry.summary || entry.file"
          >
            <button
              class="qsc-chevron"
              :class="{ 'is-open': isExpanded(entry), 'is-empty': !registered(entry).length }"
              :disabled="!registered(entry).length"
              :aria-label="isExpanded(entry) ? 'Collapse' : 'Expand'"
              :aria-expanded="isExpanded(entry)"
              @click.stop="wb.expanded[entry.file] = !isExpanded(entry)"
            >
              <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <path d="M9 6l6 6-6 6" />
              </svg>
            </button>
            <button class="qsc-row-name qsc-file-button mono" :aria-label="`Edit ${entry.file}`" @click="openFile(entry)">{{ entry.file }}</button>
            <Star v-if="wb.favorites.includes(entry.file)" :size="10" class="muted" aria-label="Favorite" />
            <span v-if="wb.isDirty(entry.file)" class="qsc-mark is-dirty" title="Unsaved changes" />
            <span v-else-if="entry.discovery_error" class="qsc-mark is-error" :title="`Cannot register: ${entry.discovery_error}`" />
            <span v-else-if="entry.syntax_error" class="qsc-mark is-error" :title="`Syntax error — ${entry.syntax_error}`" />
            <span v-else-if="entry.versions.external" class="qsc-mark is-warn" title="Changed outside QuantScript since the last recorded version" />
            <span class="qsc-row-count mono" :title="`${entry.registered} registered indicator(s)`">{{ entry.registered || '' }}</span>
          </div>
          <div v-if="isExpanded(entry) && registered(entry).length" class="qsc-children">
            <button
              v-for="c in registered(entry)"
              :key="c.key ?? c.class_name"
              class="qsc-leaf"
              :title="scoreTitle(c)"
              @click="openClass(entry, c)"
            >
              <span class="qsc-leaf-key mono">{{ c.key }}</span>
              <span class="qsc-grade" :class="gradeClass(c.certification?.grade, c.certification?.certified)">
                {{ scoreOf(c) }}
              </span>
            </button>
          </div>
        </div>
      </div>

      <details class="qsc-internals"><summary>Reference & internals</summary>
      <div v-if="registryScripts.length" class="qsc-section">
        <div class="qsc-section-title"><span>Registry</span></div>
        <button
          v-for="entry in registryScripts"
          :key="entry.file"
          class="qsc-row is-plain"
          :class="{ 'is-active': wb.activeFile === entry.file }"
          :title="`${entry.file} — the explicit registry; scripts made here register themselves`"
          @click="openFile(entry)"
        >
          <span class="qsc-row-name mono">indicators/{{ entry.file }}</span>
          <span v-if="wb.isDirty(entry.file)" class="qsc-mark is-dirty" title="Unsaved changes" />
          <span v-else-if="entry.versions.external" class="qsc-mark is-warn" title="Changed outside QuantScript" />
          <span class="qsc-row-count mono">{{ wb.listing.registry_keys || '' }}</span>
        </button>
      </div>

      <div v-if="wb.reference.length" class="qsc-section">
        <div class="qsc-section-title"><span>Reference</span></div>
        <button
          v-for="entry in wb.reference"
          :key="entry.file"
          class="qsc-row is-plain"
          :class="{ 'is-active': wb.activeFile === entry.file }"
          :title="`${entry.file} — the contract every script obeys (read-only)`"
          @click="openFile(entry)"
        >
          <svg class="qsc-lock" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <rect x="5" y="11" width="14" height="10" rx="2" /><path d="M8 11V7a4 4 0 0 1 8 0v4" />
          </svg>
          <span class="qsc-row-name mono">{{ entry.file }}</span>
        </button>
      </div>
      </details>

      <div v-if="wb.archived.length" class="qsc-section">
        <button class="qsc-section-title qsc-section-toggle" :aria-expanded="wb.archiveOpen" @click="wb.archiveOpen = !wb.archiveOpen">
          <span>Archive</span>
          <span class="qsc-section-count mono">{{ wb.archived.length }}</span>
        </button>
        <template v-if="wb.archiveOpen">
          <div
            v-for="entry in wb.archived"
            :key="entry.file"
            class="qsc-row is-plain is-archived"
            :title="`${entry.file} — ${entry.deleted ? 'deleted' : 'gone from disk'} ${timeAgo(entry.at)}, ${entry.count} version(s) kept`"
          >
            <span class="qsc-row-name mono">{{ entry.file }}</span>
            <span class="qsc-row-when muted">{{ timeAgo(entry.at) }}</span>
            <button class="qsc-btn is-sm is-ghost qsc-row-restore" title="Bring it back from its last version" @click.stop="restoreArchived(entry)">Restore</button>
          </div>
        </template>
      </div>
    </template>
  </div>
</template>

<style scoped>
.qsc-tree {
  padding: 6px 0 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.qsc-library-nav { display: flex; align-items: center; gap: 9px; margin: 2px 8px; padding: 9px 8px; border: 0; border-radius: 6px; background: transparent; text-align: left; font-size: 12px; cursor: pointer; }
.qsc-library-nav span:nth-child(2) { flex: 1; }
.qsc-library-nav.is-active, .qsc-library-nav:hover { background: var(--qss-bg-hover); }
.qsc-tree-filter { display: flex; align-items: center; gap: 4px; margin: 0 12px 4px; }
.qsc-tree-filter > .qsc-icon-btn { flex-shrink: 0; }
.qsc-internals { margin: 12px 0 0; border-top: 1px solid var(--qss-border); padding-top: 12px; }
.qsc-internals > summary { padding: 0 14px; color: var(--qss-text-muted); font-size: 11px; cursor: pointer; margin-bottom: 8px; }
.qsc-file-button { text-align: left; border: 0; background: none; cursor: pointer; padding: 3px 0; }

.qsc-tree-note {
  margin: 4px 8px 0;
}

.qsc-tree-empty {
  margin: 8px 12px;
  font-size: 12px;
  color: var(--qss-text-muted);
}

.qsc-section-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 12px 2px;
  font-size: 10.5px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--qss-text-muted);
}
.qsc-section-toggle {
  width: 100%;
  border: none;
  background: transparent;
  cursor: pointer;
  text-align: left;
}
.qsc-section-toggle:hover {
  color: var(--qss-text-secondary);
}
.qsc-section-count {
  font-size: 10.5px;
}

.qsc-row {
  border: none;
  background: transparent;
  text-align: left;
  display: flex;
  align-items: center;
  gap: 4px;
  width: calc(100% - 12px);
  margin: 0 6px;
  padding: 4px 6px 4px 2px;
  border-radius: 6px;
  color: var(--qss-text-secondary);
  cursor: pointer;
  user-select: none;
}
.qsc-row.is-plain {
  padding-left: 8px;
  gap: 6px;
}
.qsc-row.is-archived {
  cursor: default;
  color: var(--qss-text-muted);
}
.qsc-row:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.qsc-row.is-archived:hover {
  color: var(--qss-text-secondary);
}
.qsc-row.is-open {
  color: var(--qss-text);
}
.qsc-row.is-active {
  background: var(--qss-bg-card);
  color: var(--qss-text);
}

.qsc-chevron {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 18px;
  height: 18px;
  flex-shrink: 0;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qss-text-muted);
  cursor: pointer;
  transition: transform var(--qss-dur-fast) var(--qss-ease-out);
}
.qsc-chevron.is-open {
  transform: rotate(90deg);
}
.qsc-chevron.is-empty {
  visibility: hidden;
}
.qsc-chevron:hover:not(:disabled) {
  color: var(--qss-text);
  background: var(--qss-bg-card);
}

.qsc-row-name {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.qsc-row-count {
  font-size: 10.5px;
  color: var(--qss-text-muted);
  min-width: 14px;
  text-align: right;
}
.qsc-row-when {
  font-size: 10.5px;
  white-space: nowrap;
}
.qsc-row-restore {
  height: 20px;
  padding: 0 6px;
  font-size: 10.5px;
}

/* A script made here: a hollow ring, so the user's own files stand out
   from the forge's modules at a glance. */
.qsc-mine {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  border: 1.5px solid var(--qss-text-muted);
  flex-shrink: 0;
}

.qsc-mark {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
}
.qsc-mark.is-dirty { background: var(--qss-warning); }
.qsc-mark.is-warn { background: var(--qss-warning); opacity: 0.7; }
.qsc-mark.is-error { background: var(--qss-error); }

.qsc-lock {
  flex-shrink: 0;
  color: var(--qss-text-muted);
}

.qsc-children {
  display: flex;
  flex-direction: column;
  padding: 1px 0 3px;
}
.qsc-leaf {
  display: flex;
  align-items: center;
  gap: 6px;
  width: calc(100% - 12px);
  margin: 0 6px;
  padding: 2px 6px 2px 26px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-secondary);
  text-align: left;
  cursor: pointer;
}
.qsc-leaf:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.qsc-leaf-key {
  flex: 1;
  min-width: 0;
  font-size: 11.5px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
