<script setup lang="ts">
/**
 * The module header (V3) — the ONE 51px, three-section bar every module
 * renders at the top of its panel. Geometry is QuantCode's, verbatim:
 *
 *   [ 220px logo cap | center, flex:1 | 220px right cap ]
 *
 * Left cap: the module's logo (22px tall), clicking it goes to the module
 * home. Center: whatever the module needs — search, tabs, status — via the
 * default slot, `align-items: stretch` so full-height corner wedges (the
 * QuantCode sidebar toggles) keep working. Right cap: the module selector and
 * the settings button; the `right` slot may prepend extra controls.
 *
 * The settings button dispatches `qss:settings` with the module id — the
 * unified QSettingsModal, mounted once in the shell, listens.
 */
import { computed } from 'vue'
import { moduleById } from '@quantsuite/core'
import { logoFor } from '../logos'

const props = withDefaults(
  defineProps<{
    moduleId: string
    /** Logo click target; defaults to the module's registry route. Pass an
     * empty string to suppress navigation (listen to `brand` instead). */
    homeRoute?: string
    showSelector?: boolean
    showSettings?: boolean
  }>(),
  {
    homeRoute: undefined,
    // Absent boolean props cast to `false` in Vue — these must default true
    // explicitly or the right cap renders empty.
    showSelector: true,
    showSettings: true,
  }
)
const emit = defineEmits<{
  (e: 'brand'): void
}>()

const module = computed(() => moduleById(props.moduleId))
const logo = computed(() => logoFor(props.moduleId))
const selectorOn = computed(() => props.showSelector)
const settingsOn = computed(() => props.showSettings)

function goHome() {
  emit('brand')
  const route = props.homeRoute ?? module.value?.route
  if (route) window.dispatchEvent(new CustomEvent('qss:navigate', { detail: { route } }))
}

function openSettings() {
  window.dispatchEvent(new CustomEvent('qss:settings', { detail: { module: props.moduleId } }))
}
</script>

<template>
  <header class="qmh">
    <!-- Left cap: logo, 220px — the same width as the sidebar below it. -->
    <button class="qmh-brand" :title="module?.title" @click="goHome">
      <img v-if="logo" :src="logo" :alt="module?.title" class="qmh-logo" draggable="false" />
      <span v-else class="qmh-brand-text">{{ module?.title }}</span>
      <slot name="left" />
    </button>

    <!-- Center: module-owned content. Stretch, so corner wedges span the bar. -->
    <div class="qmh-center">
      <slot />
    </div>

    <!-- Right cap: mirrors the logo cap over the right sidebar. Screenshot,
         selector and settings live here (V3) — the cap is no longer empty. -->
    <div class="qmh-actions">
      <slot name="right" />
      <QScreenshotButton />
      <QModuleSelector v-if="selectorOn" :module-id="moduleId" />
      <button v-if="settingsOn" class="qmh-settings" title="Settings" aria-label="Module settings" @click="openSettings">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <circle cx="12" cy="12" r="3" />
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.01a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h.01a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.01a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
        </svg>
      </button>
    </div>
  </header>
</template>

<style scoped>
.qmh {
  flex-shrink: 0;
  display: flex;
  align-items: stretch;
  height: var(--qss-module-header-h, 51px);
  background: var(--qss-module-chrome, var(--qss-bg-raised));
  border-bottom: 1px solid var(--qss-border-subtle);
}

.qmh-brand {
  width: var(--qss-sidebar-w, 220px);
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 0 16px;
  border: none;
  border-right: 1px solid var(--qss-border);
  background: transparent;
  color: var(--qss-text);
  cursor: pointer;
}

.qmh-logo {
  /* Size by HEIGHT only — the PNGs are cropped to their ink, so one cap height
     means one letter size across every module (shell.css --qss-wordmark-h).
     No width clamp: a clamp would shrink the long marks again. */
  height: var(--qss-wordmark-h, 18px);
  width: auto;
  max-width: none;
  object-fit: contain;
}

.qmh-brand-text {
  font-size: 13px;
  font-weight: 600;
}

.qmh-center {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: stretch;
}

.qmh-actions {
  width: var(--qss-sidebar-w, 220px);
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 6px;
  padding: 0 10px;
  border-left: 1px solid var(--qss-border);
}

.qmh-settings {
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
.qmh-settings:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.qmh-brand:focus-visible,
.qmh-settings:focus-visible { outline: 2px solid var(--qss-accent); outline-offset: -4px; }
</style>
