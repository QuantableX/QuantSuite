<script setup lang="ts">
/**
 * QuantTerminal's left sidebar — the shared QSidebar shell hosting the market
 * cap ladder (2026-08-16). It used to be empty (navigation lives in the header
 * pills); the ladder moved here from the right panel when the Overview page,
 * its only host, was removed — so the top-100 list is now visible on every
 * terminal page instead of one.
 *
 * The header is a two-way switch (2026-09-03): Market Ladder — the top-100
 * list — or Watchlist — the coins the user favorited via the ladder's
 * right-click menu. The choice lives in the watchlist store and persists.
 *
 * `compact` mirrors QRightPanel's rule: below 300px the coin's long name is
 * dropped and only the symbol stays.
 */
import { useWatchlistStore } from '#terminal/stores/watchlist'

const watchlist = useWatchlistStore()

const sidebarWidth = ref(220)
const compact = computed(() => sidebarWidth.value < 300)
</script>

<template>
  <QSidebar
    storage-key="terminal.left"
    resizable
    @update:width="sidebarWidth = $event"
  >
    <template #header>
      <div class="sidebar-header">
        <div class="view-switch" role="tablist" aria-label="Sidebar view">
          <button
            type="button"
            role="tab"
            class="view-tab"
            :class="{ 'view-tab--active': watchlist.view === 'ladder' }"
            :aria-selected="watchlist.view === 'ladder'"
            title="Top coins by market cap"
            @click="watchlist.setView('ladder')"
          >
            Market Ladder
          </button>
          <button
            type="button"
            role="tab"
            class="view-tab"
            :class="{ 'view-tab--active': watchlist.view === 'watchlist' }"
            :aria-selected="watchlist.view === 'watchlist'"
            title="Your favorited coins"
            @click="watchlist.setView('watchlist')"
          >
            Watchlist
          </button>
        </div>
      </div>
    </template>

    <!-- The ladder scrolls its own list, so the QSidebar body must not scroll
         too: `h-full` pins it to the body box and its inner `flex-1` list owns
         the overflow. -->
    <div class="ladder-host">
      <TerminalDashboardMarketCapLadder :compact="compact" />
    </div>
  </QSidebar>
</template>

<style scoped>
.sidebar-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 8px;
  height: 36px;
  border-bottom: 1px solid var(--border);
}

/* Segmented switch — fills the header, one equal tab per view. */
.view-switch {
  display: flex;
  flex: 1;
  min-width: 0;
  gap: 2px;
  padding: 2px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background-color: var(--surface-2);
}

.view-tab {
  flex: 1;
  min-width: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 4px;
  padding: 3px 6px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--text-secondary);
  font-size: 10px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  white-space: nowrap;
  overflow: hidden;
  cursor: pointer;
  transition: background-color 0.15s ease, color 0.15s ease;
}

.view-tab:hover {
  color: var(--text-primary);
}

.view-tab--active {
  background-color: var(--surface-3);
  color: var(--text-primary);
}

.ladder-host {
  height: 100%;
  min-height: 0;
  overflow: hidden;
}
</style>
