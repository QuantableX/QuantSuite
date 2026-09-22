<script setup lang="ts">
/**
 * QuantScript's module shell — the shared header with the workbench toolbar
 * in its centre, the script tree on the left, the editor stage, the context
 * panel on the right (docs/PLAN-QUANTSCRIPT.md).
 *
 * Warm-cache rules (V3): the listing loads on mount only inside an active
 * KeepAlive tree and again on activation; the shortcuts stand down while
 * another module is active (useShortcuts does that itself).
 */
import { inActiveKeepAliveTree, useShortcuts } from '@quantsuite/core'
import { useWorkbenchStore } from '#script/stores/workbench'

const wb = useWorkbenchStore()
const route = useRoute()
const onForge = computed(() => route.path.startsWith('/script/forge'))
const editing = computed(() => !!wb.active && !wb.libraryOpen)
const panelsVisible = computed(() => !wb.focusMode || onForge.value)
watch(() => route.path, () => {
  if (onForge.value) wb.focusMode = false
})

function activate() {
  if (!wb.listing && !wb.listingLoading) void wb.loadListing()
}

onMounted(() => {
  if (inActiveKeepAliveTree()) activate()
})
onActivated(activate)

useShortcuts([
  // The wildcard-modifier rule: Ctrl+Shift+B must come before plain Ctrl+B.
  {
    key: 'B',
    ctrl: true,
    shift: true,
    handler: (e) => {
      e.preventDefault()
      wb.toggleSidebar('right')
    },
  },
  {
    key: 'b',
    ctrl: true,
    handler: (e) => {
      e.preventDefault()
      wb.toggleSidebar('left')
    },
  },
  // Monaco owns Ctrl+S while it has focus (it prevents the default, which
  // useShortcuts respects); this catches the key everywhere else in the module.
  {
    key: 's',
    ctrl: true,
    handler: (e) => {
      if (onForge.value || !editing.value || !wb.active?.editable || wb.newScriptOpen) return
      e.preventDefault()
      void wb.save()
    },
  },
  {
    key: ',',
    ctrl: true,
    handler: (e) => {
      e.preventDefault()
      window.dispatchEvent(new CustomEvent('qss:settings', { detail: { module: 'script' } }))
    },
  },
])
</script>

<template>
  <div class="qsc-shell">
    <QModuleHeader module-id="script">
      <ScriptLayoutToolbar />
    </QModuleHeader>
    <div class="qsc-body">
      <QSidebar :model-value="panelsVisible && wb.sidebarLeftOpen" :width="220" aria-label="Script files">
        <template #header>
          <ScriptLayoutSidebarHeader />
        </template>
        <ScriptLayoutSidebar />
      </QSidebar>
      <main class="qsc-main">
        <slot />
      </main>
      <QRightPanel :model-value="panelsVisible && wb.sidebarRightOpen" storage-key="script.inspector" aria-label="QuantScript inspector">
        <ScriptLayoutRightSidebar />
      </QRightPanel>
    </div>
    <ScriptEditorNewScriptModal v-if="wb.newScriptOpen" />
    <Transition name="qsc-notice">
      <div v-if="wb.notice" class="qsc-notice-host">
        <div class="qsc-note" :class="`is-${wb.notice.tone}`" role="status" aria-live="polite">{{ wb.notice.text }}</div>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.qsc-shell {
  position: relative;
  display: flex;
  flex-direction: column;
  /* The module fills its container, never the viewport (ARCHITECTURE.md §9). */
  width: 100%;
  height: 100%;
  overflow: hidden;
}

.qsc-body {
  position: relative;
  display: flex;
  flex: 1;
  min-height: 0;
}

.qsc-main {
  container-type: inline-size;
  position: relative;
  flex: 1;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  background: var(--qss-bg);
}

/* The one transient notice of the module — bottom centre of the panel,
   above the status bar, never over the sidebars' controls. */
.qsc-notice-host {
  position: absolute;
  left: 50%;
  bottom: 38px;
  transform: translateX(-50%);
  z-index: 30;
  max-width: min(640px, 80%);
  box-shadow: var(--qss-shadow-md);
  pointer-events: none;
}
.qsc-notice-enter-active {
  transition: opacity var(--qss-dur-base) var(--qss-ease-out), transform var(--qss-dur-base) var(--qss-ease-out);
}
.qsc-notice-leave-active {
  transition: opacity var(--qss-dur-fast) var(--qss-ease-in), transform var(--qss-dur-fast) var(--qss-ease-in);
}
.qsc-notice-enter-from,
.qsc-notice-leave-to {
  opacity: 0;
  transform: translateX(-50%) translateY(8px);
}
</style>
