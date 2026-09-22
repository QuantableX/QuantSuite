<template>
  <div class="tr-module">
    <div class="hud-top-group">
    <div class="tr-header">
      <span class="tr-title">
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          style="vertical-align: middle; margin-right: 4px"
        >
          <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" />
          <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
          <line x1="12" y1="19" x2="12" y2="22" />
        </svg>
        Transcript
      </span>
      <button v-if="entries.length" class="btn btn-ghost btn-sm" @click="clearAll">
        Clear All
      </button>
    </div>

    <!-- Engine line: model · device · language, or what it is doing -->
    <div class="tr-engine" :class="{ warn: phase === 'error' || (status && !status.installed) }">
      <span class="tr-engine-dot" :data-phase="phase"></span>
      <span class="tr-engine-text">{{ engineLabel || 'checking the engine…' }}</span>
      <button
        v-if="status && status.installed && phase === 'offline' && !setupBusy"
        class="tr-link"
        title="Start the engine and load the model now, so the first take is quick"
        @click="engineStart"
      >
        start
      </button>
    </div>

    <!-- First run: create the environment and fetch the model -->
    <div v-if="status && !status.installed" class="tr-setup">
      <p class="tr-setup-text">
        Speech-to-text runs on this machine with Whisper (faster-whisper). The first setup creates a Python environment
        and downloads the packages (~200 MB) and the <b>{{ status.config.speechModel }}</b> model
        {{ modelSizeHint(status.config.speechModel) }}. Nothing leaves the machine.
      </p>
      <label class="tr-setup-check">
        <input v-model="setupCuda" type="checkbox" :disabled="setupBusy" />
        <span>
          Also install the CUDA libraries (~700 MB) — what makes the GPU path work
          <template v-if="status.nvidia">; an NVIDIA GPU was found, so this is ticked</template>
        </span>
      </label>
      <button class="btn btn-primary tr-setup-btn" :disabled="setupBusy" @click="setup(setupCuda)">
        {{ setupBusy ? 'Setting up…' : 'Set up Whisper' }}
      </button>
      <div v-if="!status.uv && !status.python" class="tr-hint">
        Needs Python 3.9–3.13 or <code>uv</code> on PATH.
      </div>
    </div>
    <div v-else-if="status && status.installed && status.nvidia && !status.cudaExtras && !setupBusy" class="tr-gpu">
      An NVIDIA GPU was found but the CUDA libraries are not installed — Whisper runs on the CPU.
      <button class="tr-link" @click="setup(true)">Install them (~700 MB)</button>
    </div>
    <div v-if="setupBusy || (setupLines.length && setupPhase === 'error')" class="tr-setup-log">
      <div v-for="(l, i) in setupLines.slice(-6)" :key="i" class="tr-setup-line">{{ l }}</div>
    </div>

    <!-- Record Button -->
    <button
      class="tr-record-btn"
      :class="{ recording: isRecording, transcribing: isTranscribing }"
      :disabled="!installed || busy || isTranscribing || setupBusy"
      @click="toggleRecording"
    >
      <div v-if="isRecording" class="tr-level-bar" :style="{ width: Math.round(levels.rms * 100) + '%' }"></div>
      <svg
        v-if="!isRecording && !isTranscribing"
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        stroke-linecap="round"
        stroke-linejoin="round"
      >
        <path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" />
        <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
        <line x1="12" y1="19" x2="12" y2="22" />
      </svg>
      <span v-else-if="isRecording" class="tr-record-dot"></span>
      <span v-else class="tr-spinner"></span>
      {{ isTranscribing ? 'Transcribing…' : isRecording ? 'Stop Recording' : 'Start Recording' }}
    </button>
    <button v-if="isRecording" class="tr-cancel" @click="cancelRecording">discard this take</button>

    <!-- Error -->
    <div v-if="error" class="tr-error" @click="dismissError">
      {{ error }}
    </div>

    <!-- Live: the spectrum and the words so far -->
    <div v-if="isRecording || isTranscribing" class="tr-interim">
      <div class="tr-visualizer-container">
        <div class="tr-visualizer">
          <div
            v-for="(level, i) in levels.bands"
            :key="i"
            class="tr-visualizer-bar"
            :class="{ still: !isRecording }"
            :style="{ height: Math.max(8, level * 100) + '%', animationDelay: i * 0.03 + 's' }"
          ></div>
        </div>
        <span class="tr-interim-label">{{ isTranscribing ? 'TRANSCRIBING…' : 'LISTENING…' }}</span>
      </div>
      <p v-if="interim" class="tr-interim-text">{{ interim }}</p>
      <p v-else-if="isRecording" class="tr-interim-hint">
        {{ phase === 'recording' && !status?.ready ? 'The model is still loading — keep talking, the words come when it is ready.' : 'Speak — a preview appears in a moment.' }}
      </p>
    </div>

    <!-- Only when the hotkey could not be registered -->
    <div v-if="installed && status && status.hotkeyError" class="tr-hotkey">
      Hotkey {{ status.hotkey }} could not be registered — pick another one in Settings.
    </div>

    <!-- Empty state -->
    </div>
    <div v-if="!entries.length && !isRecording && !isTranscribing && !error && installed" class="tr-empty">
      <p>No transcripts yet.</p>
      <p class="tr-hint">Press the button above, or hold the hotkey anywhere.</p>
    </div>

    <!-- Transcript list -->
    <div v-if="entries.length" class="tr-list">
      <div v-for="entry in entries" :key="entry.id" class="tr-row">
        <div class="tr-content">
          <span class="tr-text">{{ entry.text.length > 240 ? entry.text.slice(0, 240) + '…' : entry.text }}</span>
          <span class="tr-time">
            {{ formatTime(entry.timestamp) }}<template v-if="entry.language"> · {{ entry.language }}</template><template v-if="entry.durationSec"> · {{ entry.durationSec.toFixed(1) }}s</template>
          </span>
        </div>
        <div class="tr-actions">
          <button
            class="ctrl-btn"
            :class="{ copied: copiedId === entry.id }"
            :title="copiedId === entry.id ? 'Copied' : 'Copy'"
            @click="onCopy(entry)"
          >
            <svg v-if="copiedId === entry.id" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="20 6 9 17 4 12" />
            </svg>
            <svg v-else width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" />
              <rect x="8" y="2" width="8" height="4" rx="1" ry="1" />
            </svg>
          </button>
          <button class="ctrl-btn ctrl-del" title="Delete" @click="removeEntry(entry.id)">&#x2715;</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { useTranscript } from '#hud/composables/useTranscript'

