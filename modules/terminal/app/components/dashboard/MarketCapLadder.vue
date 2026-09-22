<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { ChevronDown, ChevronLeft, ChevronRight, RefreshCw, Star } from 'lucide-vue-next'
import { useWatchlistStore, type MarketCapCoin } from '#terminal/stores/watchlist'

const props = withDefaults(defineProps<{
  compact?: boolean
}>(), {
  compact: false,
})

/**
 * Two views share this component (2026-09-03): the market ladder — one page of
 * 100 coins — and the watchlist — the coins favorited via the row context
 * menu. The sidebar header switches `watchlist.view`; the rows, columns, the
 * change-mode toggle and the refresh cycle are the same for both.
 */
const watchlist = useWatchlistStore()
const isWatchlist = computed(() => watchlist.view === 'watchlist')

const mcPage = ref(1)
const mcCoins = ref<MarketCapCoin[]>([])
const mcLoading = ref(false)
const mcError = ref(false)
const mcProvider = ref<'coingecko' | 'coinmarketcap'>('coingecko')
const mcProviderOpen = ref(false)
const mcCountdown = ref(60)

/** The rows on screen: the loaded ladder page, or the favorites. */
const rows = computed<MarketCapCoin[]>(() => (isWatchlist.value ? watchlist.coins : mcCoins.value))

/**
 * The change column shows the rolling 24h number (from the marketcap
 * provider) or, toggled via the column header, the change since the 00:00 UTC
 * open (from Binance, per symbol). Coins without a Binance USDT pair have no
 * daily number and render as "—".
 */
const changeMode = ref<'24h' | 'daily'>('24h')
const dailyChanges = ref<Record<string, number>>({})

let mcCountdownTimer: ReturnType<typeof setInterval> | null = null

async function fetchMarketCap(page: number) {
  mcLoading.value = true
  mcError.value = false
  try {
    const data = await invoke<MarketCapCoin[]>('plugin:terminal|get_marketcap', { page, provider: mcProvider.value })
    mcCoins.value = data
    watchlist.updateSnapshots(data)
    if (changeMode.value === 'daily') void fetchDailyChanges()
  } catch (e) {
    console.warn('Failed to fetch market cap data:', e)
    mcError.value = true
  } finally {
    mcLoading.value = false
  }
}

/**
 * Refresh the favorites: fetch every ladder page a favorite was last seen on
 * (its rank / 100) and merge the snapshots. The backend caches pages for five
 * minutes, so this is cheap even with favorites spread over several pages. A
 * coin that drifted to another page keeps its last snapshot until it is seen
 * again — acceptable at the edges of a page, and it never blocks the list.
 */
async function fetchWatchlist() {
  if (watchlist.coins.length === 0) return fetchMarketCap(mcPage.value)
  const pages = [...new Set(watchlist.coins.map((c) => Math.max(1, Math.ceil(c.rank / 100))))].sort((a, b) => a - b)
  mcLoading.value = true
  try {
    for (const page of pages) {
      const data = await invoke<MarketCapCoin[]>('plugin:terminal|get_marketcap', { page, provider: mcProvider.value })
      watchlist.updateSnapshots(data)
      // Keep the ladder fresh too when its page is among the fetched ones.
      if (page === mcPage.value) mcCoins.value = data
    }
    if (changeMode.value === 'daily') void fetchDailyChanges()
  } catch (e) {
    // Rows keep their last snapshot — a stale price beats an empty list.
    console.warn('Failed to refresh watchlist:', e)
  } finally {
    mcLoading.value = false
  }
}

/** Refresh whichever view is showing. */
function refresh() {
  return isWatchlist.value ? fetchWatchlist() : fetchMarketCap(mcPage.value)
}

async function fetchDailyChanges() {
  if (rows.value.length === 0) return
  try {
    dailyChanges.value = await invoke<Record<string, number>>('plugin:terminal|get_daily_change', {
      symbols: rows.value.map((c) => c.symbol),
    })
  } catch (e) {
    // Keep whatever we had — rows without a number fall back to "—".
    console.warn('Failed to fetch daily changes:', e)
  }
}

function toggleChangeMode() {
  changeMode.value = changeMode.value === '24h' ? 'daily' : '24h'
  if (changeMode.value === 'daily') void fetchDailyChanges()
}

function changeFor(coin: MarketCapCoin): number | null {
  if (changeMode.value === '24h') return coin.change
  return dailyChanges.value[coin.symbol] ?? null
}

function changeText(coin: MarketCapCoin): string {
  const c = changeFor(coin)
  if (c === null) return '—'
  return `${c >= 0 ? '+' : ''}${c.toFixed(2)}%`
}

