<script setup lang="ts">
/**
 * The context panel. With a script open: what it is, the indicators it
 * registers with the registry's verdicts, its version history — click a
 * version to compare it with the buffer, restore it from there — and, for
 * a script made here, the way out (delete, into the archive). On the forge
 * page: the vault, the tracks, the running job. Without either: the
 * workbench at a glance.
 */
import { useForgeStore } from '#script/stores/forge'
import { useWorkbenchStore } from '#script/stores/workbench'
import { ChevronRight, X } from 'lucide-vue-next'
import { AUTHOR_LABELS, formatBytes, gradeClass, shortSha, timeAgo } from '#script/utils/format'
import { KIND_LABELS, formatElapsed } from '#script/utils/forge'
import type { ScriptClass, VersionMeta } from '#script/types'

const wb = useWorkbenchStore()
const forge = useForgeStore()
const route = useRoute()

const onForge = computed(() => route.path.startsWith('/script/forge'))
const editing = computed(() => !!wb.active && !wb.libraryOpen)
const entry = computed(() => wb.activeEntry)
const registered = computed<ScriptClass[]>(() => entry.value?.classes.filter((c) => c.key) ?? [])
const helpers = computed<ScriptClass[]>(() => entry.value?.classes.filter((c) => !c.key) ?? [])
const currentVersion = computed(() => wb.active?.version?.version ?? null)
const inspectorTabs = [
  { id: 'outline', label: 'Outline' }, { id: 'details', label: 'Details' },
  { id: 'history', label: 'History' }, { id: 'guide', label: 'Guide' },
] as const
const outline = computed(() => {
  const lines = wb.active?.content.split('\n') ?? []
  return (entry.value?.classes ?? []).map((c) => {
    const line = lines.findIndex((text) => new RegExp(`^class\\s+${c.class_name}\\b`).test(text))
    return { ...c, line: line < 0 ? c.line : line + 1 }
  })
})

const registrationText = computed(() => {
  switch (entry.value?.registration) {
    case 'explicit':
      return "forge module — imported by indicators/__init__.py"
    case 'discovered':
      return 'made here — registers itself (REGISTER)'
    case 'none':
      return 'registers nothing'
    case 'unknown':
      return 'unknown — the registry could not be read'
    default:
      return ''
  }
})

watch(
  () => [wb.active?.file, wb.inspectorTab] as const,
  ([file, tab]) => {
    const o = wb.active
    if (file && tab === 'history' && o && o.editable && !o.versionsLoaded) void wb.loadVersions(file)
  },
  { immediate: true },
)

function verdictLabel(c: ScriptClass): string {
  const cert = c.certification
  if (!cert) return 'no verdict'
  return `${cert.score} ${cert.grade}`
}

function tracks(c: ScriptClass): string {
  const t = c.timeframes
  if (!t) return ''
  return Object.entries(t)
    .map(([tf, v]) => `${tf} ${v ? `${v.score}/${v.grade}` : '—'}`)
    .join(' · ')
}

function reveal(c: ScriptClass) {
  if (wb.active) wb.reveal(wb.active.file, c.line)
}

function isComparing(v: VersionMeta): boolean {
  return wb.active?.compare?.meta.version === v.version
}

function toggleCompare(v: VersionMeta) {
  if (!wb.active) return
  if (isComparing(v)) wb.closeCompare(wb.active.file)
  else void wb.compare(wb.active.file, v.version)
}

async function restore(v: VersionMeta) {
  if (!wb.active) return
  if (wb.active.content !== wb.active.saved && !confirm(`Restore v${v.version}? The unsaved changes in the editor are replaced.`)) return
  await wb.restore(wb.active.file, v.version)
}

