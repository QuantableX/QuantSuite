<template>
  <div class="chart-analyzer">
    <!-- Capture Controls (top, like position sizer) -->
    <div class="hud-top-group">
    <div class="ca-capture-row">
      <button
        class="btn btn-icon"
        @click="$emit('open-history')"
        title="Analysis History"
      >
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
          <circle cx="12" cy="12" r="10" />
          <polyline points="12 6 12 12 16 14" />
        </svg>
      </button>
      <button
        class="btn btn-primary ca-analyze-btn"
        :disabled="isAnalyzing || !config.aiModel"
        @click="handleAnalyze"
      >
        <svg
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          style="margin-right: 6px"
        >
          <path
            d="M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z"
          />
          <circle cx="12" cy="13" r="4" />
        </svg>
        {{ isAnalyzing ? "Analyzing..." : "Capture & Analyze" }}
      </button>
      <button
        class="btn btn-icon region-btn"
        :class="{ active: chartRegion }"
        @click="toggleRegion"
        title="Select chart region"
      >
        <svg
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <rect x="3" y="3" width="7" height="7" />
          <rect x="14" y="3" width="7" height="7" />
          <rect x="3" y="14" width="7" height="7" />
          <rect x="14" y="14" width="7" height="7" />
        </svg>
      </button>
    </div>

    <!-- Status -->
    <div class="ca-status" :class="{ 'ca-status-error': isErrorStatus }">
      {{ status }}
    </div>

    <!-- Captured Image Preview (clickable for fullscreen) -->
    <div
      v-if="capturedImage"
      class="ca-preview"
      @click="openPreview"
      title="Click to preview fullscreen"
    >
      <img
        :src="`data:image/png;base64,${capturedImage}`"
        class="ca-preview-img"
      />
    </div>

    <!-- Analysis Types -->
    <div class="card ca-section">
      <label class="ca-section-label">Analysis</label>
      <label class="ca-checkbox">
        <input
          type="checkbox"
          :checked="analysisTypes.includes('wyckoff')"
          @change="toggleAnalysisType('wyckoff')"
        />
        <span>Wyckoff Schematic</span>
      </label>
    </div>

    <!-- Result Card -->
    </div>
    <div v-if="result" class="card ca-result">
      <div class="ca-result-row">
        <span class="ca-result-label">Market Phase</span>
        <span class="ca-result-value ca-phase-badge" :class="phaseClass">
          {{ result.market_phase }}
        </span>
      </div>

      <div class="ca-result-row">
        <span class="ca-result-label">Schematic</span>
        <span class="ca-result-value">{{ result.schematic }}</span>
      </div>

      <div class="ca-result-row">
        <span class="ca-result-label">Phase</span>
        <span class="ca-result-value">{{ result.wyckoff_phase }}</span>
      </div>

      <div class="ca-result-row">
        <span class="ca-result-label">Transition</span>
        <span class="ca-result-value ca-transition">
          {{ result.current_transition }}
        </span>
      </div>

      <div v-if="result.events?.length" class="ca-result-row ca-events-row">
        <span class="ca-result-label">Events</span>
        <div class="ca-events">
          <span
            v-for="event in result.events"
            :key="event"
            class="ca-event-chip"
            :class="{ 'ca-event-current': event === result.current_transition }"
          >
            {{ event }}
          </span>
        </div>
      </div>

      <div class="ca-result-row">
        <span class="ca-result-label">Bias</span>
        <span class="ca-result-value ca-bias" :class="biasClass">
          {{ result.bias }}
        </span>
      </div>
    </div>

    <!-- Clear button -->
    <button
      v-if="result || capturedImage"
      class="btn btn-ghost ca-clear-btn"
      @click="handleClear"
    >
      Clear Results
    </button>

  </div>
</template>

<script setup lang="ts">
import { useChartAnalyzer } from '#hud/composables/useChartAnalyzer'
import { useConfig } from '#hud/composables/useConfig'
const { config, setChartAnalyzerRegion } = useConfig();

const {
  isAnalyzing,
  status,
  result,
  capturedImage,
  captureAndAnalyze,
  clearResults,
} = useChartAnalyzer();

const analysisTypes = ref(["wyckoff"]);
const chartRegion = ref<[number, number, number, number] | null>(null);

onMounted(() => {
  chartRegion.value = config.value.chartAnalyzerRegion || null;
});

const effectivePosition = computed(() => {
  const pos = config.value.windowPosition || "left";
  return pos === "dual" ? "left" : pos;
});

const isErrorStatus = computed(() => {
  const s = status.value;
  return (
    s.startsWith("Cannot connect") ||
    s.startsWith("AI error") ||
    s.startsWith("Request")
  );
});

const phaseClass = computed(() => {
  if (!result.value) return "";
  const p = result.value.market_phase?.toLowerCase();
  if (p === "accumulation" || p === "markup") return "ca-phase-bull";
  if (p === "distribution" || p === "markdown") return "ca-phase-bear";
  return "";
});

const biasClass = computed(() => {
  if (!result.value) return "";
  const b = result.value.bias?.toLowerCase();
  if (b === "bullish") return "ca-bias-bullish";
  if (b === "bearish") return "ca-bias-bearish";
  return "ca-bias-neutral";
});

