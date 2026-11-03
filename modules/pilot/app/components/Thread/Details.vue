<script setup lang="ts">
/**
 * The thread panel (QRightPanel, round 2 card 04): a working panel, not a
 * read-out. A small face and one line for what the agent does, then the
 * sections the user picked, in the user's order (drag a title; the ⋯ menu
 * shows or hides them):
 *   Agents now    every thread working or waiting, anywhere — answer an
 *                 approval inline, open the rest; results not seen yet
 *   This thread   context against the model's real window, cost, the
 *                 usage windows; Compact, Usage
 *   Changes       the files this thread changed with +/−; a click opens
 *                 the diff
 *   Branch and PR the branch, its pull requests; the GitHub view
 *   Tasks, Crew (the QuantPilot crew this thread leads or works in —
 *                 round 2 card 07 — then its own subagents), Servers
 * Without a thread only Agents now shows. Sans labels, mono only for
 * numbers (the agent-UI register); no polling beyond the dev-server probe
 * and a one-second clock while something works.
 */
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref, watch } from 'vue'
import { inActiveKeepAliveTree, qs } from '@quantsuite/core'
import { Ellipsis, GripVertical } from 'lucide-vue-next'
import { useThreadStore } from '#pilot/stores/thread'
import { usePilotStore } from '#pilot/stores/pilot'
import { useSidebarStore } from '#pilot/stores/sidebar'
import { useGitStore } from '#pilot/stores/git'
import { useAppStore } from '#pilot/stores/app'
import { useInspectorStore } from '#pilot/stores/inspector'
import { useSideStore } from '#pilot/stores/side'
import { useCrewStore } from '#pilot/stores/crew'
import { serverUrls, useDrawerStore, type DevServer } from '#pilot/stores/drawer'
import { pilot } from '#pilot/utils/invoke'
import { basename, dirname, fmtCost, fmtTokens } from '#pilot/utils/format'
import { formatDuration, subagents, taskCounts } from '#pilot/utils/timeline'
import { elapsedLabel } from '#pilot/utils/sidebar'
import { agentsNow, aggregateChanges, dropIndex, moveSection, normalizeLayout, PANEL_SECTIONS, SECTION_LABELS, tokenPair, type PanelLayout, type PanelSection } from '#pilot/utils/panel'
import type { PortOwner, ThreadItem } from '#pilot/types'

const props = defineProps<{ threadId: string | null }>()
const threads = useThreadStore()
const store = usePilotStore()
const sb = useSidebarStore()
const git = useGitStore()
const app = useAppStore()
const inspector = useInspectorStore()

const view = computed(() => (props.threadId ? threads.view(props.threadId) : null))
const thread = computed(() => (props.threadId ? store.sessions.find((s) => s.id === props.threadId) ?? null : null))

// ── The header: a small face, the state, what it does ──
const faceState = computed(() => {
  switch (view.value?.state) {
    case 'running':
    case 'starting':
      return 'working'
    case 'waiting':
      return 'waiting'
    case 'error':
    case 'limited':
      return 'error'
    default:
      return 'idle'
  }
})
const stateLabel = computed(() => {
  if (!thread.value) return 'ready'
  switch (view.value?.state) {
    case 'running':
    case 'starting':
      return 'working'
    case 'waiting':
      return 'needs you'
    case 'error':
      return 'error'
    case 'limited':
      return 'usage limit'
    case 'interrupted':
      return 'stopped'
    default:
      return thread.value?.providerSessionId ? 'idle' : 'ready'
  }
})
function liveStep(items: ThreadItem[]): string {
  for (let i = items.length - 1; i >= 0 && i >= items.length - 40; i--) {
    const it = items[i]!
    if ((it.kind === 'approval' || it.kind === 'question' || it.kind === 'plan') && it.data.status === 'pending') {
      return it.kind === 'approval' ? `Allow ${it.data.toolName}?` : it.kind === 'plan' ? 'A plan waits for you' : 'A question waits for you'
    }
    if (it.kind === 'tool' && it.data.status === 'running') return String(it.data.title || it.data.name)
  }
  return ''
}
const doing = computed(() => liveStep(view.value?.items ?? []))

// ── The layout: which sections, in which order (per machine) ──
const LAYOUT_KEY = 'pilot.panel.v1'
function readLayout(): PanelLayout {
  try {
    return normalizeLayout(JSON.parse(localStorage.getItem(LAYOUT_KEY) ?? 'null'))
  } catch {
    return normalizeLayout(null)
  }
}
const layout = ref<PanelLayout>(readLayout())
function saveLayout(next: PanelLayout) {
  layout.value = next
  try {
    localStorage.setItem(LAYOUT_KEY, JSON.stringify(next))
  } catch {
    /* private mode */
  }
}
const menuOpen = ref(false)
function toggleSection(s: PanelSection) {
  const hidden = layout.value.hidden.includes(s) ? layout.value.hidden.filter((h) => h !== s) : [...layout.value.hidden, s]
  saveLayout({ ...layout.value, hidden })
}

