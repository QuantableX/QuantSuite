// Fibonacci levels to detect (same as prototype)
export const FIB_LEVELS = [-0.2, 0, 0.25, 0.5, 0.75, 1, 1.2];

export interface FibPrices {
  [level: number]: number;
}

// OCR worker (lazy loaded)
let ocrWorker: any = null;

async function getOcrWorker() {
  if (ocrWorker) return ocrWorker;

  const Tesseract = await import("tesseract.js");
  ocrWorker = await Tesseract.createWorker("eng");
  return ocrWorker;
}

function extractFibLevelsFromText(text: string): FibPrices {
  const fibPrices: FibPrices = {};

  // Pattern: "Level (Price)" like "1.2 (3,151.25)" or "0.5 (3100.50)"
  const pattern =
    /(-?[0-9]\.?[0-9]*)\s*[\(\[\{]\s*([0-9][,0-9]*\.?[0-9]+)\s*[\)\]\}]/g;

  let match;
  while ((match = pattern.exec(text)) !== null) {
    let level = parseFloat(match[1]);
    const price = parseFloat(match[2].replace(/,/g, ""));

    // Fix: 0.2 often should be -0.2
    if (Math.abs(level - 0.2) < 0.01) {
      level = -0.2;
    }

    // Check if it matches a known fib level
    for (const fibLevel of FIB_LEVELS) {
      if (Math.abs(level - fibLevel) < 0.01) {
        fibPrices[fibLevel] = price;
        break;
      }
    }
  }

  return fibPrices;
}

export function useFibExtractor() {
  const fibPrices = ref<FibPrices>({});
  const isProcessing = ref(false);
  const status = ref("Press F9 to capture");
  const scanRegion = ref<[number, number, number, number] | null>(null);

  async function captureAndExtract() {
    if (isProcessing.value) return;

    isProcessing.value = true;
    status.value = "⏳ Capturing...";

    try {
      // Check if running in Tauri
      const isTauri =
        typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

      if (isTauri) {
        const { invoke } = await import("@tauri-apps/api/core");

        // Capture screen from Rust backend
        const result = await invoke<{
          image_base64: string;
          width: number;
          height: number;
        }>("plugin:hud|capture_screen", {
          region: scanRegion.value,
        });

        status.value = "⏳ Running OCR...";

        // Run OCR on the captured image
        const worker = await getOcrWorker();
        const imageData = `data:image/png;base64,${result.image_base64}`;
        const {
          data: { text },
        } = await worker.recognize(imageData);

        console.log("OCR text:", text);

        // Extract fib levels from OCR text
        const extracted = extractFibLevelsFromText(text);

        if (Object.keys(extracted).length > 0) {
          fibPrices.value = extracted;
          const found = Object.keys(extracted).length;
          status.value = `✓ ${found}/7 levels found`;
        } else {
          status.value = "❌ Could not extract levels";
        }
      } else {
        status.value = "⚠️ Capture only works in Tauri";
      }
    } catch (e: any) {
      console.error("Capture error:", e);
      status.value = `❌ ${e.message || "Capture failed"}`;
    } finally {
      isProcessing.value = false;
    }
  }

  function getLevelPrices(isLong: boolean) {
    const price0 = fibPrices.value[0] || 0;
    const price1 = fibPrices.value[1] || 0;
    const priceExtLow = fibPrices.value[1.2] || 0;
    const priceExtHigh = fibPrices.value[-0.2] || 0;

    const isInverted = price0 && price1 && price0 < price1;

    let entry = 0,
      tp = 0,
      sl = 0;

    if (isLong) {
      if (isInverted) {
        entry = price0;
        tp = price1;
        sl = priceExtHigh;
      } else {
        entry = price1;
        tp = price0;
        sl = priceExtLow;
      }
    } else {
      if (isInverted) {
        entry = price1;
        tp = price0;
        sl = priceExtLow;
      } else {
        entry = price0;
        tp = price1;
        sl = priceExtHigh;
      }
    }

    return { entry, tp, sl };
  }

  function clearFibPrices() {
    fibPrices.value = {};
    status.value = "Cleared";
  }

  return {
    fibPrices,
    isProcessing,
    status,
    scanRegion,
    captureAndExtract,
    getLevelPrices,
    clearFibPrices,
  };
}
