<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import type { CanvasWindow, SpecFile, SpecStatus } from '../../../shared/types'
import { useWorkspacesStore } from '../../../stores/workspaces'
import { useAppStore } from '../../../stores/app'
import { findSpecFiles, updateSpecField, serializeSpec, workspaceStorageDir } from '../../../lib/specs/engine'

defineProps<{
  window: CanvasWindow
}>()

const workspacesStore = useWorkspacesStore()
const appStore = useAppStore()

const specs = ref<SpecFile[]>([])
const expandedSpecPath = ref<string | null>(null)
const loading = ref(false)
const errorMsg = ref('')

// Board actions (status write, delete) report here, not through errorMsg —
// that one renders instead of the board, and a failed write must not cost the
// user the whole overview. Cleared by the next action that succeeds.
const actionError = ref('')

// New spec form
const newTitle = ref('')
const newPriority = ref<string>('medium')
const newVerifyMode = ref<'bypass' | 'verify'>('bypass')
const creating = ref(false)

// Pointer-based drag and drop (HTML5 drag API doesn't work reliably in Tauri)
const draggedSpec = ref<SpecFile | null>(null)
const dragOverStatus = ref<SpecStatus | null>(null)
const isDragging = ref(false)
const dragGhost = ref<HTMLElement | null>(null)
const dragStart = ref({ x: 0, y: 0 })
const dragThreshold = 5
const pendingDragSpec = ref<SpecFile | null>(null)
const columnElements = ref<Map<SpecStatus, HTMLElement>>(new Map())

const activeColumns = computed<SpecStatus[]>(() =>
  newVerifyMode.value === 'verify' ? ['open', 'verify', 'done'] : ['open', 'done'],
)

const statusConfig: Record<SpecStatus, {
  label: string
  dotColor: string
  borderColor: string
  emptyLabel: string
  emptyHint: string
}> = {
  open: {
    label: 'Open',
    dotColor: 'var(--qc-text-dim)',
    borderColor: 'var(--qc-border)',
    emptyLabel: 'No open specs',
    emptyHint: 'Create a spec to start tracking work.',
  },
  verify: {
    label: 'Verify',
    dotColor: 'var(--qc-text-dim)',
    borderColor: 'var(--qc-border)',
    emptyLabel: 'Nothing to verify',
    emptyHint: 'Specs that need to be verified land here for review.',
  },
  done: {
    label: 'Done',
    dotColor: 'var(--qc-text-dim)',
    borderColor: 'var(--qc-border)',
    emptyLabel: 'Nothing finished yet',
    emptyHint: 'Drag a spec here or mark it done.',
  },
}

const priorityConfig: Record<string, { label: string; color: string; bg: string }> = {
  critical: { label: 'CRIT', color: '#ef4444', bg: '#ef444418' },
  high: { label: 'HIGH', color: '#f97316', bg: '#f9731618' },
  medium: { label: 'MED', color: '#f59e0b', bg: '#f59e0b18' },
  low: { label: 'LOW', color: '#3b82f6', bg: '#3b82f618' },
}

function getSpecTimestamp(spec: SpecFile): number {
  const raw = spec.frontmatter.updatedAt || spec.frontmatter.createdAt
  const timestamp = Date.parse(raw)
  return Number.isFinite(timestamp) ? timestamp : 0
}

function sortSpecs(a: SpecFile, b: SpecFile): number {
  const timeDiff = getSpecTimestamp(b) - getSpecTimestamp(a)
  if (timeDiff !== 0) return timeDiff
  return a.frontmatter.title.localeCompare(b.frontmatter.title)
}

const groupedSpecs = computed(() => {
  const groups: Record<SpecStatus, SpecFile[]> = {
    open: [],
    verify: [],
    done: [],
  }

  for (const spec of specs.value) {
    groups[spec.frontmatter.status].push(spec)
  }

  groups.open.sort(sortSpecs)
  groups.verify.sort(sortSpecs)
  groups.done.sort(sortSpecs)

  return groups
})

function toggleExpand(path: string) {
  expandedSpecPath.value = expandedSpecPath.value === path ? null : path
}

