<script setup lang="ts">
/**
 * The toolkit's source control section.
 *
 * What an editor's right edge is for — the branch you are on, the files you
 * have touched, and now the history behind them. `qs.files.gitStatus` is the
 * same `git2` call QuantCanvas' diff window uses, `gitLog`/`gitCommitFiles`
 * sit right beside it; nothing new reaches disk here.
 *
 * Refreshed on activation and after every save, not on a timer: a poll
 * against a large repository is noticeable, and nothing else changes the
 * working tree while you are typing in it. The tool's title lives in
 * `RightPanel`'s switcher; the branch row here doubles as the toolbar.
 */
import { qs, type GitCommitEntry, type GitCommitFile, type GitFileEntry } from '@quantsuite/core'

const props = defineProps<{ root: string | null }>()
const emit = defineEmits<{ (e: 'open', path: string): void }>()

const branch = ref('')
const ahead = ref(0)
const behind = ref(0)
const files = ref<GitFileEntry[]>([])
const commits = ref<GitCommitEntry[]>([])
const loading = ref(false)
/** Null while unknown, a message once we know git is not usable here. */
const unavailable = ref<string | null>(null)

/** Which commit is unfolded, and the files of every commit ever unfolded. */
const expandedOid = ref<string | null>(null)
const commitFiles = ref<Map<string, GitCommitFile[]>>(new Map())

/** One row per file: a file staged AND modified comes back twice. */
const rows = computed(() => {
  const seen = new Map<string, GitFileEntry>()
  for (const f of files.value) {
    // The unstaged version wins the label — it is the newer edit.
    if (!seen.has(f.path) || !f.staged) seen.set(f.path, f)
  }
  return [...seen.values()].sort((a, b) => a.path.localeCompare(b.path))
})

