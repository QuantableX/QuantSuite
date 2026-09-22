/**
 * QuantVoice in the HUD: the state of the Whisper engine, one recording at
 * a time, and the transcript list (docs/PLAN-QUANTVOICE.md).
 *
 * Whisper runs in a Python sidecar the `hud` crate hosts (speech.rs); this
 * composable is its mirror in the webview. Everything arrives as events —
 * `speech-state`, `speech-level`, `speech-interim`, `speech-result`,
 * `speech-error`, `speech-setup`, `speech-download` — so a take started
 * from the global hotkey shows up here exactly like one started from the
 * button. The listeners are registered once, for the window, not per
 * recording: the hotkey works while the Transcript module is not open.
 */

export interface TranscriptEntry {
  id: string;
  text: string;
  timestamp: number;
  language?: string | null;
  durationSec?: number;
}

export type SpeechPhase =
  | "offline"
  | "starting"
  | "unloaded"
  | "loading"
  | "downloading"
  | "ready"
  | "recording"
  | "transcribing"
  | "error";

export interface SpeechSetupState {
  busy: boolean;
  phase: string;
  error: string | null;
  lines: string[];
}

/** crate/src/speech.rs `SpeechStatus`. */
export interface SpeechStatus {
  sidecarDir: string | null;
  python: string | null;
  installed: boolean;
  /** The pip CUDA runtime (cuBLAS, cuDNN 9) is in the venv — the GPU path works with it. */
  cudaExtras: boolean;
  /** `nvidia-smi` is on PATH. */
  nvidia: boolean;
  uv: boolean;
  running: boolean;
  ready: boolean;
  phase: SpeechPhase;
  detail: string | null;
  model: string | null;
  device: string | null;
  compute: string | null;
  recording: boolean;
  hotkey: string;
  hotkeyError: string | null;
  setup: SpeechSetupState;
  modelDir: string;
  config: {
    speechLanguage: string;
    speechModel: string;
    speechDevice: string;
    speechInsert: string;
    speechHotkey: string;
    speechLivePreview: boolean;
    speechInputDevice: string;
  };
}

export interface SpeechInputDevice {
  index: number;
  name: string;
  channels: number;
  default: boolean;
  hostapi: string;
}

export interface SpeechLevels {
  rms: number;
  peak: number;
  bands: number[];
}

interface SpeechResult {
  text: string;
  language: string | null;
  probability: number;
  durationSec: number;
  elapsedSec: number;
  empty: boolean;
  inserted: boolean;
  viaHotkey: boolean;
  insertMode: string;
}

const TRANSCRIPT_KEY = "quanthud_transcript";
const MAX_ENTRIES = 50;
const BANDS = 12;

function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
}

function inTauri(): boolean {
  return typeof window !== "undefined" && Boolean((window as any).__TAURI__);
}

// ── Shared singleton state ──
const _entries = ref<TranscriptEntry[]>([]);
const _status = ref<SpeechStatus | null>(null);
const _phase = ref<SpeechPhase>("offline");
const _detail = ref<string>("");
const _levels = ref<SpeechLevels>({ rms: 0, peak: 0, bands: Array(BANDS).fill(0) });
const _interim = ref("");
const _error = ref("");
const _busy = ref(false);
const _setupLines = ref<string[]>([]);
const _setupPhase = ref("");
const _download = ref<{ file: string; done: number; total: number; model: string } | null>(null);
const _devices = ref<SpeechInputDevice[]>([]);
let _initialized = false;
let _levelDecay = 0;

async function _load() {
  try {
    if (inTauri()) {
      const { invoke } = await import("@tauri-apps/api/core");
      const saved = await invoke<string>("plugin:hud|load_config");
      if (saved) {
        const config = JSON.parse(saved);
        if (config._transcriptHistory) {
          _entries.value = config._transcriptHistory;
          return;
        }
      }
    }
    const saved = localStorage.getItem(TRANSCRIPT_KEY);
    if (saved) _entries.value = JSON.parse(saved);
  } catch (e) {
    console.warn("Failed to load transcript history:", e);
  }
}

async function _save() {
  try {
    if (inTauri()) {
      await updateConfigFile("transcript", (config) => {
        config._transcriptHistory = _entries.value;
      });
    } else {
      localStorage.setItem(TRANSCRIPT_KEY, JSON.stringify(_entries.value));
    }
  } catch (e) {
    console.warn("Failed to save transcript history:", e);
  }
}

/** `false` when the clipboard refused — shown in the module, not just logged. */
async function _writeClipboard(text: string): Promise<boolean> {
  try {
    if (inTauri()) {
      const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
      await writeText(text);
    } else {
      await navigator.clipboard.writeText(text);
    }
    return true;
  } catch (e: any) {
    console.warn("Failed to write clipboard:", e);
    _error.value = `Could not copy to the clipboard: ${e?.message || e}`;
    return false;
  }
}

