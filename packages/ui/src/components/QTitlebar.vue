<script setup lang="ts">
/**
 * The suite titlebar — the window's top row, for **everything**.
 *
 * v2 (PLAN-V2 §2): there is exactly one bar in the application. Modules never
 * draw window chrome again — no Home buttons, no window controls, no drag
 * regions. This bar carries the brand and the window controls, and it is
 * present on the start screen and inside every module alike.
 *
 * It carries no workspace chip (docs/PLAN-WORKSPACES.md, 2026-08-26). A
 * workspace binds QuantCode and nothing else — QuantZen, QuantView and
 * QuantAgent do not scope to one — so a selector in the suite-wide bar claimed
 * a reach it never had. QuantCode's file explorer has the switcher; the
 * dashboard has the list.
 *
 * Dragging uses `v-drag-window`, not `data-tauri-drag-region`: the attribute
 * only fires when the carrying element is the click target, and this bar is
 * covered by its children (ARCHITECTURE.md §10).
 */
const emit = defineEmits<{ (e: 'home'): void }>()
</script>

<template>
  <header class="qss-titlebar" v-drag-window>
    <!-- The app icon sits in a rail-wide box, centred — it lines up exactly
         with the module rail below it. The QuantableX mark alone is the home
         button (V3.1); the wordmark beside it is just the brand, not a
         control. The rail carries no separate Dashboard entry. -->
    <span class="qss-brand">
      <button class="qss-brand-logo" title="Home" aria-label="Home" @click="emit('home')">
        <img src="/quantsuite-icon.png" alt="" draggable="false" />
      </button>
      <span class="qss-brand-name">QuantSuite</span>
    </span>

    <!-- No search trigger and no version tag here — the bar stays clean
         (user decision 2026-08-20). The palette is Ctrl+K, or the launcher
         field QuantConsole draws in its own header. -->
    <span class="qss-spacer" />

    <QWindowControls />
  </header>
</template>

<style scoped>
.qss-titlebar {
  height: var(--qss-titlebar-h, 38px);
  flex-shrink: 0;
  display: flex;
  align-items: stretch;
  gap: 10px;
  background: var(--qss-bg-chrome, var(--qss-bg-raised));
  color: var(--qss-text-secondary);
  user-select: none;
}

.qss-brand {
  display: flex;
  align-items: center;
  flex-shrink: 0;
  color: var(--qss-text);
}
.qss-brand-logo {
  padding: 0;
  border: none;
  background: none;
  border-radius: 8px;
  cursor: pointer;
  transition: background 120ms ease;
}
.qss-brand-logo:hover {
  background: var(--qss-bg-hover);
}
/* Rail-wide, icon centred — vertically continuous with the rail below. */
.qss-brand-logo {
  display: grid;
  place-items: center;
  width: var(--qss-rail-w, 60px);
  height: 100%;
}
.qss-brand-logo img {
  width: 20px;
  height: 20px;
  object-fit: contain;
}
.qss-brand-name {
  /* A breath of air off the rail edge — aligned intent, not flush contact. */
  margin-left: 6px;
  font: 600 12.5px/1 var(--qss-font-sans);
  letter-spacing: -0.01em;
}

.qss-spacer {
  flex: 1;
}
</style>
