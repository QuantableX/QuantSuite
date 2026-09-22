<script setup lang="ts">
/**
 * QuantSystems' header — the shared QModuleHeader (V3) with the system and
 * view switchers in the center slot. The screenshot button is suite chrome
 * now (QScreenshotButton, in every module header) — nothing module-specific
 * remains here.
 */
import { useAppStore } from '#systems/stores/app'
import { useSystemsStore } from '#systems/stores/systems'
import { useActiveView } from '#systems/composables/useActiveView'
import type { EngineView } from '#systems/types'

const app = useAppStore()
const systems = useSystemsStore()
const router = useRouter()
const { systemId, view } = useActiveView()

const switcherOpen = ref(false)
const viewMenuOpen = ref(false)
const switcherRef = ref<HTMLElement | null>(null)
const viewMenuRef = ref<HTMLElement | null>(null)

const activeSystem = computed(() => systems.byId(systemId.value))

const views: { id: EngineView; label: string }[] = [
  { id: 'settings', label: 'Settings' },
  { id: 'live', label: 'Live' },
  { id: 'backtest', label: 'Backtest' },
]
const activeViewLabel = computed(() => views.find(v => v.id === view.value)?.label ?? 'View')

function selectView(id: EngineView) {
  viewMenuOpen.value = false
  void router.push(`/algo/manual/${systemId.value}/${id}`)
}

onClickOutside(switcherRef, () => { switcherOpen.value = false })
onClickOutside(viewMenuRef, () => { viewMenuOpen.value = false })

function selectSystem(id: string) {
  switcherOpen.value = false
  const target = systems.byId(id)
  if (!target || target.status !== 'ready') return
  app.setActiveSystem(id)
  void router.push(`/algo/manual/${id}/${app.viewFor(id)}`)
}
</script>

<template>
  <QModuleHeader module-id="algo" home-route="/algo/manual">
    <div class="qs-titlebar__meta">
      <div ref="switcherRef" class="qs-switcher">
        <button class="qs-switcher__btn" @click.stop="switcherOpen = !switcherOpen">
          <span class="qs-switcher__short">{{ activeSystem?.short ?? '—' }}</span>
          <span class="qs-switcher__name">{{ activeSystem?.name ?? 'Select system' }}</span>
          <svg class="qs-switcher__caret" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="6 9 12 15 18 9" /></svg>
        </button>
        <div v-if="switcherOpen" class="qs-switcher__menu" @click.stop>
          <button
            v-for="s in systems.systems"
            :key="s.id"
            class="qs-switcher__item"
            :class="{ 'qs-switcher__item--disabled': s.status !== 'ready' }"
            @click="selectSystem(s.id)"
          >
            <span class="qs-switcher__item-short">{{ s.short }}</span>
            <span class="qs-switcher__item-name">{{ s.name }}</span>
            <span v-if="s.status !== 'ready'" class="qs-switcher__soon">soon</span>
          </button>
        </div>
      </div>

      <AlgoModeSwitch mode="manual" />

      <div ref="viewMenuRef" class="qs-switcher qs-viewsel">
        <button class="qs-switcher__btn qs-viewsel__btn" @click.stop="viewMenuOpen = !viewMenuOpen">
          <span class="qs-viewsel__label">{{ activeViewLabel }}</span>
          <svg class="qs-switcher__caret" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="6 9 12 15 18 9" /></svg>
        </button>
        <div v-if="viewMenuOpen" class="qs-switcher__menu qs-viewsel__menu" @click.stop>
          <button
            v-for="v in views"
            :key="v.id"
            class="qs-switcher__item qs-viewsel__item"
            :class="{ 'qs-viewsel__item--active': view === v.id }"
            @click="selectView(v.id)"
          >
            {{ v.label }}
          </button>
        </div>
      </div>
    </div>
  </QModuleHeader>
</template>

<style scoped>
.qs-titlebar__meta {
  flex: 1;
  min-width: 0;
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto minmax(0, 1fr);
  align-items: center;
  gap: 12px;
  padding: 0 16px;
}

.qs-switcher {
  min-width: 0;
  position: relative;
}

.qs-switcher__btn {
  max-width: 100%;
  white-space: nowrap;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 5px 12px;
  border: 1px solid var(--qs-border);
  border-radius: 999px;
  background: var(--qs-bg-card);
  color: var(--qs-text);
  cursor: pointer;
  transition: border-color var(--qs-transition), background var(--qs-transition);
}

.qs-switcher__btn:hover {
  border-color: var(--qs-accent);
  background: var(--qs-bg-hover);
}

.qs-switcher__short {
  font-weight: 700;
  font-size: 11px;
  letter-spacing: 0.05em;
  color: var(--qs-accent);
}

.qs-switcher__name {
  overflow: hidden;
  text-overflow: ellipsis;
  font-size: 13px;
  color: var(--qs-text-secondary);
}

.qs-switcher__caret {
  color: var(--qs-text-muted);
}

.qs-switcher__menu {
  position: absolute;
  top: calc(100% + 6px);
  left: 0;
  min-width: 280px;
  background: var(--qs-bg-card);
  border: 1px solid var(--qs-border);
  border-radius: var(--qs-radius);
  box-shadow: 0 16px 40px rgba(0, 0, 0, 0.4);
  padding: 6px;
  z-index: 1000;
}

.qs-switcher__item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  padding: 8px 10px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qs-text);
  cursor: pointer;
  text-align: left;
}

.qs-switcher__item:hover {
  background: var(--qs-bg-hover);
}

.qs-switcher__item--disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.qs-switcher__item-short {
  font-weight: 700;
  font-size: 11px;
  color: var(--qs-accent);
  width: 42px;
}

.qs-switcher__item-name {
  flex: 1;
  font-size: 13px;
}

.qs-switcher__soon {
  font-size: 10px;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--qs-warning);
}

.qs-viewsel {
  justify-self: end;
}

.qs-viewsel__menu {
  left: auto;
  right: 0;
}

.qs-viewsel__label {
  font-size: 13px;
  font-weight: 600;
  color: var(--qs-text);
}

.qs-viewsel__menu {
  min-width: 160px;
}

.qs-viewsel__item {
  font-size: 13px;
  color: var(--qs-text-secondary);
}

.qs-viewsel__item--active {
  color: var(--qs-accent);
  background: var(--qs-bg-hover);
}
</style>
