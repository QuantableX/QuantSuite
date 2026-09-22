<script setup lang="ts">
/**
 * THE QuantSpace footer — one component, mounted identically by QuantCode,
 * QuantCanvas and QuantConsole (user, 2026-08-27: same looks, same
 * functionality, everywhere). QDock supplies the chrome (grip, strip, close),
 * CodeDockPanel the content: Problems / Terminal / Search.
 *
 * The host owns only `panel` — which pane is in front, null for closed.
 * QuantCode drives it from its editor store (Ctrl+J, Ctrl+Shift+F/M);
 * the other modules hold a local ref. Marker counts come from the code
 * editor store directly: the problems ARE that store's diagnostics, no
 * matter which module's footer is showing them.
 */
import { useEditorStore } from '#code-root/stores/editor'
import type { PanelTab } from '#code-root/shared/types'

const props = defineProps<{
  panel: PanelTab | null
  /** Workspace root — search scope and the terminal's cwd. */
  root: string | null
  /** The hosting module is on screen — gates the terminal's draw. */
  visible: boolean
}>()

const emit = defineEmits<{
  (e: 'update:panel', panel: PanelTab | null): void
  (e: 'reveal', target: { path: string; line: number }): void
  /** The current height in px — for hosts that position things above the bar. */
  (e: 'height', height: number): void
}>()

const store = useEditorStore()

// Create the dock on first use, then retain its panes and shell while closed.
const lastPanel = ref<PanelTab | null>(null)
watch(() => props.panel, (panel) => {
  if (panel) lastPanel.value = panel
}, { immediate: true })

const height = ref(240)
watch(height, (h) => emit('height', h), { immediate: true })

const tabs = computed(() => [
  {
    id: 'problems',
    label: 'Problems',
    badge: store.errorCount || store.warningCount || undefined,
    badgeKind: (store.errorCount ? 'error' : 'normal') as 'error' | 'normal',
  },
  { id: 'terminal', label: 'Terminal' },
  { id: 'search', label: 'Search' },
])

/** Clicking the lit tab closes the dock — QuantCode's toggle, everyone's now. */
function select(id: string) {
  emit('update:panel', id === props.panel ? null : (id as PanelTab))
}
</script>

<template>
  <QDock
    v-if="lastPanel"
    v-show="panel !== null"
    :tabs="tabs"
    :active="panel"
    :height="height"
    @update:height="height = $event"
    @select="select"
    @close="emit('update:panel', null)"
  >
    <CodeDockPanel
      :root="root"
      :visible="visible && panel !== null"
      :panel="lastPanel"
      @reveal="emit('reveal', $event)"
    />
  </QDock>
</template>
