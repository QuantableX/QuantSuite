<template>
  <div class="cb-module">
    <div class="hud-top-group">
    <div class="cb-header">
      <span class="cb-title">
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          style="vertical-align: middle; margin-right: 4px"
        >
          <path
            d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"
          />
          <rect x="8" y="2" width="8" height="4" rx="1" ry="1" />
        </svg>
        Clipboard History
      </span>
      <button
        v-if="entries.length"
        class="btn btn-ghost btn-sm"
        @click="clearAll"
      >
        Clear All
      </button>
    </div>
    </div>
    <div v-if="!entries.length" class="cb-empty">
      <p>No clipboard entries yet.</p>
      <p class="cb-hint">Copied text will appear here automatically.</p>
    </div>
    <div v-else class="cb-list">
      <div v-for="entry in entries" :key="entry.id" class="cb-row">
        <div class="cb-content">
          <span class="cb-text">{{
            entry.text.length > 120
              ? entry.text.slice(0, 120) + "…"
              : entry.text
          }}</span>
          <span class="cb-time">{{ formatTime(entry.timestamp) }}</span>
        </div>
        <div class="cb-actions">
          <button
            class="ctrl-btn"
            @click="writeClipboard(entry.text)"
            title="Copy"
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
          <button
            class="ctrl-btn ctrl-del"
            @click="removeEntry(entry.id)"
            title="Delete"
          >
            ✕
          </button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useClipboardHistory } from '#hud/composables/useClipboardHistory'
const { entries, writeClipboard, removeEntry, clearAll } =
  useClipboardHistory();
function formatTime(ts: number) {
  const d = new Date(ts);
  return (
    d.toLocaleTimeString("en-US", {
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    }) +
    " " +
    d.toLocaleDateString("en-US", { month: "short", day: "numeric" })
  );
}
</script>

<style scoped>
.cb-module {
  display: flex;
  flex-direction: column;
  padding: 4px 0;
  min-height: 0;
  flex: 1;
}
.cb-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
  flex-shrink: 0;
}
.cb-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}
.cb-empty {
  text-align: center;
  padding: 24px 0;
  color: var(--text-secondary);
  font-size: 13px;
}
.cb-hint {
  font-size: 11px;
  margin-top: 4px;
}
.cb-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex: 1;
  overflow-y: auto;
  min-height: 0;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 4px;
  background: var(--bg-secondary);
}
.cb-row {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  padding: 6px 8px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 6px;
}
.cb-content {
  flex: 1;
  min-width: 0;
}
.cb-text {
  font-size: 12px;
  color: var(--text-primary);
  word-break: break-word;
  display: block;
}
.cb-time {
  font-size: 10px;
  color: var(--text-secondary);
  margin-top: 2px;
  display: block;
}
.cb-actions {
  display: flex;
  gap: 2px;
  flex-shrink: 0;
}
.ctrl-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 12px;
  width: 22px;
  height: 22px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 3px;
}
.ctrl-btn:hover {
  color: var(--text-primary);
}
.ctrl-del:hover {
  color: var(--accent-red);
}
.btn-sm {
  font-size: 12px;
  padding: 4px 10px;
}
</style>
