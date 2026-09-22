<script setup lang="ts">
import { useBotsStore } from '#algo/stores/bots'
import { useStrategiesStore } from '#algo/stores/strategies'
import { useExchangeStore } from '#algo/stores/exchange'
import { useAppStore } from '#algo/stores/app'
import { formatPnl } from '#algo/utils/format'
import { useEventListener } from '@vueuse/core'
import { inActiveKeepAliveTree } from '@quantsuite/core'

const bots = useBotsStore()
const strategies = useStrategiesStore()
const exchangeStore = useExchangeStore()
const appStore = useAppStore()

// ── Sidebar collapse ──

const leftCollapsed = ref(false)
const rightCollapsed = ref(false)

// ── Status computeds (every bot at once, PLAN-QUANTALGO §3.4) ──

const statusColor = computed(() => {
  if (bots.runningCount > 0) return 'var(--qa-accent)'
  if (bots.hasError) return 'var(--qa-error)'
  return 'var(--qa-text-muted)'
})

const statusLabel = computed(() => {
  if (bots.runningCount > 0) {
    const live = bots.liveRunningCount ? ` · ${bots.liveRunningCount} live` : ''
    return `${bots.runningCount} running${live}`
  }
  if (bots.hasError) return 'Error'
  return bots.list.length ? 'All stopped' : 'No bots'
})

// Paper and live never share a number (PLAN-QUANTALGO §3.4); the live
// chips appear once anything live exists.
const paperPnlText = computed(() => formatPnl(bots.pnlPaper.today).text)
const paperPnlPositive = computed(() => bots.pnlPaper.today >= 0)
const livePnlText = computed(() => formatPnl(bots.pnlLive.today).text)
const livePnlPositive = computed(() => bots.pnlLive.today >= 0)
const showLive = computed(() => bots.hasLive)

// ── Error banner ──

const showErrorBanner = computed(() => bots.lastError !== null)
const errorText = computed(() => {
  const err = bots.lastError
  if (!err) return ''
  return err.botId ? `${bots.name(err.botId)}: ${err.message}` : err.message
})

function dismissError() {
  bots.lastError = null
}

// ── App bootstrap + keyboard shortcuts ──

onMounted(async () => {
  // Bootstrap all stores in parallel so titlebar/sidebar are never stale
  await Promise.all([
    appStore.loadSettings(),
    bots.init(),
    strategies.load(),
    exchangeStore.load(),
  ])
})

// The stage only ever unmounts on teardown — that is where the bots store's
// listeners and the day-rollover timer belong, not on deactivation: the bots
// keep trading while the module is hidden.
onUnmounted(() => {
  bots.dispose()
})

// V3 warm cache: the stage keeps this layout mounted while another module is
// active, so the shortcut listener hangs off a reactive target that goes null
// on deactivation — otherwise Ctrl+B/Ctrl+Shift+B keep toggling algo's
// sidebars (and preventDefault-ing) from inside notes or canvas. The flag is set
// in BOTH onMounted and onActivated: layouts load async and mount after the
// stage's activation flush, so onActivated alone would miss the first visit —
// but that same late mount can land in a stage the user has already left, where
// setting the flag would arm algo's keys inside another module and no
// onDeactivated would ever clear it, hence the guard.
const isActive = ref(false)
const activeWindow = computed(() => (isActive.value ? window : null))

onMounted(() => { if (inActiveKeepAliveTree()) isActive.value = true })
onActivated(() => {
  isActive.value = true
  // QuantScript's forge creates strategies from indicators while this stage
  // is warm: re-read the list on every return, or the sidebar shows a
  // strategies list that is one creation behind.
  void strategies.load()
})
onDeactivated(() => { isActive.value = false })

useEventListener<KeyboardEvent>(activeWindow, 'keydown', (e) => {
  if (e.ctrlKey && !e.shiftKey && e.key === 'b') {
    e.preventDefault()
    leftCollapsed.value = !leftCollapsed.value
  }
  if (e.ctrlKey && e.shiftKey && e.key === 'B') {
    e.preventDefault()
    rightCollapsed.value = !rightCollapsed.value
  }
})
</script>