// ── Agents now ──
const tick = ref(Date.now())
const agentRows = computed(() => {
  const metas = new Map(store.sessions.map((s) => [s.id, s]))
  const rows = sb.rows
    .filter((r) => r.id !== props.threadId || r.status === 'waiting')
    .map((r) => ({ id: r.id, title: metas.get(r.id)?.title || 'New thread', status: r.status, since: r.sentAt, unread: Boolean(sb.unread[r.id]) }))
  return agentsNow(rows, 8).map((r) => {
    const items = threads.views[r.id]?.items ?? []
    const pending = [...items].reverse().find((i) => (i.kind === 'approval' || i.kind === 'question' || i.kind === 'plan') && i.data.status === 'pending') ?? null
    return {
      ...r,
      meta: metas.get(r.id)!,
      step: liveStep(items) || store.live[r.id]?.detail || '',
      approval: pending?.kind === 'approval' ? pending : null,
      pending,
    }
  })
})
const anyWorking = computed(() => agentRows.value.some((r) => r.bucket === 'working'))
const busyAnswer = ref<string | null>(null)
async function answerInline(threadId: string, item: ThreadItem, decision: 'allow' | 'deny') {
  busyAnswer.value = item.id
  try {
    await threads.answer(threadId, String(item.data.requestId), { decision })
  } finally {
    busyAnswer.value = null
  }
}

// ── This thread ──
const model = computed(() => view.value?.model || thread.value?.model || '')
const usage = computed(() => view.value?.usage)
const context = computed(() => {
  const u = usage.value
  if (!u?.contextTokens || !u.contextWindow) return null
  return { pct: Math.max(0, Math.min(100, (u.contextTokens / u.contextWindow) * 100)), label: tokenPair(u.contextTokens, u.contextWindow) }
})
const tokensLabel = computed(() => {
  const u = usage.value
  if (!u || (!u.inputTokens && !u.outputTokens)) return ''
  return `${fmtTokens(u.inputTokens)} in · ${fmtTokens(u.outputTokens)} out`
})
const LIMIT_NAMES: Record<string, string> = { five_hour: '5-hour', seven_day: 'Weekly', seven_day_opus: 'Weekly (Opus)' }
const limits = computed(() =>
  (view.value?.rateLimits ?? [])
    .filter((w) => w.usedPct != null)
    .map((w) => ({
      name: LIMIT_NAMES[w.kind] ?? w.kind.replace(/_/g, ' '),
      pct: Math.round(w.usedPct ?? 0),
      reset: w.resetsAt ? formatDuration(Math.max(0, w.resetsAt - Date.now())) : '',
    })),
)
const running = computed(() => Boolean(view.value?.turnId))
const canCompact = computed(() => !running.value && Boolean(thread.value?.providerSessionId) && thread.value?.provider === 'claude')
async function compact() {
  if (props.threadId && canCompact.value) await threads.send(props.threadId, '/compact')
}

// ── Side chats (round 2, card 10) ──
const side = useSideStore()
const sideChats = computed(() => (props.threadId && !thread.value?.parentId ? side.of(props.threadId) : []))

// ── Changes ──
const changes = computed(() => aggregateChanges(view.value?.items ?? []).slice(0, 40))
/** Before any saved change: the files the agent's tools wrote. */
const written = computed(() => {
  if (changes.value.length) return []
  const seen = new Set<string>()
  const out: string[] = []
  const items = view.value?.items ?? []
  for (let i = items.length - 1; i >= 0; i--) {
    const p = items[i]!.kind === 'tool' ? items[i]!.data.path : null
    if (typeof p === 'string' && !seen.has(p)) {
      seen.add(p)
      out.push(p)
      if (out.length >= 30) break
    }
  }
  return out
})
const changeTotals = computed(() => changes.value.reduce((t, f) => ({ added: t.added + f.added, removed: t.removed + f.removed }), { added: 0, removed: 0 }))
function allChanges() {
  inspector.showDiff(thread.value?.worktreePath ? 'branch' : 'uncommitted')
}

// ── Branch and PR ──
const gitInfo = computed(() => (props.threadId ? git.info[props.threadId] ?? null : null))
const branch = computed(() => gitInfo.value?.branch || gitInfo.value?.repo?.branch || '')
const links = computed(() => (props.threadId ? (git.links[props.threadId] ?? []).filter((l) => l.kind === 'pr' || l.kind === 'issue') : []))