function onHandleMouseDown(e: MouseEvent, spec: SpecFile) {
  e.preventDefault()
  e.stopPropagation()

  pendingDragSpec.value = spec
  dragStart.value = { x: e.clientX, y: e.clientY }

  window.addEventListener('mousemove', onMouseMove)
  window.addEventListener('mouseup', onMouseUp)
}

function onMouseMove(e: MouseEvent) {
  if (!pendingDragSpec.value && !isDragging.value) return

  if (pendingDragSpec.value && !isDragging.value) {
    const distance = Math.hypot(
      e.clientX - dragStart.value.x,
      e.clientY - dragStart.value.y,
    )

    if (distance < dragThreshold) return

    isDragging.value = true
    draggedSpec.value = pendingDragSpec.value
    pendingDragSpec.value = null
    createGhost(e)
  }

  if (!isDragging.value || !dragGhost.value) return

  dragGhost.value.style.top = `${e.clientY - 14}px`
  dragGhost.value.style.left = `${e.clientX - 50}px`

  let foundStatus: SpecStatus | null = null
  for (const [status, el] of columnElements.value) {
    const rect = el.getBoundingClientRect()
    const insideX = e.clientX >= rect.left && e.clientX <= rect.right
    const insideY = e.clientY >= rect.top && e.clientY <= rect.bottom
    if (insideX && insideY) {
      foundStatus = status
      break
    }
  }

  dragOverStatus.value = foundStatus
}

async function onMouseUp() {
  window.removeEventListener('mousemove', onMouseMove)
  window.removeEventListener('mouseup', onMouseUp)

  if (isDragging.value && draggedSpec.value && dragOverStatus.value) {
    const targetStatus = dragOverStatus.value
    if (draggedSpec.value.frontmatter.status !== targetStatus) {
      await changeStatus(draggedSpec.value, targetStatus)
    }
  }

  removeGhost()
  isDragging.value = false
  draggedSpec.value = null
  dragOverStatus.value = null
  pendingDragSpec.value = null
}

function createGhost(e: MouseEvent) {
  const ghost = document.createElement('div')
  const accent = draggedSpec.value
    ? statusConfig[draggedSpec.value.frontmatter.status].dotColor
    : 'var(--qc-accent, #6e8efb)'

  ghost.className = 'spec-drag-ghost'
  ghost.textContent = draggedSpec.value?.frontmatter.title ?? ''
  ghost.style.cssText = `
    position: fixed;
    top: ${e.clientY - 14}px;
    left: ${e.clientX - 50}px;
    z-index: 99999;
    padding: 5px 10px;
    border-radius: 8px;
    font-size: 11px;
    font-weight: 600;
    color: #fff;
    background: ${accent};
    box-shadow: 0 10px 30px rgba(0,0,0,0.35);
    pointer-events: none;
    white-space: nowrap;
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    opacity: 0.96;
  `
  document.body.appendChild(ghost)
  dragGhost.value = ghost
}

function removeGhost() {
  if (dragGhost.value) {
    dragGhost.value.remove()
    dragGhost.value = null
  }
}

function registerColumnEl(status: SpecStatus, el: HTMLElement | null) {
  if (el) {
    columnElements.value.set(status, el)
  } else {
    columnElements.value.delete(status)
  }
}

let loadVersion = 0

async function loadSpecs() {
  const version = ++loadVersion
  specs.value = []
  loading.value = true
  errorMsg.value = ''

  try {
    const workspace = workspacesStore.contentWorkspace
    if (!workspace) {
      errorMsg.value = 'No workspace open'
      return
    }

    const loaded = await findSpecFiles(workspace.path)
    if (version === loadVersion) specs.value = loaded
  } catch (err) {
    if (version === loadVersion) {
      errorMsg.value = err instanceof Error ? err.message : String(err)
      specs.value = []
    }
  } finally {
    if (version === loadVersion) loading.value = false
  }
}

const confirmOverwrite = ref(false)
const pendingSpec = ref<{ spec: SpecFile; fullPath: string } | null>(null)

// Delete confirmation
const confirmDeleteVisible = ref(false)
const pendingDeleteSpec = ref<SpecFile | null>(null)