async function refresh() {
  if (!props.root) {
    files.value = []
    commits.value = []
    branch.value = ''
    unavailable.value = null
    return
  }
  loading.value = true
  try {
    const status = await qs.files.gitStatus(props.root)
    branch.value = status.branch
    ahead.value = status.ahead
    behind.value = status.behind
    files.value = status.files
    unavailable.value = null
    // History rides along with every status refresh — a commit made in a
    // terminal shows up on the next save or activation, not never.
    commits.value = await qs.files.gitLog(props.root, 30).catch(() => [])
    commitFiles.value = new Map()
  } catch (e) {
    // Not a repository, or no backend. Say which rather than showing an
    // empty list that reads as "no changes".
    files.value = []
    commits.value = []
    branch.value = ''
    unavailable.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

watch(() => props.root, refresh, { immediate: true })
onActivated(refresh)

defineExpose({ refresh })

async function toggleCommit(oid: string) {
  if (expandedOid.value === oid) {
    expandedOid.value = null
    return
  }
  expandedOid.value = oid
  if (!commitFiles.value.has(oid) && props.root) {
    try {
      const list = await qs.files.gitCommitFiles(props.root, oid)
      commitFiles.value = new Map(commitFiles.value).set(oid, list)
    } catch {
      commitFiles.value = new Map(commitFiles.value).set(oid, [])
    }
  }
}

/** "2m", "3h", "4d" — the width a 220px column can afford. */
function relTime(unixSeconds: number): string {
  const s = Math.max(0, Math.floor(Date.now() / 1000) - unixSeconds)
  if (s < 60) return 'now'
  if (s < 3600) return `${Math.floor(s / 60)}m`
  if (s < 86400) return `${Math.floor(s / 3600)}h`
  if (s < 86400 * 30) return `${Math.floor(s / 86400)}d`
  return new Date(unixSeconds * 1000).toLocaleDateString()
}

/** First letter of the status, the way the explorer marks its rows. */
const LETTER: Record<string, string> = {
  modified: 'M',
  untracked: 'U',
  new: 'U',
  staged: 'S',
  added: 'A',
  deleted: 'D',
  renamed: 'R',
  conflicted: 'C',
}

function letterFor(status: string): string {
  return LETTER[status.toLowerCase()] ?? status.slice(0, 1).toUpperCase()
}

function nameOf(path: string): string {
  return path.split(/[/\\]/).pop() ?? path
}

function dirOf(path: string): string {
  const parts = path.replace(/\\/g, '/').split('/')
  parts.pop()
  return parts.join('/')
}

function absolute(path: string): string {
  if (/^([a-z]:|\/)/i.test(path)) return path
  return `${props.root ?? ''}/${path}`
}
</script>

<template>
  <div class="sc">
    <!-- Branch (or why there is none) and refresh, one row — the tool's own
         header now that the switcher above carries the title. -->
    <div v-if="root" class="sc-top">
      <template v-if="branch">
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="6" cy="6" r="2.5" /><circle cx="6" cy="18" r="2.5" /><circle cx="18" cy="8" r="2.5" />
          <path d="M6 8.5v7 M18 10.5v1a3 3 0 0 1-3 3H9" />
        </svg>
        <span class="sc-branch-name">{{ branch }}</span>
        <span v-if="ahead" class="sc-count" title="Commits ahead">↑{{ ahead }}</span>
        <span v-if="behind" class="sc-count" title="Commits behind">↓{{ behind }}</span>
      </template>
      <span v-else-if="unavailable" class="sc-top-note" :title="unavailable">Not a git repository.</span>
      <span v-else class="sc-top-note" />
      <button class="sc-refresh" :disabled="loading" title="Refresh" @click="refresh">
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
          <polyline points="23 4 23 10 17 10" /><path d="M20.49 15a9 9 0 1 1-2.12-9.36L23 10" />
        </svg>
      </button>
    </div>

    <p v-if="!root" class="sc-empty">No workspace open.</p>
    <template v-else-if="!unavailable">
      <p v-if="!rows.length" class="sc-empty">No changes.</p>

      <div v-else class="sc-list">
        <button
          v-for="file in rows"
          :key="file.path"
          class="sc-row"
          :title="file.path"
          @click="emit('open', absolute(file.path))"
        >
          <span class="sc-letter" :data-status="file.status.toLowerCase()">{{ letterFor(file.status) }}</span>
          <span class="sc-name">{{ nameOf(file.path) }}</span>
          <span class="sc-dir">{{ dirOf(file.path) }}</span>
        </button>
      </div>

      <!-- History: the last commits, each unfolding into its files. -->
      <div v-if="commits.length" class="sc-history">
        <div class="sc-subhead">History</div>
        <template v-for="commit in commits" :key="commit.oid">
          <button
            class="sc-commit"
            :class="{ 'is-open': expandedOid === commit.oid }"
            :title="`${commit.summary}\n${commit.author} · ${commit.short}`"
            @click="toggleCommit(commit.oid)"
          >
            <span class="sc-commit-chevron">{{ expandedOid === commit.oid ? '▾' : '▸' }}</span>
            <span class="sc-commit-summary">{{ commit.summary }}</span>
            <span class="sc-commit-time">{{ relTime(commit.time) }}</span>
          </button>
          <div v-if="expandedOid === commit.oid" class="sc-commit-body">
            <div class="sc-commit-meta">
              <span class="sc-commit-hash">{{ commit.short }}</span>
              <span class="sc-commit-author">{{ commit.author }}</span>
            </div>
            <p v-if="!commitFiles.get(commit.oid)" class="sc-empty">Loading…</p>
            <p v-else-if="!commitFiles.get(commit.oid)!.length" class="sc-empty">No files.</p>
            <button
              v-for="file in commitFiles.get(commit.oid) ?? []"
              :key="file.path"
              class="sc-row"
              :title="`${file.path} — opens the current file, not the old version`"
              @click="emit('open', absolute(file.path))"
            >
              <span class="sc-letter" :data-status="file.status.toLowerCase()">{{ letterFor(file.status) }}</span>
              <span class="sc-name">{{ nameOf(file.path) }}</span>
              <span class="sc-dir">{{ dirOf(file.path) }}</span>
            </button>
          </div>
        </template>
      </div>
    </template>
  </div>
</template>

<style scoped>
.sc {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.sc-top {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
  padding: 6px 8px 6px 12px;
  font-size: 11.5px;
  color: var(--qss-text-secondary);
}
.sc-branch-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.sc-top-note {
  flex: 1;
  min-width: 0;
  color: var(--qss-text-muted);
}

.sc-refresh {
  display: grid;
  place-items: center;
  flex-shrink: 0;
  width: 20px;
  height: 20px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text-muted);
  cursor: pointer;
}
.sc-refresh:hover:not(:disabled) {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.sc-refresh:disabled {
  opacity: 0.35;
  cursor: default;
}
.sc-count {
  font-family: var(--qss-font-mono);
  font-size: 10px;
  color: var(--qss-text-muted);
}

.sc-empty {
  padding: 4px 12px 12px;
  font-size: 11.5px;
  color: var(--qss-text-muted);
}

.sc-list {
  padding-bottom: 8px;
}

.sc-row {
  display: flex;
  align-items: baseline;
  gap: 7px;
  width: 100%;
  padding: 3px 12px;
  border: none;
  background: transparent;
  color: var(--qss-text-secondary);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}
.sc-row:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}

.sc-letter {
  flex-shrink: 0;
  width: 12px;
  font-family: var(--qss-font-mono);
  font-size: 10px;
  font-weight: 700;
  text-align: center;
  color: var(--qss-text-muted);
}
/* The explorer's colours, so one file reads the same in both panels. */
.sc-letter[data-status='modified'] {
  color: #fbbf24;
}
.sc-letter[data-status='untracked'],
.sc-letter[data-status='new'],
.sc-letter[data-status='added'] {
  color: #4ade80;
}
.sc-letter[data-status='staged'] {
  color: #60a5fa;
}
.sc-letter[data-status='deleted'] {
  color: #f87171;
}
.sc-letter[data-status='renamed'] {
  color: #c084fc;
}

.sc-name {
  flex-shrink: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 55%;
}

.sc-dir {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 10px;
  color: var(--qss-text-muted);
  opacity: 0.7;
}

/* ---- History ---- */

.sc-subhead {
  padding: 8px 12px 4px;
  border-top: 1px solid var(--qss-border-subtle, var(--qss-border));
  font-size: 9.5px;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--qss-text-muted);
}

.sc-commit {
  display: flex;
  align-items: baseline;
  gap: 6px;
  width: 100%;
  padding: 3px 12px;
  border: none;
  background: transparent;
  color: var(--qss-text-secondary);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}
.sc-commit:hover,
.sc-commit.is-open {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}

.sc-commit-chevron {
  flex-shrink: 0;
  width: 10px;
  font-size: 9px;
  color: var(--qss-text-muted);
}

.sc-commit-summary {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sc-commit-time {
  flex-shrink: 0;
  font-family: var(--qss-font-mono);
  font-size: 10px;
  color: var(--qss-text-muted);
}

.sc-commit-body {
  padding: 2px 0 6px;
}

.sc-commit-meta {
  display: flex;
  gap: 8px;
  padding: 2px 12px 4px 28px;
  font-size: 10px;
  color: var(--qss-text-muted);
}
.sc-commit-hash {
  font-family: var(--qss-font-mono);
}
.sc-commit-author {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sc-commit-body .sc-row {
  padding-left: 28px;
}
</style>
