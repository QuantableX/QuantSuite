import type { CalculatorInputs } from "./useCalculator";

export type WindowPosition = "left" | "right" | "dual" | "top";
export type ColorTheme = "light" | "dark";
export type TriggerStyle = "tab";
export type ActivationMode = "hover" | "click";
export type DisplayMode = "basic" | "pro";
export type AiProvider = "ollama" | "lmstudio";
/** Whisper: `auto` (detect) or an ISO 639-1 code — `de`, `en`, … */
export type SpeechLanguage = string;
export type SpeechDevice = "auto" | "cpu" | "cuda";
/** What a hotkey take does with its words: paste into the front app, type them, or copy only. */
export type SpeechInsert = "paste" | "type" | "clipboard";

export interface AppConfig {
  scanRegion: [number, number, number, number] | null;
  calcSettings: Partial<CalculatorInputs>;
  windowPosition: WindowPosition;
  colorTheme: ColorTheme;
  triggerStyle: TriggerStyle;
  activationMode: ActivationMode;
  monitorIndex: number;
  displayMode: DisplayMode;
  screenshotsFolder: string;
  speechLanguage: SpeechLanguage;
  speechModel: string;
  speechDevice: SpeechDevice;
  speechInsert: SpeechInsert;
  speechHotkey: string;
  speechLivePreview: boolean;
  speechInputDevice: string;
  aiProvider: AiProvider;
  aiBaseUrl: string;
  aiModel: string;
  chartAnalyzerRegion: [number, number, number, number] | null;
}

const CONFIG_KEY = "quanthub_config";

export const DEFAULT_SPEECH_HOTKEY = "Ctrl+Shift+Space";

const DEFAULT_CONFIG: AppConfig = {
  scanRegion: null,
  calcSettings: {},
  windowPosition: "left",
  colorTheme: "dark",
  triggerStyle: "tab",
  activationMode: "hover",
  monitorIndex: 0,
  displayMode: "basic",
  screenshotsFolder: "",
  speechLanguage: "auto",
  speechModel: "small",
  speechDevice: "auto",
  speechInsert: "paste",
  speechHotkey: DEFAULT_SPEECH_HOTKEY,
  speechLivePreview: true,
  speechInputDevice: "",
  aiProvider: "ollama",
  aiBaseUrl: "http://localhost:11434",
  aiModel: "llava",
  chartAnalyzerRegion: null,
};

/**
 * `auto`, or the bare ISO 639-1 code this settings page writes (`de`, `en`).
 *
 * The standalone app stored BCP-47 tags (`en-US`, `de-DE`) or `system` for
 * the Windows dictation engine. Those were a choice for that engine, not for
 * Whisper — `en-US` was its default and, mapped to `en`, pinned Whisper to
 * English for every take — so every legacy tag means `auto`: Whisper detects
 * the language per take. Only a bare code stays fixed. The Rust side
 * normalizes the same way (speech.rs).
 */
export function speechLanguageCode(value: string | undefined | null): string {
  const v = (value ?? "").trim().toLowerCase();
  if (!v || v === "system" || v === "auto" || /[-_]/.test(v)) return "auto";
  return v;
}

// ── Shared singleton state so every consumer sees the same loaded config ──
const config = ref<AppConfig>({ ...DEFAULT_CONFIG });

