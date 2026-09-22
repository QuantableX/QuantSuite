<script setup lang="ts">
/**
 * The module selector (V3.1) — the "Modules" button in every module header's
 * right cap. It fills the cap's remaining width (the settings gear keeps its
 * square) and opens a centred MODAL listing the active app's modules — logo,
 * title, active check; stubs carry a "soon" badge.
 *
 * Navigation goes through the shell's `qss:navigate` event — packages/ui
 * components own no router. Last-used persistence is the shell's job.
 */
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { appForModule, moduleById, modulesForApp } from '@quantsuite/core'
import { logoFor } from '../logos'

const props = defineProps<{
  /** The module whose header hosts this selector. */
  moduleId: string
}>()
const emit = defineEmits<{
  (e: 'select', id: string): void
}>()

const open = ref(false)

const app = computed(() => appForModule(props.moduleId))
const members = computed(() => (app.value ? modulesForApp(app.value.id) : []))

function select(id: string) {
  open.value = false
  if (id === props.moduleId) return
  const target = moduleById(id)
  if (!target || target.ownWindow) return
  window.dispatchEvent(new CustomEvent('qss:navigate', { detail: { route: target.route } }))
  emit('select', id)
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape' && open.value) {
    e.stopPropagation()
    open.value = false
  }
}

onMounted(() => document.addEventListener('keydown', onKeydown))
onBeforeUnmount(() => document.removeEventListener('keydown', onKeydown))
</script>

<template>
  <div class="qms">
    <button
      class="qms-trigger"
      :class="{ 'is-open': open }"
      :title="app ? `${app.title} modules` : 'Modules'"
      :aria-expanded="open"
      @click="open = true"
    >
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path d="M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z" />
      </svg>
      <span>Modules</span>
    </button>

    <Teleport to="body">
      <Transition name="qms-fade">
        <div v-if="open" class="qms-overlay" @mousedown.self="open = false">
          <div class="qms-panel" role="dialog" aria-modal="true" :aria-label="`${app?.title ?? ''} modules`">
            <header class="qms-head">
              <h2 class="qms-title">{{ app?.title }} — Modules</h2>
              <button class="qms-close" title="Close" @click="open = false">
                <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
                  <path d="M3 3l8 8M11 3l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
                </svg>
              </button>
            </header>

            <div class="qms-list">
              <button
                v-for="m in members"
                :key="m.id"
                class="qms-item"
                :class="{ 'is-active': m.id === moduleId }"
                @click="select(m.id)"
              >
                <span class="qms-item-text">
                  <img v-if="logoFor(m.id)" :src="logoFor(m.id)" :alt="m.title" class="qms-item-logo" />
                  <span v-else class="qms-item-title">{{ m.title }}</span>
                  <span class="qms-item-desc">{{ m.description }}</span>
                </span>
                <span v-if="m.status === 'stub'" class="qms-soon">soon</span>
                <svg v-else-if="m.id === moduleId" class="qms-check" width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
                  <polyline points="20 6 9 17 4 12" />
                </svg>
              </button>
            </div>
          </div>
        </div>
      </Transition>
    </Teleport>
  </div>
</template>

<style scoped>
/* The selector owns the cap's leftover width — the gear keeps its square. */
.qms {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
}

.qms-trigger {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  width: 100%;
  height: 30px;
  border: 1px solid var(--qss-border);
  border-radius: 8px;
  background: transparent;
  color: var(--qss-text-secondary);
  font-family: inherit;
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: background var(--qss-dur-fast) var(--qss-ease-out), color var(--qss-dur-fast) var(--qss-ease-out);
}
.qms-trigger:hover,
.qms-trigger.is-open {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}

/* ── Modal ── */
.qms-overlay {
  position: fixed;
  inset: 0;
  z-index: 10000;
  display: grid;
  place-items: center;
  background: rgb(11 11 15 / 0.6);
}

.qms-panel {
  width: 420px;
  max-width: calc(100% - 48px);
  border: 1px solid var(--qss-border);
  border-radius: 12px;
  overflow: hidden;
  background: var(--qss-bg);
  box-shadow: 0 20px 60px rgb(0 0 0 / 0.4);
}

.qms-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 16px 10px;
  border-bottom: 1px solid var(--qss-border-subtle);
}

.qms-title {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  color: var(--qss-text);
}

.qms-close {
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
.qms-close:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}

.qms-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 10px;
}

.qms-item {
  display: flex;
  align-items: center;
  gap: 12px;
  width: 100%;
  padding: 11px 12px;
  border: 1px solid transparent;
  border-radius: 9px;
  background: transparent;
  color: var(--qss-text-secondary);
  font-family: inherit;
  text-align: left;
  cursor: pointer;
  transition: background var(--qss-dur-instant) var(--qss-ease-out), color var(--qss-dur-instant) var(--qss-ease-out), border-color var(--qss-dur-instant) var(--qss-ease-out);
}
.qms-item:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}
.qms-item.is-active {
  background: var(--qss-bg-card);
  border-color: var(--qss-border);
  color: var(--qss-text);
}

/* The wordmark IS the row's title — no duplicated text next to it. */
.qms-item-logo {
  /* Height-only (shell.css --qss-wordmark-h-menu), matching the rail flyout —
     equal cap height for every name length, and no width clamp to undo it. */
  width: auto;
  height: var(--qss-wordmark-h-menu, 14px);
  max-width: none;
  object-fit: contain;
  object-position: left;
  align-self: flex-start;
}

.qms-item-text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.qms-item-title {
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qms-item-desc {
  font-size: 11px;
  color: var(--qss-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qms-soon {
  padding: 2px 7px;
  border-radius: 999px;
  background: var(--qss-bg-hover);
  color: var(--qss-text-muted);
  font-size: 10px;
  font-weight: 600;
  letter-spacing: 0.04em;
  text-transform: uppercase;
}

.qms-check {
  flex-shrink: 0;
  color: var(--qss-accent);
}

.qms-fade-enter-active {
  transition: opacity var(--qss-dur-base) var(--qss-ease-out);
}
.qms-fade-enter-active .qms-panel {
  transition: transform var(--qss-dur-entrance) var(--qss-ease-spring);
}
.qms-fade-leave-active {
  transition: opacity var(--qss-dur-fast) var(--qss-ease-in);
}
.qms-fade-enter-from,
.qms-fade-leave-to {
  opacity: 0;
}
.qms-fade-enter-from .qms-panel {
  transform: scale(0.96) translateY(8px);
}

@media (prefers-reduced-motion: reduce) {
  .qms-fade-enter-active,
  .qms-fade-leave-active,
  .qms-fade-enter-active .qms-panel {
    transition-duration: 1ms;
  }
}
</style>
