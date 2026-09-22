<script setup lang="ts">
/**
 * The suite session — `ConsoleSuiteSession` (docs/PLAN-CONSOLE.md §11.1, phase
 * P3.5). The second session kind next to a PTY, and the only one that cannot run
 * anything: the suite's own processes — the Python sidecars, the Docker stack,
 * the MCP servers — as a read-only list of blocks. It replaced
 * `apps/shell/app/pages/processes.vue` once; since 2026-08-26 both surfaces
 * exist — `/processes` is the shell's own page (start/stop and a short tail,
 * belonging to no module), `/console?view=suite` this searchable log session —
 * and they read the same register through the same composable.
 *
 * The register's contract is unchanged and it is the reason this view is so
 * plain: `qs-core` keeps a **map, not a supervisor**
 * (`crates/qs-core/src/processes.rs`). Each entry carries the
 * `plugin:<module>|<command>` names its owning module exposes, and this view
 * invokes exactly those — never a command it composed itself. Where an entry
 * names no command, no button is rendered; there is nothing honest to put behind
 * one.
 *
 * Three things that look like details and are not:
 *
 *   - **The register is read on its event, the tails on a clock.** Both live in
 *     `useProcessRegister` (`packages/core/src/useProcessRegister.ts`), shared
 *     with the shell's process page so the two cannot drift: the list arrives
 *     once and then on `core.process.changed`; only the entries that are
 *     observed rather than reported (a `refreshCommand` — the MCP servers, the
 *     docker stack) are still asked, slowly, and only the tails tick every two
 *     seconds. Without that asking, a process that died while this tab was open
 *     would keep its "running" row until the user opened its module.
 *   - **All of it stops when the tab is off screen.** Both halves matter: the
 *     `visible` prop (another tab of this module is on screen) and the V3 warm
 *     cache's `onActivated`/`onDeactivated` (another module is on screen and this
 *     one is merely still mounted — `packages/core/src/keepAlive.ts`). The
 *     composable owns the second and takes the first as its gate; left running,
 *     a docker inspect would run for a view nobody is looking at.
 *   - **The log tail has no VT parser.** Docker especially answers with ANSI
 *     escapes in it, and this view renders plain text — so the escapes are
 *     stripped here rather than rendered as mojibake, and the UI says so
 *     instead of pretending the tail is styled output.
 *
 * Read-only is structural, not a policy: there is no input line, no prefill, no
 * `run_command`. The only writes are the start/stop commands the modules named.
 */
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { LOG_TAIL_CAP, toLogLines, useProcessRegister, type ProcessInfo } from '@quantsuite/core'

const props = defineProps<{
  /** False while the tab is off screen: stop polling, keep the DOM. */
  visible: boolean
}>()

const emit = defineEmits<{
  (e: 'copied', what: string): void
}>()

/** The tails' cadence, the shell page's too: a slower tick makes a process's
 * first lines look like they never came. */
const POLL_MS = 2000

/**
 * Lines asked of a `logsCommand`, and the cap on what is rendered.
 *
 * The old page asked for 300 because it painted them into a fixed `<pre>`. A
 * tail is a block now — scrollable, selectable, and searchable once P5 lands —
 * so the number is the pane's `RENDER_TAIL` instead. Above it the front is cut
 * and the block is marked `truncated`: a module that ignores `limit` and hands
 * back its whole ring buffer must not be able to put 200 000 nodes in the DOM.
 */
const LOG_LIMIT = 2000

/** 24×24 stroke paths — the QNavItem / `ConsoleBlock` icon language. */
interface IconPath {
  d: string
  cls?: string
}

/**
 * The state icons, deliberately the same silhouettes `ConsoleBlock` uses for a
 * command's status: an entry here has to read as a sibling of a command block.
 * `running` is the only animated one — a rotating arc over a still track.
 */