async function createSpec() {
  const title = newTitle.value.trim()
  if (!title) return

  const workspace = workspacesStore.contentWorkspace
  if (!workspace) return

  creating.value = true
  try {
    const now = new Date().toISOString()
    const slug = title
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-|-$/g, '')
    const fileName = `${slug}.spec.md`
    const storage = await workspaceStorageDir(workspace.path)
    const fullPath = `${storage}/specs/${fileName}`

    const content = `\n## Requirements\n\n- \n\n## Current State\n\n- `

    const spec: SpecFile = {
      path: fullPath,
      frontmatter: {
        title,
        status: 'open',
        priority: newPriority.value as SpecFile['frontmatter']['priority'],
        verifyMode: newVerifyMode.value === 'verify' ? true : undefined,
        linkedFiles: [],
        createdAt: now,
        updatedAt: now,
      },
      content,
      rawContent: '',
    }
    spec.rawContent = serializeSpec(spec)

    let exists = false
    try {
      await invoke<string>('plugin:canvas|read_file', { path: fullPath })
      exists = true
    } catch {
      // File doesn't exist — good
    }

    if (exists) {
      pendingSpec.value = { spec, fullPath }
      confirmOverwrite.value = true
      return
    }

    await writeSpec(spec, fullPath)
  } catch (err) {
    errorMsg.value = err instanceof Error ? err.message : String(err)
  } finally {
    creating.value = false
  }
}

function refreshExplorer() {
  try {
    if (typeof appStore.triggerFileExplorerRefresh === 'function') {
      appStore.triggerFileExplorerRefresh()
    } else {
      appStore.fileExplorerRefreshKey++
    }
  } catch {
    // HMR
  }
}

async function writeSpec(spec: SpecFile, fullPath: string) {
  await invoke('plugin:canvas|write_file', { path: fullPath, content: spec.rawContent })

  newTitle.value = ''
  newPriority.value = 'medium'
  newVerifyMode.value = 'bypass'

  await loadSpecs()
  refreshExplorer()

  try {
    for (const tab of appStore.activeEditorTabs) {
      if (!tab.filePath.endsWith('.spec.md')) continue
      if (appStore.pathsMatch(tab.filePath, spec.path) || appStore.pathsMatch(tab.filePath, fullPath)) {
        appStore.notifyFileSaved(tab.filePath, spec.rawContent)
        continue
      }

      try {
        const workspace = workspacesStore.contentWorkspace
        if (!workspace) continue
        const content = await invoke<string>('plugin:canvas|read_file', {
          path: tab.filePath.includes(':') || tab.filePath.startsWith('/')
            ? tab.filePath
            : workspace.path + '/' + tab.filePath,
        })
        appStore.refreshTab(tab.filePath, content)
      } catch {
        // Tab file may have been deleted
      }
    }
  } catch {
    // HMR
  }
}

async function handleOverwrite(overwrite: boolean) {
  confirmOverwrite.value = false
  if (overwrite && pendingSpec.value) {
    try {
      await writeSpec(pendingSpec.value.spec, pendingSpec.value.fullPath)
    } catch (err) {
      errorMsg.value = err instanceof Error ? err.message : String(err)
    }
  }
  pendingSpec.value = null
  creating.value = false
}

async function openInEditor(spec: SpecFile) {
  const workspace = workspacesStore.contentWorkspace
  if (!workspace) return

  try {
    const content = await invoke<string>('plugin:canvas|read_file', {
      path: spec.path,
    })
    appStore.openTab(spec.path, content)
    if (!appStore.editorVisible) {
      appStore.toggleEditor()
    }
  } catch {
    appStore.openTab(spec.path, spec.rawContent)
    if (!appStore.editorVisible) {
      appStore.toggleEditor()
    }
  }
}

