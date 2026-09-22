<script setup lang="ts">
/**
 * Suite-wide screenshot button (V3.1) — captures the app window natively
 * (Rust/xcap) and copies the PNG to the clipboard. Lives in every module
 * header's right cap and on the Dashboard.
 *
 * The command is hosted by the systems plugin (it grew there first); it
 * captures the whole suite window, so it works from any module. The feedback
 * badge only ever shows AFTER the capture completes — never while capturing,
 * or the screenshot would photograph the badge.
 */
import { onUnmounted, ref } from 'vue'

const capturing = ref(false)
const shotState = ref<'idle' | 'done' | 'error'>('idle')
const shotError = ref('')
let shotTimer: ReturnType<typeof setTimeout> | null = null

async function captureScreenshot() {
  if (capturing.value) return
  capturing.value = true
  shotState.value = 'idle'
  await new Promise((r) => requestAnimationFrame(() => r(null)))
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('plugin:systems|screenshot_to_clipboard')
    shotState.value = 'done'
  } catch (err) {
    console.error('Screenshot failed:', err)
    shotError.value = typeof err === 'string' ? err : ((err as Error)?.message ?? String(err))
    shotState.value = 'error'
  } finally {
    capturing.value = false
    if (shotTimer) clearTimeout(shotTimer)
    shotTimer = setTimeout(() => {
      shotState.value = 'idle'
    }, shotState.value === 'error' ? 9000 : 1600)
  }
}

onUnmounted(() => {
  if (shotTimer) clearTimeout(shotTimer)
})
</script>

<template>
  <div class="qshot">
    <button
      type="button"
      class="qshot-btn"
      :title="capturing ? 'Capturing…' : 'Copy a screenshot to the clipboard'"
      @click="captureScreenshot"
    >
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M23 19a2 2 0 0 1-2 2H3a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2-3h6l2 3h4a2 2 0 0 1 2 2z" />
        <circle cx="12" cy="13" r="4" />
      </svg>
    </button>
    <span v-if="shotState !== 'idle'" class="qshot-badge" :class="`qshot-badge--${shotState}`">
      {{ shotState === 'done' ? 'Copied' : 'Failed' }}
    </span>
    <div v-if="shotState === 'error' && shotError" class="qshot-error">{{ shotError }}</div>
  </div>
</template>

<style scoped>
.qshot {
  position: relative;
  display: flex;
  align-items: center;
}

.qshot-btn {
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--qss-text-secondary);
  cursor: pointer;
  transition: background var(--qss-dur-fast) var(--qss-ease-out), color var(--qss-dur-fast) var(--qss-ease-out);
}
.qshot-btn:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}

.qshot-badge {
  position: absolute;
  top: calc(100% + 4px);
  right: 0;
  z-index: 1001;
  padding: 1px 6px;
  border-radius: 999px;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.02em;
  background: var(--qss-bg-card);
  border: 1px solid var(--qss-border);
  color: var(--qss-text-secondary);
  pointer-events: none;
}
.qshot-badge--done {
  color: var(--qss-success);
  border-color: color-mix(in srgb, var(--qss-success) 50%, var(--qss-border));
}
.qshot-badge--error {
  color: var(--qss-error);
  border-color: color-mix(in srgb, var(--qss-error) 50%, var(--qss-border));
}

.qshot-error {
  position: absolute;
  top: calc(100% + 26px);
  right: 0;
  z-index: 1000;
  max-width: 420px;
  padding: 8px 10px;
  border-radius: 8px;
  background: var(--qss-bg-card);
  border: 1px solid color-mix(in srgb, var(--qss-error) 50%, var(--qss-border));
  color: var(--qss-text);
  font-size: 11px;
  font-weight: 500;
  line-height: 1.4;
  white-space: normal;
  text-align: left;
  box-shadow: 0 16px 40px rgb(0 0 0 / 0.4);
  pointer-events: none;
}
</style>