function switchProvider(provider: 'coingecko' | 'coinmarketcap') {
  mcProvider.value = provider
  mcProviderOpen.value = false
  mcPage.value = 1
  void refresh()
}

function goToPage(page: number) {
  mcPage.value = page
  fetchMarketCap(page)
}

function formatPrice(price: number): string {
  if (price >= 1) return price.toLocaleString(undefined, { minimumFractionDigits: 2, maximumFractionDigits: 2 })
  if (price >= 0.01) return price.toFixed(4)
  return price.toFixed(6)
}

function formatMarketCap(mc: number): string {
  if (mc >= 1e12) return `$${(mc / 1e12).toFixed(2)}T`
  if (mc >= 1e9) return `$${(mc / 1e9).toFixed(2)}B`
  if (mc >= 1e6) return `$${(mc / 1e6).toFixed(1)}M`
  return `$${mc.toLocaleString()}`
}

function refreshMarketCap() {
  void refresh()
  mcCountdown.value = 60
}

function startCountdown() {
  if (mcCountdownTimer) clearInterval(mcCountdownTimer)
  mcCountdown.value = 60
  mcCountdownTimer = setInterval(() => {
    mcCountdown.value--
    if (mcCountdown.value <= 0) {
      refreshMarketCap()
    }
  }, 1000)
}

// Switching to the watchlist refreshes it right away — its snapshots may be
// as old as the last time a favorite's page was on screen. The daily-change
// numbers are per-row, so they are re-fetched for the new rows as well.
watch(isWatchlist, (showing) => {
  closeMenu()
  if (showing) void fetchWatchlist()
  else if (changeMode.value === 'daily') void fetchDailyChanges()
})

// ── Row context menu ──
//
// Right-click on a coin → "Add to watchlist" / "Remove from watchlist". The
// menu is teleported to <body> and fixed-positioned, so the scrolling list
// cannot clip it; it closes on any click, Escape, a scroll of the list, or a
// right-click elsewhere (captured, so a right-click on another row closes this
// menu first and then opens its own).

const menu = ref<{ x: number; y: number; coin: MarketCapCoin } | null>(null)
const MENU_WIDTH = 200
const MENU_HEIGHT = 72

function openMenu(e: MouseEvent, coin: MarketCapCoin) {
  const x = Math.max(8, Math.min(e.clientX, window.innerWidth - MENU_WIDTH - 8))
  const y = Math.max(8, Math.min(e.clientY, window.innerHeight - MENU_HEIGHT - 8))
  menu.value = { x, y, coin }
}

function closeMenu() {
  menu.value = null
}

function toggleFavorite() {
  if (!menu.value) return
  watchlist.toggle(menu.value.coin)
  closeMenu()
}

function onWindowKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') closeMenu()
}

// V3 warm cache: refresh and count down only while visible. Start in BOTH
// onMounted and onActivated — async pages mount after the stage's activation
// flush, so onActivated alone misses the first visit; the timer guard makes
// the overlap safe. That same late mount can land in a stage the user has
// already left, where the timer guard then blocks the real activation and the
// countdown stays dead — so onMounted starts it only inside an active tree.
function ensureLadderRunning() {
  if (mcCountdownTimer) return
  void refresh()
  startCountdown()
}

onMounted(() => {
  window.addEventListener('click', closeMenu)
  window.addEventListener('blur', closeMenu)
  window.addEventListener('keydown', onWindowKeydown)
  window.addEventListener('contextmenu', closeMenu, true)
  if (inActiveKeepAliveTree()) ensureLadderRunning()
})
onActivated(ensureLadderRunning)

onDeactivated(() => {
  closeMenu()
  if (mcCountdownTimer) {
    clearInterval(mcCountdownTimer)
    mcCountdownTimer = null
  }
})

onBeforeUnmount(() => {
  window.removeEventListener('click', closeMenu)
  window.removeEventListener('blur', closeMenu)
  window.removeEventListener('keydown', onWindowKeydown)
  window.removeEventListener('contextmenu', closeMenu, true)
  if (mcCountdownTimer) clearInterval(mcCountdownTimer)
})
</script>

