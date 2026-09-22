<script setup lang="ts">
/**
 * QuantPlan module shell.
 *
 * Loads the calendars once and keeps them; the occurrences are reloaded
 * whenever the visible range moves, because recurrence is expanded per range
 * in the crate rather than held whole in the browser.
 */
import { inActiveKeepAliveTree, useShortcuts, useTauriEvent } from '@quantsuite/core'
import { useAppStore } from '#plan/stores/app'
import { useCalendarsStore } from '#plan/stores/calendars'
import { useEventsStore } from '#plan/stores/events'

const app = useAppStore()
const calendars = useCalendarsStore()
const events = useEventsStore()
const router = useRouter()

onMounted(async () => {
  await app.loadSettings()
  await calendars.load()
  await events.loadRange(app.range.from, app.range.to)
})

// The range is derived from view + cursor, so one watcher covers navigation,
// view switches and the weekend toggle alike.
watch(
  () => app.range,
  (range) => {
    void events.loadRange(range.from, range.to)
  },
)

// Toggling a calendar changes what the crate returns, so the range has to be
// refetched rather than filtered locally.
watch(() => calendars.visibleIds, () => void events.refresh(), { deep: true })

// Another window can write an event; our own writes already refresh.
useTauriEvent('plan:events-changed', () => void events.refresh())
useTauriEvent('plan:calendars-changed', () => void calendars.load())

useShortcuts([
  { key: 'd', handler: () => app.setView('day') },
  { key: 'w', handler: () => app.setView('week') },
  { key: 'm', handler: () => app.setView('month') },
  { key: 'a', handler: () => app.setView('agenda') },
  { key: 't', handler: () => app.today() },
  { key: 'ArrowLeft', handler: () => app.step(-1) },
  { key: 'ArrowRight', handler: () => app.step(1) },
  {
    key: 'c',
    handler: (e) => {
      e.preventDefault()
      app.editorOpen = true
    },
  },
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
  {
    key: ',',
    ctrl: true,
    handler: (e) => {
      e.preventDefault()
      router.push('/flow/settings')
    },
  },
])

/**
 * V3 warm cache: the module stays mounted in a deactivated <KeepAlive> stage.
 * Coming back after a while, the range is stale — refresh on activation, and
 * on mount only when the stage is actually active (an async-resolved layout
 * can mount into a stage the user has already left).
 */
onMounted(() => {
  if (inActiveKeepAliveTree()) void events.refresh()
})
onActivated(() => void events.refresh())
</script>

<template>
  <div class="qp-shell" data-module="plan">
    <div class="qp-body">
      <!-- V3: shared sidebar shells — 220px, in-flow, collapse via store. -->
      <QSidebar :model-value="app.sidebarLeftOpen" resizable storage-key="plan.left">
        <PlanLayoutLeftSidebar @create="app.editorOpen = true" />
      </QSidebar>
      <main class="qp-main">
        <PlanLayoutAppHeader />
        <div class="qp-content">
          <slot />
        </div>
      </main>
      <QRightPanel :model-value="app.sidebarRightOpen" storage-key="plan.right">
        <PlanLayoutRightSidebar @edit="app.editorOpen = true" />
      </QRightPanel>
    </div>
  </div>
</template>

<style scoped>
.qp-shell {
  display: flex;
  flex-direction: column;
  /* The module fills its container, never the viewport (ARCHITECTURE.md §9). */
  width: 100%;
  height: 100%;
  overflow: hidden;
}

.qp-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.qp-main {
  position: relative;
  display: flex;
  flex-direction: column;
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  background: var(--qp-bg);
}

.qp-content {
  flex: 1;
  min-height: 0;
  overflow: hidden;
}
</style>
