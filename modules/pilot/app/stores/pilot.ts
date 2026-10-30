/**
 * QuantPilot's store: the session list, what each session's CLI is doing,
 * and the plumbing between a sidebar row and the terminal in the middle.
 *
 * A session row is the CLI's session: `openSession` launches it in a PTY
 * (or resumes it, or just switches to the PTY it already runs in), the
 * page mounts a console pane on that PTY, and the face reads the signals
 * that come back — Claude's hooks, a tailed Codex or pi file, or, for
 * adapters that report nothing, the terminal's own output and bell.
 */
import { defineStore } from 'pinia'
import { navigateTo } from '#app'
import { bus } from '@quantsuite/core'
import { pilot } from '#pilot/utils/invoke'
import { useThreadStore } from '#pilot/stores/thread'
import { useSidebarStore } from '#pilot/stores/sidebar'
import { useDrawerStore } from '#pilot/stores/drawer'
import { providerLabel } from '#pilot/utils/format'
import { scanBells, type BellScanState } from '#pilot/utils/bell'
import type {
  AdapterStatus,
  LaunchResult,
  LiveState,
  Mode,
  PilotContext,
  PilotSettings,
  PilotSignal,
  PilotStatus,
  RuntimeInfo,
  SessionLive,
  SessionMeta,
  SignalKind,
} from '#pilot/types'

/** Output stopped for this long → an activity-only agent is idle again. */
const ACTIVITY_DECAY_MS = 1800
/** A bell counts as "needs you" until the terminal moves on for real. */
const BELL_HOLD_MS = 1000
const FILES_CAP = 60

const decayTimers = new Map<string, ReturnType<typeof setTimeout>>()
const bellAt = new Map<string, number>()
/** Per session: did the last output chunk end inside an OSC/DCS string? */
const bellScan = new Map<string, BellScanState>()

const BUILT_IN_SIGNALS: Record<string, SignalKind> = {
  claude: 'hooks',
  codex: 'transcript',
  pi: 'transcript',
  omp: 'transcript',
  opencode: 'activity',
  gemini: 'activity',
}

const BUILT_IN_ADAPTERS: AdapterStatus[] = ['claude', 'codex', 'pi', 'omp', 'opencode', 'gemini'].map((id) => ({
  id,
  label: providerLabel(id),
  custom: false,
  signals: BUILT_IN_SIGNALS[id] ?? null,
  found: true,
  path: null,
  version: null,
  loggedIn: null,
  detail: null,
}))

export function freshLive(signals: SignalKind = 'activity'): SessionLive {
  return {
    state: 'offline',
    detail: null,
    ptyId: null,
    alive: false,
    launching: false,
    display: null,
    error: null,
    tool: null,
    cheer: 0,
    turnRunning: false,
    files: [],
    vitals: {
      model: null,
      costUsd: null,
      contextPct: null,
      linesAdded: null,
      linesRemoved: null,
      inputTokens: null,
      outputTokens: null,
    },
    notice: null,
    signals,
    lastOutputAt: 0,
    bell: false,
  }
}