<template>
  <div class="flex flex-col h-full">
    <!-- Controls bar -->
    <div class="flex items-center justify-between px-3 py-2 border-b shrink-0" style="border-color: var(--border)">
      <div class="flex items-center gap-1.5">
        <div class="relative">
          <button
            class="flex items-center gap-0.5 px-1.5 py-0.5 text-[9px] font-mono rounded transition-colors"
            style="background-color: var(--surface-2); color: var(--text-secondary)"
            @click="mcProviderOpen = !mcProviderOpen"
          >
            {{ mcProvider === 'coingecko' ? 'CG' : 'CMC' }}
            <ChevronDown class="w-2.5 h-2.5" />
          </button>
          <div
            v-if="mcProviderOpen"
            class="absolute left-0 top-full mt-1 z-30 rounded-md py-1 min-w-[120px] shadow-lg"
            style="background-color: var(--surface-2); border: 1px solid var(--border)"
            @mouseleave="mcProviderOpen = false"
          >
            <button
              class="block w-full text-left px-3 py-1 text-[11px] transition-colors hover:brightness-125"
              :style="{
                color: mcProvider === 'coingecko' ? 'var(--accent)' : 'var(--text-primary)',
                backgroundColor: mcProvider === 'coingecko' ? 'rgba(41,98,255,0.1)' : 'transparent',
              }"
              @click="switchProvider('coingecko')"
            >
              CoinGecko
            </button>
            <button
              class="block w-full text-left px-3 py-1 text-[11px] transition-colors hover:brightness-125"
              :style="{
                color: mcProvider === 'coinmarketcap' ? 'var(--accent)' : 'var(--text-primary)',
                backgroundColor: mcProvider === 'coinmarketcap' ? 'rgba(41,98,255,0.1)' : 'transparent',
              }"
              @click="switchProvider('coinmarketcap')"
            >
              CoinMarketCap
            </button>
          </div>
        </div>
      </div>
      <div class="flex items-center gap-1">
        <span class="text-[9px] font-mono" style="color: var(--muted)">{{ mcCountdown }}s</span>
        <button
          class="p-0.5 rounded transition-colors"
          style="color: var(--text-secondary); cursor: pointer"
          :class="{ 'animate-spin': mcLoading }"
          @click="refreshMarketCap"
        >
          <RefreshCw class="w-3 h-3" />
        </button>
        <!-- Watchlist: no pages — the favorites count takes the slot. -->
        <span
          v-if="isWatchlist"
          class="flex items-center gap-1 text-[10px] font-mono px-1"
          style="color: var(--text-secondary)"
          title="Coins in your watchlist"
        >
          <Star class="w-2.5 h-2.5" style="color: var(--accent)" fill="currentColor" />
          {{ watchlist.count }}
        </span>
        <template v-else>
          <button
            class="p-0.5 rounded transition-colors"
            :style="{ color: mcPage > 1 ? 'var(--text-primary)' : 'var(--muted)', cursor: mcPage > 1 ? 'pointer' : 'default' }"
            :disabled="mcPage <= 1"
            @click="mcPage > 1 && goToPage(mcPage - 1)"
          >
            <ChevronLeft class="w-3.5 h-3.5" />
          </button>
          <span class="text-[10px] font-mono px-1" style="color: var(--text-secondary)">
            {{ (mcPage - 1) * 100 + 1 }}-{{ mcPage * 100 }}
          </span>
          <button
            class="p-0.5 rounded transition-colors"
            style="color: var(--text-primary); cursor: pointer"
            @click="goToPage(mcPage + 1)"
          >
            <ChevronRight class="w-3.5 h-3.5" />
          </button>
        </template>
      </div>
    </div>

    <!-- Column headers -->
    <div class="flex items-center px-3 py-1 text-[10px] uppercase tracking-wider shrink-0" style="color: var(--muted); border-bottom: 1px solid var(--border)">
      <span class="w-7 text-right mr-2">#</span>
      <span class="flex-1">Name</span>
      <span class="w-16 text-right">Price</span>
      <button
        class="w-14 text-right uppercase tracking-wider bg-transparent border-0 p-0 cursor-pointer transition-colors"
        style="font: inherit; letter-spacing: inherit"
        :style="{ color: changeMode === 'daily' ? 'var(--accent)' : 'var(--muted)' }"
        :title="changeMode === '24h'
          ? 'Rolling 24h change — click for the daily change (since 00:00 UTC)'
          : 'Daily change since 00:00 UTC — click for the rolling 24h change'"
        @click="toggleChangeMode"
      >
        {{ changeMode === '24h' ? '24h' : 'Daily' }}
      </button>
    </div>

    <!-- Empty watchlist -->
    <div v-if="isWatchlist && rows.length === 0" class="flex-1 flex flex-col items-center justify-center gap-2 px-4 text-center">
      <Star class="w-5 h-5" style="color: var(--muted)" />
      <span class="text-xs" style="color: var(--text-secondary)">Your watchlist is empty</span>
      <span class="text-[10px] leading-relaxed" style="color: var(--muted)">
        Right-click a coin in the Market Ladder and choose "Add to watchlist".
      </span>
      <button
        class="text-[11px] px-3 py-1 rounded"
        style="background-color: var(--surface-3); color: var(--text-primary)"
        @click="watchlist.setView('ladder')"
      >
        Open Market Ladder
      </button>
    </div>

    <!-- Loading -->
    <div v-else-if="mcLoading && rows.length === 0" class="flex-1 flex items-center justify-center">
      <span class="text-xs" style="color: var(--text-secondary)">Loading...</span>
    </div>

    <!-- Error -->
    <div v-else-if="mcError && rows.length === 0" class="flex-1 flex flex-col items-center justify-center gap-2">
      <span class="text-xs" style="color: var(--text-secondary)">Failed to load data</span>
      <button
        class="text-[11px] px-3 py-1 rounded"
        style="background-color: var(--surface-3); color: var(--text-primary)"
        @click="refreshMarketCap"
      >
        Retry
      </button>
    </div>

    <!-- Coin list -->
    <div v-else class="flex-1 overflow-y-auto" @scroll.passive="closeMenu">
      <div
        v-for="coin in rows"
        :key="isWatchlist ? coin.symbol : `${coin.rank}:${coin.symbol}`"
        class="coin-row flex items-center px-2 py-1.5 text-xs border-b hover:brightness-110 transition-colors cursor-default"
        :class="{ 'coin-row--menu': menu?.coin.symbol === coin.symbol }"
        style="border-color: var(--border)"
        @contextmenu.prevent="openMenu($event, coin)"
      >
        <span class="w-5 text-left mr-1.5 font-mono text-[10px] shrink-0" style="color: var(--muted)">{{ coin.rank }}</span>
        <div class="flex-1 min-w-0 flex items-center gap-1.5 overflow-hidden">
          <span class="font-semibold whitespace-nowrap" style="color: var(--text-primary)">{{ coin.symbol }}</span>
          <Star
            v-if="watchlist.has(coin.symbol)"
            class="w-2.5 h-2.5 shrink-0"
            style="color: var(--accent)"
            fill="currentColor"
            aria-label="In watchlist"
          />
          <span v-if="!props.compact" class="truncate text-[10px]" style="color: var(--text-secondary)">{{ coin.name }}</span>
        </div>
        <span class="w-16 text-right mr-2 font-mono text-[11px] shrink-0" style="color: var(--text-primary)">${{ formatPrice(coin.price) }}</span>
        <span
          class="w-14 text-right font-mono text-[11px] shrink-0"
          :style="{
            color: changeFor(coin) === null
              ? 'var(--muted)'
              : changeFor(coin)! >= 0 ? 'var(--positive)' : 'var(--negative)',
          }"
        >
          {{ changeText(coin) }}
        </span>
      </div>
    </div>

    <Teleport to="body">
      <div
        v-if="menu"
        class="coin-ctx"
        role="menu"
        :style="{ left: menu.x + 'px', top: menu.y + 'px' }"
        @contextmenu.prevent
      >
        <div class="coin-ctx-title">
          <span class="coin-ctx-symbol">{{ menu.coin.symbol }}</span>
          <span class="coin-ctx-name">{{ menu.coin.name }}</span>
        </div>
        <button type="button" class="coin-ctx-item" role="menuitem" @click="toggleFavorite">
          <Star class="w-3 h-3 shrink-0" :fill="watchlist.has(menu.coin.symbol) ? 'currentColor' : 'none'" />
          {{ watchlist.has(menu.coin.symbol) ? 'Remove from watchlist' : 'Add to watchlist' }}
        </button>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.coin-row--menu {
  background-color: var(--surface-2);
}

.coin-ctx {
  position: fixed;
  z-index: 9999;
  min-width: 200px;
  padding: 4px;
  border: 1px solid var(--border);
  border-radius: 8px;
  background-color: var(--surface-2);
  box-shadow: 0 8px 24px rgb(0 0 0 / 0.35);
}

.coin-ctx-title {
  display: flex;
  align-items: baseline;
  gap: 6px;
  min-width: 0;
  padding: 4px 10px 6px;
  overflow: hidden;
}

.coin-ctx-symbol {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-primary);
}

.coin-ctx-name {
  flex: 1;
  min-width: 0;
  font-size: 10px;
  color: var(--muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.coin-ctx-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 5px 10px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--text-primary);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}

.coin-ctx-item:hover {
  background-color: var(--surface-3);
}

.coin-ctx-item svg {
  color: var(--accent);
}
</style>