// ── Tasks ──
const tasks = computed(() => {
  const items = view.value?.items ?? []
  for (let i = items.length - 1; i >= 0; i--) {
    const it = items[i]!
    if (it.kind === 'todo' && !it.parent && Array.isArray(it.data.todos) && it.data.todos.length) return it.data.todos as { status?: string; content?: string; activeForm?: string | null }[]
  }
  return []
})
const taskSummary = computed(() => taskCounts(tasks.value))
const copied = ref('')
async function copyText(text: string, what: string) {
  try {
    await navigator.clipboard.writeText(text)
    copied.value = what
    setTimeout(() => (copied.value = ''), 1500)
  } catch {
    /* clipboard denied */
  }
}
function copyTasks() {
  void copyText(tasks.value.map((t) => `- [${t.status === 'completed' ? 'x' : ' '}] ${t.content ?? ''}`).join('\n'), 'tasks')
}

// ── Crew ──
const CREW_SHOWN = 8
const crew = computed(() => subagents(view.value?.items ?? []))
const crews = useCrewStore()
const team = computed(() => crews.ofThread(props.threadId))

// ── Servers (card 13) ──
const drawer = useDrawerStore()
const alive = ref<number[]>([])
const stopping = ref<{ server: DevServer; owner: PortOwner | null; error: string } | null>(null)
const candidates = computed<DevServer[]>(() => {
  if (!props.threadId) return []
  const out = [...(drawer.servers[props.threadId] ?? [])]
  for (const it of view.value?.items ?? []) {
    if (it.kind !== 'tool' || typeof it.data.output !== 'string') continue
    for (const s of serverUrls(it.data.output)) {
      if (!out.some((x) => x.port === s.port)) out.push({ ...s, ptyId: null, label: String(it.data.title || it.data.name || 'Agent') })
    }
  }
  return out
})
const servers = computed(() => candidates.value.filter((s) => alive.value.includes(s.port)))
async function probe() {
  const ports = candidates.value.map((s) => s.port)
  alive.value = ports.length ? ((await pilot<number[]>('plugin:pilot|pilot_ports_alive', { ports }).catch(() => [])) ?? []) : []
}
let probeTimer: ReturnType<typeof setInterval> | null = null
let clock: ReturnType<typeof setInterval> | null = null
function start() {
  stop()
  void probe()
  probeTimer = setInterval(() => candidates.value.length && void probe(), 8000)
  syncClock()
}
function stop() {
  if (probeTimer) clearInterval(probeTimer)
  probeTimer = null
  if (clock) clearInterval(clock)
  clock = null
}
/** The one-second clock runs only while a row works. */
function syncClock() {
  if (anyWorking.value && !clock) clock = setInterval(() => (tick.value = Date.now()), 1000)
  if (!anyWorking.value && clock) {
    clearInterval(clock)
    clock = null
  }
}
watch(anyWorking, syncClock)
watch(() => candidates.value.map((s) => s.port).join(','), () => void probe())
onMounted(() => inActiveKeepAliveTree() && start())
onActivated(start)
onDeactivated(stop)
onBeforeUnmount(stop)
async function askStop(s: DevServer) {
  if (s.ptyId) {
    await qs.console.write(s.ptyId, '\x03').catch(() => undefined)
    setTimeout(() => void probe(), 1200)
    return
  }
  stopping.value = { server: s, owner: null, error: '' }
  try {
    stopping.value.owner = (await pilot<PortOwner>('plugin:pilot|pilot_port_owner', { port: s.port })) ?? null
  } catch (e) {
    stopping.value.error = String(e)
  }
}
async function confirmStop() {
  const st = stopping.value
  if (!st?.owner) return
  try {
    await pilot('plugin:pilot|pilot_port_stop', { port: st.server.port, pid: st.owner.pid })
    stopping.value = null
    setTimeout(() => void probe(), 800)
  } catch (e) {
    st.error = String(e)
  }
}

// ── Which sections show ──
const NEEDS_THREAD: PanelSection[] = ['thread', 'side', 'changes', 'branch', 'tasks', 'crew', 'servers']
function hasContent(s: PanelSection): boolean {
  switch (s) {
    case 'agents':
      return true
    case 'thread':
      return Boolean(thread.value)
    case 'side':
      return Boolean(thread.value) && !thread.value?.parentId
    case 'changes':
      return changes.value.length > 0 || written.value.length > 0
    case 'branch':
      return Boolean(branch.value) || links.value.length > 0
    case 'tasks':
      return tasks.value.length > 0
    case 'crew':
      return crew.value.length > 0 || Boolean(team.value)
    case 'servers':
      return servers.value.length > 0
  }
}
const shown = computed(() => layout.value.order.filter((s) => !layout.value.hidden.includes(s) && (!props.threadId ? !NEEDS_THREAD.includes(s) : true) && hasContent(s)))