export const usePilotStore = defineStore('pilot/pilot', {
  state: () => ({
    loaded: false,
    status: null as PilotStatus | null,
    statusBusy: false,
    settings: null as PilotSettings | null,
    contexts: [] as PilotContext[],
    sessions: [] as SessionMeta[],
    activeId: null as string | null,
    /** The main area: the open thread, or the Usage page (card 18). */
    page: 'thread' as 'thread' | 'usage',
    /** Threads visited before and after the open one (Ctrl+[ / Ctrl+], card 17). */
    history: { back: [] as string[], forward: [] as string[] },
    live: {} as Record<string, SessionLive>,
    lastError: '',
    /** The stage's grid, measured by the page, so a launch draws at the right size first time. */
    stage: null as { cols: number; rows: number } | null,
    /** Rows whose V2 terminal is in front instead of the conversation. */
    terminalShown: {} as Record<string, boolean>,
  }),

  getters: {
    active(state): SessionMeta | null {
      return state.sessions.find((s) => s.id === state.activeId) ?? null
    },
    activeLive(state): SessionLive {
      return (state.activeId && state.live[state.activeId]) || freshLive()
    },
    /** Every adapter, from the status probe — the built-ins as a stand-in until it ran. */
    adapters(state): AdapterStatus[] {
      return state.status?.adapters ?? BUILT_IN_ADAPTERS
    },
    adapterLabel(): (id: string) => string {
      return (id) => providerLabel(id, this.adapters)
    },
    adapterReady(): (id: string) => boolean {
      return (id) => {
        const a = this.adapters.find((x) => x.id === id)
        return Boolean(a?.found) && a?.loggedIn !== false
      }
    },
    /** Sessions with a PTY this app run — the ones with a pane mounted. */
    launched(state): SessionMeta[] {
      return state.sessions.filter((s) => state.live[s.id]?.ptyId)
    },
    /** Every session whose agent is waiting on the user, wherever it is. */
    needsYou(state): SessionMeta[] {
      return state.sessions.filter((s) => state.live[s.id]?.state === 'waiting')
    },
    sessionOfPty(state): Record<string, string> {
      const out: Record<string, string> = {}
      for (const [sid, l] of Object.entries(state.live)) if (l.ptyId) out[l.ptyId] = sid
      return out
    },
  },

  actions: {
    signalsOf(provider: string): SignalKind {
      if (!provider) return 'activity'
      const a = this.adapters.find((x) => x.id === provider)
      return a?.signals ?? BUILT_IN_SIGNALS[provider] ?? 'activity'
    },

    ensureLive(id: string): SessionLive {
      if (!this.live[id]) {
        const meta = this.sessions.find((s) => s.id === id)
        this.live[id] = freshLive(this.signalsOf(meta?.provider ?? ''))
      }
      return this.live[id]!
    },

    // ── Loading ──

    async load() {
      await Promise.all([this.refreshContexts(), this.refreshSessions(), this.loadSettings()])
      await this.syncRuntime()
      this.loaded = true
      void this.refreshStatus()
    },

    async refreshSessions() {
      const list = await pilot<SessionMeta[]>('plugin:pilot|pilot_sessions_list')
      if (list) {
        for (const s of list) this.noticeSettled(s)
        this.sessions = list
        for (const s of list) this.ensureLive(s.id)
      }
    },

    async refreshContexts() {
      const list = await pilot<PilotContext[]>('plugin:pilot|pilot_contexts')
      if (list) this.contexts = list
    },

    async refreshStatus() {
      if (this.statusBusy) return
      this.statusBusy = true
      try {
        const s = await pilot<PilotStatus>('plugin:pilot|pilot_status')
        if (s) {
          this.status = s
          for (const [id, l] of Object.entries(this.live)) {
            const meta = this.sessions.find((x) => x.id === id)
            if (meta) l.signals = this.signalsOf(meta.provider)
          }
        }
      } finally {
        this.statusBusy = false
      }
    },

    async loadSettings() {
      const s = await pilot<PilotSettings>('plugin:pilot|pilot_settings_get')
      if (s) this.settings = s
    },

    /** `quiet` skips the CLI status probe — for remembering a picker choice. */
    async saveSettings(patch: Partial<PilotSettings>, opts: { quiet?: boolean } = {}) {
      const next = { ...(this.settings ?? {}), ...patch } as PilotSettings
      const saved = await pilot<PilotSettings>('plugin:pilot|pilot_settings_set', { settings: next })
      if (saved) this.settings = saved
      if (!opts.quiet) void this.refreshStatus()
      return saved
    },

    /** After a webview reload the CLIs kept running — find their PTYs again. */
    async syncRuntime() {
      const map = await pilot<Record<string, RuntimeInfo>>('plugin:pilot|pilot_runtime_state')
      if (!map) return
      for (const [id, info] of Object.entries(map)) {
        const l = this.ensureLive(id)
        if (info.ptyId) {
          l.ptyId = info.ptyId
          l.alive = info.alive
          if (info.alive && l.state === 'offline') l.state = 'idle'
          if (!info.alive) l.state = 'offline'
        }
      }
    },

    // ── Sessions ──

    /** A new chat thread in a context (V3). */
    async createThread(
      opts: {
        contextId: string
        /** Absent: the project's default (card 19). */
        mode?: Mode
        title?: string
        provider?: string
        model?: string
        workspaceMode?: 'local' | 'worktree'
        baseBranch?: string
        worktreePath?: string
      },
      open = true
    ) {
      this.lastError = ''
      try {
        const meta = await pilot<SessionMeta>('plugin:pilot|pilot_thread_create', { req: opts })
        if (!meta) return null
        this.upsertSession(meta)
        if (open) await this.openSession(meta.id)
        return meta
      } catch (e) {
        this.lastError = e instanceof Error ? e.message : String(e)
        return null
      }
    },

    /** Switch to a thread. Its conversation loads; nothing starts until a message is sent. */
    async openSession(id: string, fromHistory = false) {
      if (!fromHistory && this.activeId && this.activeId !== id) {
        this.history.back = [...this.history.back, this.activeId].slice(-50)
        this.history.forward = []
      }
      this.page = 'thread'
      this.activeId = id
      useSidebarStore().markRead(id)
      await useThreadStore().open(id)
    },

    /** Back (-1) or forward (1) through the visited threads, past ones that are gone. */
    async goHistory(dir: -1 | 1): Promise<boolean> {
      const from = dir === -1 ? this.history.back : this.history.forward
      const to = dir === -1 ? this.history.forward : this.history.back
      while (from.length) {
        const id = from.pop()!
        if (id === this.activeId || !this.sessions.some((s) => s.id === id)) continue
        if (this.activeId) to.push(this.activeId)
        await this.openSession(id, true)
        return true
      }
      return false
    },

    /** V2's terminal for a row: launch (or attach to) its PTY and show it. */
    async openTerminal(id: string) {
      this.activeId = id
      this.terminalShown[id] = true
      const l = this.ensureLive(id)
      if (l.ptyId && l.alive) return
      await this.launch(id)
    },

    closeTerminal(id: string) {
      this.terminalShown[id] = false
    },

    /** A thread just settled (by hand or after inactivity): its idle shells close (card 13). */
    noticeSettled(next: SessionMeta) {
      const before = this.sessions.find((s) => s.id === next.id)
      if (before && !before.settledAt && next.settledAt) void useDrawerStore().closeIdle(next.id)
    },

    upsertSession(meta: SessionMeta) {
      this.noticeSettled(meta)
      const i = this.sessions.findIndex((s) => s.id === meta.id)
      if (i >= 0) this.sessions[i] = meta
      else this.sessions.unshift(meta)
    },

    patchSession(id: string, patch: Partial<SessionMeta>) {
      const i = this.sessions.findIndex((s) => s.id === id)
      if (i >= 0) this.sessions[i] = { ...this.sessions[i], ...patch }
    },

    async launch(id: string, size?: { cols: number; rows: number }) {
      const l = this.ensureLive(id)
      if (l.launching) return
      l.launching = true
      l.error = null
      const grid = size ?? this.stage
      try {
        const r = await pilot<LaunchResult>('plugin:pilot|pilot_session_launch', {
          id,
          cols: grid?.cols ?? null,
          rows: grid?.rows ?? null,
        })
        if (!r) return
        if (!r.attached) {
          l.files = []
          l.tool = null
          l.turnRunning = false
          l.notice = null
          l.bell = false
          bellScan.delete(id)
        }
        l.ptyId = r.ptyId
        l.alive = true
        l.display = r.display
        l.state = 'idle'
        l.detail = null
        await this.refreshSessions()
      } catch (e) {
        l.error = e instanceof Error ? e.message : String(e)
        l.state = 'error'
        l.detail = l.error
      } finally {
        l.launching = false
      }
    },

    closeSession() {
      this.activeId = null
    },

    /** End the process; the row stays and resumes on the next open. */
    async stop(id: string) {
      try {
        await pilot('plugin:pilot|pilot_session_stop', { id })
      } catch (e) {
        this.lastError = e instanceof Error ? e.message : String(e)
      }
      this.markExited(id)
    },

    async remove(id: string) {
      try {
        await pilot('plugin:pilot|pilot_session_delete', { id })
      } catch (e) {
        this.lastError = e instanceof Error ? e.message : String(e)
      }
      this.sessions = this.sessions.filter((s) => s.id !== id)
      delete this.live[id]
      delete this.terminalShown[id]
      useThreadStore().forget(id)
      if (this.activeId === id) this.activeId = this.sessions[0]?.id ?? null
    },

    /** Title, model, effort or mode; model, effort and mode apply from the next turn. */
    async update(id: string, patch: { title?: string; provider?: string; model?: string; effort?: string; mode?: Mode }) {
      try {
        const meta = await pilot<SessionMeta>('plugin:pilot|pilot_thread_update', { id, ...patch })
        if (meta) this.upsertSession(meta)
        return true
      } catch (e) {
        this.lastError = e instanceof Error ? e.message : String(e)
        return false
      }
    },

    // ── Signals ──

    applyEvent(sessionId: string, ev: PilotSignal) {
      const l = this.ensureLive(sessionId)
      const meta = this.sessions.find((s) => s.id === sessionId)
      switch (ev.type) {
        case 'launched':
          // The user typed an agent's command: the row is that agent's now.
          if (meta && meta.provider !== ev.adapter) {
            meta.provider = ev.adapter
            meta.providerSessionId = null
          }
          l.signals = this.signalsOf(ev.adapter)
          l.files = []
          l.tool = null
          l.turnRunning = false
          l.notice = null
          l.bell = false
          l.vitals = freshLive().vitals
          this.setState(l, 'idle', null)
          break
        case 'bound':
          if (meta) meta.providerSessionId = ev.providerSessionId
          if (ev.model) l.vitals.model = ev.model
          break
        case 'state':
          this.setState(l, ev.state, ev.detail)
          if (ev.state !== 'waiting') l.bell = false
          break
        case 'tool':
          l.tool = ev.done ? null : ev.title
          break
        case 'file':
          l.files = [ev.path, ...l.files.filter((f) => f !== ev.path)].slice(0, FILES_CAP)
          break
        case 'turn':
          if (ev.status === 'started') {
            l.turnRunning = true
          } else {
            l.turnRunning = false
            l.tool = null
            l.cheer += 1
          }
          break
        case 'vitals':
          if (ev.model) l.vitals.model = ev.model
          if (ev.costUsd !== null) l.vitals.costUsd = ev.costUsd
          if (ev.contextPct !== null) l.vitals.contextPct = ev.contextPct
          if (ev.linesAdded !== null) l.vitals.linesAdded = ev.linesAdded
          if (ev.linesRemoved !== null) l.vitals.linesRemoved = ev.linesRemoved
          if (ev.inputTokens !== null) l.vitals.inputTokens = ev.inputTokens
          if (ev.outputTokens !== null) l.vitals.outputTokens = ev.outputTokens
          break
        case 'title':
          if (meta && !meta.title.trim()) meta.title = ev.title
          break
        case 'notice':
          l.notice = ev.text
          break
        case 'exited':
          this.markExited(sessionId)
          break
      }
    },

    setState(l: SessionLive, state: LiveState, detail: string | null) {
      l.state = state
      l.detail = detail
      l.turnRunning = state === 'thinking' || state === 'working' || state === 'waiting'
      if (state === 'idle' || state === 'offline') l.tool = null
    },

    /**
     * Bytes from any pilot PTY. For adapters that report nothing structured
     * this is the whole picture: output means working, silence means idle.
     * The bell (`\x07`) means "needs you" for every adapter — a CLI rings it
     * when it asks something. Only a real bell counts: the same byte also
     * terminates OSC strings (title refreshes, hyperlinks, shell marks),
     * which CLIs emit constantly, idle or not — `scanBells` tells them apart.
     */
    onOutput(ptyId: string, data: string) {
      const sid = this.sessionOfPty[ptyId]
      if (!sid || !data) return
      const l = this.ensureLive(sid)
      const now = Date.now()
      l.lastOutputAt = now
      const scan = scanBells(data, bellScan.get(sid) ?? 'text')
      bellScan.set(sid, scan.state)
      if (scan.bells > 0) {
        l.bell = true
        bellAt.set(sid, now)
        if (l.state !== 'error') this.setState(l, 'waiting', 'the terminal rang its bell')
        return
      }
      if (l.bell && now - (bellAt.get(sid) ?? 0) > BELL_HOLD_MS) {
        l.bell = false
        if (l.state === 'waiting') this.setState(l, l.signals === 'activity' ? 'working' : 'idle', null)
      }
      if (l.signals !== 'activity' || l.bell) return
      if (l.state !== 'working') this.setState(l, 'working', null)
      const old = decayTimers.get(sid)
      if (old) clearTimeout(old)
      decayTimers.set(
        sid,
        setTimeout(() => {
          decayTimers.delete(sid)
          const cur = this.live[sid]
          if (cur && cur.alive && cur.state === 'working' && !cur.bell) this.setState(cur, 'idle', null)
        }, ACTIVITY_DECAY_MS)
      )
    },

    ptyExited(ptyId: string) {
      const sid = this.sessionOfPty[ptyId]
      if (sid) this.markExited(sid)
    },

    markExited(sessionId: string) {
      const l = this.ensureLive(sessionId)
      l.alive = false
      l.launching = false
      l.turnRunning = false
      l.tool = null
      l.bell = false
      bellScan.delete(sessionId)
      l.state = 'offline'
      l.detail = null
    },

    paneFailed(sessionId: string, message: string) {
      const l = this.ensureLive(sessionId)
      l.error = message
      l.state = 'error'
      l.detail = message
    },

    // ── The suite ──

    /** A file the agent touched: QuantCode opens it (the shell palette's own path). */
    async openFile(path: string) {
      await navigateTo('/code')
      setTimeout(() => {
        void bus.emit('core.file.open', { path }, 'pilot').catch(() => {})
      }, 250)
    },
  },
})