async function changeStatus(spec: SpecFile, newStatus: SpecStatus) {
  if (spec.frontmatter.status === newStatus) return

  const workspace = workspacesStore.contentWorkspace
  // No workspace, no file to write — moving the card would promise a status
  // that nothing on disk backs.
  if (!workspace) {
    actionError.value = 'No workspace open'
    return
  }

  const updatedAt = new Date().toISOString()

  // Kept for the rollback below: nothing reloads the specs after this point.
  const prevStatus = spec.frontmatter.status
  const prevUpdatedAt = spec.frontmatter.updatedAt
  const prevRawContent = spec.rawContent

  spec.frontmatter.status = newStatus
  spec.frontmatter.updatedAt = updatedAt

  let updated = updateSpecField(spec.rawContent, 'status', newStatus)
  updated = updateSpecField(updated, 'updatedAt', updatedAt)
  spec.rawContent = updated

  try {
    await invoke('plugin:canvas|write_file', {
      path: spec.path,
      content: updated,
    })
    actionError.value = ''
  } catch (err) {
    // Write failed — take the optimistic move back, the board would otherwise
    // show a status the file does not have.
    spec.frontmatter.status = prevStatus
    spec.frontmatter.updatedAt = prevUpdatedAt
    spec.rawContent = prevRawContent
    actionError.value = err instanceof Error ? err.message : String(err)
  }

  // No reload: the spec object above is the one the board renders, and it is
  // already updated. Re-scanning the whole directory per status click cost k
  // full scans for one bypass toggle (see the newVerifyMode watcher).
}

function requestDeleteSpec(spec: SpecFile) {
  pendingDeleteSpec.value = spec
  confirmDeleteVisible.value = true
}

async function handleDelete(confirmed: boolean) {
  confirmDeleteVisible.value = false
  if (!confirmed || !pendingDeleteSpec.value) {
    pendingDeleteSpec.value = null
    return
  }

  const spec = pendingDeleteSpec.value
  pendingDeleteSpec.value = null

  const workspace = workspacesStore.contentWorkspace
  if (!workspace) return

  try {
    const fullPath = spec.path
    await invoke('plugin:canvas|delete_file', { path: fullPath })
    if (expandedSpecPath.value === spec.path) expandedSpecPath.value = null

    appStore.notifyFileDeleted(fullPath)
    actionError.value = ''
    await loadSpecs()
    refreshExplorer()
  } catch (err) {
    actionError.value = err instanceof Error ? err.message : String(err)
  }
}

watch(newVerifyMode, async (mode) => {
  if (mode !== 'bypass') return
  const verifySpecs = specs.value.filter(s => s.frontmatter.status === 'verify')
  for (const spec of verifySpecs) {
    await changeStatus(spec, 'open')
  }
})

watch(() => appStore.lastDeletedFile, (deleted) => {
  if (deleted?.filePath.endsWith('.spec.md')) {
    loadSpecs()
  }
})

watch(() => workspacesStore.contentWorkspaceId, () => {
  pendingSpec.value = null
  pendingDeleteSpec.value = null
  confirmOverwrite.value = false
  confirmDeleteVisible.value = false
  expandedSpecPath.value = null
  loadSpecs()
})

onMounted(() => {
  loadSpecs()
})

onUnmounted(() => {
  window.removeEventListener('mousemove', onMouseMove)
  window.removeEventListener('mouseup', onMouseUp)
  removeGhost()
})
</script>

