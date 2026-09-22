/**
 * The suite's one update channel, as shared state for the shell.
 *
 * Settings → General shows the version and the manual check; the home topbar
 * shows a download button while an update is waiting. Both read this state,
 * so the check that runs once per start (plugins/suite-updates.client.ts)
 * lights up the topbar button without the settings ever being opened.
 */
import { Channel, invoke } from '@tauri-apps/api/core'

interface UpdateInfo { current_version: string; version: string | null; notes: string | null }
interface Progress { downloaded: number; total: number | null; installing: boolean }

const currentVersion = ref('')
const update = ref<UpdateInfo | null>(null)
const busy = ref(false)
const status = ref('Updates include every app in the suite.')
const error = ref(false)
let startupCheck: Promise<void> | null = null

function inTauri(): boolean {
  return import.meta.client && '__TAURI_INTERNALS__' in window
}

async function check() {
  if (busy.value) return
  update.value = null
  error.value = false
  if (!inTauri()) {
    status.value = 'Open the desktop app to check for updates.'
    return
  }
  busy.value = true
  status.value = 'Checking for updates…'
  try {
    const info = await invoke<UpdateInfo>('suite_check_for_update')
    currentVersion.value = info.current_version
    update.value = info
    status.value = info.version ? `Version ${info.version} is available.` : 'You’re on the latest version.'
  } catch (e) {
    // Only Settings shows this; the topbar reacts to an available update alone.
    error.value = true
    status.value = `Could not check for updates: ${e}`
  } finally {
    busy.value = false
  }
}

async function install() {
  const version = update.value?.version
  if (!version || busy.value) return
  busy.value = true
  error.value = false
  status.value = 'Downloading update…'
  const onProgress = new Channel<Progress>()
  onProgress.onmessage = (progress) => {
    status.value = progress.installing
      ? 'Verifying and installing… QuantSuite will restart.'
      : progress.total
        ? `Downloading update… ${Math.min(100, Math.round(progress.downloaded / progress.total * 100))}%`
        : `Downloading update… ${(progress.downloaded / 1048576).toFixed(1)} MB`
  }
  try {
    await invoke('suite_install_update', { version, onProgress })
    status.value = 'Restarting QuantSuite…'
  } catch (e) {
    update.value = null
    error.value = true
    status.value = `Could not install the update: ${e}`
  } finally {
    busy.value = false
  }
}

/** Once per start, and only in the main window — suite windows share the backend lock. */
function checkOnStart(): Promise<void> {
  startupCheck ??= (async () => {
    if (!inTauri()) return
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    if (getCurrentWindow().label !== 'main') return
    await check()
  })()
  return startupCheck
}

export function useSuiteUpdates() {
  if (!currentVersion.value) currentVersion.value = String(useRuntimeConfig().public.appVersion)
  return {
    currentVersion: readonly(currentVersion),
    update: readonly(update),
    available: computed(() => !!update.value?.version),
    busy: readonly(busy),
    status: readonly(status),
    error: readonly(error),
    check,
    install,
    checkOnStart,
  }
}
