<script setup lang="ts">
/**
 * The process log view — the shell's own screen for every process QuantSuite
 * runs (2026-08-26).
 *
 * History, because this page existed once before: P3.5 deleted it and moved the
 * process list into QuantConsole as a read-only *session*
 * (docs/PLAN-CONSOLE.md §11.1). That made the rail's process button a
 * navigation into a module — press it while working in QuantTerminal and the
 * terminal is what you lose. The button comes back here instead: a screen of
 * its own, owned by the shell, that belongs to no module and takes none away.
 * The stage's warm cache keeps QuantTerminal mounted behind it, so going back
 * is instant (`ModuleStage`).
 *
 * What it is *for* is the logs. The console's session lists processes and lets
 * you unfold a tail under each; this page is the other way round — the list is
 * a sidebar and the log fills the screen, because reading a log is the whole
 * job here. The console session stays exactly as it is; both read the same
 * register, and the text handling they share lives in
 * `packages/core/src/logText.ts` rather than in two copies.
 *
 * The register's contract is what keeps this page honest (ARCHITECTURE.md §6):
 * `qs-core` keeps a **map, not a supervisor**. Every entry names the
 * `plugin:<module>|<command>` its owning module exposes, and this page invokes
 * exactly those — never a command it composed itself. Where an entry names no
 * command, no button is rendered: there is nothing honest to put behind one.
 *
 * Three things that look like details and are not:
 *
 *   - **The register is read on its event, the tail on a clock.** Both live in
 *     `useProcessRegister` (`packages/core/src/useProcessRegister.ts`), shared
 *     with the console session so the two cannot drift: the list arrives once
 *     and then on `core.process.changed`; only the entries that are observed
 *     rather than reported (a `refreshCommand` — the MCP servers, the docker
 *     stack) are still asked, slowly, and only the tail ticks every two seconds.
 *   - **All of it stops when the page is off screen.** Shell pages live in the
 *     warm cache too — this page is deactivated, not destroyed, when you switch
 *     to a module (`packages/core/src/keepAlive.ts`); the composable stands down
 *     then, or a docker inspect would run for a screen nobody is looking at.
 *   - **Only the selected process is tailed.** The log commands are the
 *     expensive half of the poll, and every line but one selection's is off
 *     screen anyway.
 */