<template>
  <div class="spec-dashboard flex flex-col h-full">
    <div class="px-3 py-2.5 flex-shrink-0 spec-create-form">
      <div class="flex items-center gap-1.5">
        <select
          v-model="newVerifyMode"
          class="spec-select text-[10px] rounded-md px-1.5 py-1.5 outline-none flex-shrink-0"
          :class="{ 'verify-active': newVerifyMode === 'verify' }"
        >
          <option value="bypass">Bypass</option>
          <option value="verify">Verify</option>
        </select>
        <input
          v-model="newTitle"
          type="text"
          placeholder="New spec title..."
          class="spec-input text-xs px-2.5 py-1.5 rounded-md outline-none flex-1 min-w-0 transition-all"
          @keydown.enter="createSpec"
        >
        <select
          v-model="newPriority"
          class="spec-select text-[10px] rounded-md px-1.5 py-1.5 outline-none"
        >
          <option value="critical">Critical</option>
          <option value="high">High</option>
          <option value="medium">Medium</option>
          <option value="low">Low</option>
        </select>
        <button
          class="spec-create-btn text-[10px] px-3 py-1.5 rounded-md transition-all flex-shrink-0 font-medium"
          :class="{ active: newTitle.trim(), disabled: !newTitle.trim() || creating }"
          :disabled="!newTitle.trim() || creating"
          @click="createSpec"
        >
          {{ creating ? '...' : '+ Create' }}
        </button>
        <button
          class="spec-refresh-btn text-[10px] px-1.5 py-1.5 rounded-md transition-all flex-shrink-0"
          :disabled="loading"
          title="Refresh specs"
          @click="loadSpecs"
        >
          <span :class="{ 'inline-block animate-spin': loading }">&#8635;</span>
        </button>
      </div>
    </div>

    <div
      v-if="confirmOverwrite"
      class="px-3 py-2 flex items-center gap-2 flex-shrink-0 text-[10px] spec-overwrite-bar"
    >
      <span class="text-red-400 flex-1">File already exists. Overwrite?</span>
      <button
        class="px-2 py-0.5 rounded text-red-400 spec-btn-outline"
        @click="handleOverwrite(true)"
      >
        Overwrite
      </button>
      <button
        class="px-2 py-0.5 rounded spec-btn-outline"
        @click="handleOverwrite(false)"
      >
        Cancel
      </button>
    </div>

    <div
      v-if="actionError"
      class="px-3 py-2 flex items-center gap-2 flex-shrink-0 text-[10px] spec-action-error-bar"
    >
      <span class="text-red-400 flex-1 truncate" :title="actionError">{{ actionError }}</span>
      <button
        class="px-2 py-0.5 rounded spec-btn-outline"
        @click="actionError = ''"
      >
        Dismiss
      </button>
    </div>

    <Teleport to="body">
      <div
        v-if="confirmDeleteVisible"
        class="fixed inset-0 z-[9999] flex items-center justify-center"
        style="background: rgba(0,0,0,0.5); backdrop-filter: blur(4px)"
        @click.self="handleDelete(false)"
      >
        <div class="spec-modal rounded-lg shadow-2xl p-5 min-w-[280px] max-w-[360px] space-y-3">
          <div class="text-xs font-bold" :style="{ color: 'var(--qc-text)' }">Delete Spec</div>
          <p class="text-xs leading-relaxed" :style="{ color: 'var(--qc-text-dim)' }">
            Are you sure you want to delete
            <span class="text-red-400 font-medium">"{{ pendingDeleteSpec?.frontmatter.title }}"</span>?
            This cannot be undone.
          </p>
          <div class="flex items-center justify-end gap-2 pt-1">
            <button
              class="text-[10px] px-3 py-1.5 rounded-md transition-colors spec-btn-outline"
              @click="handleDelete(false)"
            >
              Cancel
            </button>
            <button
              class="text-[10px] px-3 py-1.5 rounded-md transition-colors text-white bg-red-500 hover:bg-red-600"
              @click="handleDelete(true)"
            >
              Delete
            </button>
          </div>
        </div>
      </div>
    </Teleport>

    <div v-if="loading" class="flex-1 flex items-center justify-center text-xs" :style="{ color: 'var(--qc-text-dim)' }">
      <span class="animate-pulse">Loading specs...</span>
    </div>

    <div
      v-else-if="errorMsg"
      class="flex-1 flex items-center justify-center text-xs px-4 text-center"
      :style="{ color: 'var(--qc-text-dim)' }"
    >
      {{ errorMsg }}
    </div>

    <div v-else class="flex-1 min-h-0 overflow-hidden">
      <div class="spec-board h-full flex overflow-x-auto">
        <section
          v-for="status in activeColumns"
          :key="status"
          :ref="(el: any) => registerColumnEl(status, el as HTMLElement)"
          class="spec-column flex-1 min-w-[240px] min-h-0"
          :class="{ 'drag-over': dragOverStatus === status && draggedSpec?.frontmatter.status !== status }"
          :style="{
            '--column-accent': statusConfig[status].dotColor,
            '--column-border': statusConfig[status].borderColor,
          }"
        >
          <div class="spec-column-header">
            <div class="flex items-start gap-2 min-w-0">
              <span
                class="w-2.5 h-2.5 rounded-full flex-shrink-0 mt-1"
                :style="{ backgroundColor: statusConfig[status].dotColor }"
              />
              <div class="min-w-0">
                <div class="text-[11px] font-bold uppercase tracking-[0.14em]" :style="{ color: 'var(--qc-text-muted)' }">
                  {{ statusConfig[status].label }}
                </div>
              </div>
            </div>

            <span class="spec-count text-[10px]">{{ groupedSpecs[status].length }}</span>
          </div>

          <div
            v-if="isDragging && draggedSpec && draggedSpec.frontmatter.status !== status && dragOverStatus === status"
            class="spec-drop-indicator mx-3 mt-3"
          >
            <span class="text-[9px]">Drop to move to {{ statusConfig[status].label }}</span>
          </div>

          <div class="spec-column-body flex-1 min-h-0 overflow-y-auto">
            <div class="space-y-2">
              <div
                v-for="spec in groupedSpecs[status]"
                :key="spec.path"
                class="spec-card rounded-lg overflow-hidden transition-all duration-200"
                :class="{
                  expanded: expandedSpecPath === spec.path,
                  dragging: draggedSpec?.path === spec.path,
                }"
                :style="{ '--card-accent': statusConfig[spec.frontmatter.status].dotColor }"
              >
                <div
                  class="spec-card-header flex items-center gap-2 px-2.5 py-2 cursor-pointer"
                  @click="toggleExpand(spec.path)"
                >
                  <span
                    class="spec-drag-handle text-[8px] cursor-grab active:cursor-grabbing flex-shrink-0"
                    title="Drag between Open and Done"
                    @mousedown="onHandleMouseDown($event, spec)"
                  >&#9776;</span>

                  <span class="text-xs flex-1 truncate font-medium" :style="{ color: 'var(--qc-text)' }">
                    {{ spec.frontmatter.title }}
                  </span>

                  <span
                    v-if="spec.frontmatter.priority && priorityConfig[spec.frontmatter.priority]"
                    class="spec-priority-badge text-[8px] px-1.5 py-0.5 rounded-sm font-bold tracking-wider"
                    :style="{
                      color: priorityConfig[spec.frontmatter.priority].color,
                      backgroundColor: priorityConfig[spec.frontmatter.priority].bg,
                    }"
                  >
                    {{ priorityConfig[spec.frontmatter.priority].label }}
                  </span>

                  <span
                    v-if="spec.frontmatter.verifyMode"
                    class="spec-verify-badge text-[8px] px-1.5 py-0.5 rounded-sm font-bold tracking-wider"
                  >
                    VFY
                  </span>

                  <span
                    v-if="spec.frontmatter.assignedTo"
                    class="text-[9px] text-[#22d3ee]"
                  >
                    {{ spec.frontmatter.assignedTo }}
                  </span>

                  <span
                    v-if="spec.frontmatter.linkedFiles?.length"
                    class="spec-files-count text-[9px] flex items-center gap-0.5"
                  >
                    <span style="font-size: 7px">&#128196;</span>
                    {{ spec.frontmatter.linkedFiles.length }}
                  </span>

                  <span
                    class="text-[9px] transition-transform duration-200"
                    :style="{ color: 'var(--qc-text-dim)', transform: expandedSpecPath === spec.path ? 'rotate(180deg)' : 'rotate(0deg)' }"
                  >
                    &#9662;
                  </span>
                </div>

                <div
                  v-if="expandedSpecPath === spec.path"
                  class="spec-card-body px-2.5 py-2.5 space-y-2.5"
                >
                  <p class="text-[11px] leading-relaxed whitespace-pre-wrap" :style="{ color: 'var(--qc-text-muted)' }">{{ spec.content }}</p>

                  <div v-if="spec.frontmatter.linkedFiles?.length" class="space-y-1">
                    <div class="text-[9px] uppercase tracking-wider font-medium" :style="{ color: 'var(--qc-text-dim)' }">Linked files</div>
                    <div
                      v-for="f in spec.frontmatter.linkedFiles"
                      :key="f"
                      class="text-[10px] text-[#22d3ee] font-mono pl-2 py-0.5 rounded spec-linked-file"
                    >
                      {{ f }}
                    </div>
                  </div>

                  <div class="flex items-center gap-2 pt-1 spec-actions">
                    <div
                      class="spec-status-toggle"
                      :style="{ '--toggle-accent': statusConfig[spec.frontmatter.status].dotColor }"
                    >
                      <button
                        class="spec-status-toggle-btn"
                        :class="{ active: spec.frontmatter.status === 'open' }"
                        @click.stop="changeStatus(spec, 'open')"
                      >
                        Open
                      </button>
                      <button
                        v-if="newVerifyMode === 'verify'"
                        class="spec-status-toggle-btn"
                        :class="{ active: spec.frontmatter.status === 'verify' }"
                        @click.stop="changeStatus(spec, 'verify')"
                      >
                        Verify
                      </button>
                      <button
                        class="spec-status-toggle-btn"
                        :class="{ active: spec.frontmatter.status === 'done' }"
                        @click.stop="changeStatus(spec, 'done')"
                      >
                        Done
                      </button>
                    </div>

                    <div class="flex-1" />

                    <button
                      class="spec-action-btn text-[10px] px-2 py-1 rounded-md transition-all"
                      @click.stop="openInEditor(spec)"
                    >
                      Edit
                    </button>
                    <button
                      class="spec-action-btn spec-delete-btn text-[10px] px-1.5 py-1 rounded-md transition-all"
                      @click.stop="requestDeleteSpec(spec)"
                    >
                      &#10005;
                    </button>
                  </div>
                </div>
              </div>

              <div
                v-if="groupedSpecs[status].length === 0 && !(draggedSpec && dragOverStatus === status)"
                class="spec-empty-state"
              >
                <div class="text-[11px] font-medium">{{ statusConfig[status].emptyLabel }}</div>
                <div class="text-[10px] mt-1">{{ statusConfig[status].emptyHint }}</div>
              </div>
            </div>
          </div>
        </section>
      </div>
    </div>
  </div>
