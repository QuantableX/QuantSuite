export interface SavedColor {
  id: string;
  hex: string;
  label: string;
}

const COLORPICKER_KEY = "quanthud_colorpicker";

function generateId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 8);
}

export function useColorPicker() {
  const currentColor = ref("#FFFFFF");
  const savedColors = ref<SavedColor[]>([]);

  async function load() {
    try {
      if (typeof window !== "undefined" && (window as any).__TAURI__) {
        const { invoke } = await import("@tauri-apps/api/core");
        const saved = await invoke<string>("plugin:hud|load_config");
        if (saved) {
          const config = JSON.parse(saved);
          if (config._colorpicker) {
            savedColors.value = config._colorpicker;
            return;
          }
        }
      }
      const saved = localStorage.getItem(COLORPICKER_KEY);
      if (saved) savedColors.value = JSON.parse(saved);
    } catch (e) {
      console.warn("Failed to load colors:", e);
    }
  }

  async function save() {
    try {
      if (typeof window !== "undefined" && (window as any).__TAURI__) {
        await updateConfigFile("colorpicker", (config) => {
          config._colorpicker = savedColors.value;
        });
      } else {
        localStorage.setItem(
          COLORPICKER_KEY,
          JSON.stringify(savedColors.value),
        );
      }
    } catch (e) {
      console.warn("Failed to save colors:", e);
    }
  }

  function saveColor(hex?: string, label?: string) {
    const color = hex || currentColor.value;
    savedColors.value.unshift({
      id: generateId(),
      hex: color,
      label: label || color,
    });
    save();
  }

  function removeColor(id: string) {
    savedColors.value = savedColors.value.filter((c) => c.id !== id);
    save();
  }

  function hexToRgb(hex: string): { r: number; g: number; b: number } | null {
    const result = /^#?([a-f\d]{2})([a-f\d]{2})([a-f\d]{2})$/i.exec(hex);
    return result
      ? {
          r: parseInt(result[1], 16),
          g: parseInt(result[2], 16),
          b: parseInt(result[3], 16),
        }
      : null;
  }

  function hexToHsl(hex: string): { h: number; s: number; l: number } | null {
    const rgb = hexToRgb(hex);
    if (!rgb) return null;
    const r = rgb.r / 255,
      g = rgb.g / 255,
      b = rgb.b / 255;
    const max = Math.max(r, g, b),
      min = Math.min(r, g, b);
    let h = 0,
      s = 0;
    const l = (max + min) / 2;
    if (max !== min) {
      const d = max - min;
      s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
      switch (max) {
        case r:
          h = ((g - b) / d + (g < b ? 6 : 0)) / 6;
          break;
        case g:
          h = ((b - r) / d + 2) / 6;
          break;
        case b:
          h = ((r - g) / d + 4) / 6;
          break;
      }
    }
    return {
      h: Math.round(h * 360),
      s: Math.round(s * 100),
      l: Math.round(l * 100),
    };
  }

  async function pickScreenColor(): Promise<string | null> {
    try {
      const isTauri =
        typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
      if (!isTauri) {
        console.warn("Screen color picker only works in Tauri");
        return null;
      }
      const { invoke } = await import("@tauri-apps/api/core");

      // Open the transparent overlay window (Rust clears stale state first)
      await invoke("plugin:hud|open_color_picker_overlay");

      // Poll for result — overlay sets it via set_picked_color then closes
      return new Promise((resolve) => {
        let retries = 0;
        const check = async () => {
          try {
            const color = await invoke<string | null>("plugin:hud|get_picked_color");
            if (color !== null && color !== undefined) {
              currentColor.value = color;
              resolve(color);
              return;
            }
            // Check if overlay window still exists
            const { WebviewWindow } =
              await import("@tauri-apps/api/webviewWindow");
            const overlay = await WebviewWindow.getByLabel(
              "color-picker-overlay",
            );
            if (overlay) {
              // Overlay still open — keep polling
              retries = 0;
              setTimeout(check, 100);
            } else if (retries < 10) {
              // Window just closed — retry a few times to catch the value
              retries++;
              setTimeout(check, 100);
            } else {
              resolve(null);
            }
          } catch {
            resolve(null);
          }
        };
        setTimeout(check, 300);
      });
    } catch (e) {
      console.warn("Failed to pick screen color:", e);
      return null;
    }
  }

  let _syncCleanup: (() => void) | null = null;
  let _unmounted = false;

  onMounted(async () => {
    await load();
    const cleanup = await onSyncEvent("colorpicker", load);
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
    currentColor,
    savedColors,
    saveColor,
    removeColor,
    hexToRgb,
    hexToHsl,
    pickScreenColor,
  };
}