async function deleteScript() {
  const e = entry.value
  if (!e || !wb.deletable(e)) return
  const keys = e.classes.filter((c) => c.key).map((c) => c.key)
  const lines = [
    `Delete ${e.file}?`,
    keys.length ? `It registers ${keys.join(', ')} — a strategy or a system that names one of these loses its indicator.` : 'It registers nothing.',
    'The last content is kept in the archive; Restore brings it back.',
  ]
  if (!confirm(lines.join('\n\n'))) return
  await wb.deleteScript(e.file)
}
</script>

<template>
  <div class="qsc-side">
    <header class="qsc-inspector-header">
      <div class="qsc-inspector-heading"><strong>{{ onForge ? 'Forge monitor' : editing ? 'Script inspector' : 'Workspace' }}</strong><button class="qsc-icon-btn" aria-label="Close inspector" @click="wb.toggleSidebar('right')"><X :size="14" /></button></div>
      <p v-if="editing && !onForge" class="qsc-inspector-file mono" :title="wb.active?.file">{{ wb.active?.file }}</p>
      <nav v-if="!onForge && editing" class="qsc-inspector-tabs" aria-label="Script inspector"><button v-for="t in inspectorTabs" :key="t.id" :aria-pressed="wb.inspectorTab === t.id" :class="{ 'is-active': wb.inspectorTab === t.id }" @click="wb.inspectorTab = t.id">{{ t.label }}</button></nav>
      <nav v-else-if="!onForge" class="qsc-inspector-tabs" aria-label="Workspace inspector"><button :class="{ 'is-active': wb.inspectorTab !== 'guide' }" :aria-pressed="wb.inspectorTab !== 'guide'" @click="wb.inspectorTab = 'outline'">Overview</button><button :class="{ 'is-active': wb.inspectorTab === 'guide' }" :aria-pressed="wb.inspectorTab === 'guide'" @click="wb.inspectorTab = 'guide'">Guide</button></nav>
    </header>
    <!-- ── The forge ─────────────────────────────────────────────────── -->
    <template v-if="onForge">
      <section class="qsc-panel">
        <h3 class="qsc-panel-title">The forge</h3>
        <div v-if="forge.info" class="qsc-meta-row">
          <span class="qsc-meta-label">vault</span>
          <span class="qsc-meta-value mono" :title="forge.info.root">{{ forge.info.root }}</span>
        </div>
        <div v-if="forge.info" class="qsc-meta-row">
          <span class="qsc-meta-label">workers</span>
          <span class="qsc-meta-value">{{ forge.info.workers }} · Python {{ forge.info.python }}</span>
        </div>
        <div v-if="forge.info?.supported_timeframes" class="qsc-meta-row">
          <span class="qsc-meta-label">tracks</span>
          <span class="qsc-meta-value">
            <span v-for="tf in forge.info.supported_timeframes" :key="tf" class="qsc-side-track mono" :class="{ 'is-off': !(forge.info.timeframes ?? []).includes(tf) }" :title="(forge.info.timeframes ?? []).includes(tf) ? `${tf}: shelf present · ${forge.info.certified_by_timeframe?.[tf]?.length ?? 0} certified` : `${tf}: no shelf yet`">
              {{ tf }}
            </span>
          </span>
        </div>
        <div v-if="forge.registry" class="qsc-meta-row">
          <span class="qsc-meta-label">roster</span>
          <span class="qsc-meta-value">{{ forge.indicators.length }} indicators · {{ forge.certified.length }} pass every track</span>
        </div>
        <div v-if="forge.info" class="qsc-meta-row">
          <span class="qsc-meta-label">shelf</span>
          <span class="qsc-meta-value">{{ forge.shelf.length }} series · {{ forge.reports.length }} reports</span>
        </div>
        <p v-if="!forge.info && forge.infoLoading" class="qsc-panel-empty qsc-pulse">Reading the vault…</p>
      </section>

      <section class="qsc-panel">
        <h3 class="qsc-panel-title">Job</h3>
        <template v-if="forge.detailJob">
          <div class="qsc-meta-row">
            <span class="qsc-meta-label">status</span>
            <span class="qsc-meta-value">
              <span class="qsc-tag" :class="forge.detailJob.status === 'running' ? 'is-running' : forge.detailJob.status === 'done' ? 'is-ok' : forge.detailJob.status === 'failed' ? 'is-error' : ''">{{ forge.detailJob.status }}</span>
            </span>
          </div>
          <div class="qsc-meta-row">
            <span class="qsc-meta-label">kind</span>
            <span class="qsc-meta-value">{{ KIND_LABELS[forge.detailJob.kind] ?? forge.detailJob.kind }}{{ forge.detailJob.request.fast ? ' · fast' : '' }}</span>
          </div>
          <div class="qsc-meta-row">
            <span class="qsc-meta-label">elapsed</span>
            <span class="qsc-meta-value mono">{{ formatElapsed(forge.detailJob.started_at, forge.detailJob.finished_at) }}</span>
          </div>
          <div class="qsc-meta-row">
            <span class="qsc-meta-label">picked</span>
            <span class="qsc-meta-value mono" :title="forge.detailJob.indicators.join(', ')">{{ forge.detailJob.indicators.join(', ') || '—' }}</span>
          </div>
          <button v-if="forge.isRunning" class="qsc-btn is-sm is-danger qsc-side-btn" @click="forge.cancel()">Cancel the job</button>
        </template>
        <p v-else class="qsc-panel-empty">No forge job this session.</p>
      </section>

      <section class="qsc-panel">
        <h3 class="qsc-panel-title">Doctrine</h3>
        <p class="qsc-help">
          Certified = score ≥ 70 and permutation p ≤ 0.10 on every track. The one score is the worst track.
          A fast run is a smoke test, never a certification. Any edit under <span class="mono">smithery/</span>
          turns existing runs historical — re-run the gauntlet after a change.
        </p>
      </section>
    </template>

    <!-- ── Script context ────────────────────────────────────────────── -->
    <template v-else-if="editing && wb.active">
      <ScriptEditorGuide v-if="wb.inspectorTab === 'guide'" />
      <ScriptEditorScriptHealth v-if="wb.inspectorTab === 'outline'" />
      <section v-if="wb.inspectorTab === 'outline'" class="qsc-panel">
        <h3 class="qsc-panel-title">Script outline <span class="mono">{{ outline.length }}</span></h3>
        <p class="qsc-help">Jump to an indicator or helper class.</p>
        <p v-if="wb.isDirty(wb.active.file)" class="qsc-help">Class list from the saved script. Save to discover new classes.</p>
        <button v-for="c in outline" :key="c.class_name" class="qsc-outline-row" @click="reveal(c)"><ChevronRight :size="12" /><span><strong>{{ c.name || c.class_name }}</strong><span class="mono">{{ c.key || c.class_name }}</span></span><span class="mono muted">{{ c.line }}</span></button>
        <p v-if="!outline.length" class="qsc-panel-empty">No classes in the saved script.</p>
        <button class="qsc-btn is-sm qsc-side-btn" @click="wb.inspectorTab = 'details'">Indicator details & parameters</button>
      </section>
      <section v-if="wb.inspectorTab === 'details'" class="qsc-panel">
        <h3 class="qsc-panel-title">Script</h3>
        <div class="qsc-meta-row">
          <span class="qsc-meta-label">file</span>
          <span class="qsc-meta-value mono" :title="wb.active.path">{{ wb.active.file }}</span>
        </div>
        <div class="qsc-meta-row">
          <span class="qsc-meta-label">kind</span>
          <span class="qsc-meta-value">{{ wb.active.kind === 'reference' ? 'reference (read-only)' : wb.active.kind }}</span>
        </div>
        <div v-if="entry && entry.kind === 'script'" class="qsc-meta-row">
          <span class="qsc-meta-label">registry</span>
          <span class="qsc-meta-value" :title="registrationText">{{ registrationText }}</span>
        </div>
        <div v-if="entry" class="qsc-meta-row">
          <span class="qsc-meta-label">size</span>
          <span class="qsc-meta-value">{{ formatBytes(entry.size) }} · {{ timeAgo(entry.modified) }}</span>
        </div>
        <div class="qsc-meta-row">
          <span class="qsc-meta-label">sha</span>
          <span class="qsc-meta-value mono" :title="wb.active.sha256">{{ shortSha(wb.active.sha256) }}</span>
        </div>
        <div v-if="wb.active.version" class="qsc-meta-row">
          <span class="qsc-meta-label">version</span>
          <span class="qsc-meta-value">v{{ wb.active.version.version }} · {{ timeAgo(wb.active.version.created_at) }}</span>
        </div>
        <p v-if="entry?.summary" class="qsc-summary">{{ entry.summary }}</p>
        <div v-if="entry?.discovery_error" class="qsc-note is-error">Cannot register — {{ entry.discovery_error }}</div>
        <div v-else-if="entry?.syntax_error" class="qsc-note is-error">Syntax error — {{ entry.syntax_error }}</div>
      </section>

      <section v-if="wb.inspectorTab === 'details' && wb.active.kind === 'script'" class="qsc-panel">
        <h3 class="qsc-panel-title">
          <span>Indicators</span>
          <span class="mono">{{ registered.length }}</span>
        </h3>
        <div v-for="c in registered" :key="c.key ?? c.class_name" class="qsc-ind">
          <div class="qsc-ind-head">
            <button class="qsc-ind-key mono" :title="`Go to class ${c.class_name} (line ${c.line})`" @click="reveal(c)">{{ c.key }}</button>
            <span class="qsc-grade" :class="gradeClass(c.certification?.grade, c.certification?.certified)" :title="tracks(c)">
              {{ verdictLabel(c) }}
            </span>
          </div>
          <div class="qsc-ind-name">
            {{ c.name ?? c.class_name }}
            <span v-if="c.certification?.source === 'historical'" class="qsc-chip" title="The verdict predates the current source">historical</span>
            <span v-else-if="c.certification?.certified" class="qsc-chip is-ok">certified</span>
          </div>
          <p v-if="c.hypothesis" class="qsc-ind-hyp" :title="c.hypothesis">{{ c.hypothesis }}</p>
          <p v-if="c.warmup_bars != null" class="qsc-help">Warm-up <span class="mono">{{ c.warmup_bars }}</span> bars</p>
          <details v-if="c.params && Object.keys(c.params).length" class="qsc-params-detail"><summary>Default parameters</summary><dl><template v-for="(value, key) in c.params" :key="key"><dt class="mono">{{ key }}</dt><dd class="mono">{{ value }}</dd></template></dl></details>
          <p v-if="c.error" class="qsc-note is-error">{{ c.error }}</p>
        </div>
        <p v-if="!registered.length" class="qsc-panel-empty">
          {{ helpers.length ? 'No registered indicator — helper classes only.' : 'No class registers here yet.' }}
        </p>
        <p v-if="helpers.length" class="qsc-helpers muted">
          also: <span v-for="(h, i) in helpers" :key="h.class_name" class="mono">{{ h.class_name }}{{ i < helpers.length - 1 ? ', ' : '' }}</span>
        </p>
        <NuxtLink class="qsc-btn is-sm qsc-side-btn" to="/script/forge" title="Gauntlet, walk-forward and reports — the forge">Open the forge</NuxtLink>
      </section>

      <section v-if="wb.inspectorTab === 'history' && wb.active.editable" class="qsc-panel">
        <h3 class="qsc-panel-title">
          <span>Versions</span>
          <span class="mono">{{ wb.active.versions.length }}</span>
        </h3>
        <p class="qsc-help">Select a version to compare it with your editor. Restoring creates a new version.</p>
        <p v-if="!wb.active.versionsLoaded" class="qsc-panel-empty qsc-pulse">Loading…</p>
        <div
          v-for="v in wb.active.versions"
          :key="v.id"
          class="qsc-ver"
          :class="{ 'is-current': v.version === currentVersion, 'is-comparing': isComparing(v) }"
        >
          <button class="qsc-ver-main" :title="isComparing(v) ? 'Close the compare' : `Compare v${v.version} with the editor`" @click="toggleCompare(v)">
            <span class="qsc-ver-no mono">v{{ v.version }}</span>
            <span class="qsc-ver-when">{{ timeAgo(v.created_at) }}</span>
            <span class="qsc-chip qsc-ver-author" :class="{ 'is-warn': v.author === 'external', 'is-error': v.author === 'delete' }">{{ AUTHOR_LABELS[v.author] ?? v.author }}</span>
            <span v-if="v.checked" class="qsc-ver-check" title="Passed the sandbox check">✓</span>
          </button>
          <div class="qsc-ver-msg" :title="v.message">{{ v.message }}</div>
          <button
            v-if="v.version !== currentVersion"
            class="qsc-btn is-sm is-ghost qsc-ver-restore"
            :disabled="wb.active.saving"
            @click="restore(v)"
          >
            Restore
          </button>
        </div>
      </section>
      <p v-if="wb.inspectorTab === 'history' && !wb.active.editable" class="qsc-help">This reference is read-only and has no editable version history.</p>

      <details v-if="wb.inspectorTab === 'details' && wb.deletable(entry)" class="qsc-panel qsc-manage"><summary class="qsc-panel-title">Manage script</summary>
        <p class="qsc-help">
          {{ entry?.registration === 'discovered' ? 'A script made here — yours to remove.' : 'This file registers nothing.' }}
          The versions stay in the archive; Restore brings the script back.
        </p>
        <button class="qsc-btn is-sm is-danger qsc-side-btn" :disabled="wb.active.saving" @click="deleteScript">Delete script</button>
      </details>
      <section v-else-if="wb.inspectorTab === 'details' && entry?.registration === 'explicit'" class="qsc-panel">
        <h3 class="qsc-panel-title">Forge module</h3>
        <p class="qsc-help">
          Imported explicitly by <span class="mono">indicators/__init__.py</span> — not deleted from here. Remove its import there first if it really has to go.
        </p>
      </section>
    </template>

    <!-- ── Workbench at a glance ─────────────────────────────────────── -->
    <ScriptEditorGuide v-else-if="wb.inspectorTab === 'guide'" />
    <ScriptEditorWorkspaceOverview v-else />
  </div>
