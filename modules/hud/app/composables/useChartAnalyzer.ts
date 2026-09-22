export interface ChartAnalysisResult {
  market_phase: string;
  schematic: string;
  wyckoff_phase: string;
  events: string[];
  current_transition: string;
  bias: string;
}

export interface AnalysisHistoryEntry {
  id: string;
  timestamp: number;
  result: ChartAnalysisResult;
  /** Downscaled thumbnail of the capture — the full screenshot is in IndexedDB
   *  under the same id, see getCapture() */
  imageBase64?: string;
}

const HISTORY_KEY = "quanthud_analysis_history";
const MAX_HISTORY = 30;
// Longest thumbnail edge: MAX_HISTORY full captures are several MB of base64
// and blow the ~5 MB localStorage quota, taking the whole history with them.
const THUMB_MAX_PX = 360;
// The full captures therefore live in IndexedDB, which is not bound to that
// quota. Everything here fails soft: without a capture the UI falls back to the
// thumbnail, exactly as if the entry predated this store.
const CAPTURE_DB = "quanthud_analysis";
const CAPTURE_STORE = "captures";

let _dbPromise: Promise<IDBDatabase | null> | null = null;

function openCaptureDb(): Promise<IDBDatabase | null> {
  if (!_dbPromise) {
    _dbPromise = new Promise<IDBDatabase | null>((resolve) => {
      try {
        if (typeof indexedDB === "undefined") {
          resolve(null);
          return;
        }
        const req = indexedDB.open(CAPTURE_DB, 1);
        req.onupgradeneeded = () => {
          if (!req.result.objectStoreNames.contains(CAPTURE_STORE)) {
            req.result.createObjectStore(CAPTURE_STORE);
          }
        };
        req.onsuccess = () => resolve(req.result);
        req.onerror = () => resolve(null);
      } catch {
        resolve(null);
      }
    });
  }
  return _dbPromise;
}

function putCapture(id: string, imageBase64: string): Promise<void> {
  return openCaptureDb().then(
    (db) =>
      new Promise<void>((resolve) => {
        if (!db) {
          resolve();
          return;
        }
        try {
          const tx = db.transaction(CAPTURE_STORE, "readwrite");
          tx.objectStore(CAPTURE_STORE).put(imageBase64, id);
          tx.oncomplete = () => resolve();
          tx.onerror = () => resolve();
          tx.onabort = () => resolve();
        } catch {
          resolve();
        }
      }),
  );
}

function deleteCaptures(ids: string[]): Promise<void> {
  return openCaptureDb().then(
    (db) =>
      new Promise<void>((resolve) => {
        if (!db || ids.length === 0) {
          resolve();
          return;
        }
        try {
          const tx = db.transaction(CAPTURE_STORE, "readwrite");
          const store = tx.objectStore(CAPTURE_STORE);
          for (const id of ids) store.delete(id);
          tx.oncomplete = () => resolve();
          tx.onerror = () => resolve();
          tx.onabort = () => resolve();
        } catch {
          resolve();
        }
      }),
  );
}

function makeThumbnail(imageBase64: string): Promise<string | undefined> {
  return new Promise((resolve) => {
    const img = new Image();
    img.onload = () => {
      const scale = Math.min(1, THUMB_MAX_PX / Math.max(img.width, img.height));
      const canvas = document.createElement("canvas");
      canvas.width = Math.max(1, Math.round(img.width * scale));
      canvas.height = Math.max(1, Math.round(img.height * scale));
      const ctx = canvas.getContext("2d");
      if (!ctx) {
        resolve(undefined);
        return;
      }
      ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
      resolve(canvas.toDataURL("image/png").split(",")[1]);
    };
    img.onerror = () => resolve(undefined);
    img.src = `data:image/png;base64,${imageBase64}`;
  });
}

