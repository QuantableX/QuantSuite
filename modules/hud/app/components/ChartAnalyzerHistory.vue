<template>
  <div class="ca-history">
    <!-- Header with back button -->
    <div class="hud-top-group">
    <div class="ca-history-header">
      <button class="btn btn-icon" @click="$emit('back')" title="Back">
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
          <polyline points="15 18 9 12 15 6" />
        </svg>
      </button>
      <span class="ca-history-title">Analysis History</span>
      <button
        v-if="history.length"
        class="btn btn-icon ca-history-clear-all"
        @click="handleClearAll"
        title="Clear all history"
      >
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <polyline points="3 6 5 6 21 6" />
          <path d="M19 6l-2 14H7L5 6" />
          <path d="M10 11v6" />
          <path d="M14 11v6" />
        </svg>
      </button>
    </div>

    <!-- Empty state -->
    </div>
    <div v-if="!history.length" class="ca-history-empty">
      No analyses yet. Use Capture &amp; Analyze to get started.
    </div>

    <!-- History list (containerized like screenshots) -->
    <div v-else class="ca-history-list">
      <div
        v-for="entry in history"
        :key="entry.id"
        class="ca-history-row"
      >
        <!-- Row header: thumbnail + info + actions -->
        <div
          class="ca-history-row-header"
          @click="toggleEntry(entry.id)"
        >
          <!-- Thumbnail -->
          <img
            v-if="entry.imageBase64"
            :src="`data:image/png;base64,${entry.imageBase64}`"
            class="ca-thumb"
            @click.stop="openPreview(entry)"
          />
          <div v-else class="ca-thumb-placeholder">
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

          <!-- Info -->
          <div class="ca-row-info">
            <span class="ca-row-time">{{ formatTime(entry.timestamp) }}</span>
            <div class="ca-row-tags">
              <span class="ca-tag" :class="phaseClass(entry.result)">
                {{ entry.result.market_phase }}
              </span>
              <span class="ca-tag" :class="biasClass(entry.result)">
                {{ entry.result.bias }}
              </span>
            </div>
          </div>

          <!-- Actions -->
          <div class="ca-row-actions">
            <button
              class="ca-entry-delete"
              title="Delete"
              @click.stop="deleteHistoryEntry(entry.id)"
            >
              &times;
            </button>
            <svg
              width="12"
              height="12"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
              class="ca-chevron"
              :class="{ 'ca-chevron-open': expandedId === entry.id }"
            >
              <polyline points="6 9 12 15 18 9" />
            </svg>
          </div>
        </div>

        <!-- Expanded detail -->
        <div v-if="expandedId === entry.id" class="ca-history-detail">
          <!-- Full image preview when expanded -->
          <div v-if="entry.imageBase64" class="ca-detail-preview">
            <img
              :src="`data:image/png;base64,${entry.imageBase64}`"
              class="ca-detail-preview-img"
              @click="openPreview(entry)"
            />
          </div>

          <div class="ca-detail-row">
            <span class="ca-detail-label">Market Phase</span>
            <span class="ca-detail-value ca-phase-badge" :class="phaseClass(entry.result)">
              {{ entry.result.market_phase }}
            </span>
          </div>

          <div class="ca-detail-row">
            <span class="ca-detail-label">Schematic</span>
            <span class="ca-detail-value">{{ entry.result.schematic }}</span>
          </div>

          <div class="ca-detail-row">
            <span class="ca-detail-label">Phase</span>
            <span class="ca-detail-value">{{ entry.result.wyckoff_phase }}</span>
          </div>

          <div class="ca-detail-row">
            <span class="ca-detail-label">Transition</span>
            <span class="ca-detail-value ca-transition">
              {{ entry.result.current_transition }}
            </span>
          </div>

          <div v-if="entry.result.events?.length" class="ca-detail-row ca-events-row">
            <span class="ca-detail-label">Events</span>
            <div class="ca-events">
              <span
                v-for="event in entry.result.events"
                :key="event"
                class="ca-event-chip"
                :class="{ 'ca-event-current': event === entry.result.current_transition }"
              >
                {{ event }}
              </span>
            </div>
          </div>

          <div class="ca-detail-row">
            <span class="ca-detail-label">Bias</span>
            <span class="ca-detail-value ca-bias-badge" :class="biasClass(entry.result)">
              {{ entry.result.bias }}
            </span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useChartAnalyzer } from '#hud/composables/useChartAnalyzer'
import type { AnalysisHistoryEntry, ChartAnalysisResult } from '#hud/composables/useChartAnalyzer';

defineEmits<{
  back: [];
}>();

const { history, getCapture, deleteHistoryEntry, clearHistory } = useChartAnalyzer();

const expandedId = ref<string | null>(null);

function toggleEntry(id: string) {
  expandedId.value = expandedId.value === id ? null : id;
}

function handleClearAll() {
  clearHistory();
  expandedId.value = null;
}

