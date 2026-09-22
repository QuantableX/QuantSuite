<script setup lang="ts">
import { Channel, invoke } from '@tauri-apps/api/core'
import { Download, RefreshCw } from 'lucide-vue-next'

interface UpdateInfo { current_version: string; version: string | null; notes: string | null }
interface Progress { downloaded: number; total: number | null; installing: boolean }

const config = useRuntimeConfig()
const currentVersion = ref(String(config.public.appVersion))
const update = ref<UpdateInfo | null>(null)
const busy = ref(false)
const status = ref('Updates include every app in the suite.')
const error = ref(false)

async function checkForUpdate() {
  if (busy.value) return
  update.value = null
  error.value = false
  if (!window.__TAURI_INTERNALS__) {
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
    error.value = true
    status.value = `Could not check for updates: ${e}`
  } finally {
    busy.value = false
  }
}

async function installUpdate() {
  const version = update.value?.version
  if (!version || busy.value) return
  busy.value = true
  error.value = false
  status.value = 'Downloading update…'
  const onProgress = new Channel<Progress>()
  onProgress.onmessage = progress => {
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
</script>

<template>
  <section class="suite-updates" aria-labelledby="suite-updates-title" :aria-busy="busy">
    <div class="suite-updates-heading">
      <h2 id="suite-updates-title">QuantSuite</h2><span>v{{ currentVersion }}</span>
    </div>
    <p role="status" aria-live="polite" :class="{ error }">{{ status }}</p>
    <details v-if="update?.version && update.notes">
      <summary>What’s new</summary><p class="release-notes">{{ update.notes }}</p>
    </details>
    <p v-if="update?.version" class="restart-note">Save your work before installing. All suite windows will restart.</p>
    <div class="suite-updates-actions">
      <button :disabled="busy" @click="checkForUpdate"><RefreshCw :size="14" />Check for updates</button>
      <button v-if="update?.version" :disabled="busy" @click="installUpdate"><Download :size="14" />Install &amp; restart</button>
    </div>
  </section>
</template>

<style scoped>
.suite-updates { padding: 20px; border: 1px solid var(--qss-border-subtle); border-radius: 14px; background: var(--qss-bg-raised); }
.suite-updates-heading { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; }
h2 { margin: 0; font-size: 15px; font-weight: 600; }
span, p, summary { font-size: 12px; color: var(--qss-text-secondary); }
p { margin: 10px 0; line-height: 1.6; overflow-wrap: anywhere; }
.error { color: var(--qss-danger, #e87979); }
.suite-updates-actions { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px; }
button { display: inline-flex; align-items: center; gap: 7px; padding: 8px 10px; border: 1px solid var(--qss-border-subtle); border-radius: 7px; background: var(--qss-bg); color: var(--qss-text); font: inherit; font-size: 12px; cursor: pointer; }
button:hover:not(:disabled) { background: var(--qss-bg-hover); }
button:disabled { opacity: 0.55; cursor: wait; }
summary { cursor: pointer; }
.release-notes { white-space: pre-wrap; max-height: 180px; overflow: auto; }
</style>