export function useChartAnalyzer() {
  const isAnalyzing = ref(false);
  const status = ref("Ready");
  const result = ref<ChartAnalysisResult | null>(null);
  const rawResponse = ref("");
  const capturedImage = ref("");
  const history = ref<AnalysisHistoryEntry[]>([]);

  function loadHistory() {
    try {
      const saved = localStorage.getItem(HISTORY_KEY);
      if (saved) history.value = JSON.parse(saved);
    } catch {
      history.value = [];
    }
  }

  function saveHistory() {
    try {
      localStorage.setItem(HISTORY_KEY, JSON.stringify(history.value));
    } catch {
      status.value = "History not saved (storage full)";
    }
  }

  /** Full capture behind a history entry, `undefined` for entries stored
   *  before the capture store existed or evicted since. */
  function getCapture(id: string): Promise<string | undefined> {
    return openCaptureDb().then(
      (db) =>
        new Promise<string | undefined>((resolve) => {
          if (!db) {
            resolve(undefined);
            return;
          }
          try {
            const tx = db.transaction(CAPTURE_STORE, "readonly");
            const req = tx.objectStore(CAPTURE_STORE).get(id);
            req.onsuccess = () => resolve(req.result as string | undefined);
            req.onerror = () => resolve(undefined);
          } catch {
            resolve(undefined);
          }
        }),
    );
  }

  async function addToHistory(res: ChartAnalysisResult, imageBase64?: string) {
    const entry: AnalysisHistoryEntry = {
      id: Date.now().toString(36),
      timestamp: Date.now(),
      result: res,
      imageBase64: imageBase64 ? await makeThumbnail(imageBase64) : undefined,
    };
    if (imageBase64) await putCapture(entry.id, imageBase64);
    history.value.unshift(entry);
    if (history.value.length > MAX_HISTORY) {
      const evicted = history.value.slice(MAX_HISTORY);
      history.value = history.value.slice(0, MAX_HISTORY);
      deleteCaptures(evicted.map((e) => e.id));
    }
    saveHistory();
  }

  function deleteHistoryEntry(id: string) {
    history.value = history.value.filter((e) => e.id !== id);
    deleteCaptures([id]);
    saveHistory();
  }

  function clearHistory() {
    deleteCaptures(history.value.map((e) => e.id));
    history.value = [];
    saveHistory();
  }

  async function captureAndAnalyze(
    region: [number, number, number, number] | null,
    provider: string,
    baseUrl: string,
    model: string,
    analysisTypes: string[],
    windowConfig: {
      position: string;
      monitorIndex: number;
    },
  ) {
    if (isAnalyzing.value) return;

    isAnalyzing.value = true;
    status.value = "Capturing...";
    result.value = null;
    rawResponse.value = "";

    try {
      const isTauri =
        typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
      if (!isTauri) {
        status.value = "Only works in Tauri";
        return;
      }

      const { invoke } = await import("@tauri-apps/api/core");

      // Hide HUD so it doesn't appear in the screenshot
      await invoke("plugin:hud|tuck_window", {
        position: windowConfig.position,
        monitorIndex: windowConfig.monitorIndex,
      });
      await new Promise((r) => setTimeout(r, 250));

      // Capture screenshot
      const capture = await invoke<{
        image_base64: string;
        width: number;
        height: number;
      }>("plugin:hud|capture_screen", {
        region,
        defaultCrop: false,
      });

      // Show HUD again immediately after capture
      await invoke("plugin:hud|show_window", {
        position: windowConfig.position,
        monitorIndex: windowConfig.monitorIndex,
      });

      capturedImage.value = capture.image_base64;
      status.value = "Analyzing chart...";

      // Send to local AI for analysis
      const response = await invoke<string>("plugin:hud|analyze_chart", {
        imageBase64: capture.image_base64,
        analysisTypes,
        provider,
        baseUrl,
        model,
      });

      rawResponse.value = response;

      // Parse JSON from response
      try {
        let jsonStr = response;
        const fenceMatch = jsonStr.match(/```(?:json)?\s*([\s\S]*?)```/);
        if (fenceMatch) {
          jsonStr = fenceMatch[1];
        }
        const jsonMatch = jsonStr.match(/\{[\s\S]*\}/);
        if (jsonMatch) {
          const parsed = JSON.parse(jsonMatch[0]);
          result.value = parsed;
          status.value = "Analysis complete";
          await addToHistory(parsed, capturedImage.value || undefined);
        } else {
          status.value = "Could not parse response";
        }
      } catch {
        status.value = "Could not parse response";
      }
    } catch (e: any) {
      const msg = e.message || String(e);
      status.value = msg.length > 80 ? msg.slice(0, 80) + "..." : msg;
      // Try to show window on error too
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        await invoke("plugin:hud|show_window", {
          position: windowConfig.position,
          monitorIndex: windowConfig.monitorIndex,
        });
      } catch {
        /* ignore */
      }
    } finally {
      isAnalyzing.value = false;
    }
  }

  function clearResults() {
    result.value = null;
    rawResponse.value = "";
    capturedImage.value = "";
    status.value = "Ready";
  }

  // Load history on creation
  loadHistory();

  return {
    isAnalyzing,
    status,
    result,
    rawResponse,
    capturedImage,
    history,
    captureAndAnalyze,
    clearResults,
    getCapture,
    deleteHistoryEntry,
    clearHistory,
  };
}