</template>

<style scoped>
.spec-dashboard {
  background: var(--qc-bg-window);
}

.spec-refresh-btn {
  color: var(--qc-text-dim);
  background: transparent;
  border: 1px solid transparent;
}
.spec-refresh-btn:hover {
  color: var(--qc-text-muted);
  background: var(--qc-bg-surface);
  border-color: var(--qc-border);
}

.spec-create-form {
  border-bottom: 1px solid var(--qc-border);
  background: var(--qc-bg-header);
}

.spec-input {
  background: var(--qc-bg);
  border: 1px solid var(--qc-border);
  color: var(--qc-text);
}
.spec-input:focus {
  border-color: var(--qc-accent);
  box-shadow: 0 0 0 1px var(--qc-accent);
}
.spec-input::placeholder {
  color: var(--qc-text-dim);
}

.spec-select {
  background: var(--qc-bg);
  border: 1px solid var(--qc-border);
  color: var(--qc-text);
}
.spec-select:focus {
  border-color: var(--qc-accent);
}
.spec-select.verify-active {
  border-color: var(--qc-border-subtle, var(--qc-border));
  color: var(--qc-text);
}

.spec-create-btn {
  color: var(--qc-text-dim);
  background: var(--qc-bg-surface);
  border: 1px solid var(--qc-border);
}
.spec-create-btn.active {
  color: #fff;
  background: var(--qc-accent);
  border-color: var(--qc-accent);
}
.spec-create-btn.active:hover {
  filter: brightness(1.1);
}
.spec-create-btn.disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.spec-overwrite-bar,
.spec-action-error-bar {
  border-bottom: 1px solid var(--qc-border);
  background: rgba(239, 68, 68, 0.05);
}

