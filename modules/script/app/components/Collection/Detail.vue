<script setup lang="ts">
/**
 * One catalog item: what it is, its five versions — Standard, Optimized
 * (general), Optimized 1H / 4H / 1D — with parameters and per-track
 * verdicts, what it requires, its README and changelog; Install / Update /
 * Remove, and Open script once it is in the library.
 */
import { Download, ExternalLink, Trash2 } from 'lucide-vue-next'
import { useCatalogStore } from '#script/stores/catalog'
import { useWorkbenchStore } from '#script/stores/workbench'
import { COLLECTION_TRACKS, itemState, roleTrack, shortCommit } from '#script/utils/catalog'
import { decisionLabel } from '#script/utils/forge'
import { formatParams } from '#script/utils/format'
import type { CollectionManifestVersion, VersionRole } from '#script/types'

const store = useCatalogStore()
const wb = useWorkbenchStore()
const router = useRouter()

const ROLES: { role: VersionRole; label: string }[] = [
  { role: 'standard', label: 'Standard' },
  { role: 'optimized', label: 'Optimized (general)' },
  { role: 'optimized_1h', label: 'Optimized 1H' },
  { role: 'optimized_4h', label: 'Optimized 4H' },
  { role: 'optimized_1d', label: 'Optimized 1D' },
]

const item = computed(() => (store.selectedKey ? store.byKey.get(store.selectedKey) ?? null : null))
const manifest = computed(() => (store.detail && store.detail.manifest.key === store.selectedKey ? store.detail.manifest : null))
const local = computed(() => item.value?.local ?? null)
const state = computed(() => (item.value ? itemState(item.value.local, item.value.version) : null))
const installed = computed(() => !!local.value?.installed_version)
/** 'install' | 'update' | 'remove' while its dialog is open. */
const dialog = ref<'install' | 'update' | 'remove' | null>(null)
const done = ref<string | null>(null)

watch(() => store.selectedKey, () => {
  done.value = null
})

const standardParams = computed(() => manifest.value?.versions?.standard?.params ?? {})

function versionParams(role: VersionRole, v: CollectionManifestVersion): string {
  if (role === 'standard') return formatParams(v.params) || 'parameter-free'
  const changed = Object.fromEntries(
    Object.entries(v.params).filter(([k, value]) => JSON.stringify(value) !== JSON.stringify(standardParams.value[k])),
  )
  return Object.keys(changed).length ? formatParams(changed) : 'same as Standard'
}

function why(role: VersionRole, v: CollectionManifestVersion): string {
  if (role === 'standard') return 'the normal parameters'
  return decisionLabel(v.decision)
}

function requirementState(key: string): string {
  const req = store.byKey.get(key)
  if (!req) return 'not in the catalog'
  if (req.local?.installed_version) return `installed ${req.local.installed_version}`
  if (req.local?.conflict) return 'in your library (not from the Collection)'
  return 'installed with it'
}

async function openScript() {
  if (!item.value) return
  await wb.loadListing(true)
  await wb.openScript(`${item.value.key}.py`)
  void router.push('/script')
}

function finished(message: string) {
  dialog.value = null
  done.value = message
  // The tree and the library list the new (or removed) scripts.
  void wb.loadListing(true)
}
</script>

