<script setup lang="ts">
/**
 * Crews on the canvas (round 2, card 09). When the thread of a chat window
 * leads a QuantPilot crew (card 07), each of its workers gets a chat window
 * of its own, laid out in a cluster to the right of the orchestrator and
 * connected to it (the edges are drawn by Edges.vue). Windows the user moved
 * stay where they are; only new workers are placed. A collapsed cluster is
 * one node: the crew's compact live graph (card 08), done of all, Expand.
 */
import { onBeforeUnmount, onMounted, watch } from 'vue'
import { v4 as uuidv4 } from 'uuid'
import { useCanvasStore } from '../../../stores/canvas'
import { useWorkspacesStore } from '../../../stores/workspaces'
import { useCrewStore } from '#pilot/stores/crew'
import { usePilotStore } from '#pilot/stores/pilot'
import { crewGraph } from '#pilot/utils/crewGraph'
import { countLabel } from '#pilot/utils/crew'
import { clusterSlots, rectOf } from '../../utils/edges'
import { useCanvasEdges } from '../../utils/canvasLinks'
import type { CrewSnapshot, SessionMeta } from '#pilot/types'

const canvas = useCanvasStore()
const workspaces = useWorkspacesStore()
const crews = useCrewStore()
const pilotStore = usePilotStore()
const { clusters, windows } = useCanvasEdges()

// The crew store is QuantPilot's; the canvas may be open without QuantPilot's page.
// New threads (a crew's workers, a thread an agent started) reach the canvas's chat windows too.
const unlisten: (() => void)[] = []
onMounted(async () => {
  if (!crews.loaded) void crews.load()
  if (!pilotStore.loaded) void pilotStore.load()
  try {
    const { listen } = await import('@tauri-apps/api/event')
    unlisten.push(await listen<CrewSnapshot | { deleted: string }>('pilot-crew', (e) => crews.apply(e.payload)))
    unlisten.push(await listen<SessionMeta>('pilot-thread-created', (e) => pilotStore.upsertSession(e.payload)))
  } catch {
    /* outside the app */
  }
})
onBeforeUnmount(() => unlisten.forEach((u) => u()))

// New workers get a window in their cluster.
watch(
  () => clusters.value.map((c) => `${c.window.id}:${c.snap.tasks.map((t) => t.workerId ?? '').join(',')}`).join('|'),
  () => {
    const ws = workspaces.contentWorkspaceId
    if (!ws) return
    for (const c of clusters.value) {
      const t = c.window.chatConfig?.threadId
      if (!t) continue
      const have = new Set(windows.value.map((w) => w.chatConfig?.threadId).filter(Boolean))
      const missing = c.snap.tasks.filter((x) => x.workerId && !have.has(x.workerId))
      if (!missing.length) continue
      const slots = clusterSlots(rectOf(c.window), c.members.length + missing.length)
      missing.forEach((task, i) => {
        const title = pilotStore.sessions.find((s) => s.id === task.workerId)?.title || task.key
        canvas.addWindow(ws, {
          id: uuidv4(),
          type: 'chat',
          title: `Chat · ${title}`,
          position: slots[c.members.length + i]!,
          status: 'idle',
          minimized: false,
          zIndex: 0,
          chatConfig: { threadId: task.workerId },
          crewOf: t,
        })
      })
    }
  },
  { immediate: true }
)

function expand(windowId: string) {
  const ws = workspaces.contentWorkspaceId
  if (ws) canvas.updateWindow(ws, windowId, { crewCollapsed: false })
}
const providerOf = (id: string) => pilotStore.sessions.find((s) => s.id === id)?.provider
</script>

<template>
  <template v-for="c in clusters" :key="c.window.id">
    <div
      v-if="c.collapsed && c.members.length"
      class="cv-cluster"
      :data-window-id="`cluster:${c.window.id}`"
      :style="{ left: `${c.node.x}px`, top: `${c.node.y}px`, width: `${c.node.width}px`, height: `${c.node.height}px` }"
      @mousedown.stop
    >
      <div class="cv-cluster-head">
        <span class="cv-cluster-title" :title="c.snap.crew.objective">Crew · {{ c.snap.crew.objective || 'workers' }}</span>
        <span class="cv-cluster-count">{{ countLabel(c.snap.tasks) }} done</span>
      </div>
      <PilotCrewGraph :graph="crewGraph(c.snap, providerOf)" :snap="c.snap" compact @open="expand(c.window.id)" />
      <button class="cv-cluster-open" :title="`Show the ${c.members.length} worker windows`" @click.stop="expand(c.window.id)">Expand {{ c.members.length }} workers</button>
    </div>
  </template>
</template>

<style scoped>
.cv-cluster {
  position: absolute;
  z-index: 1;
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 10px 12px;
  border: 1px solid color-mix(in srgb, #7aa2f7 55%, var(--qc-border, #3f3f46));
  border-radius: 12px;
  background: var(--qc-bg-window, #1b1b21);
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.35);
  color: var(--qc-text, #d4d4d8);
}
.cv-cluster-head { display: flex; align-items: baseline; gap: 8px; min-width: 0; }
.cv-cluster-title { flex: 1; min-width: 0; font-size: 12px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.cv-cluster-count { flex: none; font-size: 11px; color: var(--qc-text-muted, #8a8a95); }
.cv-cluster-open { align-self: center; padding: 3px 10px; border: 1px solid var(--qc-border, #3f3f46); border-radius: 7px; background: transparent; color: var(--qc-text, #d4d4d8); font-size: 11.5px; cursor: pointer; }
.cv-cluster-open:hover { background: var(--qss-bg-hover, #313139); }
</style>
