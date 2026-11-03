/**
 * The connected board (round 2, card 09): which windows are linked and how
 * each link looks, and the actions that open a linked window — a kanban
 * card's thread, a thread's diff and preview. Call the composables in a
 * component's setup (they read QuantPilot's stores and drafts).
 *
 * Edges come from two places: saved ones (`state.edges`, made by these
 * actions or by the user from a window's port) and crew ones, derived live
 * from QuantPilot's crews (orchestrator chat → worker chats, card 07).
 */
import { computed, ref } from 'vue'
import { v4 as uuidv4 } from 'uuid'
import { useCanvasStore } from '../../stores/canvas'
import { useWorkspacesStore } from '../../stores/workspaces'
import { useCardsStore } from '../../stores/cards'
import { usePilotStore } from '#pilot/stores/pilot'
import { useSidebarStore } from '#pilot/stores/sidebar'
import { useGitStore } from '#pilot/stores/git'
import { useCrewStore } from '#pilot/stores/crew'
import { pilot } from '#pilot/utils/invoke'
import { cardPrompt, threadsByCard } from '#pilot/utils/board'
import { saveDrafts, useDrafts } from '#pilot/utils/drafts'
import { besideOf, boundsOf, cardState, edgeGeometry, rectOf, taskState, threadState, worse, type EdgeGeometry, type EdgeState, type Rect } from './edges'
import type { CanvasWindow } from '../../shared/types'
import type { CrewSnapshot } from '#pilot/types'
import type { KanbanCard } from '@quantsuite/core'

/** A port drag in progress: from a window, the pointer in world coordinates. */
export const connecting = ref<{ from: string; x: number; y: number } | null>(null)

export const CLUSTER_SIZE = { width: 300, height: 250 }

export interface Cluster {
  /** The orchestrator's chat window. */
  window: CanvasWindow
  snap: CrewSnapshot
  /** Its workers' chat windows on the canvas. */
  members: CanvasWindow[]
  collapsed: boolean
  /** Where the folded node sits (the members' top-left). */
  node: Rect
}

export interface DrawnEdge {
  id: string
  from: string
  to: string
  kind: 'link' | 'manual' | 'crew'
  state: EdgeState
  /** A crew edge's task: messages travel on it. */
  taskId?: string
  geometry: EdgeGeometry
}

export function useCanvasEdges() {
  const canvas = useCanvasStore()
  const workspaces = useWorkspacesStore()
  const cards = useCardsStore()
  const pilotStore = usePilotStore()
  const sb = useSidebarStore()
  const crews = useCrewStore()

  const state = computed(() => canvas.activeCanvasState)
  const windows = computed(() => state.value?.windows ?? [])

  const threadOf = (w: CanvasWindow) => (w.type === 'chat' ? w.chatConfig?.threadId ?? null : null)

  /** Orchestrator chats on the canvas with their crews. */
  const clusters = computed<Cluster[]>(() => {
    const out: Cluster[] = []
    for (const w of windows.value) {
      const t = threadOf(w)
      if (!t) continue
      const snap = crews.ofThread(t)
      if (!snap || snap.crew.orchestratorId !== t) continue
      const workerIds = new Set(snap.tasks.map((x) => x.workerId).filter(Boolean) as string[])
      const members = windows.value.filter((m) => m.id !== w.id && (m.crewOf === t || (threadOf(m) && workerIds.has(threadOf(m)!))))
      const box = boundsOf(members.map(rectOf))
      const node = box ? { x: box.x, y: box.y, ...CLUSTER_SIZE } : { ...besideOf(rectOf(w), CLUSTER_SIZE, 120) }
      out.push({ window: w, snap, members, collapsed: Boolean(w.crewCollapsed), node })
    }
    return out
  })

  /** Worker windows folded into their cluster node. */
  const hidden = computed(() => new Set(clusters.value.filter((c) => c.collapsed).flatMap((c) => c.members.map((m) => m.id))))

  function windowState(w: CanvasWindow): EdgeState {
    if (w.type === 'chat') {
      const t = threadOf(w)
      return t ? threadState(sb.statusOf(t)) : 'idle'
    }
    if (w.type === 'card' && w.cardConfig) return cardState(cards.card(w.cardConfig.boardId, w.cardConfig.cardId))
    return 'idle'
  }

  const edges = computed<DrawnEdge[]>(() => {
    const byId = new Map(windows.value.map((w) => [w.id, w]))
    const rect = (id: string): Rect | null => {
      const w = byId.get(id)
      if (w) return rectOf(w)
      const c = clusters.value.find((x) => `cluster:${x.window.id}` === id)
      return c ? c.node : null
    }
    const out: DrawnEdge[] = []
    for (const e of state.value?.edges ?? []) {
      if (hidden.value.has(e.from) || hidden.value.has(e.to)) continue
      const a = rect(e.from)
      const b = rect(e.to)
      const wa = byId.get(e.from)
      const wb = byId.get(e.to)
      if (!a || !b || !wa || !wb) continue
      out.push({ id: e.id, from: e.from, to: e.to, kind: e.kind, state: worse(windowState(wa), windowState(wb)), geometry: edgeGeometry(a, b) })
    }
    for (const c of clusters.value) {
      const from = rectOf(c.window)
      if (c.collapsed) {
        if (!c.members.length) continue
        const open = c.snap.tasks.map((t) => taskState(t.status)).reduce(worse, 'idle' as EdgeState)
        out.push({ id: `crew:${c.window.id}:cluster`, from: c.window.id, to: `cluster:${c.window.id}`, kind: 'crew', state: open, geometry: edgeGeometry(from, c.node) })
        continue
      }
      for (const m of c.members) {
        const task = c.snap.tasks.find((t) => t.workerId === threadOf(m))
        out.push({ id: `crew:${c.window.id}:${m.id}`, from: c.window.id, to: m.id, kind: 'crew', state: task ? taskState(task.status) : windowState(m), taskId: task?.id, geometry: edgeGeometry(from, rectOf(m)) })
      }
    }
    return out
  })

  return { windows, clusters, hidden, edges, workspaceId: computed(() => workspaces.contentWorkspaceId), pilotStore }
}

