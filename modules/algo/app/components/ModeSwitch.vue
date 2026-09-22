<script setup lang="ts">
import { SlidersHorizontal, Zap } from 'lucide-vue-next'

type Mode = 'manual' | 'automated'
const props = defineProps<{ mode: Mode }>()
const router = useRouter()
const destinations = useState<Record<Mode, string>>('algo-mode-routes', () => ({
  manual: '/algo/manual',
  automated: '/algo',
}))
const lastMode = useState<Mode>('algo-last-mode', () => 'manual')

// Both cached headers observe navigation, but only remember their own mode.
watch(router.currentRoute, (route) => {
  if (route.path !== '/algo' && !route.path.startsWith('/algo/')) return
  const mode = route.path === '/algo/manual' || route.path.startsWith('/algo/manual/')
    ? 'manual' : 'automated'
  if (mode === props.mode) {
    destinations.value[mode] = route.fullPath
    lastMode.value = mode
  }
}, { immediate: true })

function select(mode: Mode) {
  if (mode !== props.mode) void router.push(destinations.value[mode])
}
</script>

<template>
  <div class="algo-mode-center">
    <div class="algo-mode-switch" role="group" aria-label="QuantAlgo mode">
      <button type="button" :aria-pressed="mode === 'manual'" @click="select('manual')">
        <SlidersHorizontal :size="13" :stroke-width="1.8" aria-hidden="true" />
        <span>Manual</span>
      </button>
      <button type="button" :aria-pressed="mode === 'automated'" @click="select('automated')">
        <Zap :size="13" :stroke-width="1.8" aria-hidden="true" />
        <span>Automated</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.algo-mode-center {
  display: flex;
  align-items: center;
  justify-content: center;
  min-width: 0;
  width: 228px;
}
.algo-mode-switch {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 3px;
  width: 100%;
  padding: 3px;
  border: 1px solid color-mix(in srgb, var(--qss-border) 80%, transparent);
  border-radius: 10px;
  background: color-mix(in srgb, var(--qss-bg) 85%, var(--qss-bg-chrome));
  box-shadow: inset 0 1px 3px rgb(0 0 0 / 12%);
}
.algo-mode-switch button {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 7px;
  min-width: 0;
  height: 28px;
  padding: 0 8px;
  border: 1px solid transparent;
  border-radius: 7px;
  background: transparent;
  color: var(--qss-text-secondary);
  font: inherit;
  font-size: 11px;
  font-weight: 600;
  line-height: 1;
  cursor: pointer;
  transition: background 160ms ease, color 160ms ease, box-shadow 160ms ease;
}
.algo-mode-switch button:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.algo-mode-switch button[aria-pressed='true'] {
  border-color: color-mix(in srgb, var(--qss-accent) 70%, var(--qss-text));
  background: linear-gradient(180deg,
    color-mix(in srgb, var(--qss-accent) 85%, var(--qss-text)),
    var(--qss-accent));
  color: var(--qss-accent-ink);
  box-shadow: 0 1px 3px rgb(0 0 0 / 22%), inset 0 1px 0 rgb(255 255 255 / 18%);
}
.algo-mode-switch button:focus-visible {
  outline: 2px solid var(--qss-accent);
  outline-offset: 3px;
}
@media (prefers-reduced-motion: reduce) {
  .algo-mode-switch button { transition: none; }
}
</style>