.spec-btn-outline {
  color: var(--qc-text-dim);
  border: 1px solid var(--qc-border);
}
.spec-btn-outline:hover {
  background: var(--qc-bg-surface);
}

.spec-modal {
  background: var(--qc-bg-header);
  border: 1px solid var(--qc-border);
}

.spec-board {
  align-items: stretch;
  min-height: 0;
  gap: 0;
}

.spec-column {
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: transparent;
  transition: background 0.2s ease;
}

.spec-column + .spec-column {
  border-left: 1px solid var(--qc-border);
}

.spec-column.drag-over {
  background: rgba(255, 255, 255, 0.02);
}

.spec-column-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--qc-border);
  background: transparent;
}

.spec-column-body {
  min-height: 0;
  padding: 12px;
}

.spec-count {
  color: var(--qc-text-dim);
  background: var(--qc-bg-surface);
  padding: 0 7px;
  border-radius: 999px;
  min-width: 22px;
  line-height: 18px;
  text-align: center;
  border: 1px solid var(--qc-border);
}

.spec-drop-indicator {
  border: 1px dashed var(--qc-border-subtle);
  border-radius: 8px;
  padding: 8px;
  text-align: center;
  color: var(--qc-text-dim);
  background: var(--qc-bg-surface);
  animation: pulse-border 1.5s ease infinite;
}