</template>

<style scoped>
.qsc-side {
  height: 100%;
  min-height: 0;
  overflow-y: auto;
  padding: 14px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.qsc-inspector-header { flex-shrink: 0; position: sticky; top: -14px; z-index: 2; background: var(--qss-bg-raised); margin: -14px -14px 0; padding: 12px 14px 0; border-bottom: 1px solid var(--qss-border); }
.qsc-inspector-heading { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding-bottom: 10px; }
.qsc-inspector-heading strong { font-size: 12px; font-weight: 600; }
.qsc-inspector-file { font-size: 10px; color: var(--qss-text-muted); overflow: hidden; white-space: nowrap; text-overflow: ellipsis; padding-bottom: 8px; }
.qsc-inspector-tabs { display: flex; align-items: center; margin: 0 -4px; }
.qsc-inspector-tabs > button:not(.qsc-icon-btn) { padding: 6px 7px 12px; border: none; border-bottom: 2px solid transparent; background: none; cursor: pointer; font-size: 11px; color: var(--qss-text-muted); }
.qsc-inspector-tabs > button.is-active { border-bottom-color: var(--qss-text); color: var(--qss-text); }
.qsc-inspector-tabs .qsc-icon-btn { margin-left: auto; flex-shrink: 0; }
.qsc-outline-row { display: flex; align-items: center; gap: 8px; border: none; border-radius: 6px; background: transparent; padding: 9px 3px; text-align: left; cursor: pointer; }
.qsc-outline-row:hover { background: var(--qss-bg-hover); }
.qsc-outline-row > span:nth-child(2) { flex: 1; display: flex; flex-direction: column; gap: 3px; min-width: 0; }
.qsc-outline-row strong { font-size: 12px; font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qsc-outline-row .mono { color: var(--qss-text-muted); font-size: 10px; overflow-wrap: anywhere; }
.qsc-params-detail summary, .qsc-manage summary { cursor: pointer; font-size: 11px; color: var(--qss-text-muted); margin: 8px 0; }
.qsc-params-detail dl { display: grid; grid-template-columns: 1fr auto; gap: 6px; font-size: 10px; padding: 6px 0; }
.qsc-params-detail dt, .qsc-params-detail dd { overflow-wrap: anywhere; }

.qsc-summary,
.qsc-help {
  margin-top: 4px;
  font-size: 12px;
  line-height: 1.45;
  color: var(--qss-text-secondary);
}
.qsc-summary {
  display: -webkit-box;
  -webkit-line-clamp: 4;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

.qsc-side-btn {
  max-width: 100%;
  white-space: normal;
  height: auto;
  min-height: 24px;
  align-self: flex-start;
  margin-top: 4px;
  text-decoration: none;
}

.qsc-side-track {
  display: inline-block;
  margin-right: 4px;
  padding: 0 5px;
  border: 1px solid var(--qss-border);
  border-radius: 4px;
  font-size: 11px;
  color: var(--qss-text);
}
.qsc-side-track.is-off {
  color: var(--qss-text-muted);
  border-style: dashed;
}

.qsc-ind {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 6px 8px;
  border: 1px solid var(--qss-border-subtle);
  border-radius: 8px;
  background: var(--qss-bg);
}
.qsc-ind-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 6px;
}
.qsc-ind-key {
  border: none;
  background: transparent;
  padding: 0;
  color: var(--qss-text);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.qsc-ind-key:hover {
  text-decoration: underline;
  text-underline-offset: 2px;
}
.qsc-ind-name {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11.5px;
  color: var(--qss-text-secondary);
}
.qsc-ind-hyp {
  font-size: 11.5px;
  line-height: 1.4;
  color: var(--qss-text-muted);
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.qsc-ind-params {
  font-size: 10.5px;
  color: var(--qss-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.qsc-helpers {
  font-size: 11px;
}

.qsc-ver {
  display: grid;
  grid-template-columns: 1fr auto;
  align-items: center;
  column-gap: 6px;
  padding: 4px 6px;
  border-radius: 7px;
  border: 1px solid transparent;
}
.qsc-ver:hover {
  background: var(--qss-bg-hover);
}
.qsc-ver.is-current {
  border-color: var(--qss-border-subtle);
}
.qsc-ver.is-comparing {
  border-color: var(--qss-warning);
}
.qsc-ver-main {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  border: none;
  background: transparent;
  padding: 0;
  color: var(--qss-text-secondary);
  cursor: pointer;
  text-align: left;
}
.qsc-ver-no {
  font-size: 12px;
  font-weight: 600;
  color: var(--qss-text);
  flex-shrink: 0;
}
.qsc-ver-when {
  font-size: 11px;
  white-space: nowrap;
}
.qsc-ver-author {
  height: 16px;
  padding: 0 6px;
  font-size: 10px;
}
.qsc-ver-check {
  color: var(--qss-success);
  font-size: 11px;
}
.qsc-ver-msg {
  grid-column: 1 / -1;
  font-size: 11px;
  color: var(--qss-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.qsc-ver-restore {
  grid-column: 2;
  grid-row: 1;
}
</style>