// ── Drag a section by its title (pointer events) ──
const root = ref<HTMLElement | null>(null)
const drag = ref<{ section: PanelSection; y: number; to: number; started: boolean; startY: number } | null>(null)
function onGrip(e: PointerEvent, section: PanelSection) {
  if (e.button !== 0) return
  e.preventDefault()
  try {
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
  } catch {
    /* a pointer the browser no longer tracks */
  }
  drag.value = { section, y: e.clientY, to: shown.value.indexOf(section), started: false, startY: e.clientY }
}
function mids(): number[] {
  return [...(root.value?.querySelectorAll<HTMLElement>('[data-panel-section]') ?? [])].map((el) => {
    const r = el.getBoundingClientRect()
    return r.top + r.height / 2
  })
}
function onGripMove(e: PointerEvent) {
  const d = drag.value
  if (!d) return
  if (!d.started && Math.abs(e.clientY - d.startY) < 4) return
  d.started = true
  d.y = e.clientY
  d.to = dropIndex(mids(), e.clientY)
}
function onGripUp() {
  const d = drag.value
  drag.value = null
  if (!d?.started) return
  const visible = shown.value
  const target = visible.filter((s) => s !== d.section)[d.to] ?? null
  const rest = layout.value.order.filter((s) => s !== d.section)
  const at = target ? rest.indexOf(target) : rest.length
  saveLayout({ ...layout.value, order: moveSection(layout.value.order, d.section, at) })
}
const dropLine = computed(() => {
  const d = drag.value
  if (!d?.started || !root.value) return null
  const els = [...root.value.querySelectorAll<HTMLElement>('[data-panel-section]')]
  const base = root.value.getBoundingClientRect().top - root.value.scrollTop
  const el = els[d.to]
  const top = el ? el.getBoundingClientRect().top : (els.at(-1)?.getBoundingClientRect().bottom ?? 0)
  return top - base
})
</script>