function _addEntry(text: string, extra: Partial<TranscriptEntry> = {}) {
  const trimmed = text.trim();
  if (!trimmed) return;
  _entries.value.unshift({
    id: generateId(),
    text: trimmed,
    timestamp: Date.now(),
    ...extra,
  });
  if (_entries.value.length > MAX_ENTRIES) _entries.value.pop();
  _save();
}

async function _invoke<T = unknown>(cmd: string, args?: Record<string, unknown>): Promise<T | null> {
  if (!inTauri()) return null;
  const { invoke } = await import("@tauri-apps/api/core");
  return (await invoke(cmd, args)) as T;
}

function _clearLevels() {
  _levels.value = { rms: 0, peak: 0, bands: Array(BANDS).fill(0) };
}

async function _listen() {
  if (!inTauri()) return;
  const { listen } = await import("@tauri-apps/api/event");
  const secondary = await isDualRightWindow();

  await listen<{ phase: SpeechPhase; detail?: string | null }>("speech-state", (ev) => {
    const p = ev.payload;
    _phase.value = p.phase;
    _detail.value = p.detail ?? "";
    if (p.phase === "error") _error.value = p.detail || "engine error";
    if (p.phase !== "recording") {
      _clearLevels();
      if (p.phase !== "transcribing") _interim.value = "";
    }
    if (p.phase === "ready" || p.phase === "offline" || p.phase === "unloaded" || p.phase === "error") {
      void refreshStatus();
    }
  });
  await listen<SpeechLevels>("speech-level", (ev) => {
    if (_phase.value !== "recording") return;
    const bands = ev.payload.bands ?? [];
    _levels.value = {
      rms: ev.payload.rms ?? 0,
      peak: ev.payload.peak ?? 0,
      bands: Array.from({ length: BANDS }, (_, i) => bands[i] ?? 0),
    };
    clearTimeout(_levelDecay);
    _levelDecay = window.setTimeout(_clearLevels, 400);
  });
  await listen<{ text: string }>("speech-interim", (ev) => {
    if (_phase.value === "recording") _interim.value = ev.payload.text ?? "";
  });
  await listen<SpeechResult>("speech-result", (ev) => {
    const r = ev.payload;
    _interim.value = "";
    _busy.value = false;
    if (!r.text) {
      if (!r.viaHotkey) _error.value = "Nothing was recognised — was the microphone picking anything up?";
      return;
    }
    // The other overlay of dual mode reloads the list through the sync event.
    if (secondary) return;
    _addEntry(r.text, { language: r.language, durationSec: r.durationSec });
    // A hotkey take was already put where it belongs (pasted, typed, or
    // copied by the Rust side); a button take is copied here, as before.
    if (!r.viaHotkey) void _writeClipboard(r.text);
  });
  await listen<{ message?: string }>("speech-error", (ev) => {
    _error.value = ev.payload?.message || "Speech recognition error";
    _busy.value = false;
    _interim.value = "";
    _clearLevels();
  });
  await listen<{ phase: string; line?: string }>("speech-setup", (ev) => {
    _setupPhase.value = ev.payload.phase;
    if (ev.payload.line) {
      _setupLines.value.push(ev.payload.line);
      if (_setupLines.value.length > 60) _setupLines.value.shift();
    }
    if (ev.payload.phase === "done" || ev.payload.phase === "error") void refreshStatus();
  });
  await listen<{ file: string; done: number; total: number; model: string }>("speech-download", (ev) => {
    _download.value = ev.payload;
    if (ev.payload.total && ev.payload.done >= ev.payload.total) {
      window.setTimeout(() => {
        _download.value = null;
      }, 1500);
    }
  });
}

async function refreshStatus() {
  try {
    const s = await _invoke<SpeechStatus>("plugin:hud|speech_status");
    if (s) {
      _status.value = s;
      _phase.value = s.phase;
      if (s.detail) _detail.value = s.detail;
      if (s.setup.error) _error.value = s.setup.error;
    }
  } catch (e) {
    console.warn("speech_status failed:", e);
  }
}

