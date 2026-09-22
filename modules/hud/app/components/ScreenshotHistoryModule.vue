<template>
  <div class="ss-module">
    <div class="hud-top-group">
    <div class="ss-header">
      <span class="ss-title"
        ><svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          style="vertical-align: middle; margin-right: 3px"
        >
          <path
            d="M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z"
          />
          <circle cx="12" cy="13" r="4" />
        </svg>
        Screenshots</span
      >
      <button class="btn btn-primary btn-sm" @click="loadEntries">
        <svg
          width="12"
          height="12"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          style="vertical-align: middle; margin-right: 2px"
        >
          <polyline points="23 4 23 10 17 10" />
          <polyline points="1 20 1 14 7 14" />
          <path
            d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"
          />
        </svg>
        Refresh
      </button>
    </div>
    <button class="ss-open-folder" @click="openFolder">
      <svg
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
        style="vertical-align: middle; margin-right: 3px"
      >
        <path
          d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"
        />
      </svg>
      Open Screenshots Folder
    </button>
    </div>
    <div v-if="loading" class="ss-empty">
      <p>Loading screenshots...</p>
    </div>
    <div v-else-if="!entries.length" class="ss-empty">
      <p>No screenshots found.</p>
      <p class="ss-hint">Take a screenshot with Win+PrintScreen and refresh.</p>
    </div>
    <div v-else class="ss-list">
      <div v-for="entry in entries" :key="entry.path" class="ss-row">
        <img
          v-if="entry.image_base64"
          :src="'data:image/png;base64,' + entry.image_base64"
          class="ss-thumb"
          @click="openPreview(entry)"
        />
        <div v-else class="ss-thumb-placeholder">
          <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
            <circle cx="8.5" cy="8.5" r="1.5" />
            <polyline points="21 15 16 10 5 21" />
          </svg>
        </div>
        <div class="ss-info">
          <span class="ss-filename">{{ entry.filename }}</span>
          <span class="ss-date">{{ formatDate(entry.modified) }}</span>
        </div>
        <button
          class="ss-copy-btn"
          @click.stop="copyScreenshot(entry)"
          title="Copy image to clipboard"
        >
          <svg
            width="12"
            height="12"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
          >
            <path
              d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"
            />
            <rect x="8" y="2" width="8" height="4" rx="1" ry="1" />
          </svg>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useScreenshotHistory } from '#hud/composables/useScreenshotHistory'
const props = defineProps<{
  screenshotsFolder?: string;
}>();

const folderRef = computed(() => props.screenshotsFolder || "");

const {
  entries,
  loading,
  loadEntries,
  loadThumbnail,
  copyScreenshot,
  openFolder,
} = useScreenshotHistory(folderRef);

// Auto-load thumbnails when entries change
watch(
  entries,
  async (list) => {
    for (const entry of list) {
      if (!entry.image_base64) {
        await loadThumbnail(entry);
      }
    }
  },
  { immediate: true },
);

async function openPreview(entry: { path: string }) {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("plugin:hud|open_screenshot_preview", { path: entry.path });
  } catch (e) {
    console.warn("Failed to open preview:", e);
  }
}

function formatDate(unix: number): string {
  const d = new Date(unix * 1000);
  return d.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}
</script>

<style scoped>
.ss-module {
  display: flex;
  flex-direction: column;
  padding: 4px 0;
  min-height: 0;
  flex: 1;
}
.ss-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 6px;
  flex-shrink: 0;
}
.ss-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}
.ss-open-folder {
  width: 100%;
  padding: 5px 10px;
  margin-bottom: 8px;
  font-size: 11px;
  background: var(--bg-secondary);
  color: var(--text-primary);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.15s;
  flex-shrink: 0;
}
.ss-open-folder:hover {
  background: var(--bg-card);
}
.ss-empty {
  text-align: center;
  padding: 24px 0;
  color: var(--text-secondary);
  font-size: 13px;
}
.ss-hint {
  font-size: 11px;
  margin-top: 4px;
}
.ss-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
  overflow-y: auto;
  min-height: 0;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 4px;
  background: var(--bg-secondary);
}
.ss-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  transition: background 0.15s;
  flex-shrink: 0;
}
.ss-row:hover {
  background: var(--bg-secondary);
}
.ss-thumb {
  width: 48px;
  height: 32px;
  object-fit: cover;
  border-radius: 3px;
  flex-shrink: 0;
  background: var(--bg-secondary);
  cursor: pointer;
}
.ss-thumb-placeholder {
  width: 48px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 14px;
  background: var(--bg-secondary);
  border-radius: 3px;
  flex-shrink: 0;
}
.ss-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.ss-filename {
  font-size: 11px;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.ss-date {
  font-size: 10px;
  color: var(--text-secondary);
}
.ss-copy-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 12px;
  width: 24px;
  height: 24px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  flex-shrink: 0;
}
.ss-copy-btn:hover {
  color: var(--text-primary);
  background: var(--bg-card);
}
.btn-sm {
  font-size: 12px;
  padding: 4px 10px;
}
</style>