function toggleAnalysisType(type: string) {
  const idx = analysisTypes.value.indexOf(type);
  if (idx >= 0) {
    analysisTypes.value.splice(idx, 1);
  } else {
    analysisTypes.value.push(type);
  }
}

async function handleAnalyze() {
  if (!config.value.aiModel) {
    status.value = "Set an AI model in Settings first";
    return;
  }
  if (analysisTypes.value.length === 0) {
    status.value = "Select at least one analysis type";
    return;
  }
  await captureAndAnalyze(
    chartRegion.value,
    config.value.aiProvider || "ollama",
    config.value.aiBaseUrl || "http://localhost:11434",
    config.value.aiModel,
    analysisTypes.value,
    {
      position: effectivePosition.value,
      monitorIndex: config.value.monitorIndex || 0,
    },
  );
}

async function openPreview() {
  if (!capturedImage.value) return;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const path = await invoke<string>("plugin:hud|save_temp_image", {
      imageBase64: capturedImage.value,
    });
    await invoke("plugin:hud|open_screenshot_preview", { path });
  } catch (e) {
    console.warn("Failed to open preview:", e);
  }
}

async function toggleRegion() {
  if (chartRegion.value) {
    chartRegion.value = null;
    setChartAnalyzerRegion(null);
    status.value = "Region cleared";
  } else {
    await selectRegion();
  }
}

async function selectRegion() {
  const isTauri =
    typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  if (!isTauri) return;

  const { invoke } = await import("@tauri-apps/api/core");
  await invoke("plugin:hud|open_region_selector");

  const checkResult = async () => {
    const region = await invoke<[number, number, number, number] | null>("plugin:hud|get_selected_region",
    );
    if (region) {
      chartRegion.value = region;
      setChartAnalyzerRegion(region);
      status.value = `Region: ${region[2]}x${region[3]}`;
    } else {
      const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
      const selectorWindow =
        await WebviewWindow.getByLabel("region-selector");
      if (selectorWindow) {
        setTimeout(checkResult, 100);
      } else {
        status.value = "Region selection cancelled";
      }
    }
  };

  setTimeout(checkResult, 200);
}

function handleClear() {
  clearResults();
}

defineEmits<{
  'open-history': [];
}>();
</script>

<style scoped>
.chart-analyzer {
  padding: 8px 0;
}

.ca-section {
  margin-bottom: 8px;
}

.ca-section-label {
  display: block;
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  margin-bottom: 6px;
}

/* Checkboxes */
.ca-checkbox {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--text-primary);
  cursor: pointer;
  padding: 4px 0;
}

.ca-checkbox input[type="checkbox"] {
  accent-color: var(--accent-blue);
  width: 16px;
  height: 16px;
  cursor: pointer;
}

/* Capture row */
.ca-capture-row {
  display: flex;
  justify-content: center;
  gap: 8px;
  margin: 6px 0;
}

.ca-analyze-btn {
  min-width: 160px;
}

.region-btn.active {
  background: var(--accent-green-dim);
  color: white;
}

/* Status */
.ca-status {
  text-align: center;
  font-size: 13px;
  color: var(--text-secondary);
  margin: 3px 0;
}

.ca-status-error {
  color: #ff4757;
}

/* Preview (clickable) */
.ca-preview {
  margin: 6px 0 8px;
  border-radius: 8px;
  overflow: hidden;
  border: 1px solid var(--border-color);
  cursor: pointer;
  transition: border-color 0.15s ease;
}

.ca-preview:hover {
  border-color: var(--accent-blue);
}

.ca-preview-img {
  width: 100%;
  height: auto;
  display: block;
  max-height: 160px;
  object-fit: cover;
}

/* Result card */
.ca-result {
  margin-bottom: 8px;
}

.ca-result-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 0;
  border-bottom: 1px solid var(--border-color);
}

.ca-result-row:last-child {
  border-bottom: none;
  padding-bottom: 0;
}

.ca-result-row:first-child {
  padding-top: 0;
}

.ca-events-row {
  flex-direction: column;
  align-items: flex-start;
  gap: 6px;
}

.ca-result-label {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.3px;
  flex-shrink: 0;
}

.ca-result-value {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  text-align: right;
}

/* Phase badge */
.ca-phase-badge {
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 12px;
}

.ca-phase-bull {
  background: var(--accent-green-dim);
  color: white;
}

.ca-phase-bear {
  background: var(--accent-red-dim);
  color: white;
}

/* Transition */
.ca-transition {
  color: var(--accent-blue);
}

/* Events */
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

/* Bias */
.ca-bias {
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 12px;
}

.ca-bias-bullish {
  background: var(--accent-green-dim);
  color: white;
}

.ca-bias-bearish {
  background: var(--accent-red-dim);
  color: white;
}

.ca-bias-neutral {
  background: var(--btn-primary);
  color: white;
}

/* Clear */
.ca-clear-btn {
  width: 100%;
  margin-top: 4px;
  font-size: 12px;
}

</style>
