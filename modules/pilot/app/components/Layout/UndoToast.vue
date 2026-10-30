<script setup lang="ts">
/**
 * The 5 s "Settled · Undo" toast after an unpin, settle, snooze or delete
 * (T3 Code). Ctrl+Z outside editable fields does the same; the newest
 * action is the one undone.
 */
import { computed } from 'vue'
import { useSidebarStore } from '#pilot/stores/sidebar'
import { useKeysStore } from '#pilot/stores/keys'

const sb = useSidebarStore()
const keys = useKeysStore()
const entry = computed(() => sb.latestUndo)
</script>

<template>
  <Transition name="qp-toast">
    <div v-if="entry" :key="entry.id" class="qp-toast" role="status">
      <span class="qp-toast-text">{{ entry.label }}</span>
      <button class="qp-toast-undo" :title="keys.hint('thread.undo', 'Undo')" @click="sb.undoLatest()">Undo</button>
      <button class="qp-toast-x" aria-label="Dismiss" @click="sb.dismissUndo(entry.id)">×</button>
    </div>
  </Transition>
</template>

<style scoped>
.qp-toast {
  position: absolute;
  left: 50%;
  bottom: 16px;
  z-index: 40;
  display: flex;
  align-items: center;
  gap: 10px;
  max-width: min(420px, calc(100% - 32px));
  padding: 7px 8px 7px 12px;
  transform: translateX(-50%);
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 8px;
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 10px 28px rgba(0, 0, 0, 0.45);
  color: var(--qss-text, #d4d4d8);
  font-size: 12.5px;
}
.qp-toast-text { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-toast-undo {
  flex-shrink: 0;
  padding: 3px 10px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-accent, #7c8cff);
  font: inherit;
  font-weight: 600;
  cursor: pointer;
}
.qp-toast-undo:hover { background: var(--qss-bg-hover, #313139); }
.qp-toast-x { flex-shrink: 0; border: none; background: transparent; color: var(--qss-text-muted, #6e6e7a); font-size: 15px; line-height: 1; cursor: pointer; padding: 0 4px; }
.qp-toast-x:hover { color: var(--qss-text, #d4d4d8); }
.qp-toast-enter-active,
.qp-toast-leave-active { transition: opacity 140ms ease, transform 140ms ease; }
.qp-toast-enter-from,
.qp-toast-leave-to { opacity: 0; transform: translate(-50%, 6px); }
@media (prefers-reduced-motion: reduce) {
  .qp-toast-enter-active,
  .qp-toast-leave-active { transition: none; }
}
</style>