const STATE_ICONS: Record<'running' | 'stopped' | 'failed', IconPath[]> = {
  running: [
    { d: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18', cls: 'csx-track' },
    { d: 'M12 3a9 9 0 0 1 9 9', cls: 'csx-arc' },
  ],
  // A stop square in a circle: "announced, not up" — distinguishable from the
  // running track at 15px, which a plain hollow circle is not.
  stopped: [{ d: 'M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18' }, { d: 'M9.5 9.5h5v5h-5z' }],
  failed: [{ d: 'M6.4 6.4l11.2 11.2' }, { d: 'M17.6 6.4L6.4 17.6' }],
}

const LOCK_ICON = ['M5.5 11h13v10h-13z', 'M8.5 11V7.5a3.5 3.5 0 0 1 7 0V11']
const START_ICON = ['M8 5.5l11 6.5-11 6.5z']
const STOP_ICON = ['M6.5 6.5h11v11h-11z']
const COPY_ICON = [
  'M20 9h-9a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h9a2 2 0 0 0 2-2v-9a2 2 0 0 0-2-2z',
  'M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1',
]
const CHEVRON_ICON = 'M6 9.5l6 6 6-6'

/** One process's fetched tail. Kept per id so a collapse does not throw it away
 * and a re-expand does not show an empty block for one poll. */
interface Tail {
  /** Plain visible text, one entry per log line — the escapes are stripped. */
  lines: string[]
  /** The module returned more than `LOG_TAIL_CAP` lines and the front was cut. */
  truncated: boolean
  /** Why there is no tail right now — shown in the block, never swallowed. */
  error: string | null
}

/** The list, the bridge and the whole refresh/tail lifecycle; `tailExpanded`
 * below is this view's half of it, and `visible` its gate. */
const { processes, bridge, running, refresh } = useProcessRegister({
  tail: tailExpanded,
  enabled: () => props.visible,
  tailMs: POLL_MS,
})
const tails = ref<Record<string, Tail>>({})
/** Expanded entries, replaced rather than mutated — a `Set` is not reactive. */
const expanded = ref<Set<string>>(new Set())
/** Ids with a start/stop in flight, and the last failure per id. */
const busy = ref<Set<string>>(new Set())
const failures = ref<Record<string, string>>({})
const copyError = ref<Record<string, string>>({})

/** Log fetches in flight, so a slow docker tail cannot stack up behind the poll. */
const fetching = new Set<string>()

/** Drop one id's message. A fresh object, because the template reads the map. */
function without(map: Record<string, string>, id: string): Record<string, string> {
  if (!(id in map)) return map
  const next = { ...map }
  delete next[id]
  return next
}

function isRunning(p: ProcessInfo): boolean {
  return p.status.state === 'running'
}

/** Whether the entry's single action button has anything to invoke. */
function actionCommand(p: ProcessInfo): string | null {
  return isRunning(p) ? p.stopCommand : p.startCommand
}

function stateIcon(p: ProcessInfo): IconPath[] {
  if (p.status.state === 'running') return STATE_ICONS.running
  if (p.status.state === 'failed') return STATE_ICONS.failed
  return STATE_ICONS.stopped
}

function stateLabel(p: ProcessInfo): string {
  if (p.status.state === 'running') return 'Running'
  if (p.status.state === 'failed') return 'Failed'
  return 'Stopped'
}

/** Uptime from the `since` the module reported, coarse — the list repaints every
 * two seconds and a ticking seconds counter would just be noise. */
function uptime(since: number): string {
  const secs = Math.max(0, Math.round((Date.now() - since) / 1000))
  if (secs < 60) return `${secs}s`
  if (secs < 3600) return `${Math.floor(secs / 60)}m`
  return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`
}

/** The metadata row's state text: pid and uptime, or the failure verbatim. */
function meta(p: ProcessInfo): string {
  if (p.status.state === 'failed') return p.status.message
  if (p.status.state !== 'running') return 'stopped'
  // No pid means the module has no OS process of ours to point at (Docker).
  const who = p.status.pid === null ? 'running' : `pid ${p.status.pid}`
  return `${who} · up ${uptime(p.status.since)}`
}

/**
 * The module's answer as block output. `toLogLines` does the work (it is shared
 * with the shell's process page — `packages/core/src/logText.ts`); the error
 * slot is this view's own, because a block never swallows why it is empty.
 */
function toTail(raw: string[]): Tail {
  return { ...toLogLines(raw, LOG_TAIL_CAP), error: null }
}

/**
 * One entry's tail. Raw `invoke` because the command name is a dynamic string
 * the module chose: the typed wrappers in `@quantsuite/core` cannot cover it,
 * and the contract every logs command honours is `{ limit }` in, oldest-first
 * lines out.
 */
async function fetchTail(p: ProcessInfo): Promise<void> {
  const cmd = p.logsCommand
  if (!cmd || bridge.value !== 'ok' || fetching.has(p.id)) return
  fetching.add(p.id)
  try {
    const raw = await invoke<string[]>(cmd, { limit: LOG_LIMIT })
    tails.value = { ...tails.value, [p.id]: toTail(raw) }
  } catch (e) {
    // Keep the lines that are already on screen: a module restarting its log
    // source should not blank a tail the user is reading.
    const previous = tails.value[p.id]
    tails.value = {
      ...tails.value,
      [p.id]: {
        lines: previous?.lines ?? [],
        truncated: previous?.truncated ?? false,
        error: e instanceof Error ? e.message : String(e),
      },
    }
  } finally {
    fetching.delete(p.id)
  }
}

function refreshTails(): void {
  for (const p of processes.value) if (expanded.value.has(p.id)) void fetchTail(p)
}

function toggle(p: ProcessInfo): void {
  const next = new Set(expanded.value)
  if (next.has(p.id)) {
    next.delete(p.id)
  } else {
    next.add(p.id)
    void fetchTail(p) // do not make the user wait out the poll interval
  }
  expanded.value = next
}

/**
 * Start or stop, through the name the entry carries and nothing else.
 *
 * Nothing throws out of here: an `invoke` that rejects in a click handler takes
 * the handler with it, and the button would stay stuck in its busy state.
 */
async function act(p: ProcessInfo): Promise<void> {
  const cmd = actionCommand(p)
  if (!cmd) return
  failures.value = without(failures.value, p.id)
  busy.value = new Set(busy.value).add(p.id)
  try {
    await invoke(cmd) // start/stop take no arguments, by contract
  } catch (e) {
    failures.value = { ...failures.value, [p.id]: e instanceof Error ? e.message : String(e) }
  }
  const stillBusy = new Set(busy.value)
  stillBusy.delete(p.id)
  busy.value = stillBusy
  // The module reports the transition through the register, so the truth is one
  // refresh away — and the tail usually gains its first lines right here.
  await refresh()
}

/**
 * Copy a tail the way a block is copied: the plain text, no chrome, no line
 * numbers. A failed clipboard write is reported *here* — the page only hears
 * about successes, and a silent no-op means the user pastes yesterday's
 * clipboard into a ticket.
 */
async function copyTail(p: ProcessInfo): Promise<void> {
  copyError.value = without(copyError.value, p.id)

  const tail = tails.value[p.id]
  if (!tail || !tail.lines.length) return
  const text = tail.lines.join('\n')

  if (!navigator.clipboard) {
    copyError.value = { ...copyError.value, [p.id]: 'Clipboard unavailable' }
    return
  }
  try {
    await navigator.clipboard.writeText(text)
    emit('copied', `${p.id} log tail`)
  } catch (e) {
    copyError.value = {
      ...copyError.value,
      [p.id]: e instanceof Error ? e.message : 'Copy failed',
    }
  }
}

// ── this view's half of the dance ────────────────────────────────────────────

/**
 * The preselection may only ever run once. This component is deactivated, not
 * destroyed, and re-expanding on every return would re-open a tail the user
 * deliberately collapsed.
 */
let preselected = false

/** Run after every register read and on the tails' clock: unfold the first
 * entry that can produce a log, once, then fetch every unfolded tail. */
function tailExpanded(): void {
  if (!preselected && !expanded.value.size) {
    const first = processes.value.find((p) => p.logsCommand)
    if (first) {
      preselected = true
      toggle(first)
    }
  }
  refreshTails()
}
</script>

<template>
  <!-- Hidden, not unmounted, exactly like `ConsolePane`: the tab keeps its
       expanded tails and its scroll position while another tab is on screen. -->
  <div v-show="visible" class="csx">
    <header class="csx-head">
      <span class="csx-lock" role="img" aria-label="Read-only session">
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="1.7"
          stroke-linecap="round"
          stroke-linejoin="round"
          aria-hidden="true"
        >
          <path v-for="d in LOCK_ICON" :key="d" :d="d" />
        </svg>
      </span>
      <span class="csx-title">Suite processes</span>
      <span class="csx-head-note">
        Read-only — nothing can be typed here. The only actions are the start, stop and log
        commands each module named.
      </span>
      <span v-if="bridge === 'ok'" class="csx-count">
        {{ running }} of {{ processes.length }} running
      </span>
    </header>

    <div v-if="bridge === 'absent'" class="csx-empty">
      <p>The process register lives in the app.</p>
      <span>
        Running in a plain browser there are no processes to observe — open QuantSuite itself.
      </span>
    </div>

    <div v-else-if="bridge === 'ok' && !processes.length" class="csx-empty">
      <p>No module has announced a process.</p>
      <span>
        Modules announce theirs when their plugin sets up, so this list is normally filled at
        startup — an empty one means no module registered.
      </span>
    </div>

    <div v-else class="csx-list">
      <article
        v-for="p in processes"
        :key="p.id"
        class="csx-proc"
        :class="[`is-${p.status.state}`, { 'is-open': expanded.has(p.id) }]"
        :aria-busy="p.status.state === 'running' ? 'true' : undefined"
      >
        <header class="csx-proc-head">
          <span class="csx-state" :title="stateLabel(p)" role="img" :aria-label="stateLabel(p)">
            <svg
              width="15"
              height="15"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.7"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path v-for="path in stateIcon(p)" :key="path.d" :d="path.d" :class="path.cls" />
            </svg>
          </span>

          <code class="csx-id" :title="p.id">{{ p.id }}</code>
          <span class="csx-label" :title="p.label">{{ p.label }}</span>

          <div class="csx-meta">
            <span class="csx-tag" :title="`Owned by the ${p.module} module`">{{ p.module }}</span>
            <span class="csx-state-text" :title="meta(p)">{{ meta(p) }}</span>
            <span v-if="!p.logsCommand" class="csx-tag" title="This module offers no log tail">
              no tail
            </span>
            <span
              v-if="tails[p.id]?.truncated"
              class="csx-tag is-warn"
              title="The tail was longer than this view renders — its start was cut."
            >truncated</span>
          </div>

          <span v-if="failures[p.id]" class="csx-fail" role="alert" :title="failures[p.id]">
            {{ failures[p.id] }}
          </span>
          <span v-if="copyError[p.id]" class="csx-fail" role="status" :title="copyError[p.id]">
            copy failed
          </span>

          <div class="csx-actions">
            <button
              v-if="tails[p.id]?.lines.length"
              class="csx-act"
              title="Copy this log tail"
              @click="copyTail(p)"
            >
              <svg class="csx-ico" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <path v-for="d in COPY_ICON" :key="d" :d="d" />
              </svg>
              <span>Log</span>
            </button>

            <!-- No command, no button: the module cannot serve this action. -->
            <button
              v-if="actionCommand(p)"
              class="csx-act is-primary"
              :disabled="busy.has(p.id)"
              :title="isRunning(p) ? `Stop via ${p.stopCommand}` : `Start via ${p.startCommand}`"
              @click="act(p)"
            >
              <svg class="csx-ico" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                <path v-for="d in isRunning(p) ? STOP_ICON : START_ICON" :key="d" :d="d" />
              </svg>
              <span>{{ isRunning(p) ? 'Stop' : 'Start' }}</span>
            </button>
          </div>

          <button
            class="csx-fold"
            :title="expanded.has(p.id) ? 'Collapse' : 'Show log tail'"
            :aria-expanded="expanded.has(p.id) ? 'true' : 'false'"
            @click="toggle(p)"
          >
            <svg
              class="csx-ico csx-chev"
              :class="{ 'is-folded': !expanded.has(p.id) }"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.7"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path :d="CHEVRON_ICON" />
            </svg>
          </button>
        </header>

        <div v-if="expanded.has(p.id)" class="csx-body">
          <template v-if="p.logsCommand">
            <div class="csx-tailbar">
              <span
                class="csx-tailbar-hint"
                title="No VT parser here: ANSI escapes and colours are stripped, and a carriage-return rewrite (a progress bar) is reduced to the text it ended on. The pane's blocks are the styled renderer."
              >plain text · escapes stripped</span>
              <span class="csx-tailbar-hint">
                last {{ tails[p.id]?.lines.length ?? 0 }} lines, refreshed every
                {{ POLL_MS / 1000 }}s
              </span>
            </div>

            <p v-if="tails[p.id]?.error" class="csx-tail-error" role="alert">
              No tail right now — {{ tails[p.id]?.error }}
            </p>

            <!-- Plain text: the tail is stripped of escapes above, and what was
                 cut is what the module was never asked for — `truncated` is the
                 honest statement. -->
            <pre v-if="tails[p.id]?.lines.length" class="csx-tail-pre"><span
              v-if="tails[p.id]!.truncated"
              class="csx-tail-cut"
            >… earlier output trimmed
</span>{{ tails[p.id]!.lines.join('\n') }}</pre>
            <p v-else-if="!tails[p.id]?.error" class="csx-tail-empty">
              Nothing captured yet — output appears here the moment the process writes.
            </p>
          </template>

          <p v-else class="csx-tail-empty">
            {{ p.label }} offers no log tail. Its output goes to the suite log
            (<code>~/.quantsuite/logs/</code>) instead.
          </p>
        </div>
      </article>
    </div>
  </div>
</template>

<style scoped>
.csx {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  background: var(--qss-bg);
  color: var(--qss-text);
}

/* ── the read-only banner ── */

.csx-head {
  flex: none;
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  padding: 6px 12px;
  border-bottom: 1px solid var(--qss-border-subtle);
  background: var(--qss-bg-chrome);
  /* Chrome, not content: never part of a selection dragged over a tail. */
  user-select: none;
  -webkit-user-select: none;
}
.csx-lock {
  flex: none;
  display: grid;
  place-items: center;
  color: var(--qss-text-muted);
}
.csx-title {
  flex: none;
  font-family: var(--qss-font-sans);
  font-size: 12px;
  font-weight: 600;
}
.csx-head-note {
  flex: 1 1 24ch;
  min-width: 0;
  font-family: var(--qss-font-sans);
  font-size: 11px;
  color: var(--qss-text-muted);
}
.csx-count {
  flex: none;
  font-family: var(--qss-font-mono);
  font-size: 11px;
  color: var(--qss-text-muted);
  font-variant-numeric: tabular-nums;
}

/* ── empty states ── */

.csx-empty {
  margin: auto;
  max-width: 52ch;
  padding: 24px;
  text-align: center;
}
.csx-empty p {
  margin: 0 0 6px;
  font-family: var(--qss-font-sans);
  font-size: 13px;
  color: var(--qss-text-secondary);
}
.csx-empty span {
  font-family: var(--qss-font-sans);
  font-size: 11.5px;
  color: var(--qss-text-muted);
}

/* ── the list ── */

.csx-list {
  flex: 1;
  min-height: 0;
  overflow: auto;
}

/*
 * One entry, built like `ConsoleBlock`: the state colour is a single custom
 * property on the root, so the gutter stripe, the icon and the state text cannot
 * drift apart.
 */
.csx-proc {
  --csx-accent: var(--qss-text-muted);
  position: relative;
  padding-left: 10px;
  border-bottom: 1px solid var(--qss-border-subtle);
}
.csx-proc.is-running {
  --csx-accent: var(--qss-success);
}
.csx-proc.is-failed {
  --csx-accent: var(--qss-error);
}

/* The gutter stripe lives in the root's PADDING, so the sticky header — which
   only spans the content box — can never paint over it. */
.csx-proc::before {
  content: '';
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  width: 2px;
  background: var(--csx-accent);
  opacity: 0.55;
}
.csx-proc:hover::before,
.csx-proc.is-open::before {
  opacity: 1;
}
.csx-proc.is-running::before {
  opacity: 1;
  /* Opacity on a 2px box — the cheapest possible "still alive", and a keyframe
     rather than a JS timer so it costs nothing on the main thread. */
  animation: csx-breathe calc(var(--qss-dur-slow) * 4) var(--qss-ease-inout) infinite;
}

.csx-proc-head {
  /* Sticky, so the process a 2000-line tail belongs to stays readable while the
     tail scrolls — the same rule the block header follows. */
  position: sticky;
  top: 0;
  z-index: 1;
  display: flex;
  align-items: flex-start;
  /* Wraps: this view also has to work in a narrow split pane, where the metadata
     and the actions drop to a second row instead of squeezing the id away. */
  flex-wrap: wrap;
  gap: 8px;
  padding: 6px 10px 6px 6px;
  background: var(--qss-bg-raised);
  border-bottom: 1px solid var(--qss-border-subtle);
}
.csx-proc.is-open > .csx-proc-head {
  background: var(--qss-bg-card);
}

.csx-state {
  flex: none;
  display: grid;
  place-items: center;
  width: 15px;
  height: 15px;
  /* Centres the icon on the id's first line (12.5px × 1.45 ≈ 18px). */
  margin-top: 2px;
  color: var(--csx-accent);
  user-select: none;
}
.csx-track {
  opacity: 0.25;
}
.csx-arc {
  transform-origin: 12px 12px;
  animation: csx-spin calc(var(--qss-dur-slow) * 2) linear infinite;
}

.csx-id {
  flex: none;
  font-family: var(--qss-font-mono);
  font-size: 12.5px;
  line-height: 1.45;
  color: var(--qss-text);
}
.csx-label {
  flex: 1 1 12ch;
  min-width: 0;
  font-family: var(--qss-font-sans);
  font-size: 11.5px;
  line-height: 1.55;
  color: var(--qss-text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.csx-meta,
.csx-actions,
.csx-fail {
  /* Ours, not the process's: kept out of a selection so a copied tail stays a
     copied tail. */
  user-select: none;
  -webkit-user-select: none;
}

.csx-meta {
  flex: none;
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  min-height: 18px;
  font-size: 11px;
  color: var(--qss-text-muted);
  font-variant-numeric: tabular-nums;
}
.csx-state-text {
  /* A `Failed` message is arbitrary text from the module — it must not push the
     row wider than the list. */
  max-width: 42ch;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-family: var(--qss-font-mono);
  color: var(--csx-accent);
}
.csx-proc.is-stopped .csx-state-text {
  color: var(--qss-text-muted);
}
.csx-tag {
  flex: none;
  padding: 1px 6px;
  border: 1px solid var(--qss-border);
  border-radius: 999px;
  font-family: var(--qss-font-sans);
  font-size: 9.5px;
  letter-spacing: 0.04em;
  text-transform: uppercase;
  color: var(--qss-text-muted);
}
.csx-tag.is-warn {
  color: var(--qss-warning);
  border-color: color-mix(in srgb, var(--qss-warning) 40%, transparent);
}

.csx-fail {
  flex: none;
  max-width: 30ch;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-family: var(--qss-font-sans);
  font-size: 10.5px;
  color: var(--qss-error);
}

.csx-actions {
  flex: none;
  display: flex;
  align-items: center;
  gap: 2px;
  opacity: 0;
  /* Invisible controls must not be clickable — but keyboard focus still lands on
     them, which is what brings the group back. */
  pointer-events: none;
  transition: opacity var(--qss-dur-fast) var(--qss-ease-out);
}
.csx-proc:hover > .csx-proc-head > .csx-actions,
.csx-proc.is-open > .csx-proc-head > .csx-actions,
.csx-actions:focus-within {
  opacity: 1;
  pointer-events: auto;
}

.csx-act,
.csx-fold {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 6px;
  border: 1px solid transparent;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-secondary);
  font-family: var(--qss-font-sans);
  font-size: 10.5px;
  cursor: pointer;
  transition: background var(--qss-dur-instant) var(--qss-ease-out),
    color var(--qss-dur-instant) var(--qss-ease-out);
}
.csx-act:hover:not(:disabled),
.csx-fold:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.csx-act.is-primary {
  border-color: var(--qss-border);
}
.csx-act:disabled {
  opacity: 0.4;
  cursor: default;
}
.csx-fold {
  /* Always visible, and a header child rather than an action: expanding is how a
     user reaches a tail at all. */
  flex: none;
  padding: 2px 4px;
  color: var(--qss-text-muted);
}

.csx-ico {
  flex: none;
  width: 13px;
  height: 13px;
}
.csx-chev {
  transition: transform var(--qss-dur-fast) var(--qss-ease-out);
}
.csx-chev.is-folded {
  transform: rotate(-90deg);
}

/* ── the tail ── */

.csx-body {
  padding: 4px 10px 8px 6px;
}

.csx-tailbar {
  display: flex;
  align-items: baseline;
  flex-wrap: wrap;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 4px;
  user-select: none;
  -webkit-user-select: none;
}
.csx-tailbar-hint {
  font-family: var(--qss-font-sans);
  font-size: 10.5px;
  color: var(--qss-text-muted);
}
.csx-tailbar-hint:first-child {
  /* Hoverable: the tooltip is where the stripping is explained. */
  border-bottom: 1px dotted var(--qss-border);
  cursor: help;
}

.csx-tail-pre {
  margin: 0;
  padding: 2px 0;
  font: 400 12px/1.35 var(--qss-font-mono);
  color: var(--qss-text-secondary);
  white-space: pre-wrap;
  word-break: break-word;
}
.csx-tail-cut {
  color: var(--qss-text-muted);
  font-family: var(--qss-font-sans);
  font-size: 10.5px;
}

.csx-tail-error,
.csx-tail-empty {
  margin: 0;
  font-family: var(--qss-font-sans);
  font-size: 11.5px;
  color: var(--qss-text-muted);
}
.csx-tail-error {
  padding: 5px 9px;
  border: 1px solid color-mix(in srgb, var(--qss-error) 40%, transparent);
  border-left-width: 3px;
  border-radius: 7px;
  background: color-mix(in srgb, var(--qss-error) 10%, transparent);
  color: var(--qss-text);
}
.csx-tail-empty code {
  font-family: var(--qss-font-mono);
  font-size: 11px;
}

@keyframes csx-spin {
  to {
    transform: rotate(360deg);
  }
}
@keyframes csx-breathe {
  0%,
  100% {
    opacity: 1;
  }
  50% {
    opacity: 0.35;
  }
}

@media (prefers-reduced-motion: reduce) {
  .csx-arc,
  .csx-proc.is-running::before {
    animation: none;
  }
  .csx-chev {
    transition: none;
  }
}
</style>
