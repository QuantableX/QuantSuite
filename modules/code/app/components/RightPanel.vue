<script setup lang="ts">
/**
 * The right panel's toolkit — one tool at a time.
 *
 * A three-column switcher on top (icons + labels, equal thirds of the 220px
 * panel), the chosen tool filling the space below. This replaced the first
 * accordion cut on 2026-08-27: with three tools the stacked sections spent
 * their height on headers, where one tool at full height is what a 220px
 * column is actually good for. The choice is persisted; all three tools stay
 * mounted (`v-show`) so switching away and back loses no state.
 */
import { GitBranch, Clock, Bookmark } from 'lucide-vue-next'
import { qs } from '@quantsuite/core'
import { useEditorStore } from '#code-root/stores/editor'

const props = defineProps<{ root: string | null }>()
const emit = defineEmits<{
  (e: 'open', path: string): void
  (e: 'reveal', target: { path: string; line: number }): void
}>()

const store = useEditorStore()

const sourceControlRef = ref<{ refresh: () => void } | null>(null)

// A save changes the working tree; the git tool is the only thing that has
// to be told, since nothing polls it. (The Timeline watches this key itself.)
watch(() => store.explorerRefreshKey, () => sourceControlRef.value?.refresh())

// ---- Tool selection, persisted ----

type ToolId = 'source' | 'timeline' | 'bookmarks'
const active = ref<ToolId>('source')

const SETTINGS_SCOPE = 'code'
const SETTINGS_KEY = 'toolkit.activeTool'

onMounted(async () => {
  try {
    const saved = await qs.core.getSetting<ToolId>(SETTINGS_SCOPE, SETTINGS_KEY)
    if (saved === 'source' || saved === 'timeline' || saved === 'bookmarks') active.value = saved
  } catch {
    // No backend — the default stands.
  }
})

function select(id: ToolId) {
  active.value = id
  void qs.core.setSetting(SETTINGS_SCOPE, SETTINGS_KEY, id).catch(() => {})
}

</script>

<template>
  <div class="tk">
    <nav class="tk-tabs" aria-label="Editor tools">
      <button
        class="tk-tab"
        :class="{ 'is-active': active === 'source' }"
        :aria-pressed="active === 'source'"
        type="button"
        title="Source control"
        @click="select('source')"
      >
        <GitBranch :size="14" :stroke-width="1.8" aria-hidden="true" />
        <span class="tk-tab-label">Source</span>
      </button>
      <button
        class="tk-tab"
        :class="{ 'is-active': active === 'timeline' }"
        :aria-pressed="active === 'timeline'"
        type="button"
        title="Timeline — local save history"
        @click="select('timeline')"
      >
        <Clock :size="14" :stroke-width="1.8" aria-hidden="true" />
        <span class="tk-tab-label">Timeline</span>
      </button>
      <button
        class="tk-tab"
        :class="{ 'is-active': active === 'bookmarks' }"
        :aria-pressed="active === 'bookmarks'"
        type="button"
        title="Bookmarks (Ctrl+Alt+K)"
        @click="select('bookmarks')"
      >
        <Bookmark :size="14" :stroke-width="1.8" aria-hidden="true" />
        <span class="tk-tab-label">Marks</span>
      </button>
    </nav>

    <div class="tk-body">
      <div v-show="active === 'source'" class="tk-tool">
        <CodeSourceControl
          ref="sourceControlRef"
          :root="root"
          @open="(path: string) => emit('open', path)"
        />
      </div>
      <div v-show="active === 'timeline'" class="tk-tool">
        <CodeTimeline />
      </div>
      <div v-show="active === 'bookmarks'" class="tk-tool">
        <CodeBookmarks
          :root="root"
          @reveal="(target: { path: string; line: number }) => emit('reveal', target)"
        />
      </div>
    </div>
  </div>
</template>

<style scoped>
.tk {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
}

.tk-tabs {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 3px;
  flex-shrink: 0;
  margin: 6px 6px 0;
  padding: 3px;
  border: 1px solid color-mix(in srgb, var(--qss-border) 80%, transparent);
  border-radius: 10px;
  background: color-mix(in srgb, var(--qss-bg) 85%, var(--qss-bg-chrome));
  box-shadow: inset 0 1px 3px rgb(0 0 0 / 12%);
}
.tk-tab {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 4px;
  min-width: 0;
  height: 44px;
  padding: 0 2px;
  border: 1px solid transparent;
  border-radius: 7px;
  background: transparent;
  color: var(--qss-text-secondary);
  font: inherit;
  font-size: 11px;
  font-weight: 600;
  line-height: 1;
  cursor: pointer;
  transition: background 160ms ease, color 160ms ease, box-shadow 160ms ease;
}
.tk-tab:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.tk-tab[aria-pressed='true'] {
  border-color: color-mix(in srgb, var(--qss-accent) 70%, var(--qss-text));
  background: linear-gradient(180deg,
    color-mix(in srgb, var(--qss-accent) 85%, var(--qss-text)),
    var(--qss-accent));
  color: var(--qss-accent-ink);
  box-shadow: 0 1px 3px rgb(0 0 0 / 22%), inset 0 1px 0 rgb(255 255 255 / 18%);
}
.tk-tab:focus-visible {
  outline: 2px solid var(--qss-accent);
  outline-offset: 3px;
}
@media (prefers-reduced-motion: reduce) {
  .tk-tab { transition: none; }
}

.tk-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
}

.tk-tool {
  min-height: 0;
}
</style>
