<script setup lang="ts">
/**
 * The toolkit's Timeline section — the active file's local save history.
 *
 * Git answers "what changed since the last commit"; this answers "what did
 * this file look like twenty minutes ago". Every save drops a snapshot
 * (`timeline_snapshot`, deduped and capped in the backend); this panel lists
 * them for whichever file is focused, opens one as a read-only viewer tab,
 * and can put a snapshot's content back into the live buffer — dirty, so the
 * restore is yours to save or undo, never a silent write to disk.
 */
import { qs, type TimelineEntry } from '@quantsuite/core'
import { useEditorStore, languageFor } from '#code-root/stores/editor'

const store = useEditorStore()

const entries = ref<TimelineEntry[]>([])
const loading = ref(false)

/** Snapshots describe files on disk — a read-only viewer has no timeline. */
const activePath = computed(() => {
  const tab = store.activeTab
  return tab && !tab.readOnly && tab.kind === 'text' ? tab.path : null
})

async function refresh() {
  const path = activePath.value
  if (!path) {
    entries.value = []
    return
  }
  loading.value = true
  try {
    entries.value = await qs.files.timelineList(path)
  } catch {
    entries.value = []
  } finally {
    loading.value = false
  }
}

// A new file in focus, or a save just landed (the key bumps on every save).
watch(activePath, refresh, { immediate: true })
watch(() => store.explorerRefreshKey, refresh)
onActivated(refresh)

defineExpose({ refresh })

function timeLabel(millis: number): string {
  const d = new Date(millis)
  const today = new Date()
  const sameDay = d.toDateString() === today.toDateString()
  const clock = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })
  return sameDay ? clock : `${d.toLocaleDateString([], { month: 'short', day: 'numeric' })} ${clock}`
}

function sizeLabel(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  return `${(bytes / 1024).toFixed(1)} KB`
}

/** Open a snapshot beside the file — read-only, transient, clearly labelled. */
async function openSnapshot(entry: TimelineEntry) {
  const path = activePath.value
  const tab = store.activeTab
  if (!path || !tab) return
  try {
    const content = await qs.files.timelineRead(path, entry.id)
    await store.openFile(`${path}@${entry.id}`, {
      content,
      readOnly: true,
      title: `${tab.fileName} @ ${timeLabel(entry.savedAt)}`,
      language: languageFor(path),
    })
  } catch (e) {
    console.error('[code] snapshot open failed', e)
  }
}

/** Put the snapshot's content into the live buffer. Dirty until saved. */
async function restore(entry: TimelineEntry) {
  const path = activePath.value
  const tab = store.activeTab
  if (!path || !tab) return
  try {
    const content = await qs.files.timelineRead(path, entry.id)
    store.updateContent(tab.id, content)
  } catch (e) {
    console.error('[code] snapshot restore failed', e)
  }
}
</script>

<template>
  <div class="tl">
    <p v-if="!activePath" class="tl-empty">No file focused.</p>
    <p v-else-if="!entries.length && !loading" class="tl-empty">
      No snapshots yet — one is kept per save.
    </p>

    <div v-else class="tl-list">
      <div v-for="(entry, i) in entries" :key="entry.id" class="tl-row">
        <button
          class="tl-main"
          :title="`Open this snapshot read-only\n${new Date(entry.savedAt).toLocaleString()}`"
          @click="openSnapshot(entry)"
        >
          <span class="tl-dot" :class="{ 'is-latest': i === 0 }" />
          <span class="tl-time">{{ timeLabel(entry.savedAt) }}</span>
          <span class="tl-size">{{ sizeLabel(entry.bytes) }}</span>
        </button>
        <button
          class="tl-restore"
          title="Restore into the editor (does not save)"
          @click="restore(entry)"
        >
          <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <polyline points="1 4 1 10 7 10" /><path d="M3.51 15a9 9 0 1 0 2.13-9.36L1 10" />
          </svg>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.tl {
  display: flex;
  flex-direction: column;
  min-height: 0;
}

.tl-empty {
  padding: 8px 12px 12px;
  font-size: 11.5px;
  color: var(--qss-text-muted);
}

.tl-list {
  padding: 4px 0 8px;
}

.tl-row {
  display: flex;
  align-items: center;
}
.tl-row:hover {
  background: var(--qss-bg-hover);
}

.tl-main {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 3px 4px 3px 12px;
  border: none;
  background: transparent;
  color: var(--qss-text-secondary);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}
.tl-row:hover .tl-main {
  color: var(--qss-text);
}

.tl-dot {
  flex-shrink: 0;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--qss-text-muted);
  opacity: 0.6;
}
.tl-dot.is-latest {
  background: var(--qss-text);
  opacity: 1;
}

.tl-time {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tl-size {
  flex-shrink: 0;
  font-family: var(--qss-font-mono);
  font-size: 10px;
  color: var(--qss-text-muted);
}

.tl-restore {
  flex-shrink: 0;
  display: grid;
  place-items: center;
  width: 22px;
  height: 22px;
  margin-right: 8px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text-muted);
  cursor: pointer;
  opacity: 0;
}
.tl-row:hover .tl-restore {
  opacity: 1;
}
.tl-restore:hover {
  background: var(--qss-bg-active, var(--qss-bg-hover));
  color: var(--qss-text);
}
</style>