<template>
  <div class="app-shell">
    <!-- The shared module header (V3): 51px, logo cap / status center /
         selector + settings cap. -->
    <QModuleHeader module-id="algo">
      <div class="titlebar-center">
        <div class="status-left">
          <span
            class="status-dot"
            :class="{ 'status-dot--pulse': bots.runningCount > 0 }"
            :style="{ background: statusColor }"
          />
          <span class="status-label">Bots:&nbsp;</span>
          <span class="status-value" :style="{ color: statusColor }">{{ statusLabel }}</span>
          <span v-if="bots.runningCount > 0" class="strategy-chip mono">{{ formatPnl(bots.equityByMode.paper).text.replace('+', '') }} paper</span>
          <span v-if="bots.equityByMode.live > 0" class="strategy-chip strategy-chip--live mono">{{ formatPnl(bots.equityByMode.live).text.replace('+', '') }} live</span>
        </div>
        <AlgoModeSwitch mode="automated" />
        <div class="status-right">
          <span class="metric-pill" :class="paperPnlPositive ? 'metric-pill--pos' : 'metric-pill--neg'">
            PAPER: {{ paperPnlText }}
          </span>
          <span v-if="showLive" class="metric-pill" :class="livePnlPositive ? 'metric-pill--pos' : 'metric-pill--neg'">
            LIVE: {{ livePnlText }}
          </span>
          <span class="metric-pill">OPEN: {{ bots.openByMode.paper }}<template v-if="showLive"> · {{ bots.openByMode.live }} LIVE</template></span>
        </div>
      </div>
    </QModuleHeader>

    <!-- Error Banner -->
    <div v-if="showErrorBanner" class="error-banner">
      <span class="error-banner__icon">&#9888;</span>
      <span class="error-banner__msg">{{ errorText }}</span>
      <button class="error-banner__dismiss" @click="dismissError">&times;</button>
    </div>

    <!-- Body -->
    <div class="app-body">
      <AlgoLayoutSidebar v-show="!leftCollapsed" />
      <main class="main-content">
        <slot />
      </main>
      <QRightPanel v-show="!rightCollapsed" storage-key="algo.right">
        <AlgoLayoutRightSidebar />
      </QRightPanel>
    </div>

  </div>
</template>

<style scoped>
.app-shell {
  display: flex;
  flex-direction: column;
  /* Was 100vw/100vh. The module fills its container, never the viewport. */
  width: 100%;
  height: 100%;
  overflow: hidden;
}

/* ── Center: status (inside the shared QModuleHeader) ── */

.titlebar-center {
  flex: 1;
  min-width: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  align-items: center;
  gap: 12px;
  padding: 0 16px;
}

.status-left {
  min-width: 0;
  overflow-x: auto;
  white-space: nowrap;
  scrollbar-width: none;
  display: flex;
  align-items: center;
  gap: 8px;
}

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.status-dot--pulse {
  animation: pulse-dot 2s ease-in-out infinite;
}

.status-label {
  font-size: 12px;
  color: var(--qa-text-muted);
}

.status-value {
  font-size: 12px;
  font-weight: 600;
}

.strategy-chip {
  flex-shrink: 0;
  padding: 2px 8px;
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  border: 1px solid var(--qa-border);
  border-radius: 9999px;
  color: var(--qa-text-secondary);
}

.status-right {
  min-width: 0;
  overflow-x: auto;
  white-space: nowrap;
  scrollbar-width: none;
  justify-self: end;
  max-width: 100%;
  display: flex;
  align-items: center;
  gap: 8px;
}

.metric-pill {
  flex-shrink: 0;
  padding: 3px 10px;
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  border: 1px solid var(--qa-border);
  border-radius: 9999px;
  color: var(--qa-text-secondary);
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', Menlo, Consolas, monospace;
}

.metric-pill--pos {
  color: var(--qa-success);
  border-color: color-mix(in srgb, var(--qa-success) 30%, transparent);
}

.metric-pill--neg {
  color: var(--qa-error);
  border-color: color-mix(in srgb, var(--qa-error) 30%, transparent);
}

/* ── Error Banner ── */

.error-banner {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 16px;
  background: color-mix(in srgb, var(--qa-error) 15%, var(--qa-bg));
  border-bottom: 1px solid color-mix(in srgb, var(--qa-error) 40%, transparent);
  font-size: 13px;
  color: var(--qa-error);
  flex-shrink: 0;
}

.error-banner__icon {
  font-size: 14px;
  flex-shrink: 0;
}

.error-banner__msg {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.error-banner__dismiss {
  background: none;
  border: none;
  color: var(--qa-error);
  font-size: 18px;
  cursor: pointer;
  padding: 0 4px;
  line-height: 1;
  opacity: 0.7;
  transition: opacity 0.15s;
}

.error-banner__dismiss:hover {
  opacity: 1;
}

/* ── Body ── */

.app-body {
  display: flex;
  flex: 1;
  min-height: 0;
}

.main-content {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  padding: 24px 32px;
  background: var(--qa-bg);
}
.strategy-chip--live {
  color: var(--qa-error);
}
</style>
