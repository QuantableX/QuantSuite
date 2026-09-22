/**
 * The shell half of the agent-call broker (PLAN-V2 E4).
 *
 * qs-core's broker gates every `quantsuite.*` call an external agent makes
 * and emits `agent.call.requested`. This composable is the other end:
 *
 *   - `allow`  → dispatch immediately: `invoke` the capability's command and
 *     report the outcome back with `agent_call_complete`
 *   - `prompt` → into the approval queue; the user approves (dispatch) or
 *     rejects (the agent is told so, verbatim) in the drawer's Agent tab
 *
 * Denied calls never reach this code — the gate answers the agent directly,
 * so no UI bug can upgrade a deny. Instantiated once in app.vue; the queue
 * is shared state the Agent tab renders.
 */

import { invoke } from '@tauri-apps/api/core'
import { bus, qs, isModuleEnabled, type PendingAgentCall } from '@quantsuite/core'

export function useAgentBroker(primaryWindow = true) {
  const queue = useState<PendingAgentCall[]>('qss-agent-queue', () => [])

  /**
   * Every callId this window has already sent to `invoke`. The recovery
   * snapshot below is taken *after* the bus handler is live, and Rust keeps a
   * call in `pending` until `agent_call_complete` lands — so a call arriving
   * during that round-trip reaches both paths, and without this ledger the
   * agent's command would run twice (two identical kanban cards) and be
   * completed twice.
   */
  const dispatched = new Set<string>()
  // Events can retire a call while the startup snapshot is still in flight.
  const retired = new Set<string>()

  async function dispatch(call: PendingAgentCall) {
    if (dispatched.has(call.callId)) return
    dispatched.add(call.callId)
    let claimed = false
    try {
      claimed = await qs.core.agentCallClaim(call.callId)
      if (!claimed) return
      if (!isModuleEnabled(call.tool.split('.')[1] ?? '')) throw new Error('This app is deactivated in Settings > Apps')
      const result = await invoke(call.command, call.args ?? {})
      await qs.core.agentCallComplete(
        call.callId,
        typeof result === 'string' ? result : JSON.stringify(result ?? null),
        null
      )
    } catch (e) {
      if (!claimed) {
        dispatched.delete(call.callId)
        console.error('[agent] could not claim call', e)
        return
      }
      const message = e instanceof Error ? e.message : String(e)
      await qs.core.agentCallComplete(call.callId, null, `dispatch failed: ${message}`).catch(() => {})
    }
  }

  function drop(callId: string) {
    queue.value = queue.value.filter((c) => c.callId !== callId)
  }

  async function approve(call: PendingAgentCall) {
    await dispatch(call)
  }

  async function reject(call: PendingAgentCall) {
    try {
      if (!await qs.core.agentCallClaim(call.callId)) return
      drop(call.callId)
      await qs.core.agentCallComplete(call.callId, null, 'rejected by the user')
    } catch (e) {
      console.error('[agent] could not reject call', e)
    }
  }

  let offRequested: (() => void) | undefined
  let offCompleted: (() => void) | undefined
  let offClaimed: (() => void) | undefined

  async function start() {
    offRequested = bus.on<PendingAgentCall>('agent.call.requested', (event) => {
      const call = event.payload
      if (call.decision === 'allow') {
        // The resident window survives closing any additional view.
        if (primaryWindow) void dispatch(call)
      } else if (!retired.has(call.callId) && !queue.value.some((c) => c.callId === call.callId)) {
        queue.value.push(call)
      }
    })

    // A decision in any window immediately removes every copy of the prompt.
    const retire = (event: { payload: { callId: string } }) => {
      retired.add(event.payload.callId)
      drop(event.payload.callId)
    }
    offClaimed = bus.on<{ callId: string }>('agent.call.claimed', retire)
    offCompleted = bus.on<{ callId: string }>('agent.call.completed', retire)

    // Recover after a reload: prompted calls are still waiting in Rust.
    try {
      const pending = await qs.core.agentPendingCalls()
      // Merge, never assign: a prompted call the bus delivered while the
      // snapshot was in flight is already queued, and replacing the array
      // would hide it until a reload while its agent waits.
      const known = new Set(queue.value.map((c) => c.callId))
      queue.value = [
        ...queue.value,
        ...pending.filter((c) => c.decision === 'prompt' && !known.has(c.callId) && !retired.has(c.callId)),
      ]
      // An `allow` call caught mid-reload would otherwise wait out its leash.
      // The same window applies here as to the merge above — `dispatch` drops
      // the ones the bus already handled.
      for (const call of pending.filter((c) => primaryWindow && c.decision === 'allow' && !retired.has(c.callId))) {
        void dispatch(call)
      }
    } catch {
      // Browser development — no broker to recover from.
    }
  }

  function stop() {
    offRequested?.()
    offCompleted?.()
    offClaimed?.()
  }

  return { queue, approve, reject, start, stop }
}
