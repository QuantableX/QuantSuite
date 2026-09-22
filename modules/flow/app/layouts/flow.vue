<script setup lang="ts">
import CalendarSurface from '#plan/layouts/plan.vue'
import HabitSurface from '#habit/layouts/habit.vue'

const route = useRoute()
// The suite caches this layout when another module is active. Keep its surface pinned.
const path = ref(route.path)
watch(() => route.path, (next) => {
  if (next === '/flow' || next.startsWith('/flow/')) path.value = next
})
const isHabit = computed(() => path.value.startsWith('/flow/habits'))
const tabs = [
  { to: '/flow', label: 'Calendar' },
  { to: '/flow/habits', label: 'Habits' },
  { to: '/flow/habits/stats', label: 'Progress' },
]
</script>

<template>
  <div class="qflow-shell">
    <QModuleHeader module-id="flow">
      <nav class="qflow-tabs" aria-label="QuantFlow sections">
        <NuxtLink v-for="tab in tabs" :key="tab.to" :to="tab.to" :class="{ 'is-active': path === tab.to }" :aria-current="path === tab.to ? 'page' : undefined">
          {{ tab.label }}
        </NuxtLink>
      </nav>
    </QModuleHeader>
    <component :is="isHabit ? HabitSurface : CalendarSurface" class="qflow-surface">
      <slot />
    </component>
  </div>
</template>

<style scoped>
.qflow-shell { display: flex; flex-direction: column; height: 100%; min-height: 0; color: var(--qss-text); }
.qflow-surface { flex: 1; min-height: 0; }
.qflow-tabs { display: flex; align-items: center; gap: 4px; padding: 0 16px; }
.qflow-tabs a { padding: 5px 12px; border-radius: 6px; color: var(--qss-text-muted); font-size: 12px; text-decoration: none; white-space: nowrap; }
.qflow-tabs a:hover, .qflow-tabs a.is-active { background: var(--qss-bg-card); color: var(--qss-text); }
</style>