async function openPreview(entry: AnalysisHistoryEntry) {
  try {
    // The entry only carries a thumbnail — the full capture comes from the
    // capture store, older entries only have the thumbnail to show.
    const imageBase64 = (await getCapture(entry.id)) || entry.imageBase64;
    if (!imageBase64) return;
    const { invoke } = await import("@tauri-apps/api/core");
    const path = await invoke<string>("plugin:hud|save_temp_image", { imageBase64 });
    await invoke("plugin:hud|open_screenshot_preview", { path });
  } catch (e) {
    console.warn("Failed to open preview:", e);
  }
}

function formatTime(ts: number) {
  const d = new Date(ts);
  const now = new Date();
  const isToday = d.toDateString() === now.toDateString();
  const time = d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  if (isToday) return time;
  return `${d.toLocaleDateString([], { month: "short", day: "numeric" })} ${time}`;
}

function phaseClass(r: ChartAnalysisResult) {
  const p = r.market_phase?.toLowerCase();
  if (p === "accumulation" || p === "markup") return "ca-tag-bull";
  if (p === "distribution" || p === "markdown") return "ca-tag-bear";
  return "ca-tag-neutral";
}

function biasClass(r: ChartAnalysisResult) {
  const b = r.bias?.toLowerCase();
  if (b === "bullish") return "ca-tag-bull";
  if (b === "bearish") return "ca-tag-bear";
  return "ca-tag-neutral";
}
</script>

<style scoped>
.ca-history {
  padding: 0;
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}

/* Header */
.ca-history-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border-color);
  margin-bottom: 8px;
}

.ca-history-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  flex: 1;
}

.ca-history-clear-all {
  opacity: 0.5;
}

.ca-history-clear-all:hover {
  opacity: 1;
  color: #ff4757;
}

/* Empty state */
.ca-history-empty {
  text-align: center;
  padding: 32px 16px;
  font-size: 13px;
  color: var(--text-secondary);
}

/* Containerized list (matches screenshots style) */
.ca-history-list {
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

/* Row entry (matches ss-row) */
.ca-history-row {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  overflow: hidden;
  flex-shrink: 0;
}

.ca-history-row-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  cursor: pointer;
  transition: background 0.15s;
}

.ca-history-row-header:hover {
  background: var(--bg-secondary);
}

/* Thumbnail (matches ss-thumb) */
.ca-thumb {
  width: 48px;
  height: 32px;
  object-fit: cover;
  border-radius: 3px;
  flex-shrink: 0;
  background: var(--bg-secondary);
  cursor: pointer;
}

.ca-thumb-placeholder {
  width: 48px;
  height: 32px;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 14px;
  background: var(--bg-secondary);
  border-radius: 3px;
  flex-shrink: 0;
  color: var(--text-secondary);
}

/* Row info */
.ca-row-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}

.ca-row-time {
  font-size: 11px;
  color: var(--text-secondary);
  font-family: monospace;
}

.ca-row-tags {
  display: flex;
  gap: 4px;
}

.ca-tag {
  font-size: 10px;
  font-weight: 600;
  padding: 1px 6px;
  border-radius: 3px;
}

.ca-tag-bull {
  background: var(--accent-green-dim);
  color: white;
}

.ca-tag-bear {
  background: var(--accent-red-dim);
  color: white;
}

.ca-tag-neutral {
  background: var(--btn-primary);
  color: white;
}

/* Row actions */
.ca-row-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}

.ca-entry-delete {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 16px;
  line-height: 1;
  padding: 0 2px;
  opacity: 0.4;
  transition: opacity 0.12s;
}

.ca-entry-delete:hover {
  opacity: 1;
  color: #ff4757;
}

.ca-chevron {
  color: var(--text-secondary);
  transition: transform 0.15s ease;
}

.ca-chevron-open {
  transform: rotate(180deg);
}

/* Expanded detail */
.ca-history-detail {
  padding: 8px 10px;
  border-top: 1px solid var(--border-color);
  background: var(--card-bg, var(--input-bg));
}

/* Detail image preview */
.ca-detail-preview {
  margin-bottom: 8px;
  border-radius: 4px;
  overflow: hidden;
  border: 1px solid var(--border-color);
  cursor: pointer;
}

.ca-detail-preview-img {
  width: 100%;
  height: auto;
  display: block;
  max-height: 140px;
  object-fit: cover;
}

.ca-detail-preview:hover {
  border-color: var(--accent-blue);
}

.ca-detail-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 5px 0;
  border-bottom: 1px solid var(--border-color);
}

.ca-detail-row:last-child {
  border-bottom: none;
  padding-bottom: 0;
}

.ca-events-row {
  flex-direction: column;
  align-items: flex-start;
  gap: 4px;
}

.ca-detail-label {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.3px;
  flex-shrink: 0;
}

.ca-detail-value {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  text-align: right;
}

.ca-phase-badge,
.ca-bias-badge {
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 12px;
}

.ca-transition {
  color: var(--accent-blue);
}

.ca-events {
  display: flex;
  flex-wrap: wrap;
  gap: 4px;
}

.ca-event-chip {
  display: inline-block;
  font-size: 11px;
  font-weight: 600;
  padding: 2px 6px;
  background: var(--input-bg);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  color: var(--text-primary);
  font-family: monospace;
}

.ca-event-current {
  background: var(--accent-blue);
  border-color: var(--accent-blue);
  color: white;
}
</style>
