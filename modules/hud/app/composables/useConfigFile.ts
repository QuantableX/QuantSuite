/**
 * Serialized read-modify-write on the shared HUD config file.
 * Every module keeps its state under its own key in one JSON blob, so two
 * writers that read the same pre-state each drop the other's change. All writes
 * run through one promise chain and re-read the file inside it, never before.
 */

let _writeQueue: Promise<void> = Promise.resolve();

async function _write(module: string, mutate: (config: any) => void) {
  const { invoke } = await import("@tauri-apps/api/core");
  const raw = await invoke<string>("plugin:hud|load_config");
  const config = raw ? JSON.parse(raw) : {};
  mutate(config);
  await invoke("plugin:hud|save_config", { config: JSON.stringify(config) });
  emitSync(module);
}

/**
 * Merge this module's keys into the stored config and save it. `mutate` runs
 * when the write's turn comes, so it must read the state it persists at that
 * moment. Rejects like a plain save — callers keep their own error handling.
 */
export function updateConfigFile(
  module: string,
  mutate: (config: any) => void,
): Promise<void> {
  const write = _writeQueue.then(() => _write(module, mutate));
  // A failed write must not poison the chain for the writers behind it
  _writeQueue = write.catch(() => {});
  return write;
}
