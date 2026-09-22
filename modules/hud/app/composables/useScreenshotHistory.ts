export interface ScreenshotEntry {
  path: string;
  filename: string;
  modified: number; // unix seconds
  image_base64?: string; // loaded on demand
}

export function useScreenshotHistory(customFolder?: Ref<string>) {
  const entries = ref<ScreenshotEntry[]>([]);
  const loading = ref(false);

  async function loadEntries() {
    try {
      const isTauri =
        typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
      if (!isTauri) {
        console.warn("Screenshots only work in Tauri");
        return;
      }
      loading.value = true;
      const { invoke } = await import("@tauri-apps/api/core");
      const folder = customFolder?.value || "";
      const list = await invoke<
        { path: string; filename: string; modified: number }[]
      >("plugin:hud|list_os_screenshots", { customFolder: folder || null });
      entries.value = list.map((s) => ({
        path: s.path,
        filename: s.filename,
        modified: s.modified,
      }));
    } catch (e) {
      console.warn("Failed to list screenshots:", e);
    } finally {
      loading.value = false;
    }
  }

  async function loadThumbnail(entry: ScreenshotEntry) {
    if (entry.image_base64) return;
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const base64 = await invoke<string>("plugin:hud|read_screenshot_thumbnail", {
        path: entry.path,
        maxWidth: 120,
      });
      entry.image_base64 = base64;
    } catch (e) {
      console.warn("Failed to load thumbnail:", e);
    }
  }

  async function loadFullImage(entry: ScreenshotEntry): Promise<string | null> {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const base64 = await invoke<string>("plugin:hud|read_screenshot_file", {
        path: entry.path,
      });
      return `data:image/png;base64,${base64}`;
    } catch (e) {
      console.warn("Failed to load full image:", e);
      return null;
    }
  }

  async function copyScreenshot(entry: ScreenshotEntry) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("plugin:hud|copy_screenshot_to_clipboard", { path: entry.path });
    } catch (e) {
      console.warn("Failed to copy screenshot:", e);
    }
  }

  async function openFolder() {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const folder = customFolder?.value || "";
      await invoke("plugin:hud|open_screenshots_folder", { customFolder: folder || null });
    } catch (e) {
      console.warn("Failed to open folder:", e);
    }
  }

  onMounted(() => {
    loadEntries();
  });

  return {
    entries,
    loading,
    loadEntries,
    loadThumbnail,
    loadFullImage,
    copyScreenshot,
    openFolder,
  };
}
