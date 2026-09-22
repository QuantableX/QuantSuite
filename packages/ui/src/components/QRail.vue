<script setup lang="ts">
/**
 * The app rail — the left edge of the application (V3).
 *
 * One entry per suite app (modules/apps.json), the group centred vertically
 * in the rail, as LINE ICONS — chart, cpu chip, dev brackets. The PNG logos
 * belong to the module headers, not here (user decision 2026-08-15).
 * Hovering an app icon slides a module flyout out of the icon's right edge,
 * vertically centred on it (user decision 2026-08-20) — the header's Modules
 * button stays as the in-module path.
 */
import { ref } from 'vue'
import { modulesForApp } from '@quantsuite/core'
import type { AppInfo } from '@quantsuite/core'
import { logoFor } from '../logos'
import QAppIcon from './QAppIcon.vue'
import { Settings2 } from 'lucide-vue-next'

defineProps<{
  apps: AppInfo[]
  activeAppId?: string | null
  /** Highlights the current module inside its app's flyout. */
  activeModuleId?: string | null
  /** Running process count — colours the process-center dot. */
  running?: number
  processesActive?: boolean
}>()
const emit = defineEmits<{
  (e: 'select', id: string): void
  (e: 'open-module', id: string): void
  (e: 'settings'): void
  (e: 'processes'): void
}>()

/** The flyout is pure :hover CSS, so after a click it would linger over the
 * stage until the pointer happens to leave. Suppress it per app on click;
 * mouseleave re-arms. */
const suppressed = ref<string | null>(null)

/** A mouse click leaves the button focused, and `:focus-within` (there for
 * keyboard users) would pin the flyout open once mouseleave re-arms the
 * hover. Drop focus on click — after a click, hover alone governs. */
function dropFocus() {
  const el = document.activeElement
  if (el instanceof HTMLElement) el.blur()
}

function pickApp(appId: string) {
  suppressed.value = appId
  dropFocus()
  emit('select', appId)
}
function pickModule(appId: string, moduleId: string) {
  suppressed.value = appId
  dropFocus()
  emit('open-module', moduleId)
}

</script>

<template>
  <nav class="qss-rail" aria-label="Apps">
    <div
      v-for="a in apps"
      :key="a.id"
      class="qss-rail-item"
      :class="{ 'is-suppressed': suppressed === a.id }"
      @mouseleave="suppressed = null"
    >
      <!-- No `title` here: the flyout already names the app, and the native
           tooltip popping up next to it reads as a stray second block. -->
      <button
        class="qss-rail-btn"
        :class="{ 'is-active': a.id === activeAppId }"
        :aria-label="a.title"
        @click="pickApp(a.id)"
      >
        <QAppIcon :app-id="a.id" :size="18" />
      </button>

      <!-- Module flyout: slides out of the icon's right edge, centred on it. -->
      <div class="qss-rail-flyout" role="menu" :aria-label="`${a.title} modules`">
        <div class="qss-rail-flyout-panel">
          <span class="qss-rail-flyout-title">{{ a.title }}</span>
          <button
            v-for="m in modulesForApp(a.id)"
            :key="m.id"
            class="qss-rail-flyout-item"
            :class="{ 'is-active': m.id === activeModuleId }"
            role="menuitem"
            @click="pickModule(a.id, m.id)"
          >
            <!-- The wordmark IS the row — same treatment as the selector modal. -->
            <img v-if="logoFor(m.id)" :src="logoFor(m.id)" :alt="m.title" class="qss-rail-flyout-logo" />
            <span v-else>{{ m.title }}</span>
            <span v-if="m.status === 'stub'" class="qss-rail-flyout-soon">soon</span>
          </button>
        </div>
      </div>
    </div>

    <div class="qss-rail-tail">
      <!-- Process center: every child process the suite owns (E4). -->
      <button
        class="qss-rail-btn"
        :class="{ 'is-active': processesActive }"
        :title="running ? `Processes — ${running} running` : 'Processes'"
        @click="emit('processes')"
      >
        <span class="qss-rail-status-dot" :class="{ 'is-running': (running ?? 0) > 0 }" />
      </button>

      <button class="qss-rail-btn" title="Settings" @click="emit('settings')">
        <Settings2 :size="18" :stroke-width="1.5" aria-hidden="true" />
      </button>
    </div>
  </nav>
</template>

<style scoped>
.qss-rail {
  /* z-index over the stage: the module flyouts overhang the panel. */
  position: relative;
  z-index: 40;
  width: var(--qss-rail-w, 60px);
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
  /* Side gutters derive from the rail width so the button pills stay 44px
     no matter how much width the rail lends to the stage's right gutter. */
  padding: 8px calc((var(--qss-rail-w, 60px) - 44px) / 2);
  background: var(--qss-bg-chrome, var(--qss-bg-raised));
  user-select: none;
}

.qss-rail-item {
  position: relative;
  width: 100%;
}