<template>
  <div ref="root" class="qp-panel" :class="{ dragging: drag?.started }">
    <header class="qp-head">
      <div class="qp-head-face"><PilotFace :state="faceState" :cheer="view?.cheer ?? 0" /></div>
      <div class="qp-head-text">
        <span class="qp-head-state" :class="`is-${faceState}`">{{ stateLabel }}</span>
        <span v-if="doing" class="qp-head-doing" :title="doing">{{ doing }}</span>
      </div>
      <span class="qp-head-menu">
        <button class="qp-icon-btn" title="Sections" aria-haspopup="menu" :aria-expanded="menuOpen" @click="menuOpen = !menuOpen"><Ellipsis :size="14" /></button>
        <div v-if="menuOpen" class="qp-menu-veil" @pointerdown="menuOpen = false" />
        <div v-if="menuOpen" class="qp-menu" role="menu" aria-label="Sections">
          <div class="qp-menu-title">Show in the panel</div>
          <label v-for="s in PANEL_SECTIONS" :key="s" class="qp-menu-row" role="menuitemcheckbox" :aria-checked="!layout.hidden.includes(s)">
            <input type="checkbox" :checked="!layout.hidden.includes(s)" @change="toggleSection(s)" />
            {{ SECTION_LABELS[s] }}
          </label>
          <div class="qp-menu-note">Drag a section's title to move it.</div>
        </div>
      </span>
    </header>

    <div v-if="dropLine !== null" class="qp-drop-line" :style="{ top: `${dropLine}px` }" />

    <template v-for="s in shown" :key="s">
      <section class="qp-block" :data-panel-section="s" :class="{ lifted: drag?.started && drag.section === s }">
        <h4 class="qp-block-title" @pointerdown="onGrip($event, s)" @pointermove="onGripMove" @pointerup="onGripUp" @pointercancel="drag = null">
          <GripVertical class="qp-grip" :size="11" />
          <span>{{ SECTION_LABELS[s] }}</span>
          <span v-if="s === 'agents' && agentRows.length" class="qp-block-num">{{ agentRows.length }}</span>
          <span v-if="s === 'tasks'" class="qp-block-num">{{ taskSummary.done }}/{{ taskSummary.total }}</span>
          <span v-if="s === 'changes' && changes.length" class="qp-block-num"><span class="add">+{{ changeTotals.added }}</span> <span class="del">−{{ changeTotals.removed }}</span></span>
        </h4>

        <!-- Agents now -->
        <template v-if="s === 'agents'">
          <div v-if="!agentRows.length" class="qp-pn-empty">Nothing is working or waiting.</div>
          <div v-for="r in agentRows" :key="r.id" class="qp-now" :class="`is-${r.bucket}`">
            <button class="qp-now-main" :title="`${r.title} — open it`" @click="store.openSession(r.id)">
              <PilotAgentIcon :agent="r.meta?.provider || null" :size="13" :status="r.bucket === 'done' ? null : r.status" />
              <span class="qp-now-text">
                <span class="qp-now-title">{{ r.title }}</span>
                <span class="qp-now-sub">
                  <template v-if="r.bucket === 'working'"><span class="mono">{{ elapsedLabel(tick - r.since) }}</span><template v-if="r.step"> · {{ r.step }}</template></template>
                  <template v-else-if="r.bucket === 'waiting'">{{ r.step || 'Needs you' }}</template>
                  <template v-else>Finished · not seen yet</template>
                </span>
              </span>
            </button>
            <div v-if="r.approval" class="qp-now-actions">
              <button class="qp-mini primary" :disabled="busyAnswer === r.approval.id" @click="answerInline(r.id, r.approval, 'allow')">Allow</button>
              <button class="qp-mini" :disabled="busyAnswer === r.approval.id" @click="answerInline(r.id, r.approval, 'deny')">Deny</button>
            </div>
            <div v-else-if="r.pending" class="qp-now-actions">
              <button class="qp-mini primary" @click="store.openSession(r.id)">Answer</button>
            </div>
          </div>
        </template>

        <!-- This thread -->
        <template v-else-if="s === 'thread'">
          <div class="qp-vital">
            <span class="qp-vital-k">Agent</span>
            <span class="qp-vital-v qp-vital-agent"><PilotAgentIcon :agent="thread?.provider || null" :size="12" decorative />{{ store.adapterLabel(thread?.provider || 'claude') }}</span>
          </div>
          <div v-if="model" class="qp-vital">
            <span class="qp-vital-k">Model</span>
            <span class="qp-vital-v" :title="model">{{ model }}</span>
          </div>
          <div v-if="context" class="qp-vital is-bar" :title="`${Math.round(context.pct)}% of the model's context window`">
            <span class="qp-vital-k">Context</span>
            <span class="qp-vital-v mono">{{ context.label }}</span>
            <span class="qp-bar"><span class="qp-bar-fill" :class="{ hot: context.pct >= 80 }" :style="{ width: `${context.pct}%` }" /></span>
          </div>
          <div v-for="l in limits" :key="l.name" class="qp-vital is-bar">
            <span class="qp-vital-k">{{ l.name }}</span>
            <span class="qp-vital-v mono" :title="l.reset ? `Resets in ${l.reset}` : undefined">{{ l.pct }}%<template v-if="l.reset"> · {{ l.reset }}</template></span>
            <span class="qp-bar"><span class="qp-bar-fill" :class="{ hot: l.pct >= 80 }" :style="{ width: `${l.pct}%` }" /></span>
          </div>
          <div v-if="usage?.costUsd" class="qp-vital">
            <span class="qp-vital-k">Cost</span>
            <span class="qp-vital-v mono">{{ fmtCost(usage.costUsd) }}</span>
          </div>
          <div v-if="tokensLabel" class="qp-vital">
            <span class="qp-vital-k">Tokens</span>
            <span class="qp-vital-v mono">{{ tokensLabel }}</span>
          </div>
          <div v-if="thread?.lastError && view?.state === 'error'" class="qp-err">{{ thread.lastError }}</div>
          <div class="qp-actions">
            <button class="qp-mini" :disabled="!canCompact" :title="canCompact ? 'Summarize the conversation to free context (/compact)' : 'Compact works on an idle Claude Code thread that has started'" @click="compact">Compact</button>
            <button class="qp-mini" title="Usage and limits of every agent" @click="store.page = 'usage'">Usage</button>
            <button v-if="thread?.lastError && view?.state === 'error'" class="qp-mini" @click="copyText(thread.lastError ?? '', 'error')">{{ copied === 'error' ? 'Copied' : 'Copy error' }}</button>
          </div>
        </template>

        <!-- Side chats: ask beside the main run (card 10) -->
        <template v-else-if="s === 'side'">
          <button
            v-for="c in sideChats"
            :key="c.id"
            class="qp-side-row"
            :class="{ on: side.open?.sideId === c.id }"
            :title="`${c.title} — open beside the conversation`"
            @click="side.show(threadId!, c.id)"
          >
            <PilotAgentIcon :agent="c.provider || null" :size="12" :status="sb.statusOf(c.id)" />
            <span class="qp-side-title">{{ c.title.replace(/^Side: /, '') || 'Side chat' }}</span>
          </button>
          <div v-if="!sideChats.length" class="qp-pn-empty">Ask something without touching this run: /btw &lt;question&gt;.</div>
          <div class="qp-actions">
            <button class="qp-mini" title="A side chat beside this thread — same project, agent and checkout" @click="side.startNew(threadId!)">New side chat</button>
          </div>
        </template>

        <!-- Changes -->
        <template v-else-if="s === 'changes'">
          <button v-for="f in changes" :key="f.path" class="qp-file" :title="`${f.path} — open the diff`" @click="inspector.showDiff('turn', f.turnId, f.path)">
            <span class="qp-file-line">
              <span class="qp-file-name">{{ basename(f.path) }}</span>
              <span class="qp-file-num mono"><span class="add">+{{ f.added }}</span> <span class="del">−{{ f.removed }}</span></span>
            </span>
            <span class="qp-file-dir">{{ dirname(f.path) }}</span>
          </button>
          <button v-for="p in written" :key="p" class="qp-file" :title="`${p} — open it`" @click="threadId && inspector.openFile(threadId, p)">
            <span class="qp-file-name">{{ basename(p) }}</span>
            <span class="qp-file-dir">{{ dirname(p) }}</span>
          </button>
          <div class="qp-actions">
            <button class="qp-mini" :title="thread?.worktreePath ? 'Everything on this branch' : 'Everything not committed'" @click="allChanges">All changes</button>
          </div>
        </template>

        <!-- Branch and PR -->
        <template v-else-if="s === 'branch'">
          <div v-if="branch" class="qp-vital">
            <span class="qp-vital-k">{{ gitInfo?.mode === 'worktree' ? 'Worktree' : 'Branch' }}</span>
            <span class="qp-vital-v mono" :title="branch">{{ branch }}</span>
          </div>
          <div v-if="links.length" class="qp-links">
            <PilotGitPrChip v-for="l in links" :key="l.url" :link="l" />
          </div>
          <div v-else class="qp-pn-empty">No pull request yet.</div>
          <div class="qp-actions">
            <button class="qp-mini" title="Pull requests, checks and issues of this repository" @click="app.setView('github')">GitHub</button>
          </div>
        </template>

        <!-- Tasks -->
        <template v-else-if="s === 'tasks'">
          <ul class="qp-tasklist">
            <li v-for="(t, i) in tasks" :key="i" :class="`is-${t.status}`" :title="t.content">{{ t.status === 'in_progress' ? t.activeForm || t.content : t.content }}</li>
          </ul>
          <div class="qp-actions">
            <button class="qp-mini" @click="copyTasks">{{ copied === 'tasks' ? 'Copied' : 'Copy as list' }}</button>
          </div>
        </template>

        <!-- Crew: the QuantPilot crew (round 2, card 07), then subagents -->
        <template v-else-if="s === 'crew'">
          <PilotCrewPanel v-if="team && threadId" :thread-id="threadId" />
          <div v-if="team && crew.length" class="qp-pn-sub">Subagents</div>
          <button
            v-for="a in crew.slice(0, CREW_SHOWN)"
            :key="a.callId"
            class="qp-agent"
            :class="{ on: inspector.open && inspector.tab === 'agent' && threadId && inspector.agent[threadId] === a.callId }"
            :title="`${a.title} — open what it did in the inspector`"
            @click="threadId && inspector.openAgent(threadId, a.callId)"
          >
            <PilotAgentIcon :agent="thread?.provider || null" :size="12" :status="a.status === 'running' ? 'working' : a.status === 'failed' ? 'error' : null" />
            <span class="qp-agent-title">{{ a.title }}</span>
            <span class="qp-agent-sub"><template v-if="a.type">{{ a.type }} · </template><span class="mono">{{ a.tools }}</span> tool{{ a.tools === 1 ? '' : 's' }}</span>
            <span v-if="a.status === 'running' && a.last" class="qp-agent-last">{{ a.last }}</span>
          </button>
          <div v-if="crew.length > CREW_SHOWN" class="qp-pn-empty">and {{ crew.length - CREW_SHOWN }} earlier</div>
        </template>

        <!-- Servers (card 13) -->
        <template v-else-if="s === 'servers'">
          <div v-for="sv in servers" :key="sv.port" class="qp-srv">
            <span class="qp-srv-port mono" :title="`${sv.url} — from ${sv.label}`">:{{ sv.port }}</span>
            <span class="qp-srv-from" :title="sv.label">{{ sv.label }}</span>
            <button class="qp-mini" :title="`Open ${sv.url} in the Browser tab`" @click="threadId && inspector.openBrowser(threadId, sv.url)">Open</button>
            <button class="qp-mini" :title="sv.ptyId ? 'Ctrl+C in its terminal' : 'End the process that holds the port'" @click="askStop(sv)">Stop</button>
          </div>
          <div v-if="stopping" class="qp-srv-confirm" role="alertdialog">
            <template v-if="stopping.error">{{ stopping.error }}</template>
            <template v-else-if="stopping.owner">End {{ stopping.owner.name }} (PID <span class="mono">{{ stopping.owner.pid }}</span>) and what it started?</template>
            <template v-else>Looking up the process…</template>
            <span class="qp-srv-actions">
              <button class="qp-mini" @click="stopping = null">{{ stopping.error ? 'Close' : 'Keep' }}</button>
              <button v-if="stopping.owner && !stopping.error" class="qp-mini danger" @click="confirmStop">End it</button>
            </span>
          </div>
        </template>
      </section>
    </template>
  </div>
