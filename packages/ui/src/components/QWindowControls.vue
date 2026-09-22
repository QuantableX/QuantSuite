<script setup lang="ts">
import { onMounted, onUnmounted, ref } from 'vue'
import { currentWindowLabel, qs } from '@quantsuite/core'

/**
 * Minimize / maximize / close for the suite window.
 *
 * v2 (PLAN-V2 §2): lives in exactly one place — the shell titlebar. Modules
 * never render window controls; the isolation guard fails any that try.
 *
 * Styling uses `currentColor` rather than fixed tokens so it inherits the
 * bar's text colour.
 *
 * Close hides the resident window to the tray; additional windows close.
 */
withDefaults(defineProps<{ height?: string }>(), { height: '100%' })

const isTauri = ref(false)
const isMaximized = ref(false)
const isMainWindow = ref(true)
const opening = ref(false)
const openError = ref('')
let unlisten: (() => void) | undefined
let resizeTimer: ReturnType<typeof setTimeout> | null = null

async function win() {
  const { getCurrentWindow } = await import('@tauri-apps/api/window')
  return getCurrentWindow()
}

async function minimize() {
  ;(await win()).minimize()
}
async function toggleMaximize() {
  const w = await win()
  await w.toggleMaximize()
  isMaximized.value = await w.isMaximized()
}
async function close() {
  ;(await win()).close()
}

async function newWindow() {
  if (opening.value) return
  opening.value = true
  openError.value = ''
  try {
    await qs.core.windowNew()
  } catch (error) {
    openError.value = `Could not open a new window: ${error instanceof Error ? error.message : String(error)}`
  } finally {
    opening.value = false
  }
}

onMounted(async () => {
  if (typeof window === 'undefined' || !('__TAURI_INTERNALS__' in window)) return
  isTauri.value = true
  isMainWindow.value = currentWindowLabel() === 'main'
  const w = await win()
  isMaximized.value = await w.isMaximized()
  unlisten = await w.onResized(() => {
    // Resized fires continuously while an edge is dragged; the maximized state
    // can only change at the end of the gesture, so ask once, trailing.
    if (resizeTimer) clearTimeout(resizeTimer)
    resizeTimer = setTimeout(async () => {
      isMaximized.value = await w.isMaximized()
    }, 100)
  })
})

onUnmounted(() => {
  unlisten?.()
  if (resizeTimer) clearTimeout(resizeTimer)
})
</script>

<template>
  <div v-if="isTauri" class="qss-wc" :style="{ height }">
    <div v-if="openError" class="qss-wc-error" role="alert" data-no-drag>
      <span>{{ openError }}</span>
      <button type="button" aria-label="Dismiss error" @click="openError = ''">×</button>
    </div>
    <button
      class="qss-wc-btn"
      title="New window"
      aria-label="New window"
      :disabled="opening"
      :aria-busy="opening"
      @click="newWindow"
    >
      <svg width="15" height="15" viewBox="0 0 16 16" fill="none" aria-hidden="true">
        <path d="M5.5 3.5v-1h8v8h-1M2.5 5.5h8v8h-8zM4.5 9.5h4M6.5 7.5v4" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
    </button>
    <button class="qss-wc-btn" title="Minimize" @click="minimize">
      <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
        <path d="M2 6h8" stroke="currentColor" stroke-width="1.2" />
      </svg>
    </button>
    <button class="qss-wc-btn" :title="isMaximized ? 'Restore' : 'Maximize'" @click="toggleMaximize">
      <svg v-if="!isMaximized" width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
        <rect x="1.5" y="1.5" width="9" height="9" rx="1" stroke="currentColor" stroke-width="1.2" fill="none" />
      </svg>
      <svg v-else width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
        <rect x="2.5" y="3.5" width="7" height="7" rx="1" stroke="currentColor" stroke-width="1.2" fill="none" />
        <path d="M4.5 3.5V2.5a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v4a1 1 0 0 1-1 1H8.5" stroke="currentColor" stroke-width="1.2" fill="none" />
      </svg>
    </button>
    <button class="qss-wc-btn qss-wc-close" :title="isMainWindow ? 'Hide to tray' : 'Close window'" @click="close">
      <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
        <path d="M2.5 2.5l7 7M9.5 2.5l-7 7" stroke="currentColor" stroke-width="1.2" />
      </svg>
    </button>
  </div>
</template>

<style scoped>
.qss-wc {
  position: relative;
  display: flex;
  align-items: stretch;
  flex-shrink: 0;
  -webkit-app-region: no-drag;
}

.qss-wc-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 46px;
  height: 100%;
  padding: 0;
  border: none;
  background: transparent;
  color: inherit;
  opacity: 0.9;
  cursor: pointer;
  transition: background 0.1s, opacity 0.1s;
}
.qss-wc-btn:hover {
  background: color-mix(in srgb, currentColor 12%, transparent);
  opacity: 1;
}
.qss-wc-btn:active {
  background: color-mix(in srgb, currentColor 18%, transparent);
}
.qss-wc-btn:disabled {
  opacity: 0.4;
  cursor: wait;
}
.qss-wc-btn:focus-visible {
  outline: 1px solid currentColor;
  outline-offset: -4px;
}
.qss-wc-error {
  position: absolute;
  top: 100%;
  right: 8px;
  z-index: 100;
  display: flex;
  gap: 12px;
  width: 320px;
  padding: 12px;
  border: 1px solid var(--qss-warning);
  border-radius: 8px;
  background: var(--qss-bg-raised);
  color: var(--qss-text);
  font: 12px/1.5 var(--qss-font-sans);
  overflow-wrap: anywhere;
  user-select: text;
}
.qss-wc-error button {
  align-self: flex-start;
  border: 0;
  background: none;
  color: inherit;
  cursor: pointer;
}

.qss-wc-close:hover {
  background: #e81123;
  color: #fff;
  opacity: 1;
}
.qss-wc-close:active {
  background: #c50f1f;
  color: #fff;
}
</style>
