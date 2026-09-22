/**
 * Cross-window state sync via Tauri events.
 * When a composable saves data, it calls emitSync('moduleName').
 * The other window listens for that module's event and reloads.
 */

let _currentLabel: string | null = null;

async function getLabel(): Promise<string> {
  if (_currentLabel) return _currentLabel;
  try {
    const { getCurrentWebviewWindow } = await import(
      "@tauri-apps/api/webviewWindow"
    );
    _currentLabel = getCurrentWebviewWindow().label;
  } catch {
    _currentLabel = "unknown";
  }
  return _currentLabel;
}

/**
 * True in the secondary overlay of dual mode (see pages/hud/index.vue, which
 * keys its per-window setup off the same label). Singletons that would
 * otherwise run twice — anything that notifies the user — check this.
 */
export async function isDualRightWindow(): Promise<boolean> {
  return (await getLabel()) === "dual-right";
}

/** Broadcast a sync event to all windows after saving data. */
export async function emitSync(module: string) {
  try {
    const { emit } = await import("@tauri-apps/api/event");
    const sender = await getLabel();
    await emit("state-sync", { module, sender });
  } catch {
    // Not in Tauri context — ignore
  }
}

/**
 * Listen for sync events for a specific module.
 * Calls `callback` when another window saves that module's data.
 * Returns an unlisten function for cleanup.
 */
export async function onSyncEvent(
  module: string,
  callback: () => void,
): Promise<() => void> {
  try {
    const { listen } = await import("@tauri-apps/api/event");
    const myLabel = await getLabel();
    const unlisten = await listen<{ module: string; sender: string }>(
      "state-sync",
      (event) => {
        if (
          event.payload.module === module &&
          event.payload.sender !== myLabel
        ) {
          callback();
        }
      },
    );
    return unlisten;
  } catch {
    return () => {};
  }
}

