<script setup lang="ts">
/**
 * QuantFinance module shell.
 *
 * The plan is a standing monthly picture, so there is nothing to page through
 * and nothing to reload on navigation — one load, then writes refresh it.
 */
import { inActiveKeepAliveTree, useShortcuts, useTauriEvent } from '@quantsuite/core'
import { useAppStore } from '#finance/stores/app'
import { usePlanStore } from '#finance/stores/plan'

const app = useAppStore()
const plan = usePlanStore()
const router = useRouter()
const route = useRoute()
const isFunds = computed(() => route.path === '/finance/funds')

onMounted(async () => {
  await app.loadSettings()
  await plan.load()
})

// Another window can edit the plan; our own writes already reload.
useTauriEvent('finance:plan-changed', () => void plan.load())

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
  {
    key: ',',
    ctrl: true,
    handler: (e) => {
      e.preventDefault()
      router.push('/finance/settings')
    },
  },
])

/**
 * V3 warm cache: the module stays mounted in a deactivated <KeepAlive> stage.
 * Coming back later the figures may be stale — reload on activation, and on
 * mount only when the stage is actually active (an async-resolved layout can
 * mount into a stage the user has already left).
 */
onMounted(() => {
  if (inActiveKeepAliveTree()) void plan.load()
})
onActivated(() => void plan.load())
</script>

<template>
  <div class="qf-shell">
    <FinanceLayoutAppHeader />
    <div class="qf-body">
      <!-- V3: shared sidebar shells — 220px, in-flow, collapse via store. -->
      <QSidebar :model-value="app.sidebarLeftOpen" resizable storage-key="finance.left" @update:model-value="app.settings.sidebarLeftOpen = $event">
        <FinanceFundsLeftSidebar v-if="isFunds" />
        <FinanceLayoutLeftSidebar v-else />
      </QSidebar>
      <main class="qf-main">
        <slot />
      </main>
      <QRightPanel :model-value="app.sidebarRightOpen" storage-key="finance.right" @update:model-value="app.settings.sidebarRightOpen = $event">
        <FinanceFundsRightSidebar v-if="isFunds" />
        <FinanceLayoutRightSidebar v-else />
      </QRightPanel>
    </div>
  </div>
</template>

<style scoped>
.qf-shell {
  display: flex;
  flex-direction: column;
  /* The module fills its container, never the viewport (ARCHITECTURE.md §9). */
  width: 100%;
  height: 100%;
  overflow: hidden;
}

.qf-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.qf-main {
  position: relative;
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  background: var(--qf-bg);
}
</style>
