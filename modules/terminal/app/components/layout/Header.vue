<script setup lang="ts">
/**
 * QuantTerminal's header — the shared QModuleHeader (V3) with the module's
 * nav pills in the center slot. 51px, logo cap, selector + settings cap:
 * identical geometry to every other module.
 */
</script>

<template>
  <QModuleHeader module-id="terminal">
    <nav class="header-nav-links flex items-center gap-1 ml-3" aria-label="Market sections">
      <NuxtLink to="/terminal" class="btn-ghost text-xs">Terminal</NuxtLink>
      <NuxtLink to="/terminal/metrics" class="btn-ghost text-xs">Metrics</NuxtLink>
      <NuxtLink to="/terminal/journal" class="btn-ghost text-xs">Journal</NuxtLink>
      <NuxtLink to="/terminal/dojo" class="btn-ghost text-xs">Dojo</NuxtLink>
    </nav>
  </QModuleHeader>
</template>

<style scoped>
.header-nav-links {
  align-self: center;
  min-width: 0;
  overflow-x: auto;
  scrollbar-width: none;
}
.header-nav-links a { flex-shrink: 0; white-space: nowrap; }

/*
  `exact`, not `router-link-active`: every nav target lives under `/terminal`,
  so the non-exact class would keep "Terminal" lit on every sub-page.
*/
.header-nav-links .router-link-exact-active {
  background-color: var(--surface-2);
  color: var(--text-primary);
}
</style>