export function useTranscript() {
  if (!_initialized) {
    _initialized = true;
    _load();
    onSyncEvent("transcript", _load);
    void _listen().then(refreshStatus);
  }

  const isRecording = computed(() => _phase.value === "recording");
  const isTranscribing = computed(() => _phase.value === "transcribing");
  const installed = computed(() => Boolean(_status.value?.installed));
  const setupBusy = computed(() => Boolean(_status.value?.setup.busy) || _setupPhase.value === "venv" || _setupPhase.value === "install" || _setupPhase.value === "cuda");

  /** One line for the engine: model, device, language. */
  const engineLabel = computed(() => {
    const s = _status.value;
    if (!s) return "";
    if (!s.installed) return "Whisper is not set up yet";
    const model = s.model || s.config.speechModel;
    const device = s.device ? (s.device === "cuda" ? "GPU" : "CPU") : s.config.speechDevice === "cpu" ? "CPU" : "auto";
    const lang = s.config.speechLanguage === "auto" ? "auto-detect" : s.config.speechLanguage;
    switch (_phase.value) {
      case "offline":
        return `Whisper ${model} · engine off · ${lang}`;
      case "starting":
        return "starting the engine…";
      case "unloaded":
      case "loading":
        return `loading ${model}…`;
      case "downloading": {
        const d = _download.value;
        const pct = d && d.total ? ` ${Math.round((d.done / d.total) * 100)}%` : "";
        return `downloading ${model}${pct} (once)`;
      }
      case "error":
        return `Whisper ${model} · error`;
      default:
        return `Whisper ${model} · ${device} · ${lang}`;
    }
  });

  async function setup(cuda = false) {
    _error.value = "";
    _setupLines.value = [];
    _setupPhase.value = "venv";
    try {
      await _invoke("plugin:hud|speech_setup", { cuda });
    } catch (e: any) {
      _error.value = `Setup failed to start: ${e?.message || e}`;
      _setupPhase.value = "error";
    }
    await refreshStatus();
  }

  async function startRecording() {
    if (_busy.value || isRecording.value) return;
    _error.value = "";
    _interim.value = "";
    _busy.value = true;
    try {
      await _invoke("plugin:hud|speech_start");
      _phase.value = "recording";
    } catch (e: any) {
      _error.value = `Could not start recording: ${e?.message || e}`;
      _phase.value = _status.value?.phase ?? "offline";
      void refreshStatus();
    } finally {
      _busy.value = false;
    }
  }

  async function stopRecording() {
    if (!isRecording.value) return;
    _busy.value = true;
    _clearLevels();
    try {
      await _invoke("plugin:hud|speech_stop");
    } catch (e: any) {
      _error.value = `Could not stop recording: ${e?.message || e}`;
      _busy.value = false;
      void refreshStatus();
    }
  }

  async function cancelRecording() {
    try {
      await _invoke("plugin:hud|speech_cancel");
    } catch (e) {
      console.warn("speech_cancel failed:", e);
    }
    _interim.value = "";
    _clearLevels();
    _busy.value = false;
    void refreshStatus();
  }

  function toggleRecording() {
    if (isRecording.value) void stopRecording();
    else void startRecording();
  }

  /** After a settings change: the hotkey and the engine's model/device/language. */
  async function applySettings() {
    try {
      const s = await _invoke<SpeechStatus>("plugin:hud|speech_apply_settings");
      if (s) {
        _status.value = s;
        _phase.value = s.phase;
        if (s.hotkeyError) _error.value = `Hotkey: ${s.hotkeyError}`;
      }
    } catch (e: any) {
      _error.value = `Could not apply the settings: ${e?.message || e}`;
    }
  }

  async function loadDevices() {
    try {
      const r = await _invoke<{ devices: SpeechInputDevice[] }>("plugin:hud|speech_devices");
      if (r?.devices) _devices.value = r.devices;
    } catch (e: any) {
      _error.value = `Could not list microphones: ${e?.message || e}`;
    }
  }

  async function engineStart() {
    _error.value = "";
    try {
      await _invoke("plugin:hud|speech_engine_start");
    } catch (e: any) {
      _error.value = `${e?.message || e}`;
    }
    await refreshStatus();
  }

  async function engineStop() {
    try {
      await _invoke("plugin:hud|speech_engine_stop");
    } catch (e) {
      console.warn("speech_engine_stop failed:", e);
    }
    await refreshStatus();
  }

  function copyEntry(text: string) {
    return _writeClipboard(text);
  }

  function removeEntry(id: string) {
    _entries.value = _entries.value.filter((e) => e.id !== id);
    _save();
  }

  function clearAll() {
    _entries.value = [];
    _save();
  }

  function dismissError() {
    _error.value = "";
  }

  return {
    entries: _entries,
    status: _status,
    phase: _phase,
    detail: _detail,
    levels: _levels,
    interim: _interim,
    error: _error,
    busy: _busy,
    setupLines: _setupLines,
    setupPhase: _setupPhase,
    download: _download,
    devices: _devices,
    isRecording,
    isTranscribing,
    installed,
    setupBusy,
    engineLabel,
    refreshStatus,
    setup,
    startRecording,
    stopRecording,
    cancelRecording,
    toggleRecording,
    applySettings,
    loadDevices,
    engineStart,
    engineStop,
    copyEntry,
    removeEntry,
    clearAll,
    dismissError,
  };
}
