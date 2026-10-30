<script setup lang="ts">
/**
 * The GitHub view (card 23): the workspaces whose `origin` is on GitHub,
 * and per repository three tabs — its Threads (and a box to start one),
 * open Pull requests (review state, checks, size; Review, Open in thread,
 * Fix failing checks, Address review comments, Merge when ready) and open
 * Issues (Solve, Open in thread). Every thread started here is an ordinary
 * thread linked to its item; its prompt names `#123`, so the item goes
 * along as GitHub context. A row whose item has a thread offers Thread.
 * Everything runs through the signed-in `gh`; nothing else talks to GitHub.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { CircleDot, ExternalLink, GitPullRequest, LoaderCircle, MoreHorizontal, RefreshCw } from 'lucide-vue-next'
import { usePilotStore } from '#pilot/stores/pilot'
import { useAppStore } from '#pilot/stores/app'
import { useGitStore } from '#pilot/stores/git'
import { useGithubStore } from '#pilot/stores/github'
import { useThreadStore } from '#pilot/stores/thread'
import { useSidebarStore } from '#pilot/stores/sidebar'
import { openExternal } from '#pilot/utils/external'
import { timeAgo } from '#pilot/utils/format'
import { saveDrafts, useDrafts } from '#pilot/utils/drafts'
import { addressReviewPrompt, draftAbout, fixChecksPrompt, REVIEW_LABEL, reviewPrompt, sizeOf, solvePrompt, threadFor } from '#pilot/utils/github'
import type { MenuOption } from '#pilot/utils/models'
import type { GhIssue, GhPull } from '#pilot/types'

const store = usePilotStore()
const app = useAppStore()
const git = useGitStore()
const github = useGithubStore()
const threads = useThreadStore()
const sidebar = useSidebarStore()
const drafts = useDrafts()

const current = computed(() => github.repos.find((r) => r.contextId === github.selected) ?? null)
const pulls = computed(() => (current.value ? github.pulls[current.value.contextId] : undefined))
const issues = computed(() => (current.value ? github.issues[current.value.contextId] : undefined))
const repoThreads = computed(() =>
  current.value
    ? store.sessions
        .filter((s) => s.contextId === current.value!.contextId && s.kind === 'chat')
        .sort((a, b) => (b.lastActivityAt ?? 0) - (a.lastActivityAt ?? 0))
        .slice(0, 60)
    : []
)
const linksOf = (id: string) => (git.links[id] ?? []).filter((l) => l.kind === 'pr' || l.kind === 'issue')

const search = ref('')
const task = ref('')
const busy = ref<string | null>(null)
const notice = ref<{ text: string; error: boolean } | null>(null)

function load(force = false) {
  const r = current.value
  if (!r) return
  if (github.tab === 'pulls') void github.loadPulls(r.contextId, force, search.value.trim())
  else if (github.tab === 'issues') void github.loadIssues(r.contextId, force, search.value.trim())
}
watch(() => [github.selected, github.tab] as const, () => {
  search.value = ''
  menuFor.value = null
  load()
})
onMounted(async () => {
  if (!github.reposLoaded) await github.loadRepos()
  load()
})

function openThread(id: string) {
  void store.openSession(id)
  app.setView('conversation')
}

const prItem = (pr: GhPull) => ({ kind: 'pr' as const, number: pr.number, url: pr.url, title: pr.title, state: pr.state })
const issueItem = (i: GhIssue) => ({ kind: 'issue' as const, number: i.number, url: i.url, title: i.title, state: 'open' })
const threadOfPr = (pr: GhPull) => (current.value ? threadFor(git.links, current.value.url, 'pr', pr.number) : null)
const threadOfIssue = (i: GhIssue) => (current.value ? threadFor(git.links, current.value.url, 'issue', i.number) : null)

function prefill(threadId: string, text: string) {
  drafts.value[threadId] = text
  saveDrafts(drafts.value)
}

async function start(key: string, item: ReturnType<typeof prItem> | ReturnType<typeof issueItem>, prompt: string, send: boolean) {
  const r = current.value
  if (!r || busy.value) return
  busy.value = key
  notice.value = null
  menuFor.value = null
  try {
    const id = await github.startThread(r.contextId, item, prompt, { send, worktree: send, prefill })
    if (!id) notice.value = { text: store.lastError || 'The thread could not be created.', error: true }
  } finally {
    busy.value = null
  }
}

async function newThread() {
  const r = current.value
  const text = task.value.trim()
  if (!r || !text || busy.value) return
  busy.value = 'new'
  try {
    const meta = await store.createThread({ contextId: r.contextId })
    if (!meta) {
      notice.value = { text: store.lastError || 'The thread could not be created.', error: true }
      return
    }
    task.value = ''
    app.setView('conversation')
    await threads.send(meta.id, text)
  } finally {
    busy.value = null
  }
}

// ── The row menus ──
const menuFor = ref<string | null>(null)
function prMenu(pr: GhPull): MenuOption[] {
  return [
    { id: 'open', label: 'Open in thread', description: 'A new thread with the prompt ready for you to finish' },
    { id: 'fix', label: 'Fix failing checks', description: pr.checks.failing.join(', ') || undefined, disabled: pr.checks.failed === 0, reason: 'No check fails.' },
    { id: 'address', label: 'Address review comments', description: 'Make the requested changes on its branch', disabled: pr.review !== 'CHANGES_REQUESTED', reason: 'Nobody requested changes.' },
    { id: 'merge', label: 'Merge when ready', description: 'Checks pass, approved, merges cleanly', disabled: !pr.ready, reason: 'Needs passing checks, an approval and a clean merge.' },
  ]
}
async function prAction(pr: GhPull, action: string) {
  menuFor.value = null
  const key = `pr:${pr.number}`
  if (action === 'open') return start(key, prItem(pr), draftAbout('pr', pr.number, pr.title), false)
  if (action === 'fix') return start(key, prItem(pr), fixChecksPrompt(pr), true)
  if (action === 'address') return start(key, prItem(pr), addressReviewPrompt(pr), true)
  if (action === 'merge' && current.value) {
    if (!confirm(`Merge pull request #${pr.number} "${pr.title}" on GitHub?`)) return
    busy.value = key
    try {
      notice.value = { text: await github.merge(current.value.contextId, pr.number), error: false }
    } catch (e) {
      notice.value = { text: e instanceof Error ? e.message : String(e), error: true }
    } finally {
      busy.value = null
    }
  }
}
const issueMenu: MenuOption[] = [{ id: 'open', label: 'Open in thread', description: 'A new thread with the prompt ready for you to finish' }]

// A click elsewhere closes an open row menu.
function onPointer(e: PointerEvent) {
  if (menuFor.value && !(e.target as Element | null)?.closest('.qp-ghv-more')) menuFor.value = null
}
watch(menuFor, (open) => (open ? window.addEventListener('pointerdown', onPointer, true) : window.removeEventListener('pointerdown', onPointer, true)))
onBeforeUnmount(() => window.removeEventListener('pointerdown', onPointer, true))

const CHECK_TITLE = (pr: GhPull) =>
  pr.checks.total ? `${pr.checks.passed} passed, ${pr.checks.failed} failed, ${pr.checks.pending} running${pr.checks.failing.length ? ` — failing: ${pr.checks.failing.join(', ')}` : ''}` : 'No checks'
</script>

<template>
  <div class="qp-ghv">
    <aside class="qp-ghv-repos" aria-label="Repositories">
      <div class="qp-ghv-side-head">Repositories</div>
      <button
        v-for="r in github.repos"
        :key="r.contextId"
        class="qp-ghv-repo"
        :class="{ on: r.contextId === github.selected }"
        :title="r.path"
        @click="github.select(r.contextId)"
      >
        <span class="qp-ghv-repo-name">{{ r.name }}</span>
        <span class="qp-ghv-repo-slug">{{ r.repo.owner }}/{{ r.repo.name }}</span>
      </button>
      <p v-if="github.reposLoaded && !github.repos.length" class="qp-ghv-none">
        No workspace has a GitHub remote. Open a folder whose <code>origin</code> is on GitHub as a workspace.
      </p>
    </aside>

    <section v-if="current" class="qp-ghv-main">
      <header class="qp-ghv-bar">
        <span class="qp-ghv-title">{{ current.repo.owner }}/{{ current.repo.name }}</span>
        <button class="qp-ghv-icon" title="Open on GitHub" aria-label="Open on GitHub" @click="openExternal(current.url)"><ExternalLink :size="13" /></button>
        <div class="qp-ghv-tabs" role="tablist">
          <button role="tab" :aria-selected="github.tab === 'threads'" :class="{ on: github.tab === 'threads' }" @click="github.setTab('threads')">Threads</button>
          <button role="tab" :aria-selected="github.tab === 'pulls'" :class="{ on: github.tab === 'pulls' }" @click="github.setTab('pulls')">
            Pull requests <span v-if="pulls?.at" class="qp-ghv-count">{{ pulls.rows.length }}</span>
          </button>
          <button role="tab" :aria-selected="github.tab === 'issues'" :class="{ on: github.tab === 'issues' }" @click="github.setTab('issues')">
            Issues <span v-if="issues?.at" class="qp-ghv-count">{{ issues.rows.length }}</span>
          </button>
        </div>
        <div class="qp-ghv-spacer" />
        <input
          v-if="github.tab !== 'threads'"
          v-model="search"
          class="qp-ghv-search"
          :placeholder="github.tab === 'pulls' ? 'Search pull requests — Enter' : 'Search issues — Enter'"
          spellcheck="false"
          @keydown.enter="load(true)"
        />
        <button v-if="github.tab !== 'threads'" class="qp-ghv-icon" title="Refresh" aria-label="Refresh" @click="load(true)">
          <RefreshCw :size="13" :class="{ 'qp-ghv-spin': (github.tab === 'pulls' ? pulls : issues)?.loading }" />
        </button>
      </header>
      <p v-if="notice" class="qp-ghv-notice" :class="{ 'is-error': notice.error }" role="status">
        <span>{{ notice.text }}</span>
        <button class="qp-btn qp-btn-sm" @click="notice = null">Dismiss</button>
      </p>

      <!-- Threads -->
      <div v-if="github.tab === 'threads'" class="qp-ghv-list">
        <form class="qp-ghv-new" @submit.prevent="newThread">
          <input v-model="task" class="qp-ghv-new-input" :placeholder="`A new task in ${current.name}…`" spellcheck="false" />
          <button class="qp-btn qp-btn-sm qp-btn-primary" type="submit" :disabled="!task.trim() || busy === 'new'">Start</button>
        </form>
        <button v-for="t in repoThreads" :key="t.id" class="qp-ghv-row is-thread" @click="openThread(t.id)">
          <LoaderCircle v-if="sidebar.statusOf(t.id) === 'working'" class="qp-ghv-spin qp-ghv-working" :size="12" />
          <span v-else class="qp-ghv-dot" :class="`is-${sidebar.statusOf(t.id)}`" />
          <span class="qp-ghv-row-title">{{ t.title || 'New thread' }}</span>
          <PilotGitPrChip v-for="l in linksOf(t.id)" :key="l.url" :link="l" compact />
          <span class="qp-ghv-when">{{ timeAgo(t.lastActivityAt) }}</span>
        </button>
        <p v-if="!repoThreads.length" class="qp-ghv-empty">No threads in {{ current.name }} yet.</p>
      </div>

      <!-- Pull requests -->
      <div v-else-if="github.tab === 'pulls'" class="qp-ghv-list">
        <p v-if="pulls?.error" class="qp-ghv-empty is-error">{{ pulls.error }}</p>
        <p v-else-if="pulls?.loading && !pulls.rows.length" class="qp-ghv-empty">Loading pull requests…</p>
        <p v-else-if="pulls?.at !== undefined && !pulls.loading && !pulls.rows.length" class="qp-ghv-empty">No open pull requests{{ search ? ' match' : '' }}.</p>
        <div v-for="pr in pulls?.rows ?? []" :key="pr.number" class="qp-ghv-row" :data-pr="pr.number">
          <GitPullRequest class="qp-ghv-kind" :class="`is-${pr.state}`" :size="14" />
          <div class="qp-ghv-text">
            <button class="qp-ghv-row-title is-link" :title="`Open #${pr.number} on GitHub`" @click="openExternal(pr.url)">
              <span class="qp-ghv-num">#{{ pr.number }}</span> {{ pr.title }}
            </button>
            <div class="qp-ghv-meta">
              {{ pr.author }} · {{ pr.branch }} · {{ timeAgo(Date.parse(pr.updatedAt)) }}<template v-if="pr.state === 'draft'"> · draft</template>
            </div>
          </div>
          <span v-if="pr.review" class="qp-ghv-review" :class="`is-${pr.review.toLowerCase()}`">{{ REVIEW_LABEL[pr.review] }}</span>
          <span class="qp-ghv-checks" :title="CHECK_TITLE(pr)">
            <template v-if="pr.checks.total">
              <span v-if="pr.checks.passed" class="ok">✓ {{ pr.checks.passed }}</span>
              <span v-if="pr.checks.failed" class="bad">✗ {{ pr.checks.failed }}</span>
              <span v-if="pr.checks.pending" class="wait">◷ {{ pr.checks.pending }}</span>
            </template>
            <span v-else class="none">no checks</span>
          </span>
          <span class="qp-ghv-size">{{ sizeOf(pr) }}</span>
          <button v-if="threadOfPr(pr)" class="qp-btn qp-btn-sm" title="Go to its thread" @click="openThread(threadOfPr(pr)!)">Thread</button>
          <button
            v-else
            class="qp-btn qp-btn-sm"
            :disabled="busy !== null"
            title="A thread that checks the pull request out and reviews it"
            @click="start(`pr:${pr.number}`, prItem(pr), reviewPrompt(pr), true)"
          >
            {{ busy === `pr:${pr.number}` ? 'Starting…' : 'Review' }}
          </button>
          <span class="qp-ghv-more">
            <button class="qp-ghv-icon" :aria-label="`More for #${pr.number}`" @click="menuFor = menuFor === `pr:${pr.number}` ? null : `pr:${pr.number}`">
              <MoreHorizontal :size="14" />
            </button>
            <div v-if="menuFor === `pr:${pr.number}`" class="qp-ghv-pop">
              <PilotThreadMenuList :options="prMenu(pr)" selected="" :title="`#${pr.number}`" @pick="(id: string) => prAction(pr, id)" @close="menuFor = null" />
            </div>
          </span>
        </div>
      </div>

      <!-- Issues -->
      <div v-else class="qp-ghv-list">
        <p v-if="issues?.error" class="qp-ghv-empty is-error">{{ issues.error }}</p>
        <p v-else-if="issues?.loading && !issues.rows.length" class="qp-ghv-empty">Loading issues…</p>
        <p v-else-if="issues?.at !== undefined && !issues.loading && !issues.rows.length" class="qp-ghv-empty">No open issues{{ search ? ' match' : '' }}.</p>
        <div v-for="i in issues?.rows ?? []" :key="i.number" class="qp-ghv-row" :data-issue="i.number">
          <CircleDot class="qp-ghv-kind is-open" :size="14" />
          <div class="qp-ghv-text">
            <button class="qp-ghv-row-title is-link" :title="`Open #${i.number} on GitHub`" @click="openExternal(i.url)">
              <span class="qp-ghv-num">#{{ i.number }}</span> {{ i.title }}
            </button>
            <div class="qp-ghv-meta">
              {{ i.author }} · {{ timeAgo(Date.parse(i.updatedAt)) }}<template v-if="i.comments"> · {{ i.comments }} comment{{ i.comments === 1 ? '' : 's' }}</template>
            </div>
          </div>
          <span v-for="l in i.labels.slice(0, 3)" :key="l" class="qp-ghv-label">{{ l }}</span>
          <button v-if="threadOfIssue(i)" class="qp-btn qp-btn-sm" title="Go to its thread" @click="openThread(threadOfIssue(i)!)">Thread</button>
          <button
            v-else
            class="qp-btn qp-btn-sm"
            :disabled="busy !== null"
            title="A thread that fixes the issue on a branch of its own"
            @click="start(`issue:${i.number}`, issueItem(i), solvePrompt(i), true)"
          >
            {{ busy === `issue:${i.number}` ? 'Starting…' : 'Solve' }}
          </button>
          <span class="qp-ghv-more">
            <button class="qp-ghv-icon" :aria-label="`More for #${i.number}`" @click="menuFor = menuFor === `issue:${i.number}` ? null : `issue:${i.number}`">
              <MoreHorizontal :size="14" />
            </button>
            <div v-if="menuFor === `issue:${i.number}`" class="qp-ghv-pop">
              <PilotThreadMenuList
                :options="issueMenu"
                selected=""
                :title="`#${i.number}`"
                @pick="() => start(`issue:${i.number}`, issueItem(i), draftAbout('issue', i.number, i.title), false)"
                @close="menuFor = null"
              />
            </div>
          </span>
        </div>
      </div>
    </section>
    <section v-else class="qp-ghv-main qp-ghv-blank">
      <p class="qp-ghv-empty">{{ github.reposLoaded ? 'Pick a repository.' : 'Looking for repositories…' }}</p>
    </section>
  </div>
</template>

<style scoped>
.qp-ghv { flex: 1; min-height: 0; display: flex; min-width: 0; }
.qp-ghv-repos {
  flex-shrink: 0;
  width: 220px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 8px;
  border-right: 1px solid var(--qss-border-subtle, #35353d);
  overflow-y: auto;
}
.qp-ghv-side-head { padding: 4px 8px 6px; font-size: 11.5px; font-weight: 600; color: var(--qss-text-secondary, #9a9aa5); }
.qp-ghv-repo {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding: 6px 8px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  text-align: left;
  cursor: pointer;
}
.qp-ghv-repo:hover { background: var(--qss-bg-hover, #313139); }
.qp-ghv-repo.on { background: var(--qss-bg-hover, #313139); }
.qp-ghv-repo-name { font-size: 12.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-ghv-repo-slug { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-ghv-none { margin: 6px 8px; font-size: 12px; line-height: 1.5; color: var(--qss-text-muted, #6e6e7a); }
.qp-ghv-none code { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; }
.qp-ghv-main { flex: 1; min-width: 0; min-height: 0; display: flex; flex-direction: column; }
.qp-ghv-blank { align-items: center; justify-content: center; }
.qp-ghv-bar {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 14px;
  border-bottom: 1px solid var(--qss-border-subtle, #35353d);
}
.qp-ghv-title { font-size: 13px; font-weight: 600; color: var(--qss-text, #d4d4d8); white-space: nowrap; }
.qp-ghv-tabs { display: inline-flex; gap: 2px; margin-left: 8px; }
.qp-ghv-tabs button {
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-secondary, #9a9aa5);
  font: inherit;
  font-size: 12px;
  padding: 4px 10px;
  cursor: pointer;
}
.qp-ghv-tabs button:hover { color: var(--qss-text, #d4d4d8); }
.qp-ghv-tabs button.on { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-ghv-count { margin-left: 3px; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 10.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-ghv-spacer { flex: 1; }
.qp-ghv-search {
  width: 260px;
  min-width: 0;
  padding: 4px 9px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12px;
  outline: none;
}
.qp-ghv-search:focus { border-color: var(--qss-accent, #a0a0a8); }
.qp-ghv-icon { display: inline-flex; padding: 4px; border: none; border-radius: 6px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-ghv-icon:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-ghv-spin { animation: qp-ghv-spin 900ms linear infinite; }
@keyframes qp-ghv-spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) {
  .qp-ghv-spin { animation-duration: 2.4s; }
}
.qp-ghv-notice {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 8px 14px 0;
  padding: 4px 6px 4px 10px;
  border: 1px solid var(--qss-border-subtle, #35353d);
  border-radius: 7px;
  font-size: 12px;
  color: var(--qss-text, #d4d4d8);
}
.qp-ghv-notice span { flex: 1; min-width: 0; }
.qp-ghv-notice.is-error { border-color: color-mix(in srgb, var(--qss-error, #f87171) 40%, transparent); background: color-mix(in srgb, var(--qss-error, #f87171) 9%, transparent); }
.qp-ghv-list { flex: 1; min-height: 0; overflow-y: auto; padding: 8px 14px 14px; display: flex; flex-direction: column; gap: 2px; }
.qp-ghv-new { display: flex; gap: 8px; margin-bottom: 8px; }
.qp-ghv-new-input {
  flex: 1;
  min-width: 0;
  padding: 6px 10px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 7px;
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12.5px;
  outline: none;
}
.qp-ghv-new-input:focus { border-color: var(--qss-accent, #a0a0a8); }
.qp-ghv-row {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
  padding: 7px 8px;
  border: none;
  border-radius: 7px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12.5px;
  text-align: left;
}
.qp-ghv-row:hover { background: color-mix(in srgb, var(--qss-bg-hover, #313139) 60%, transparent); }
.qp-ghv-row.is-thread { cursor: pointer; }
.qp-ghv-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
.qp-ghv-row-title { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.is-thread .qp-ghv-row-title { flex: 1; }
.qp-ghv-row-title.is-link { border: none; background: transparent; color: inherit; font: inherit; text-align: left; padding: 0; cursor: pointer; }
.qp-ghv-row-title.is-link:hover { text-decoration: underline; }
.qp-ghv-num { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-ghv-meta { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-ghv-kind { flex-shrink: 0; }
.qp-ghv-kind.is-open { color: var(--qss-success, #34d399); }
.qp-ghv-kind.is-draft { color: var(--qss-text-muted, #6e6e7a); }
.qp-ghv-review { flex-shrink: 0; font-size: 11px; padding: 1px 7px; border-radius: 999px; border: 1px solid var(--qss-border-subtle, #35353d); color: var(--qss-text-secondary, #9a9aa5); }
.qp-ghv-review.is-approved { color: var(--qss-success, #34d399); }
.qp-ghv-review.is-changes_requested { color: var(--qss-error, #f87171); }
.qp-ghv-checks { flex-shrink: 0; display: inline-flex; gap: 6px; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; min-width: 76px; justify-content: flex-end; }
.qp-ghv-checks .ok { color: var(--qss-success, #34d399); }
.qp-ghv-checks .bad { color: var(--qss-error, #f87171); }
.qp-ghv-checks .wait { color: var(--qss-warning, #d29a3f); }
.qp-ghv-checks .none { font-family: var(--qss-font-sans, system-ui, sans-serif); color: var(--qss-text-muted, #6e6e7a); }
.qp-ghv-size { flex-shrink: 0; min-width: 76px; text-align: right; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-ghv-label { flex-shrink: 0; max-width: 120px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; padding: 1px 7px; border-radius: 999px; border: 1px solid var(--qss-border-subtle, #35353d); color: var(--qss-text-secondary, #9a9aa5); }
.qp-ghv-more { position: relative; flex-shrink: 0; }
.qp-ghv-pop { position: absolute; top: calc(100% + 4px); right: 0; z-index: 30; }
.qp-ghv-dot { flex-shrink: 0; width: 7px; height: 7px; border-radius: 50%; background: var(--qss-text-muted, #6e6e7a); }
.qp-ghv-dot.is-waiting,
.qp-ghv-dot.is-limited { background: var(--qss-warning, #d29a3f); }
.qp-ghv-dot.is-error { background: var(--qss-error, #f87171); }
.qp-ghv-working { flex-shrink: 0; color: var(--qss-success, #34d399); }
.qp-ghv-when { flex-shrink: 0; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-ghv-empty { margin: 16px 8px; font-size: 12.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-ghv-empty.is-error { color: var(--qss-error, #f87171); }
</style>
