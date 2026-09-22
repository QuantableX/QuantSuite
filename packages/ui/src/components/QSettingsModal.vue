<script setup lang="ts">
/**
 * The unified settings modal (V3) — ONE settings surface for the suite.
 *
 * Mounted once in the shell (app.vue). Opens on the `qss:settings` event that
 * every module header's gear dispatches, with the module id in `detail`.
 * Left column: a "Suite" group (sections the shell registers under the
 * reserved id `suite`) and a group for the active module (sections that
 * module registered via `registerSettingsSections`). Right pane renders the
 * selected section component.
 *
 * Visual language: QuantCode's settings modal (the donor design), ported to
 * `--qss-*` tokens as `qsm-*` classes.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { moduleById, settingsSectionsFor, type SettingsSection } from '@quantsuite/core'

const open = ref(false)
const moduleId = ref<string | null>(null)
const selected = ref<{ group: string; id: string } | null>(null)

const suiteSections = computed(() => settingsSectionsFor('suite'))
const moduleSections = computed(() => (moduleId.value ? settingsSectionsFor(moduleId.value) : []))
const moduleTitle = computed(() => (moduleId.value ? moduleById(moduleId.value)?.title ?? moduleId.value : null))

const current = computed<SettingsSection | null>(() => {
  if (!selected.value) return null
  const pool = selected.value.group === 'suite' ? suiteSections.value : moduleSections.value
  return pool.find((s) => s.id === selected.value?.id) ?? null
})

function openFor(id: string | null) {
  moduleId.value = id
  // Land on the module's first section when it has any; suite otherwise.
  if (id && settingsSectionsFor(id).length > 0) {
    selected.value = { group: 'module', id: settingsSectionsFor(id)[0]!.id }
  } else if (suiteSections.value.length > 0) {
    selected.value = { group: 'suite', id: suiteSections.value[0]!.id }
  } else {
    selected.value = null
  }
  open.value = true
}

function close() {
  open.value = false
}

function onSettingsEvent(e: Event) {
  const detail = (e as CustomEvent).detail
  openFor(typeof detail?.module === 'string' ? detail.module : null)
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && open.value) {
    e.stopPropagation()
    close()
  }
}

onMounted(() => {
  window.addEventListener('qss:settings', onSettingsEvent)
  // Sections that navigate away (e.g. "Open full settings" links) close the
  // modal through this event.
  window.addEventListener('qss:settings-close', close)
  document.addEventListener('keydown', onKeydown)
})
onBeforeUnmount(() => {
  window.removeEventListener('qss:settings', onSettingsEvent)
  window.removeEventListener('qss:settings-close', close)
  document.removeEventListener('keydown', onKeydown)
})

// If a module registered sections after the modal opened (HMR), re-anchor.
watch(moduleSections, (sections) => {
  if (open.value && selected.value?.group === 'module' && sections.length === 0) {
    selected.value = suiteSections.value[0] ? { group: 'suite', id: suiteSections.value[0].id } : null
  }
  if (open.value && selected.value === null && sections.length > 0) {
    selected.value = { group: 'module', id: sections[0]!.id }
  }
})
</script>

<template>
  <Teleport to="body">
    <Transition name="qsm-fade">
      <div v-if="open" class="qsm-overlay" @mousedown.self="close">
        <div class="qsm-panel" role="dialog" aria-modal="true" aria-label="Settings">
          <aside class="qsm-nav">
            <div class="qsm-nav-group">
              <p class="qsm-nav-label">Suite</p>
              <button
                v-for="s in suiteSections"
                :key="'suite:' + s.id"
                class="qsm-nav-item"
                :class="{ 'is-active': selected?.group === 'suite' && selected?.id === s.id }"
                @click="selected = { group: 'suite', id: s.id }"
              >
                {{ s.label }}
              </button>
            </div>

            <div v-if="moduleTitle && moduleSections.length" class="qsm-nav-group">
              <p class="qsm-nav-label">{{ moduleTitle }}</p>
              <button
                v-for="s in moduleSections"
                :key="'module:' + s.id"
                class="qsm-nav-item"
                :class="{ 'is-active': selected?.group === 'module' && selected?.id === s.id }"
                @click="selected = { group: 'module', id: s.id }"
              >
                {{ s.label }}
              </button>
            </div>
          </aside>

          <div class="qsm-content">
            <header class="qsm-head">
              <h2 class="qsm-title">{{ current?.label ?? 'Settings' }}</h2>
              <button class="qsm-close" title="Close" @click="close">
                <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
                  <path d="M3 3l8 8M11 3l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
                </svg>
              </button>
            </header>
            <div class="qsm-body">
              <component :is="current.component" v-if="current" />
              <p v-else class="qsm-empty">No settings registered.</p>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.qsm-overlay {
  position: fixed;
  inset: 0;
  z-index: 10000;
  display: grid;
  place-items: center;
  background: rgb(11 11 15 / 0.6);
}

.qsm-panel {
  display: flex;
  width: min(840px, calc(100vw - 32px));
  height: 580px;
  max-height: 80%;
  border: 1px solid var(--qss-border);
  border-radius: 18px;
  overflow: hidden;
  background: var(--qss-bg);
  box-shadow: 0 20px 60px rgb(0 0 0 / 0.4);
}

.qsm-nav {
  width: 190px;
  flex-shrink: 0;
  padding: 20px 10px;
  border-right: 1px solid var(--qss-border);
  background: var(--qss-bg-raised);
  overflow-y: auto;
}

.qsm-nav-group + .qsm-nav-group {
  margin-top: 14px;
}

@media (max-width: 600px) {
  .qsm-nav { width: 120px; }
}

.qsm-nav-label {
  margin: 0 0 4px;
  padding: 0 10px;
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  color: var(--qss-text-muted);
}

.qsm-nav-item {
  display: block;
  width: 100%;
  padding: 7px 10px;
  border: none;
  border-radius: 7px;
  background: transparent;
  color: var(--qss-text-secondary);
  font-family: inherit;
  font-size: 12.5px;
  font-weight: 500;
  text-align: left;
  cursor: pointer;
  transition: background var(--qss-dur-instant) var(--qss-ease-out), color var(--qss-dur-instant) var(--qss-ease-out);
}
.qsm-nav-item:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.qsm-nav-item.is-active {
  background: var(--qss-bg-card);
  color: var(--qss-text);
}

.qsm-content {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.qsm-head {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 22px 28px 18px;
  border-bottom: 1px solid var(--qss-border-subtle);
}

.qsm-title {
  margin: 0;
  font-size: 22px;
  font-weight: 600;
  color: var(--qss-text);
}

.qsm-close {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border: none;
  border-radius: 7px;
  background: transparent;
  color: var(--qss-text-muted);
  cursor: pointer;
  transition: background var(--qss-dur-instant) var(--qss-ease-out), color var(--qss-dur-instant) var(--qss-ease-out);
}
.qsm-close:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}

.qsm-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 24px 28px;
}

.qsm-empty {
  color: var(--qss-text-muted);
  font-size: 13px;
}

.qsm-fade-enter-active {
  transition: opacity var(--qss-dur-base) var(--qss-ease-out);
}
.qsm-fade-enter-active .qsm-panel {
  transition: transform var(--qss-dur-entrance) var(--qss-ease-spring);
}
.qsm-fade-leave-active {
  transition: opacity var(--qss-dur-fast) var(--qss-ease-in);
}
.qsm-fade-enter-from,
.qsm-fade-leave-to {
  opacity: 0;
}
.qsm-fade-enter-from .qsm-panel {
  transform: scale(0.97) translateY(8px);
}

@media (prefers-reduced-motion: reduce) {
  .qsm-fade-enter-active,
  .qsm-fade-leave-active,
  .qsm-fade-enter-active .qsm-panel {
    transition-duration: 1ms;
  }
}
</style>