<template>
  <section class="qst-detail qsc-card is-flush" aria-label="Indicator">
    <div v-if="!store.selectedKey" class="qst-placeholder">
      <p>Pick an indicator to see its versions, requirements and README.</p>
    </div>
    <template v-else-if="item">
      <header class="qst-detail-head">
        <div class="qst-detail-title">
          <h2>{{ item.name }}</h2>
          <p><span class="mono">{{ item.key }}</span> · {{ item.type === 'library' ? 'library' : 'indicator' }} · <span class="mono">v{{ item.version }}</span></p>
        </div>
        <div class="qst-detail-actions">
          <span v-if="state" class="qsc-chip" :class="state.tone" :title="state.title">{{ state.label }}</span>
          <button v-if="installed" class="qsc-btn is-sm" :disabled="!!store.busy" title="The script in the editor" @click="openScript">
            <ExternalLink :size="13" /> Open script
          </button>
          <button
            v-if="!installed"
            class="qsc-btn is-sm is-primary"
            :disabled="!!store.busy || local?.too_new"
            @click="dialog = 'install'"
          >
            <Download :size="13" /> Install
          </button>
          <button v-else-if="local?.update" class="qsc-btn is-sm is-primary" :disabled="!!store.busy || local?.too_new" @click="dialog = 'update'">
            <Download :size="13" /> Update
          </button>
          <button v-if="installed" class="qsc-btn is-sm is-danger" :disabled="!!store.busy" @click="dialog = 'remove'">
            <Trash2 :size="13" /> Remove
          </button>
        </div>
      </header>

      <div class="qst-detail-body">
        <div v-if="done" class="qsc-note is-ok" role="status">
          <p>{{ done }}</p>
          <p class="muted">A running QuantSystems engine loads the indicators at start — restart it to use the new ones there.</p>
        </div>
        <p v-if="store.detailLoading && !manifest" class="muted qsc-pulse">Reading the package…</p>
        <div v-else-if="store.detailError" class="qsc-note is-error" role="alert">{{ store.detailError }}</div>

        <template v-if="manifest">
          <p class="qst-description">{{ manifest.description || manifest.summary }}</p>

          <div v-if="manifest.versions" class="qst-block">
            <h3>Versions</h3>
            <table class="qsc-table qst-versions">
              <thead>
                <tr>
                  <th>Version</th>
                  <th>Parameters</th>
                  <th v-for="tf in COLLECTION_TRACKS" :key="tf" class="num">{{ tf }}</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="r in ROLES" :key="r.role" :class="{ 'is-empty': !manifest.versions[r.role] }">
                  <td>
                    <div>{{ r.label }}</div>
                    <div class="qst-sub">
                      <template v-if="manifest.versions[r.role]"><span class="mono">{{ manifest.versions[r.role]!.key }}</span> · {{ why(r.role, manifest.versions[r.role]!) }}</template>
                      <template v-else>not in this package</template>
                    </div>
                  </td>
                  <td class="mono qst-params">{{ manifest.versions[r.role] ? versionParams(r.role, manifest.versions[r.role]!) : '' }}</td>
                  <td
                    v-for="tf in COLLECTION_TRACKS"
                    :key="tf"
                    class="num mono"
                    :class="{ 'qst-own': roleTrack(r.role) === tf }"
                    :title="manifest.versions[r.role]?.evidence[tf] ? `${manifest.versions[r.role]!.evidence[tf]!.grade}${manifest.versions[r.role]!.evidence[tf]!.date ? ' · ' + manifest.versions[r.role]!.evidence[tf]!.date : ''}` : 'not measured on this track'"
                  >
                    <span v-if="manifest.versions[r.role]?.evidence[tf]" :class="{ 'qsc-ok': manifest.versions[r.role]!.evidence[tf]!.certified }">{{
                      manifest.versions[r.role]!.evidence[tf]!.score.toFixed(1)
                    }}</span>
                    <span v-else class="muted">—</span>
                  </td>
                </tr>
              </tbody>
            </table>
            <p class="qst-hint">Pick the version that matches the bot's timeframe — 1H for 1h bots, 4H for 4h, 1D for daily. Green: certified on that track.</p>
          </div>

          <div class="qst-block">
            <h3>Requirements</h3>
            <p v-if="!manifest.requires.length" class="qst-hint">None — the script stands on its own.</p>
            <ul v-else class="qst-reqs">
              <li v-for="r in manifest.requires" :key="r">
                <button class="qsc-link" @click="store.openItem(r)">{{ store.byKey.get(r)?.name ?? r }}</button>
                <span class="mono muted">{{ r }}</span>
                <span class="muted">{{ requirementState(r) }}</span>
              </li>
            </ul>
          </div>

          <div v-if="store.detail?.readme" class="qst-block qst-readme">
            <h3>README</h3>
            <QMarkdownPreview :source="store.detail.readme" tokens="qss" />
          </div>

          <div class="qst-block">
            <h3>Changelog</h3>
            <ul class="qst-changelog">
              <li v-for="c in [...manifest.changelog].reverse()" :key="c.version">
                <span class="mono">{{ c.version }}</span>
                <span class="muted">{{ c.date }}</span>
                <span>{{ c.note }}</span>
              </li>
            </ul>
          </div>

          <p class="qst-meta">
            {{ manifest.author }} · {{ manifest.license }} · contract <span class="mono">{{ manifest.contract }}</span>
            <template v-if="manifest.warmup_bars != null"> · warm-up <span class="mono">{{ manifest.warmup_bars }}</span> bars</template>
            · {{ store.source?.name }} @ <span class="mono">{{ shortCommit(store.detail?.commit) }}</span>
            <template v-if="local?.installed_commit"> · installed from <span class="mono">{{ shortCommit(local.installed_commit) }}</span></template>
          </p>
        </template>
      </div>
    </template>

    <ScriptCollectionInstallModal
      v-if="dialog && item"
      :item="item"
      :mode="dialog"
      @close="dialog = null"
      @done="finished"
    />
  </section>
</template>

<style scoped>
.qst-detail {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}
.qst-placeholder {
  display: flex;
  align-items: center;
  justify-content: center;
  flex: 1;
  padding: 30px;
  font-size: 12px;
  color: var(--qss-text-muted);
  text-align: center;
}
.qst-detail-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 14px 16px;
  border-bottom: 1px solid var(--qss-border-subtle);
  flex-wrap: wrap;
}
.qst-detail-title {
  min-width: 0;
}
.qst-detail-title h2 {
  font-size: 15px;
  font-weight: 600;
  color: var(--qss-text);
}
.qst-detail-title p {
  margin-top: 2px;
  font-size: 11.5px;
  color: var(--qss-text-muted);
}
.qst-detail-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}
.qst-detail-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 14px 16px 18px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.qst-description {
  font-size: 12.5px;
  line-height: 1.55;
  color: var(--qss-text);
}
.qst-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.qst-block h3 {
  font-size: 12px;
  font-weight: 600;
  color: var(--qss-text-secondary);
}
.qst-versions {
  font-size: 12px;
}
.qst-versions tr.is-empty td {
  opacity: 0.55;
}
.qst-sub {
  font-size: 11px;
  color: var(--qss-text-muted);
}
.qst-params {
  font-size: 11.5px;
  white-space: normal;
  word-break: break-word;
}
.qst-own {
  font-weight: 600;
}
.qst-hint,
.qst-meta {
  font-size: 11.5px;
  line-height: 1.5;
  color: var(--qss-text-muted);
}
.qst-reqs,
.qst-changelog {
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 12px;
}
.qst-reqs li,
.qst-changelog li {
  display: flex;
  gap: 10px;
  align-items: baseline;
  flex-wrap: wrap;
}
.qst-readme :deep(h1) {
  font-size: 14px;
}
</style>