/** Opening linked windows. */
export function useCanvasActions() {
  const canvas = useCanvasStore()
  const workspaces = useWorkspacesStore()
  const cards = useCardsStore()
  const pilotStore = usePilotStore()
  const git = useGitStore()
  const drafts = useDrafts()

  const ws = () => workspaces.contentWorkspaceId!
  const all = () => canvas.activeCanvasState?.windows ?? []
  const context = () => (workspaces.contentWorkspaceId === workspaces.GENERAL_CONTENT ? 'general' : workspaces.contentWorkspaceId ?? 'general')

  function add(win: Omit<CanvasWindow, 'id' | 'zIndex' | 'status' | 'minimized'>): CanvasWindow {
    const full: CanvasWindow = { id: uuidv4(), status: 'idle', minimized: false, zIndex: 0, ...win }
    canvas.addWindow(ws(), full)
    return full
  }

  /** The chat window of a thread — the one there, or a new one beside `near`. */
  function chatFor(threadId: string, near: Rect, extra: Partial<CanvasWindow> = {}): CanvasWindow {
    const there = all().find((w) => w.type === 'chat' && w.chatConfig?.threadId === threadId)
    if (there) return there
    const title = pilotStore.sessions.find((s) => s.id === threadId)?.title
    return add({ type: 'chat', title: title ? `Chat · ${title}` : 'Chat', position: besideOf(near, { width: 520, height: 620 }), chatConfig: { threadId }, ...extra })
  }

  /** A card's thread: the newest linked one, or a new thread with the card's prompt in its box (nothing starts until sent). */
  async function cardThread(win: CanvasWindow): Promise<CanvasWindow | null> {
    const cfg = win.cardConfig
    if (!cfg) return null
    if (!pilotStore.loaded) await pilotStore.load()
    await git.loadLinks()
    const ids = threadsByCard(git.links)[cfg.cardId] ?? []
    let threadId = [...ids].reverse().find((id) => pilotStore.sessions.some((s) => s.id === id)) ?? null
    if (!threadId) {
      const card: KanbanCard | null = cards.card(cfg.boardId, cfg.cardId)
      const meta = await pilotStore.createThread({ contextId: card?.workspace_id && card.workspace_id !== 'general' ? card.workspace_id : context(), title: cfg.title }, false)
      if (!meta) throw new Error(pilotStore.lastError || 'The thread could not be created.')
      drafts.value[meta.id] = cardPrompt({ id: cfg.cardId, title: cfg.title, description: card?.description ?? '', workspace_id: card?.workspace_id ?? cfg.boardId } as never, workspaces.contentWorkspace?.name ?? 'this')
      saveDrafts(drafts.value)
      await pilot('plugin:pilot|pilot_card_link', { id: meta.id, cardId: cfg.cardId, title: cfg.title })
      await git.loadLinks()
      threadId = meta.id
    }
    const chat = chatFor(threadId, rectOf(win))
    canvas.addEdge(ws(), win.id, chat.id, 'link')
    return chat
  }

  /**
   * A thread's changes: its worktree against its base, or its checkout's
   * uncommitted changes. A card's thread works in the card's worktree once
   * it claimed the card (AGENT.md): that one, against the project's branch.
   */
  function threadDiff(chat: CanvasWindow): CanvasWindow | null {
    const t = pilotStore.sessions.find((s) => s.id === chat.chatConfig?.threadId)
    if (!t) return null
    const st = canvas.activeCanvasState
    const cardWin = st?.windows.find((w) => w.type === 'card' && (st.edges ?? []).some((e) => (e.from === w.id && e.to === chat.id) || (e.to === w.id && e.from === chat.id)))
    const card = cardWin?.cardConfig ? cards.card(cardWin.cardConfig.boardId, cardWin.cardConfig.cardId) : null
    const projectBranch = git.info[t.id]?.repo?.branch || undefined
    const repoPath = card?.worktree_path || t.worktreePath || t.cwd
    const base = card?.worktree_path ? projectBranch || 'main' : t.workspaceMode === 'worktree' && t.baseBranch ? t.baseBranch : undefined
    const there = all().find((w) => w.type === 'diff' && w.diffConfig?.repoPath === repoPath)
    const diff = there ?? add({ type: 'diff', title: `Diff · ${t.branch || t.title || 'thread'}`, position: besideOf(rectOf(chat), { width: 700, height: 520 }), diffConfig: { repoPath, base } })
    canvas.addEdge(ws(), chat.id, diff.id, 'link')
    return diff
  }

  /** A preview browser beside a window, connected to it. */
  function preview(from: CanvasWindow, url = 'http://localhost:3000'): CanvasWindow {
    const b = add({ type: 'browser', title: 'Browser', position: besideOf(rectOf(from), { width: 700, height: 500 }), browserConfig: { url } })
    canvas.addEdge(ws(), from.id, b.id, 'link')
    return b
  }

  /** A card dropped on the canvas: its node, its thread (connected), and its diff once work exists. */
  async function dropCard(card: KanbanCard, boardId: string, at: { x: number; y: number }): Promise<void> {
    const existing = all().find((w) => w.type === 'card' && w.cardConfig?.cardId === card.id)
    const node = existing ?? add({ type: 'card', title: card.title, position: { x: at.x, y: at.y, width: 280, height: 150 }, cardConfig: { cardId: card.id, boardId, title: card.title } })
    void cards.load(boardId)
    const chat = await cardThread(node)
    if (!chat) return
    const t = pilotStore.sessions.find((s) => s.id === chat.chatConfig?.threadId)
    if (t?.worktreePath || (t && t.lastActivityAt && t.workspaceMode === 'worktree')) threadDiff(chat)
  }

  /** "New crew": an orchestrator chat whose box asks for the work. */
  async function newCrew(at: { x: number; y: number }): Promise<CanvasWindow | null> {
    if (!pilotStore.loaded) await pilotStore.load()
    const meta = await pilotStore.createThread({ contextId: context(), title: 'Crew lead' }, false)
    if (!meta) throw new Error(pilotStore.lastError || 'The thread could not be created.')
    drafts.value[meta.id] =
      'Lead a crew for this work: <describe it>.\n\nSplit it into independent tasks and start workers with crew_dispatch (each self-contained: the target, the change, what it may edit, the evidence that proves it done). Then end your turn — QuantPilot wakes you with their questions and results.'
    saveDrafts(drafts.value)
    return add({ type: 'chat', title: 'Chat · Crew lead', position: { x: at.x, y: at.y, width: 520, height: 620 }, chatConfig: { threadId: meta.id } })
  }

  return { cardThread, threadDiff, preview, dropCard, newCrew, chatFor }
}