/* Full rail width, symmetric gutters — the hit area is the whole strip. */
.qss-rail-btn {
  position: relative;
  flex-shrink: 0; /* the column must scroll conceptually, never crush */
  display: grid;
  place-items: center;
  width: 100%;
  height: 44px;
  border: none;
  border-radius: 10px;
  background: transparent;
  color: var(--qss-text-secondary);
  cursor: pointer;
  transition: background 120ms ease, color 120ms ease;
}
.qss-rail-btn:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.qss-rail-btn.is-active {
  background: var(--qss-bg-card);
  color: var(--qss-text);
}
/* Active marker: a short bar hugging the rail's left edge, IDE-style.
   Offset mirrors the rail's side gutter, or the bar leaves the strip. */
.qss-rail-btn.is-active::before {
  content: '';
  position: absolute;
  left: calc((var(--qss-rail-w, 60px) - 44px) / -2);
  top: 12px;
  bottom: 12px;
  width: 2px;
  border-radius: 2px;
  background: var(--qss-accent);
}

/* The dashboard's serif suite mark — same glyph as the titlebar wordmark. */
.qss-rail-mark {
  font: 600 20px/1 var(--qss-font-brand, Georgia, serif);
}

.qss-rail-spacer {
  flex: 1;
}

/* Takes the rail's leftover height; its buttons pin to the bottom. */
.qss-rail-tail {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  justify-content: flex-end;
  gap: 4px;
}

/* ---- Module flyout ------------------------------------------------------ */

/* Positioning shell: anchored to the icon's right edge, vertically centred
   on it. The left padding is an invisible hover bridge so the pointer can
   travel from icon to panel without the :hover dropping. */
.qss-rail-flyout {
  position: absolute;
  left: 100%;
  top: 50%;
  transform: translateY(-50%);
  padding-left: 12px;
  z-index: 60;
  opacity: 0;
  pointer-events: none;
  transition: opacity 130ms ease;
}
.qss-rail-item:not(.is-suppressed):hover .qss-rail-flyout,
.qss-rail-item:not(.is-suppressed):focus-within .qss-rail-flyout {
  opacity: 1;
  pointer-events: auto;
}

.qss-rail-flyout-panel {
  position: relative; /* anchors the notch */
  min-width: 172px;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 6px;
  border-radius: 12px;
  background: var(--qss-bg-raised);
  border: 1px solid var(--qss-border-subtle, var(--qss-border));
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.4);
  /* The slide: grows out of the icon's right edge, centre-left origin. */
  transform: translateX(-8px) scale(0.96);
  transform-origin: left center;
  transition: transform 130ms ease;
}
.qss-rail-item:not(.is-suppressed):hover .qss-rail-flyout-panel,
.qss-rail-item:not(.is-suppressed):focus-within .qss-rail-flyout-panel {
  transform: translateX(0) scale(1);
}

/* Notch pointing back at the icon, on the panel's vertical centre. */
.qss-rail-flyout-panel::before {
  content: '';
  position: absolute;
  left: -5px;
  top: 50%;
  width: 9px;
  height: 9px;
  transform: translateY(-50%) rotate(45deg);
  background: inherit;
  border-left: 1px solid var(--qss-border-subtle, var(--qss-border));
  border-bottom: 1px solid var(--qss-border-subtle, var(--qss-border));
}

.qss-rail-flyout-title {
  padding: 4px 9px 3px;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.09em;
  text-transform: uppercase;
  color: var(--qss-text-muted);
  white-space: nowrap;
}

.qss-rail-flyout-item {
  display: flex;
  align-items: center;
  gap: 8px;
  text-align: left;
  padding: 8px 9px;
  border: none;
  border-radius: 8px;
  background: transparent;
  color: var(--qss-text-muted);
  font-family: inherit;
  font-size: 12.5px;
  white-space: nowrap;
  cursor: pointer;
  transition: background 100ms ease, color 100ms ease;
}
.qss-rail-flyout-item:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.qss-rail-flyout-item.is-active {
  background: var(--qss-bg-card);
  color: var(--qss-text);
}

/* Wordmark rows — the PNG logo replaces the text title (selector parity). */
.qss-rail-flyout-logo {
  /* Height-only, same token as the selector rows — see shell.css
     --qss-wordmark-h-menu. The panel is content-sized, so no width clamp:
     one would shrink QUANTTERMINAL below QUANTALGO again. */
  display: block;
  height: var(--qss-wordmark-h-menu, 14px);
  width: auto;
  max-width: none;
  object-fit: contain;
  object-position: left;
}

.qss-rail-flyout-soon {
  margin-left: auto;
  padding: 1px 6px;
  border-radius: 999px;
  background: var(--qss-bg-hover);
  color: var(--qss-text-muted);
  font-size: 9px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.qss-rail-status-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: currentColor;
}
.qss-rail-status-dot.is-running {
  background: var(--qss-success);
  opacity: 1;
}
</style>
