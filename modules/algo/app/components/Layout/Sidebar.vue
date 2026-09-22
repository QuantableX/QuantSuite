<script setup lang="ts">
/**
 * QuantAlgo's left nav — the shared QSidebar + QNavItem (V3). The unicode
 * glyph icons (⊞ ✎ ⧉ …) are gone: chrome uses 24×24 stroke-path line icons.
 * Settings is a page like any other; the header gear opens the unified modal.
 */
const route = useRoute()
const router = useRouter()

const navItems = [
  { icon: 'M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z', label: 'Dashboard', to: '/algo' },
  { icon: 'M12 2a3 3 0 0 1 3 3v1h2a3 3 0 0 1 3 3v9a3 3 0 0 1-3 3H7a3 3 0 0 1-3-3V9a3 3 0 0 1 3-3h2V5a3 3 0 0 1 3-3z M9 13h.01 M15 13h.01 M9 17h6', label: 'Bots', to: '/algo/bots' },
  { icon: 'M12 20h9 M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z', label: 'Strategies', to: '/algo/strategies' },
  { icon: 'M12 22a10 10 0 1 1 0-20 10 10 0 0 1 0 20z M12 6v6l4 2', label: 'Backtest', to: '/algo/backtest' },
  { icon: 'M4 17l6-5-6-5 M12 19h8', label: 'Terminal', to: '/algo/terminal' },
  { icon: 'M4 19.5A2.5 2.5 0 0 1 6.5 17H20 M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z', label: 'Journal', to: '/algo/journal' },
  { icon: 'M3 3v18h18 M7 14l4-5 3 3 5-7', label: 'Charts', to: '/algo/charts' },
  { icon: 'M17 1l4 4-4 4 M21 5H9 M7 23l-4-4 4-4 M3 19h12', label: 'Exchange', to: '/algo/exchange' },
  { icon: 'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.01a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51h.01a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.01a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z', label: 'Settings', to: '/algo/settings' },
]

function isActive(to: string): boolean {
  if (to === '/algo') return route.path === '/algo'
  return route.path.startsWith(to)
}
</script>

<template>
  <QSidebar>
    <nav class="py-2">
      <QNavItem
        v-for="item in navItems"
        :key="item.to"
        :label="item.label"
        :icon="item.icon"
        :active="isActive(item.to)"
        @click="router.push(item.to)"
      />
    </nav>
  </QSidebar>
</template>
