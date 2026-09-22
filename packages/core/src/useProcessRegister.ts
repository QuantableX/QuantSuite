/**
 * The suite's one process-register composable (V3) — the "refresh, then tail"
 * dance the shell's process page (`apps/shell/app/pages/processes.vue`) and
 * QuantConsole's suite session (`SuiteSession.vue`) used to carry as two
 * copies, which drifted.
 *
 * What the register *is* decides how it is read (`crates/qs-core/src/processes.rs`):
 * a **map of reported state**, not a supervisor. It changes only when a module
 * calls `mark_*`, and every real transition goes out on the bus as
 * `core.process.changed` — so the list is read once on arming and then on that
 * event, never on a clock. Two things still need a clock while a page is on
 * screen, and only then:
 *
 *   - **The tails.** Log lines announce nothing; the page's `tail` callback runs
 *     every `tailMs` and fetches whatever the page is showing.
 *   - **The observed entries.** `systems` and `algo` hold their child and report
 *     the moment it exits; the MCP servers and the docker stack are only ever
 *     observed by asking, and name a `refreshCommand` for it. Every `refreshMs`
 *     those are invoked: the module re-checks, reports what it finds, and if
 *     that is news the register announces it like any other transition — the
 *     tick itself reads nothing. Without it a process that died while the page
 *     was open would keep its "running" row.
 *
 * Warm-cache rules (`keepAlive.ts`): both pages are deactivated, not destroyed,
 * while another module is on screen. Arming happens in `onMounted` only inside
 * an active KeepAlive tree (a page resolves async and can mount into a stage
 * already switched away from), again in `onActivated`, and everything stands
 * down in `onDeactivated` — a docker inspect every few seconds for a screen
 * nobody is looking at is what the two copies each had to guard against.
 * `enabled` is the page's own extra gate (a tab's `visible` prop).
 */

import { computed, onActivated, onDeactivated, onMounted, onUnmounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import * as bus from './bus'
import { qs } from './commands'
import { inActiveKeepAliveTree } from './keepAlive'
import type { ProcessInfo } from './types'

/** Bus topic of a real transition — `CHANGED_TOPIC` in `processes.rs`. The
 * payload names the entry and its new state; subscribers re-read the register
 * rather than patching a copy, so nothing here depends on the shape. */
export const PROCESS_CHANGED = 'core.process.changed'

/**
 * Tri-state on purpose. "No bridge" and "not asked yet" produce the same empty
 * list but opposite explanations, and flashing "open QuantSuite itself" inside
 * QuantSuite for one frame is a lie worth avoiding.
 */
export type ProcessBridge = 'unknown' | 'ok' | 'absent'

export interface ProcessRegisterOptions {
  /**
   * Fetch the tails the page shows. Runs after every register read and on its
   * own cadence in between. Must not reject: the pages keep a failure in their
   * own state, per tail, so a broken log source never blanks the list.
   */
  tail: () => void | Promise<void>
  /** The page's own on-screen gate, AND-ed with the warm cache's. */
  enabled?: () => boolean
  /** Cadence of `tail` while armed. */
  tailMs?: number
  /** Cadence of the observed entries' `refreshCommand`s while armed. */
  refreshMs?: number
}

export function useProcessRegister(options: ProcessRegisterOptions) {
  const { tail, enabled = () => true, tailMs = 2000, refreshMs = 5000 } = options

  const processes = ref<ProcessInfo[]>([])
  const bridge = ref<ProcessBridge>('unknown')
  const running = computed(() => processes.value.filter((p) => p.status.state === 'running').length)

  /**
   * Ask the modules that only learn by looking to look. Failures are swallowed
   * on purpose: a module that cannot answer right now leaves its last reported
   * state standing, which is still better than blanking the list.
   */
  async function poke(): Promise<void> {
    await Promise.all(
      processes.value
        .map((p) => p.refreshCommand)
        .filter((cmd): cmd is string => !!cmd)
        .map((cmd) => invoke(cmd).catch(() => undefined))
    )
  }

  /** Read the register, then the tails — chained, not parallel, because a tail
   * fetched before the register answered would be attributed to a stale entry. */
  async function read(): Promise<void> {
    try {
      processes.value = await qs.core.processList()
      bridge.value = 'ok'
    } catch {
      bridge.value = 'absent' // plain browser — no register to talk to
    }
    await tail()
  }

  /**
   * The full dance: poke the observed entries, read, tail. What a page runs
   * after a start/stop — the module reports the transition through the
   * register, so the truth is one refresh away, and the log usually gains its
   * first lines right here.
   */
  async function refresh(): Promise<void> {
    await poke()
    await read()
  }

  let offChanged: (() => void) | null = null
  let tailTimer: ReturnType<typeof setInterval> | null = null
  let refreshTimer: ReturnType<typeof setInterval> | null = null
  /** True while no KeepAlive ancestor is deactivated — see the file header. */
  let stageActive = false

  function arm(): void {
    if (offChanged) return
    void refresh()
    offChanged = bus.on(PROCESS_CHANGED, () => void read())
    tailTimer = setInterval(() => void tail(), tailMs)
    refreshTimer = setInterval(() => {
      if (processes.value.some((p) => p.refreshCommand)) void poke()
    }, refreshMs)
  }

  function disarm(): void {
    offChanged?.()
    offChanged = null
    if (tailTimer) clearInterval(tailTimer)
    if (refreshTimer) clearInterval(refreshTimer)
    tailTimer = null
    refreshTimer = null
  }

  /** Armed only with both permissions: an active stage *and* the page's gate. */
  function sync(): void {
    if (stageActive && enabled()) arm()
    else disarm()
  }

  // The arm lives in BOTH hooks: a page resolves async and can mount after the
  // stage's activation flush, so `onActivated` alone would miss the first visit
  // — and that same late mount can land in a stage already switched away from,
  // which is what `inActiveKeepAliveTree()` asks about. `arm` is idempotent, so
  // a mount into an active stage (both hooks fire) arms once.
  onMounted(() => {
    stageActive = inActiveKeepAliveTree()
    sync()
  })
  onActivated(() => {
    stageActive = true
    sync()
  })
  onDeactivated(() => {
    stageActive = false
    sync()
  })
  onUnmounted(disarm)
  watch(enabled, sync)

  return { processes, bridge, running, refresh }
}
