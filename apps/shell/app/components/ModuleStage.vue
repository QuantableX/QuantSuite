<script setup lang="ts">
import { computed, onActivated, ref, shallowRef, watch } from 'vue'
import type { NuxtLayouts } from 'nuxt/app'
import { isModuleEnabled, stageForRoute } from '@quantsuite/core'

/**
 * One cached stage per module (V3 warm cache). app.vue keys this component by
 * module id inside a <KeepAlive>: switching modules deactivates the old stage
 * instead of destroying it, so a module keeps its full component tree — open
 * tabs, terminal buffers, chart state — and reactivation is instant.
 *
 * The catch: KeepAlive-deactivated trees still react to global state, and the
 * router's currentRoute is exactly that. Without pinning, every cached stage
 * would re-resolve the *new* module's layout and page the moment the route
 * changes — each hidden stage mounting its own copy of the active page. So the
 * stage renders a **pinned route**: the last route that belonged to its own
 * module. NuxtPage's `route` prop feeds it to RouterView, and Nuxt's
 * RouteProvider hands it to the page as `useRoute()`, so the cached page never
 * sees foreign routes. NuxtLayout is pinned the same way via `name`.
 */
const props = defineProps<{ moduleId: string | null; stageId: string | null }>()

const router = useRouter()

/** Vue-router route snapshots are immutable per navigation — safe to hold. */
const pinned = shallowRef(router.currentRoute.value)

/** `null` module = the shell's own pages (`/`, `/processes`, `/settings`). */
function belongs(path: string): boolean {
  return stageForRoute(path) === props.stageId
}

watch(router.currentRoute, (r) => {
  // Follow navigation only within our own module; while another module is
  // active this stage is deactivated and stays frozen on its last route.
  if (belongs(r.path)) pinned.value = r
})

/** Shell pages carry no layout — `false` renders NuxtLayout as a passthrough.
 *
 * `keyof NuxtLayouts`, not `LayoutKey`: `<NuxtLayout name>` is typed off the
 * generated `NuxtLayouts` interface, while Nuxt's `LayoutKey` alias widens to
 * `string` here (`.nuxt/types/layouts.d.ts`), so casting to it fails vue-tsc.
 * Route meta is untyped either way, so this stays a cast. */
const layoutName = computed(() => (pinned.value.meta.layout as keyof NuxtLayouts | false | undefined) ?? false)

/**
 * The enter animation is a CSS keyframe that fires on mount (deliberately not
 * a <Transition> — see the app.vue history). A cache hit skips the mount, so
 * replay it by hand: clearing the animation, forcing a reflow, restoring it.
 * `prefers-reduced-motion` still wins — restoring to '' re-applies the
 * stylesheet value, which is `none` under that media query.
 */
const rootEl = ref<HTMLElement | null>(null)
const nuxtApp = useNuxtApp()
onActivated(() => {
  // Nuxt defers its `_route` (what `useRoute()` returns outside pages) until
  // the incoming page resolves in a NuxtPage — but a cache hit resolves no
  // page, so pull it up to date by hand or every `useRoute()` consumer
  // (module layouts' nav highlighting, most visibly) would lag one module
  // behind. Fresh mounts sync themselves via NuxtPage's Suspense resolve.
  ;(nuxtApp._route as { sync?: () => void } | undefined)?.sync?.()

  const el = rootEl.value
  if (!el) return
  el.style.animation = 'none'
  void el.offsetWidth
  el.style.animation = ''
})
</script>

<template>
  <!-- data-module is what every module stylesheet scopes itself to (§9);
       each cached stage keeps its own attribute, so hidden trees stay
       correctly scoped. NuxtLayout is required, not optional: without it a
       module's own layout never renders and only its page content appears. -->
  <div ref="rootEl" class="qss-module-root" :data-module="stageId === 'algo-manual' ? 'systems' : moduleId ?? undefined">
    <NuxtLayout v-if="!moduleId || isModuleEnabled(moduleId)" :name="layoutName">
      <NuxtPage :route="pinned" />
    </NuxtLayout>
  </div>
</template>

<style scoped>
.qss-module-root {
  flex: 1;
  min-height: 0;
  overflow: auto;
  border-radius: var(--qss-radius-lg);
  border: 3px solid var(--qss-border);
  background: var(--qss-bg);
  animation: qss-module-in 150ms ease;
  /*
    Containing block for the modules' `position: fixed` UI — pills, floating
    toggles, modals. They were written when the app WAS the window; without
    this they anchor to the real viewport and drift over the shell chrome
    (offset pills, floating above the drawer). With it they anchor to the
    module panel, get clipped by its rounded edge, and — because this is now
    a stacking context — the drawer, a later sibling, always paints on top.
  */
  transform: translateZ(0);
}

@keyframes qss-module-in {
  from {
    opacity: 0;
  }
}

@media (prefers-reduced-motion: reduce) {
  .qss-module-root {
    animation: none;
  }
}
</style>
