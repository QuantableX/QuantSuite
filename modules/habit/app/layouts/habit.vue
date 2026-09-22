<script setup lang="ts">
/**
 * QuantHabit module shell — same skeleton as QuantFinance: shared header,
 * QSidebar / main / QRightPanel, warm-cache reload on activation.
 */
import { inActiveKeepAliveTree, useShortcuts, useTauriEvent } from '@quantsuite/core'
import { useAppStore } from '#habit/stores/app'
import { useHabitsStore } from '#habit/stores/habits'

const app = useAppStore()
const habits = useHabitsStore()

onMounted(async () => {
  await app.loadSettings()
})

// Another surface (the tray, an agent through the MCP bridge) can change the
// data; our own writes already reload.
useTauriEvent('habit:changed', () => void habits.load())

useShortcuts([
  // An unspecified modifier is a wildcard and the first match wins, so the
  // Ctrl+Shift+B binding has to come before the plain Ctrl+B one.
  {
    key: 'B',
    ctrl: true,
    shift: true,
    handler: (e) => {
      e.preventDefault()
      app.toggleSidebar('right')
    },
  },
  {
    key: 'b',
    ctrl: true,
    handler: (e) => {
      e.preventDefault()
      app.toggleSidebar('left')
    },
  },
])

/**
 * V3 warm cache: the module stays mounted in a deactivated <KeepAlive> stage.
 * Coming back later the day may have changed — reload on activation, and on
 * mount only when the stage is actually active (an async-resolved layout can
 * mount into a stage the user has already left).
 */
onMounted(() => {
  if (inActiveKeepAliveTree()) void habits.load()
})
onActivated(() => void habits.load())
</script>

<template>
  <div class="qh-shell" data-module="habit">
    <div class="qh-body">
      <!-- V3: shared sidebar shells — 220px, in-flow, collapse via store. -->
      <QSidebar :model-value="app.sidebarLeftOpen" resizable storage-key="habit.left">
        <HabitLayoutLeftSidebar />
      </QSidebar>
      <main class="qh-main">
        <HabitLayoutAppHeader />
        <div class="qh-content">
          <slot />
        </div>
      </main>
      <QRightPanel :model-value="app.sidebarRightOpen" storage-key="habit.right">
        <HabitLayoutRightSidebar />
      </QRightPanel>
    </div>
    <!-- The one habit dialog — opened from the header (+ New habit) and from
         the right panel's edit buttons, via the store. -->
    <HabitModal />
  </div>
</template>

<style scoped>
.qh-shell {
  display: flex;
  flex-direction: column;
  /* The module fills its container, never the viewport (ARCHITECTURE.md §9). */
  width: 100%;
  height: 100%;
  overflow: hidden;
}

.qh-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.qh-main {
  position: relative;
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  background: var(--qh-bg);
}

.qh-content {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}
</style>