@keyframes pulse-border {
  0%, 100% { opacity: 0.6; }
  50% { opacity: 1; }
}

.spec-card {
  background: var(--qc-bg);
  border: 1px solid var(--qc-border);
  border-left: 2px solid var(--card-accent, var(--qc-border));
  position: relative;
}
.spec-card:hover {
  border-color: var(--qc-border-subtle);
  background: var(--qc-bg-header);
}
.spec-card.expanded {
  border-color: var(--qc-border-subtle);
}
.spec-card.dragging {
  opacity: 0.4;
  transform: scale(0.98);
}
.spec-card-header:hover {
  background: rgba(255, 255, 255, 0.02);
}

.spec-drag-handle {
  color: var(--qc-text-dim);
  opacity: 0.4;
  transition: opacity 0.15s, color 0.15s;
  line-height: 1;
}
.spec-card:hover .spec-drag-handle {
  opacity: 0.7;
}
.spec-drag-handle:hover {
  opacity: 1 !important;
  color: var(--qc-accent);
}

.spec-priority-badge {
  letter-spacing: 0.05em;
}

.spec-verify-badge {
  color: var(--qc-text-dim);
  background: var(--qc-bg-surface);
  letter-spacing: 0.05em;
}

.spec-files-count {
  color: var(--qc-text-dim);
}

.spec-card-body {
  border-top: 1px solid var(--qc-border);
  background: var(--qc-bg-header);
}

.spec-linked-file {
  background: rgba(34, 211, 238, 0.05);
}
.spec-linked-file:hover {
  background: rgba(34, 211, 238, 0.1);
}

.spec-actions {
  border-top: 1px solid var(--qc-border);
  padding-top: 8px;
}

.spec-status-toggle {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  padding: 2px;
  border-radius: 9px;
  border: 1px solid var(--qc-border);
  background: var(--qc-bg);
}

.spec-status-toggle-btn {
  border: none;
  background: transparent;
  color: var(--qc-text-dim);
  font-size: 10px;
  line-height: 1;
  padding: 5px 8px;
  border-radius: 7px;
  transition: background 0.15s ease, color 0.15s ease;
}

.spec-status-toggle-btn:hover {
  color: var(--qc-text);
}

.spec-status-toggle-btn.active {
  color: #fff;
  background: var(--toggle-accent, var(--qc-accent));
}

.spec-action-btn {
  color: var(--qc-text-dim);
  background: var(--qc-bg-surface);
  border: 1px solid var(--qc-border);
}
.spec-action-btn:hover {
  color: var(--qc-text);
  border-color: var(--qc-border-subtle);
}

.spec-delete-btn:hover {
  color: #ef4444;
  border-color: #ef444440;
  background: #ef444410;
}

.spec-empty-state {
  margin: 4px;
  padding: 20px 14px;
  border-radius: 12px;
  border: 1px dashed var(--qc-border);
  color: var(--qc-text-dim);
  text-align: center;
  background: rgba(255, 255, 255, 0.015);
}

.spec-dashboard ::-webkit-scrollbar {
  width: 4px;
  height: 4px;
}
.spec-dashboard ::-webkit-scrollbar-track {
  background: transparent;
}
.spec-dashboard ::-webkit-scrollbar-thumb {
  background: var(--qc-scrollbar-thumb);
  border-radius: 4px;
}
.spec-dashboard ::-webkit-scrollbar-thumb:hover {
  background: var(--qc-scrollbar-thumb-hover);
}
</style>
