<script setup lang="ts">
import { useWorkspacesStore } from '../../../stores/workspaces'
import { useAppStore } from '../../../stores/app'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { invoke } from '@tauri-apps/api/core'

const workspacesStore = useWorkspacesStore()
const appStore = useAppStore()

// ── File quick-open ──
//
// The bar itself is `QFileSearch` (packages/ui) since 2026-08-26 — QuantCode
// and QuantConsole show the same one. It walks the workspace and hands back a
// path; reading the file and putting it in a tab stays here.
const searchRef = ref<{ arm: () => void; disarm: () => void } | null>(null)

async function openSearchResult(path: string) {
  try {
    const content = await invoke<string>('plugin:canvas|read_file', { path })
    appStore.openTab(path, content)
  } catch {
    appStore.openTab(path, `// Could not read ${path}\n`)
  }
  if (!appStore.editorVisible) appStore.toggleEditor()
}

// ── Workspace actions (kept for global event + downstream use) ──

/**
 * Switch workspace. Saving and restoring the canvas state is NOT done here —
 * the canvas layout watches the active workspace and does it for every switch,
 * whatever triggered it (docs/PLAN-WORKSPACES.md). This function only makes the
 * switch and records it for back/forward.
 */
async function switchWorkspace(id: string) {
  // Back/forward can land on the General board — its id is the sentinel,
  // not a registry entry.
  if (id === workspacesStore.GENERAL_CONTENT) {
    workspacesStore.setCanvasGeneral(true)
    return
  }
  // Picking a workspace here also leaves the General view and drops the
  // explorer's pin — the board follows the pick.
  workspacesStore.setCanvasGeneral(false)
  workspacesStore.followActive()
  if (id === workspacesStore.activeWorkspaceId) return
  await workspacesStore.setActiveWorkspace(id)
  // Track in navigation history
  appStore.pushNavHistory()
}

// Register so app store can read workspace ID and switch during back/forward
appStore.registerActiveWorkspaceGetter(() => workspacesStore.contentWorkspaceId)
appStore.registerWorkspaceSwitch(switchWorkspace)

/** Ctrl+O and the header's brand click: pick a folder, open it as a workspace. */
async function openNewWorkspace() {
  if (await workspacesStore.openFolder()) {
    appStore.pushNavHistory()
  }
}

// Alt+Left/Right — back/forward. Ctrl+P belongs to the search bar, which
// arms and disarms itself with this component (see below).
function onGlobalKeydown(e: KeyboardEvent) {
  if (e.altKey && e.key === 'ArrowLeft') {
    e.preventDefault()
    appStore.navigateBack()
    return
  }
  if (e.altKey && e.key === 'ArrowRight') {
    e.preventDefault()
    appStore.navigateForward()
  }
}

// Mouse back/forward buttons (button 3 = back, button 4 = forward)
function onMouseNav(e: MouseEvent) {
  if (e.button === 3) {
    e.preventDefault()
    appStore.navigateBack()
  } else if (e.button === 4) {
    e.preventDefault()
    appStore.navigateForward()
  }
}

// V3 warm cache: the stage keeps this component mounted while another module is
// active — the three window listeners are armed only while canvas is visible,
// or Ctrl+P (notes binds it too), Alt+Left/Right and the mouse back/forward
// buttons keep being intercepted from everywhere else. Bound in BOTH onMounted
// and onActivated: pages load async and mount after the stage's activation
// flush, so onActivated alone would miss the first visit; the bound flag makes
// the overlap safe. That same late mount can land in a stage the user has
// already left, where the bound flag turns against us — the listeners would
// stay armed inside another module and block the real activation — so
// onMounted binds only inside an active tree.
let listenersBound = false

function bindGlobalListeners() {
  if (listenersBound) return
  listenersBound = true
  window.addEventListener('quantcode:open-workspace', openNewWorkspace)
  window.addEventListener('keydown', onGlobalKeydown)
  window.addEventListener('mouseup', onMouseNav)
  searchRef.value?.arm()
}

function unbindGlobalListeners() {
  if (!listenersBound) return
  listenersBound = false
  window.removeEventListener('quantcode:open-workspace', openNewWorkspace)
  window.removeEventListener('keydown', onGlobalKeydown)
  window.removeEventListener('mouseup', onMouseNav)
  searchRef.value?.disarm()
}

onMounted(() => {
  if (inActiveKeepAliveTree()) bindGlobalListeners()
})
onActivated(bindGlobalListeners)

onDeactivated(unbindGlobalListeners)
onUnmounted(unbindGlobalListeners)
</script>

<template>
  <!-- The shared module header (V3): 51px, logo cap / center / selector +
       settings cap. The brand click keeps QuantCanvas's behaviour — it opens
       a workspace folder, it does not navigate. -->
  <QModuleHeader module-id="canvas" home-route="" @brand="openNewWorkspace">
      <!-- Explorer wedge (far left) -->
      <QHeaderEdge
        side="left"
        :active="appStore.fileExplorerVisible"
        label="Toggle Explorer (Ctrl+B)"
        @click="appStore.toggleFileExplorer()"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <rect x="3" y="4" width="18" height="16" rx="1.5" />
          <line x1="9" y1="4" x2="9" y2="20" />
        </svg>
      </QHeaderEdge>

      <!-- The shared search bar. Back/forward and the notes toggle ride in its
           slots, so the field stays centred in the header. -->
      <QFileSearch
        ref="searchRef"
        :root="workspacesStore.contentWorkspace?.path ?? null"
        @open="openSearchResult"
      >
        <template #left>
          <QHeaderAction
            :disabled="!appStore.canGoBack"
            label="Go Back (Alt+Left)"
            @click="appStore.navigateBack()"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="15 18 9 12 15 6" />
            </svg>
          </QHeaderAction>
          <QHeaderAction
            :disabled="!appStore.canGoForward"
            label="Go Forward (Alt+Right)"
            @click="appStore.navigateForward()"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
              <polyline points="9 18 15 12 9 6" />
            </svg>
          </QHeaderAction>
        </template>

        <template #right>
          <QHeaderAction
            :active="appStore.notesBarVisible"
            label="Toggle panel (Ctrl+J)"
            @click="appStore.toggleNotesBar()"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
              <rect x="3" y="4" width="18" height="16" rx="1.5" />
              <line x1="3" y1="14" x2="21" y2="14" />
            </svg>
          </QHeaderAction>
        </template>
      </QFileSearch>

      <!-- Editor wedge (far right) -->
      <QHeaderEdge
        side="right"
        :active="appStore.editorVisible"
        label="Toggle Editor (Ctrl+Shift+B)"
        @click="appStore.toggleEditor()"
      >
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">
          <rect x="3" y="4" width="18" height="16" rx="1.5" />
          <line x1="15" y1="4" x2="15" y2="20" />
        </svg>
      </QHeaderEdge>
  </QModuleHeader>
</template>

<style scoped>
/* The bar frame is QModuleHeader, the wedges are QHeaderEdge, the round
   actions are QHeaderAction and the field is QFileSearch — all shared
   (packages/ui). This component has no chrome of its own left. */
</style>
