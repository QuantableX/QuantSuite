/**
 * GitHub through the signed-in `gh` (card 23): whether it is there (the
 * switch in Settings → Source control, installed, signed in), each thread's
 * repository, the composer's `#` rows, and the GitHub view's repositories,
 * pull requests and issues. Lists are cached a minute; the view refreshes
 * on demand.
 */
import { defineStore } from 'pinia'
import { pilot } from '#pilot/utils/invoke'
import { usePilotStore } from '#pilot/stores/pilot'
import { useGitStore } from '#pilot/stores/git'
import { useThreadStore } from '#pilot/stores/thread'
import { useAppStore } from '#pilot/stores/app'
import type { GhIssue, GhPull, GhRef, GhRepo, GhRepoRow } from '#pilot/types'

const FRESH_MS = 60_000

function read(key: string, fallback: string): string {
  try {
    return localStorage.getItem(key) ?? fallback
  } catch {
    return fallback
  }
}
function write(key: string, value: string) {
  try { localStorage.setItem(key, value) } catch { /* private mode */ }
}

interface Listed<T> {
  rows: T[]
  at: number
  loading: boolean
  error: string
}
const blank = <T>(): Listed<T> => ({ rows: [], at: 0, loading: false, error: '' })

export const useGithubStore = defineStore('pilot/github', {
  state: () => ({
    /** `gh` installed and signed in; null until checked. */
    signedIn: null as boolean | null,
    statusError: '',
    /** Thread → its GitHub repository (null: not on GitHub). */
    repoOf: {} as Record<string, GhRepo | null>,
    /** `owner/name` → the `#` menu's recent rows. */
    recent: {} as Record<string, { at: number; rows: GhRef[] }>,
    repos: [] as GhRepoRow[],
    reposLoaded: false,
    /** The GitHub view's repository (a context id) and tab, remembered. */
    selected: read('qp-gh-repo', ''),
    tab: read('qp-gh-tab', 'pulls') as 'threads' | 'pulls' | 'issues',
    pulls: {} as Record<string, Listed<GhPull>>,
    issues: {} as Record<string, Listed<GhIssue>>,
  }),

  getters: {
    on(): boolean {
      return usePilotStore().settings?.github !== false
    },
    /** The GitHub view and the `#` menu exist. */
    available(): boolean {
      return this.on && this.signedIn === true
    },
  },

  actions: {
    async loadStatus() {
      if (!this.on) {
        this.signedIn = false
        return
      }
      try {
        await pilot('plugin:pilot|pilot_gh_status')
        this.signedIn = true
        this.statusError = ''
      } catch (e) {
        this.signedIn = false
        this.statusError = e instanceof Error ? e.message : String(e)
      }
    },

    async repoFor(threadId: string): Promise<GhRepo | null> {
      if (!this.available) return null
      if (threadId in this.repoOf) return this.repoOf[threadId] ?? null
      const repo = (await pilot<GhRepo | null>('plugin:pilot|pilot_gh_repo', { id: threadId }).catch(() => null)) ?? null
      this.repoOf[threadId] = repo
      return repo
    },

    /** The `#` menu: digits filter the recent rows, words search GitHub. */
    async search(threadId: string, query: string): Promise<GhRef[]> {
      const repo = await this.repoFor(threadId)
      if (!repo) return []
      const slug = `${repo.owner}/${repo.name}`
      const q = query.trim()
      const words = q && !/^\d+$/.test(q)
      const cached = this.recent[slug]
      let rows: GhRef[]
      if (!words && cached && Date.now() - cached.at < FRESH_MS) rows = cached.rows
      else {
        rows = (await pilot<GhRef[]>('plugin:pilot|pilot_gh_refs', { id: threadId, query: words ? q : '' }).catch(() => [])) ?? []
        if (!words) this.recent[slug] = { at: Date.now(), rows }
      }
      return q && !words ? rows.filter((r) => String(r.number).startsWith(q)) : rows
    },

    /** Number → state for the composer's colours. */
    statesFor(threadId: string): Record<number, string> {
      const repo = this.repoOf[threadId]
      const rows = repo ? (this.recent[`${repo.owner}/${repo.name}`]?.rows ?? []) : []
      return Object.fromEntries(rows.map((r) => [r.number, r.state]))
    },

    async loadRepos() {
      this.repos = (await pilot<GhRepoRow[]>('plugin:pilot|pilot_gh_repos').catch(() => [])) ?? []
      this.reposLoaded = true
      if (!this.repos.some((r) => r.contextId === this.selected) && this.repos.length) this.select(this.repos[0]!.contextId)
    },

    select(contextId: string) {
      this.selected = contextId
      write('qp-gh-repo', contextId)
    },
    setTab(tab: 'threads' | 'pulls' | 'issues') {
      this.tab = tab
      write('qp-gh-tab', tab)
    },

    async loadPulls(contextId: string, force = false, search = '') {
      // Through the store's proxy, so the view sees every change.
      if (!this.pulls[contextId]) this.pulls[contextId] = blank<GhPull>()
      const l = this.pulls[contextId]!
      if (!force && !search && l.at && Date.now() - l.at < FRESH_MS) return
      l.loading = true
      try {
        l.rows = (await pilot<GhPull[]>('plugin:pilot|pilot_gh_pulls', { contextId, search: search || null })) ?? []
        l.error = ''
        l.at = search ? 0 : Date.now()
      } catch (e) {
        l.error = e instanceof Error ? e.message : String(e)
      } finally {
        l.loading = false
      }
    },

    async loadIssues(contextId: string, force = false, search = '') {
      if (!this.issues[contextId]) this.issues[contextId] = blank<GhIssue>()
      const l = this.issues[contextId]!
      if (!force && !search && l.at && Date.now() - l.at < FRESH_MS) return
      l.loading = true
      try {
        l.rows = (await pilot<GhIssue[]>('plugin:pilot|pilot_gh_issues', { contextId, search: search || null })) ?? []
        l.error = ''
        l.at = search ? 0 : Date.now()
      } catch (e) {
        l.error = e instanceof Error ? e.message : String(e)
      } finally {
        l.loading = false
      }
    },

    async merge(contextId: string, number: number): Promise<string> {
      const done = (await pilot<string>('plugin:pilot|pilot_gh_merge', { contextId, number })) ?? ''
      await this.loadPulls(contextId, true)
      return done
    },

    /**
     * A thread for a pull request or issue: in the workspace (a worktree for
     * work on a branch), linked to the item, then either sent at once or left
     * as a draft; the main area switches to it.
     */
    async startThread(
      contextId: string,
      item: { kind: 'pr' | 'issue'; number: number; url: string; title: string; state: string },
      prompt: string,
      opts: { send: boolean; worktree: boolean; prefill?: (threadId: string, text: string) => void }
    ): Promise<string | null> {
      const store = usePilotStore()
      const meta = await store.createThread({ contextId, title: `#${item.number} ${item.title}`.slice(0, 60), workspaceMode: opts.worktree ? 'worktree' : undefined })
      if (!meta) return null
      await pilot('plugin:pilot|pilot_gh_link', { id: meta.id, kind: item.kind, number: item.number, url: item.url, title: item.title, itemState: item.state }).catch(() => null)
      await useGitStore().loadLinks()
      if (!opts.send) opts.prefill?.(meta.id, prompt)
      useAppStore().setView('conversation')
      if (opts.send) await useThreadStore().send(meta.id, prompt)
      return meta.id
    },
  },
})
