<script setup lang="ts">
/**
 * QuantFinance's header — the shared QModuleHeader (V3) with the two tabs.
 *
 * There is no month picker: the plan is a standing monthly picture, not a
 * month you page through.
 */
const route = useRoute()

/** 24x24 stroke paths, per PLAN-V3 3 - no emoji, no glyphs. */
const TABS = [
  { to: '/finance', label: 'Plan', icon: 'M4 6h16 M4 12h16 M4 18h10' },
  { to: '/finance/flow', label: 'Flow', icon: 'M4 5v14 M4 8h6c3 0 3-3 6-3h4 M4 15h6c3 0 3 4 6 4h4' },
  { to: '/finance/funds', label: 'Funds', icon: 'M3 8h18v12H3z M3 8l9-5 9 5 M8 12v4 M12 12v4 M16 12v4' },
]

const isActive = (to: string) => (to === '/finance' ? route.path === to : route.path.startsWith(to))
</script>

<template>
  <QModuleHeader module-id="finance">
    <div class="qf-hd">
      <span class="qf-hd__what">{{ route.path === '/finance/funds' ? 'Funds & savings plans' : 'Monthly plan' }}</span>

      <nav class="qf-hd__tabs">
        <NuxtLink
          v-for="tab in TABS"
          :key="tab.to"
          :to="tab.to"
          class="qf-hd__tab"
          :class="{ 'is-active': isActive(tab.to) }"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path :d="tab.icon" />
          </svg>
          <span>{{ tab.label }}</span>
        </NuxtLink>
      </nav>
    </div>
  </QModuleHeader>
</template>

<style scoped>
.qf-hd {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 0 16px;
}

.qf-hd__what {
  color: var(--qf-text-muted);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qf-hd__tabs {
  display: flex;
  flex-shrink: 0;
  gap: 2px;
}

.qf-hd__tab {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 4px 9px;
  border-radius: var(--qf-radius);
  color: var(--qf-text-muted);
  font-size: 12px;
  text-decoration: none;
}

.qf-hd__tab:hover {
  background: var(--qf-bg-hover);
  color: var(--qf-text-secondary);
}

.qf-hd__tab.is-active {
  background: var(--qf-bg-card);
  color: var(--qf-text);
}
</style>