const {
  entries,
  status,
  phase,
  levels,
  interim,
  error,
  busy,
  setupLines,
  setupPhase,
  isRecording,
  isTranscribing,
  installed,
  setupBusy,
  engineLabel,
  setup,
  toggleRecording,
  cancelRecording,
  engineStart,
  copyEntry,
  removeEntry,
  clearAll,
  dismissError,
} = useTranscript();

// The CUDA wheels are what makes the GPU path work: ticked when an NVIDIA
// GPU is around and they are not there yet.
const setupCuda = ref(false);
watch(
  status,
  (s) => {
    if (s) setupCuda.value = s.nvidia && !s.cudaExtras;
  },
  { immediate: true },
);

// The copy button shows a tick for a moment: a copy that failed (the
// clipboard permission was missing once) must not look like one that worked.
const copiedId = ref<string | null>(null);
let copiedTimer = 0;
async function onCopy(entry: { id: string; text: string }) {
  if (!(await copyEntry(entry.text))) return;
  copiedId.value = entry.id;
  clearTimeout(copiedTimer);
  copiedTimer = window.setTimeout(() => (copiedId.value = null), 1200);
}

const MODEL_SIZES: Record<string, string> = {
  tiny: "(~75 MB)",
  base: "(~145 MB)",
  small: "(~480 MB)",
  medium: "(~1.5 GB)",
  "large-v3": "(~3 GB)",
  "large-v3-turbo": "(~1.6 GB)",
  turbo: "(~1.6 GB)",
  "distil-large-v3": "(~1.5 GB)",
};
function modelSizeHint(model: string) {
  return MODEL_SIZES[model] ?? "";
}