export function useConfig() {
  async function loadConfig() {
    try {
      // Try Tauri store first
      if (window.__TAURI__) {
        const { invoke } = await import("@tauri-apps/api/core");
        const saved = await invoke<string>("plugin:hud|load_config");
        if (saved) {
          // Merge with defaults so new fields are always present
          config.value = { ...config.value, ...JSON.parse(saved) };
        }
      } else {
        // Fallback to localStorage for dev
        const saved = localStorage.getItem(CONFIG_KEY);
        if (saved) {
          config.value = { ...config.value, ...JSON.parse(saved) };
        }
      }
    } catch (e) {
      console.warn("Failed to load config:", e);
    }
    // A legacy tag from the Windows engine becomes `auto` in memory as well,
    // so the next save writes the value the engine actually runs with.
    config.value.speechLanguage = speechLanguageCode(config.value.speechLanguage);
    // Retire both legacy shapes on load and persist through the shared-file
    // merge, keeping clipboard/calendar and other modules' keys intact.
    if (config.value.triggerStyle !== "tab") {
      config.value.triggerStyle = "tab";
      await saveConfig();
    }
  }

  async function saveConfig() {
    try {
      if (window.__TAURI__) {
        // Read-modify-write: the file is shared with _clipboardHistory,
        // _worldclock, _calendar, ... — only our own keys may be replaced, and
        // the queue keeps a parallel writer from dropping them again
        await updateConfigFile("config", (stored) => {
          for (const key of Object.keys(DEFAULT_CONFIG) as (keyof AppConfig)[]) {
            stored[key] = config.value[key];
          }
        });
      } else {
        localStorage.setItem(CONFIG_KEY, JSON.stringify(config.value));
      }
    } catch (e) {
      console.warn("Failed to save config:", e);
    }
  }

  function setScanRegion(region: [number, number, number, number] | null) {
    config.value.scanRegion = region;
    saveConfig();
  }

  function setCalcSettings(settings: Partial<CalculatorInputs>) {
    config.value.calcSettings = { ...config.value.calcSettings, ...settings };
    saveConfig();
  }

  function setWindowPosition(position: WindowPosition) {
    config.value.windowPosition = position;
    saveConfig();
  }

  function setColorTheme(theme: ColorTheme) {
    config.value.colorTheme = theme;
    saveConfig();
  }

  function setActivationMode(mode: ActivationMode) {
    config.value.activationMode = mode;
    saveConfig();
  }

  function setMonitorIndex(index: number) {
    config.value.monitorIndex = index;
    saveConfig();
  }

  function setDisplayMode(mode: DisplayMode) {
    config.value.displayMode = mode;
    saveConfig();
  }

  function setScreenshotsFolder(folder: string) {
    config.value.screenshotsFolder = folder;
    saveConfig();
  }

  // The speech setters return the save, so a caller can hand the engine the
  // new values only once they are on disk (the Rust side reads the file).
  function setSpeechLanguage(lang: SpeechLanguage) {
    config.value.speechLanguage = speechLanguageCode(lang);
    return saveConfig();
  }

  function setSpeechModel(model: string) {
    config.value.speechModel = model.trim() || DEFAULT_CONFIG.speechModel;
    return saveConfig();
  }

  function setSpeechDevice(device: SpeechDevice) {
    config.value.speechDevice = device;
    return saveConfig();
  }

  function setSpeechInsert(mode: SpeechInsert) {
    config.value.speechInsert = mode;
    return saveConfig();
  }

  function setSpeechHotkey(hotkey: string) {
    config.value.speechHotkey = hotkey.trim() || DEFAULT_SPEECH_HOTKEY;
    return saveConfig();
  }

  function setSpeechLivePreview(on: boolean) {
    config.value.speechLivePreview = on;
    return saveConfig();
  }

  function setSpeechInputDevice(name: string) {
    config.value.speechInputDevice = name;
    return saveConfig();
  }

  function setAiProvider(provider: AiProvider) {
    config.value.aiProvider = provider;
    saveConfig();
  }

  function setAiBaseUrl(url: string) {
    config.value.aiBaseUrl = url;
    saveConfig();
  }

  function setAiModel(model: string) {
    config.value.aiModel = model;
    saveConfig();
  }

  function setChartAnalyzerRegion(
    region: [number, number, number, number] | null,
  ) {
    config.value.chartAnalyzerRegion = region;
    saveConfig();
  }

  let _syncCleanup: (() => void) | null = null;
  let _unmounted = false;

  // Load on init
  onMounted(async () => {
    await loadConfig();
    const cleanup = await onSyncEvent("config", loadConfig);
    // The awaits above can outlast the component — unlisten right away then
    if (_unmounted) cleanup();
    else _syncCleanup = cleanup;
  });

  onUnmounted(() => {
    _unmounted = true;
    _syncCleanup?.();
    _syncCleanup = null;
  });

  return {
    config,
    loadConfig,
    saveConfig,
    setScanRegion,
    setCalcSettings,
    setWindowPosition,
    setColorTheme,
    setActivationMode,
    setMonitorIndex,
    setDisplayMode,
    setScreenshotsFolder,
    setSpeechLanguage,
    setSpeechModel,
    setSpeechDevice,
    setSpeechInsert,
    setSpeechHotkey,
    setSpeechLivePreview,
    setSpeechInputDevice,
    setAiProvider,
    setAiBaseUrl,
    setAiModel,
    setChartAnalyzerRegion,
  };
}