import { computed, nextTick, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { LOG_TAIL_CAP, toLogLines, useProcessRegister, type ProcessInfo } from '@quantsuite/core'

/** The tail's cadence, the console session's too: a slower tick makes a
 * process's first lines look like they never came. */
const POLL_MS = 2000

/** Lines asked of a `logsCommand`. The render cap is `LOG_TAIL_CAP`. */
const LOG_LIMIT = 2000

/** The list, the bridge and the whole refresh/tail lifecycle; `tailSelected`
 * below is this page's half of it. */
const { processes, bridge, running, refresh } = useProcessRegister({ tail: tailSelected, tailMs: POLL_MS })

const selectedId = ref<string | null>(null)
const lines = ref<string[]>([])
const truncated = ref(false)
const logError = ref<string | null>(null)
/** Ids with a start/stop in flight, and the last failure per id. */
const busy = ref<Set<string>>(new Set())
const failures = ref<Record<string, string>>({})
const copyError = ref<string | null>(null)
const copied = ref(false)

/** A log fetch in flight, so a slow docker tail cannot stack up behind the poll. */
let fetching = false

const selected = computed(() => processes.value.find((p) => p.id === selectedId.value) ?? null)

function isRunning(p: ProcessInfo): boolean {
  return p.status.state === 'running'
}

/** Whether the entry's single action button has anything to invoke. */
function actionCommand(p: ProcessInfo): string | null {
  return isRunning(p) ? p.stopCommand : p.startCommand
}

function stateOf(p: ProcessInfo): 'running' | 'stopped' | 'failed' {
  if (p.status.state === 'running') return 'running'
  if (p.status.state === 'failed') return 'failed'
  return 'stopped'
}

/** Uptime from the `since` the module reported, coarse — the list repaints
 * every two seconds and a ticking seconds counter would just be noise. */
function uptime(since: number): string {
  const secs = Math.max(0, Math.round((Date.now() - since) / 1000))
  if (secs < 60) return `${secs}s`
  if (secs < 3600) return `${Math.floor(secs / 60)}m`
  return `${Math.floor(secs / 3600)}h ${Math.floor((secs % 3600) / 60)}m`
}

/** The status line: pid and uptime, or the failure verbatim. */
function meta(p: ProcessInfo): string {
  if (p.status.state === 'failed') return p.status.message
  if (p.status.state !== 'running') return 'stopped'
  // No pid means the module has no OS process of ours to point at (Docker).
  const who = p.status.pid === null ? 'running' : `pid ${p.status.pid}`
  return `${who} · up ${uptime(p.status.since)}`
}

/** Drop one id's message. A fresh object, because the template reads the map. */
function without(map: Record<string, string>, id: string): Record<string, string> {
  if (!(id in map)) return map
  const next = { ...map }
  delete next[id]
  return next
}

// ── the register ────────────────────────────────────────────────────────────

/**
 * This page's half of the dance, run after every register read and on the
 * tail's clock: keep a selection alive across restarts and reorderings — pick
 * the first process that can actually produce a log if there is none — then
 * fetch its tail.
 */
async function tailSelected(): Promise<void> {
  if (!processes.value.some((p) => p.id === selectedId.value)) selectedId.value = null
  if (!selectedId.value) {
    const first = processes.value.find((p) => p.logsCommand) ?? processes.value[0]
    if (first) select(first)
  }
  await fetchTail()
}

/**
 * The selected process's tail. Raw `invoke` because the command name is a
 * dynamic string the module chose: the typed wrappers in `@quantsuite/core`
 * cannot cover it, and the contract every logs command honours is `{ limit }`
 * in, oldest-first lines out.
 */
async function fetchTail(): Promise<void> {
  const p = selected.value
  if (!p) return
  if (!p.logsCommand || bridge.value !== 'ok' || fetching) return
  fetching = true
  const asked = p.id
  try {
    const raw = await invoke<string[]>(p.logsCommand, { limit: LOG_LIMIT })
    // The selection can change while a slow tail is in flight — a docker log
    // landing in another process's pane is the worst kind of wrong.
    if (asked !== selectedId.value) return
    const tail = toLogLines(raw, LOG_TAIL_CAP)
    lines.value = tail.lines
    truncated.value = tail.truncated
    logError.value = null
    void scrollToEnd()
  } catch (e) {
    if (asked !== selectedId.value) return
    // Keep the lines already on screen: a module restarting its log source
    // must not blank a tail the user is reading.
    logError.value = e instanceof Error ? e.message : String(e)
  } finally {
    fetching = false
  }
}

function select(p: ProcessInfo): void {
  if (selectedId.value === p.id) return
  selectedId.value = p.id
  lines.value = []
  truncated.value = false
  logError.value = null
  copyError.value = null
  follow.value = true
  void fetchTail() // do not make the user wait out the poll interval
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
  // refresh away — and the log usually gains its first lines right here.
  await refresh()
}

/**
 * Copy the tail the way a terminal selection would be copied: plain text, no
 * chrome, no line numbers. A failed clipboard write is reported *here* — a
 * silent no-op means the user pastes yesterday's clipboard into a ticket.
 */
async function copyTail(): Promise<void> {
  copyError.value = null
  if (!lines.value.length) return
  if (!navigator.clipboard) {
    copyError.value = 'Clipboard unavailable'
    return
  }
  try {
    await navigator.clipboard.writeText(lines.value.join('\n'))
    copied.value = true
    setTimeout(() => {
      copied.value = false
    }, 1600)
  } catch (e) {
    copyError.value = e instanceof Error ? e.message : 'Copy failed'
  }
}

// ── following the tail ──────────────────────────────────────────────────────

/**
 * Auto-scroll, and the one rule that makes it bearable: scrolling up turns it
 * off, scrolling back to the bottom turns it on again. A log that yanks itself
 * down every two seconds while you are reading the middle of it is unusable.
 */
const follow = ref(true)
const logEl = ref<HTMLElement | null>(null)

async function scrollToEnd(): Promise<void> {
  if (!follow.value) return
  await nextTick()
  const el = logEl.value
  if (el) el.scrollTop = el.scrollHeight
}

function onLogScroll(): void {
  const el = logEl.value
  if (!el) return
  // A few pixels of slack: browsers report fractional scroll positions at the
  // bottom, and an exact comparison would drop follow on its own.
  follow.value = el.scrollHeight - el.scrollTop - el.clientHeight < 24
}

watch(selectedId, () => void scrollToEnd())
</script>

<template>
  <div class="proc">
    <header class="proc-head">
      <div class="proc-head-text">
        <h1>Processes</h1>
        <p>Every process the suite's modules have announced — and their logs.</p>
      </div>
      <span class="proc-spacer" />
      <span class="proc-count" :class="{ 'is-live': running > 0 }">
        {{ running }} of {{ processes.length }} running
      </span>
    </header>

    <!-- The bridge is what tells the two empty states apart. -->
    <div v-if="bridge === 'absent'" class="proc-empty">
      Running in a plain browser there are no processes to observe — open QuantSuite itself.
    </div>
    <div v-else-if="bridge === 'ok' && !processes.length" class="proc-empty">
      No module has announced a process yet.
    </div>

    <div v-else class="proc-body">
      <!-- The list is a sidebar here; the log is the page. -->
      <nav class="proc-list" aria-label="Processes">
        <button
          v-for="p in processes"
          :key="p.id"
          class="proc-item"
          :class="{ 'is-selected': p.id === selectedId }"
          @click="select(p)"
        >
          <span class="proc-dot" :data-state="stateOf(p)" />
          <span class="proc-item-text">
            <span class="proc-id">{{ p.id }}</span>
            <span class="proc-label">{{ p.label }}</span>
          </span>
          <span class="proc-module">{{ p.module }}</span>
        </button>
      </nav>

      <section v-if="selected" class="proc-log-pane">
        <header class="proc-log-head">
          <span class="proc-dot" :data-state="stateOf(selected)" />
          <span class="proc-log-title">{{ selected.id }}</span>
          <span class="proc-log-meta">{{ meta(selected) }}</span>
          <span class="proc-spacer" />

          <button
            v-if="actionCommand(selected)"
            class="proc-btn"
            :disabled="busy.has(selected.id)"
            @click="act(selected)"
          >
            {{ isRunning(selected) ? 'Stop' : 'Start' }}
          </button>
          <button class="proc-btn" :disabled="!lines.length" @click="copyTail">
            {{ copied ? 'Copied' : 'Copy' }}
          </button>
          <!-- A toggle, not a status: the poll turns it off when you scroll up,
               and this is how you get it back without scrolling down. -->
          <button
            class="proc-btn"
            :class="{ 'is-on': follow }"
            title="Scroll to the newest line as it arrives"
            @click="follow = !follow; scrollToEnd()"
          >
            Follow
          </button>
        </header>

        <p v-if="failures[selected.id]" class="proc-error">{{ failures[selected.id] }}</p>
        <p v-if="copyError" class="proc-error">{{ copyError }}</p>

        <div ref="logEl" class="proc-log" @scroll.passive="onLogScroll">
          <p v-if="truncated" class="proc-cut">
            Older lines were cut — showing the last {{ LOG_TAIL_CAP }}.
          </p>
          <pre v-if="lines.length" class="proc-lines">{{ lines.join('\n') }}</pre>
          <p v-else-if="logError" class="proc-error">{{ logError }}</p>
          <p v-else-if="!selected.logsCommand" class="proc-hint">
            {{ selected.module }} exposes no log command for this process.
          </p>
          <p v-else class="proc-hint">No output yet.</p>
        </div>

        <footer class="proc-log-foot">
          <span>plain text · escapes stripped</span>
          <span class="proc-spacer" />
          <span>last {{ LOG_LIMIT }} lines, refreshed every {{ POLL_MS / 1000 }}s</span>
        </footer>
      </section>
    </div>
  </div>
</template>

<style scoped>
.proc {
  height: 100%;
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 20px 24px 24px;
  gap: 16px;
}

.proc-head {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-shrink: 0;
}
.proc-head-text h1 {
  margin: 0;
  font-size: 18px;
  font-weight: 650;
  color: var(--qss-text);
}
.proc-head-text p {
  margin: 2px 0 0;
  font-size: 12px;
  color: var(--qss-text-muted);
}
.proc-spacer {
  flex: 1;
}
.proc-count {
  padding: 4px 10px;
  border: 1px solid var(--qss-border);
  border-radius: 999px;
  font-size: 12px;
  color: var(--qss-text-muted);
}
.proc-count.is-live {
  color: var(--qss-text);
  border-color: var(--qss-accent, #4a9eff);
}

.proc-empty {
  padding: 24px;
  border: 1px dashed var(--qss-border);
  border-radius: var(--qss-radius-lg, 12px);
  color: var(--qss-text-muted);
  font-size: 13px;
}

/* Sidebar + log. `minmax(0, 1fr)` so a long log line cannot widen the grid. */
.proc-body {
  flex: 1;
  min-height: 0;
  display: grid;
  grid-template-columns: 260px minmax(0, 1fr);
  gap: 16px;
}

.proc-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  overflow-y: auto;
  padding-right: 4px;
}
.proc-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border: 1px solid transparent;
  border-radius: 10px;
  background: none;
  color: var(--qss-text-muted);
  text-align: left;
  cursor: pointer;
  transition: background var(--qss-dur-instant) ease, color var(--qss-dur-instant) ease;
}
.proc-item:hover {
  background: var(--qss-bg-hover);
}
.proc-item.is-selected {
  background: var(--qss-bg-raised);
  border-color: var(--qss-border);
  color: var(--qss-text);
}
.proc-item-text {
  display: flex;
  flex-direction: column;
  min-width: 0;
  flex: 1;
}
.proc-id {
  font-size: 12px;
  font-weight: 600;
  font-family: var(--qss-font-mono);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.proc-label {
  font-size: 11px;
  color: var(--qss-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.proc-module {
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
  color: var(--qss-text-muted);
}

/* One dot, three states — the same vocabulary the rail's process dot uses. */
.proc-dot {
  width: 8px;
  height: 8px;
  flex-shrink: 0;
  border-radius: 50%;
  background: var(--qss-text-muted);
}
.proc-dot[data-state='running'] {
  background: #3fb950;
  box-shadow: 0 0 0 3px rgb(63 185 80 / 0.15);
}
.proc-dot[data-state='failed'] {
  background: #f85149;
  box-shadow: 0 0 0 3px rgb(248 81 73 / 0.15);
}

.proc-log-pane {
  display: flex;
  flex-direction: column;
  min-height: 0;
  min-width: 0;
  border: 1px solid var(--qss-border);
  border-radius: var(--qss-radius-lg, 12px);
  background: var(--qss-bg-raised);
  overflow: hidden;
}
.proc-log-head {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-shrink: 0;
  padding: 10px 12px;
  border-bottom: 1px solid var(--qss-border);
}
.proc-log-title {
  font-family: var(--qss-font-mono);
  font-size: 12px;
  font-weight: 600;
  color: var(--qss-text);
}
.proc-log-meta {
  font-size: 11px;
  color: var(--qss-text-muted);
}
.proc-btn {
  padding: 4px 10px;
  border: 1px solid var(--qss-border);
  border-radius: 8px;
  background: none;
  color: var(--qss-text-muted);
  font-size: 11px;
  cursor: pointer;
  transition: background var(--qss-dur-instant) ease, color var(--qss-dur-instant) ease;
}
.proc-btn:hover:not(:disabled) {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.proc-btn:disabled {
  opacity: 0.45;
  cursor: default;
}
.proc-btn.is-on {
  color: var(--qss-text);
  border-color: var(--qss-accent, #4a9eff);
}

/* The log itself: the only thing on this page allowed to grow. */
.proc-log {
  flex: 1;
  min-height: 0;
  overflow: auto;
  padding: 10px 12px;
  background: var(--qss-bg);
}
.proc-lines {
  margin: 0;
  font-family: var(--qss-font-mono);
  font-size: 11.5px;
  line-height: 1.5;
  color: var(--qss-text);
  /* Wrapped, not clipped: a log line that runs off the right edge is a line
     the user cannot read without a horizontal scrollbar in the way. */
  white-space: pre-wrap;
  word-break: break-word;
  user-select: text;
}
.proc-cut,
.proc-hint,
.proc-error {
  margin: 0 0 8px;
  font-size: 11px;
  color: var(--qss-text-muted);
}
.proc-error {
  color: #f85149;
}
.proc-log-foot {
  display: flex;
  align-items: center;
  flex-shrink: 0;
  padding: 6px 12px;
  border-top: 1px solid var(--qss-border);
  font-size: 10.5px;
  color: var(--qss-text-muted);
}
</style>
