/**
 * Modules asking for the user — "needs you" (QuantPilot card 6): an agent
 * waits on an approval, a question or a plan. A module sets its count; the
 * app rail shows it on the module's app and in the flyout, so the request
 * is visible from anywhere in the suite.
 */
import { reactive, readonly } from 'vue'

const counts = reactive<Record<string, number>>({})

/** How many things in `moduleId` wait on the user (0 clears it). */
export function setModuleAttention(moduleId: string, count: number): void {
  if (count > 0) counts[moduleId] = count
  else delete counts[moduleId]
}

/** Every module's count, read-only and reactive. */
export const moduleAttention = readonly(counts)
