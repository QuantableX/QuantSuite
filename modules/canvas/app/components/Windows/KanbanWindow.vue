<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { GENERAL_BOARD_ID, type KanbanCard } from '@quantsuite/core'
import type { CanvasWindow } from '../../../shared/types'
import { useWorkspacesStore } from '../../../stores/workspaces'

defineProps<{ window: CanvasWindow }>()

const workspaces = useWorkspacesStore()
const showLegacy = ref(false)

// Canvas can pin a different workspace from the suite-active one. Bind the
// kanban to the folder its explorer actually shows, using the canonical id.
const boardId = computed(() => {
  const id = workspaces.contentWorkspaceId
  return id === workspaces.GENERAL_CONTENT ? GENERAL_BOARD_ID : id
})

watch(boardId, () => { showLegacy.value = false })

/** A card dragged out of the board (round 2, card 09): the canvas places it, connected to its thread. */
function onDragOut(card: KanbanCard, x: number, y: number) {
  if (!boardId.value) return
  window.dispatchEvent(new CustomEvent('qc-card-drop', { detail: { card, boardId: boardId.value, x, y } }))
}
</script>

<template>
  <div class="canvas-kanban">
    <div class="canvas-kanban__toolbar">
      <span>{{ showLegacy ? 'Legacy spec files' : 'Shared workspace kanban · drag a card onto the canvas to connect it' }}</span>
      <button
        v-if="workspaces.contentWorkspace"
        type="button"
        :aria-pressed="showLegacy"
        title="Existing .spec.md files are preserved in the workspace"
        @click="showLegacy = !showLegacy"
      >{{ showLegacy ? 'Back to kanban' : 'Legacy specs' }}</button>
    </div>
    <div class="canvas-kanban__body">
      <CanvasWindowsSpecWindow v-if="showLegacy" :key="boardId ?? 'loading'" :window="window" />
      <QDrawerKanban v-else-if="boardId" :board-id="boardId" drag-out @drag-out="onDragOut" />
      <p v-else class="canvas-kanban__loading">Loading workspace…</p>
    </div>
  </div>
</template>

<style scoped>
.canvas-kanban {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}
.canvas-kanban__toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  flex-shrink: 0;
  padding: 8px 14px 0;
  color: var(--qc-text-dim);
  font-size: 11px;
}
.canvas-kanban__toolbar button {
  flex-shrink: 0;
  border: 1px solid var(--qc-border);
  border-radius: 6px;
  padding: 3px 8px;
  color: var(--qc-text);
  background: var(--qc-bg-surface);
  cursor: pointer;
}
.canvas-kanban__toolbar button:hover {
  background: var(--qc-bg-window);
}
.canvas-kanban__body {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}
.canvas-kanban__loading {
  padding: 8px 14px;
  color: var(--qc-text-dim);
  font-size: 12px;
}
</style>