function formatTime(ts: number) {
  const d = new Date(ts);
  return (
    d.toLocaleTimeString("en-US", { hour: "2-digit", minute: "2-digit", hour12: false }) +
    " " +
    d.toLocaleDateString("en-US", { month: "short", day: "numeric" })
  );
}
</script>

<style scoped>
.tr-module {
  display: flex;
  flex-direction: column;
  padding: 4px 0;
  min-height: 0;
  flex: 1;
}
.tr-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
  flex-shrink: 0;
}
.tr-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}
.tr-engine {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-bottom: 8px;
  font-size: 11px;
  color: var(--text-secondary);
  flex-shrink: 0;
  min-width: 0;
}
.tr-engine.warn { color: #ffb400; }
.tr-engine-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
.tr-engine-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--border-color);
  flex-shrink: 0;
}
.tr-engine-dot[data-phase='ready'] { background: #34d399; }
.tr-engine-dot[data-phase='recording'] { background: #ff4d4d; }
.tr-engine-dot[data-phase='transcribing'],
.tr-engine-dot[data-phase='loading'],
.tr-engine-dot[data-phase='downloading'],
.tr-engine-dot[data-phase='starting'],
.tr-engine-dot[data-phase='unloaded'] { background: #4a9eff; animation: pulse-dot 1s ease-in-out infinite; }
.tr-engine-dot[data-phase='error'] { background: #ff4d4d; }
.tr-link {
  background: none;
  border: none;
  padding: 0;
  color: var(--accent-blue, #4a9eff);
  font-size: 11px;
  cursor: pointer;
  text-decoration: underline;
  text-underline-offset: 2px;
  flex-shrink: 0;
}
.tr-setup {
  padding: 10px;
  margin-bottom: 8px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  flex-shrink: 0;
}
.tr-setup-text { margin: 0; font-size: 11.5px; line-height: 1.45; color: var(--text-secondary); }
.tr-setup-text b { color: var(--text-primary); font-weight: 600; }
.tr-setup-check { display: flex; align-items: flex-start; gap: 6px; font-size: 11px; line-height: 1.4; color: var(--text-secondary); }
.tr-setup-check input { margin-top: 2px; }
.tr-setup-btn { align-self: flex-start; }
.tr-gpu {
  padding: 6px 10px;
  margin-bottom: 8px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  font-size: 11px;
  line-height: 1.45;
  color: var(--text-secondary);
  flex-shrink: 0;
}
.tr-setup-log {
  padding: 6px 8px;
  margin-bottom: 8px;
  background: var(--input-bg, #1a1a1a);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  font-family: ui-monospace, Consolas, monospace;
  font-size: 10px;
  color: var(--text-secondary);
  flex-shrink: 0;
  overflow: hidden;
}
.tr-setup-line { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.tr-record-btn {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  width: 100%;
  padding: 10px 16px;
  margin-bottom: 8px;
  font-size: 13px;
  font-weight: 600;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.15s ease;
  background: var(--bg-card);
  color: var(--text-primary);
  flex-shrink: 0;
  overflow: hidden;
}
.tr-record-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.tr-level-bar {
  position: absolute;
  left: 0;
  top: 0;
  height: 100%;
  background: rgba(255, 77, 77, 0.25);
  transition: width 0.08s linear;
  pointer-events: none;
}
.tr-record-btn:hover:not(:disabled) {
  border-color: var(--accent-blue);
  background: var(--bg-secondary);
}
.tr-record-btn.recording {
  background: var(--accent-red-dim, #5c2020);
  border-color: var(--accent-red, #ff4d4d);
  color: #ff6b6b;
}
.tr-record-btn.recording:hover {
  background: var(--accent-red, #ff4d4d);
  color: white;
}
.tr-record-btn.transcribing { color: var(--accent-blue, #4a9eff); }
.tr-record-dot {
  width: 10px;
  height: 10px;
  border-radius: 50%;
  background: #ff4d4d;
  animation: pulse-dot 1s ease-in-out infinite;
}
.tr-spinner {
  width: 11px;
  height: 11px;
  border-radius: 50%;
  border: 2px solid currentColor;
  border-right-color: transparent;
  animation: tr-spin 0.8s linear infinite;
}
@keyframes tr-spin { to { transform: rotate(360deg); } }
.tr-cancel {
  align-self: center;
  margin: -4px 0 8px;
  background: none;
  border: none;
  color: var(--text-secondary);
  font-size: 11px;
  cursor: pointer;
  text-decoration: underline;
  text-underline-offset: 2px;
}
@keyframes pulse-dot {
  0%,
  100% { opacity: 1; }
  50% { opacity: 0.3; }
}
.tr-error {
  padding: 6px 10px;
  margin-bottom: 8px;
  background: var(--accent-red-dim, #5c2020);
  border: 1px solid var(--accent-red, #ff4d4d);
  border-radius: 6px;
  font-size: 11px;
  color: #ff6b6b;
  word-break: break-word;
  flex-shrink: 0;
  cursor: pointer;
}
.tr-interim {
  padding: 8px 10px;
  margin-bottom: 8px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  flex-shrink: 0;
}
.tr-visualizer-container {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
}
.tr-visualizer {
  display: flex;
  align-items: flex-end;
  justify-content: center;
  gap: 3px;
  height: 32px;
  width: 100%;
  padding: 4px 0;
}
.tr-visualizer-bar {
  width: 5px;
  min-height: 4px;
  background: var(--accent-blue, #4a9eff);
  border-radius: 2px;
  animation: visualizer-pulse 0.8s ease-in-out infinite;
  transition: height 0.08s linear;
}
.tr-visualizer-bar.still { animation: none; opacity: 0.35; }
@keyframes visualizer-pulse {
  0%,
  100% { opacity: 0.4; }
  50% { opacity: 1; }
}
.tr-interim-label {
  font-size: 10px;
  font-weight: 600;
  color: var(--accent-blue, #4a9eff);
  text-transform: uppercase;
  letter-spacing: 0.5px;
  text-align: center;
}
.tr-interim-text {
  font-size: 12px;
  color: var(--text-primary);
  margin: 4px 0 0;
  word-break: break-word;
  text-align: center;
  line-height: 1.4;
}
.tr-interim-hint {
  font-size: 11px;
  color: var(--text-secondary);
  margin: 6px 0 0;
  text-align: center;
}
.tr-hotkey {
  font-size: 10.5px;
  line-height: 1.5;
  color: #ffb400;
  margin-bottom: 8px;
  flex-shrink: 0;
}
.tr-empty {
  text-align: center;
  padding: 24px 0;
  color: var(--text-secondary);
  font-size: 13px;
}
.tr-hint {
  font-size: 11px;
  margin-top: 4px;
  color: var(--text-secondary);
}
.tr-hint code { font-family: ui-monospace, Consolas, monospace; }
.tr-list {
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
.tr-row {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  padding: 6px 8px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 6px;
}
.tr-content {
  flex: 1;
  min-width: 0;
}
.tr-text {
  font-size: 12px;
  color: var(--text-primary);
  word-break: break-word;
  display: block;
  line-height: 1.4;
}
.tr-time {
  font-size: 10px;
  color: var(--text-secondary);
  margin-top: 2px;
  display: block;
}
.tr-actions {
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
.ctrl-btn.copied,
.ctrl-btn.copied:hover {
  color: #34d399;
}
.ctrl-del:hover {
  color: var(--accent-red);
}
.btn-sm {
  font-size: 12px;
  padding: 4px 10px;
}
</style>