</template>

<style scoped>
.qp-panel { position: relative; display: flex; flex-direction: column; height: 100%; min-height: 0; overflow-y: auto; }
.qp-panel.dragging { cursor: grabbing; user-select: none; }
.qp-head { display: flex; align-items: center; gap: 8px; padding: 10px 10px 10px 12px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); }
.qp-head-face { width: 34px; flex-shrink: 0; }
.qp-head-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
.qp-head-state { font-size: 12.5px; font-weight: 600; color: var(--qss-text-secondary, #9a9aa5); }
.qp-head-state.is-working { color: var(--qss-success, #34d399); }
.qp-head-state.is-waiting { color: var(--qss-warning, #d29a3f); }
.qp-head-state.is-error { color: var(--qss-error, #f87171); }
.qp-head-doing { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-head-menu { position: relative; flex-shrink: 0; }
.qp-icon-btn { display: inline-flex; padding: 4px; border: none; border-radius: 6px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-icon-btn:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-menu-veil { position: fixed; inset: 0; z-index: 40; }
.qp-menu { position: absolute; top: calc(100% + 4px); right: 0; z-index: 41; width: 190px; padding: 6px; border: 1px solid var(--qss-border, #47474f); border-radius: 8px; background: var(--qss-bg-raised, #1f1f25); box-shadow: 0 10px 28px rgba(0, 0, 0, 0.45); }
.qp-menu-title { padding: 2px 6px 4px; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-menu-row { display: flex; align-items: center; gap: 7px; padding: 4px 6px; border-radius: 5px; font-size: 12px; color: var(--qss-text, #d4d4d8); cursor: pointer; }
.qp-menu-row:hover { background: var(--qss-bg-hover, #313139); }
.qp-menu-note { padding: 4px 6px 0; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-drop-line { position: absolute; left: 8px; right: 8px; height: 2px; border-radius: 1px; background: var(--qss-accent, #7aa2f7); pointer-events: none; z-index: 5; }
.qp-block { padding: 9px 12px 10px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); display: flex; flex-direction: column; gap: 5px; }
.qp-block.lifted { opacity: 0.55; }
.qp-block-title { display: flex; align-items: center; gap: 4px; margin: 0 0 2px; font-size: 12px; font-weight: 600; color: var(--qss-text-secondary, #9a9aa5); cursor: grab; user-select: none; touch-action: none; }
.qp-grip { margin-left: -9px; color: var(--qss-text-muted, #6e6e7a); opacity: 0; transition: opacity 0.12s; }
.qp-block-title:hover .qp-grip { opacity: 1; }
.qp-block-num { margin-left: auto; font-weight: 400; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.add { color: var(--qss-success, #34d399); }
.del { color: var(--qss-error, #f87171); }
.qp-pn-empty { font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.mono { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; }
.qp-actions { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 2px; }
.qp-mini { padding: 2px 8px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: 6px; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; font-size: 11.5px; cursor: pointer; }
.qp-mini:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-mini:disabled { opacity: 0.45; cursor: default; }
.qp-mini.primary { border-color: color-mix(in srgb, var(--qss-accent, #7aa2f7) 60%, transparent); color: var(--qss-text, #d4d4d8); }
.qp-mini.danger { color: var(--qss-error, #f87171); }
.qp-now { display: flex; flex-direction: column; gap: 3px; padding: 4px 6px; border-radius: 7px; }
.qp-now.is-waiting { background: color-mix(in srgb, var(--qss-warning, #d29a3f) 8%, transparent); }
.qp-now:hover { background: var(--qss-bg-hover, #313139); }
.qp-now-main { display: flex; align-items: flex-start; gap: 8px; padding: 0; border: none; background: transparent; color: var(--qss-text, #d4d4d8); text-align: left; cursor: pointer; min-width: 0; }
.qp-now-main > :first-child { margin-top: 2px; }
.qp-now-text { flex: 1; min-width: 0; display: flex; flex-direction: column; }
.qp-now-title { font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-now-sub { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-now.is-waiting .qp-now-sub { color: var(--qss-warning, #d29a3f); }
.qp-now-actions { display: flex; gap: 4px; padding-left: 21px; }
.qp-vital { display: grid; grid-template-columns: auto 1fr; align-items: baseline; gap: 2px 8px; font-size: 12px; }
.qp-vital-k { color: var(--qss-text-muted, #6e6e7a); }
.qp-vital-v { text-align: right; color: var(--qss-text, #d4d4d8); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-vital-v.mono { font-size: 11.5px; }
.qp-vital-agent { display: inline-flex; align-items: center; justify-content: flex-end; gap: 5px; }
.qp-bar { grid-column: 1 / -1; height: 3px; border-radius: 2px; background: var(--qss-bg-card, #292930); overflow: hidden; }
.qp-bar-fill { display: block; height: 100%; background: var(--qss-text-secondary, #9a9aa5); }
.qp-bar-fill.hot { background: var(--qss-warning, #d29a3f); }
.qp-err { font-size: 11.5px; color: var(--qss-error, #f87171); white-space: pre-wrap; overflow-wrap: anywhere; max-height: 120px; overflow-y: auto; user-select: text; }
.qp-file { display: flex; flex-direction: column; gap: 0; padding: 3px 6px; border: none; border-radius: 5px; background: transparent; text-align: left; cursor: pointer; min-width: 0; }
.qp-file:hover { background: var(--qss-bg-hover, #313139); }
.qp-file-line { display: flex; align-items: baseline; gap: 6px; min-width: 0; }
.qp-file-name { flex: 1; min-width: 0; font-size: 12px; color: var(--qss-text, #d4d4d8); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-file-num { flex-shrink: 0; }
.qp-file-dir { font-size: 10.5px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; }
.qp-links { display: flex; flex-wrap: wrap; gap: 4px; }
.qp-pn-sub { margin-top: 6px; font-size: 11px; font-weight: 600; color: var(--qss-text-muted, #6e6e7a); }
.qp-side-row { display: flex; align-items: center; gap: 7px; padding: 4px 6px; border: none; border-radius: 6px; background: transparent; color: var(--qss-text, #d4d4d8); text-align: left; cursor: pointer; min-width: 0; }
.qp-side-row:hover,
.qp-side-row.on { background: var(--qss-bg-hover, #313139); }
.qp-side-title { min-width: 0; font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-tasklist { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; max-height: 180px; overflow-y: auto; }
.qp-tasklist li { position: relative; padding-left: 14px; font-size: 12px; line-height: 1.4; color: var(--qss-text, #d4d4d8); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-tasklist li::before { content: ''; position: absolute; left: 1px; top: 5px; width: 7px; height: 7px; border-radius: 50%; border: 1px solid var(--qss-text-muted, #6e6e7a); }
.qp-tasklist li.is-in_progress::before { border-color: var(--qss-accent, #7aa2f7); background: color-mix(in srgb, var(--qss-accent, #7aa2f7) 40%, transparent); }
.qp-tasklist li.is-completed { color: var(--qss-text-muted, #6e6e7a); text-decoration: line-through; }
.qp-tasklist li.is-completed::before { border-color: var(--qss-success, #34d399); background: var(--qss-success, #34d399); }
.qp-agent { display: grid; grid-template-columns: 16px 1fr; align-items: center; gap: 0 6px; padding: 4px 6px; border: none; border-radius: 6px; background: transparent; color: var(--qss-text, #d4d4d8); text-align: left; cursor: pointer; min-width: 0; }
.qp-agent:hover { background: var(--qss-bg-hover, #313139); }
.qp-agent.on { background: var(--qss-bg-card, #292930); }
.qp-agent-title { font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-agent-sub,
.qp-agent-last { grid-column: 2; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-srv { display: flex; align-items: center; gap: 6px; padding: 2px 0; font-size: 12px; }
.qp-srv-port { color: var(--qss-success, #34d399); }
.qp-srv-from { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text-muted, #6e6e7a); }
.qp-srv-confirm { margin-top: 4px; padding: 6px 8px; border-radius: 6px; border: 1px solid var(--qss-border, #47474f); background: var(--qss-bg-raised, #1f1f25); font-size: 11.5px; line-height: 1.5; color: var(--qss-text-secondary, #9a9aa5); }
.qp-srv-actions { display: flex; justify-content: flex-end; gap: 4px; margin-top: 4px; }
</style>
