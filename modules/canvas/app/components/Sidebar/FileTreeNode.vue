<script setup lang="ts">
import type { FileNode, GitFileStatus } from '../../../shared/types'
import { getFileIcon, getFileStemAndExt } from '../../utils/fileIcons'

const props = defineProps<{
  node: FileNode
  depth: number
  gitStatusStyle: Record<GitFileStatus, { letter: string; color: string }>
  selectedPath?: string | null
  dropTargetPath?: string | null
}>()

const emit = defineEmits<{
  (e: 'toggle', node: FileNode): void
  (e: 'open', node: FileNode): void
  (e: 'contextmenu', event: MouseEvent, node: FileNode): void
  (e: 'new-file', node: FileNode): void
}>()

const paddingLeft = computed(() => `${props.depth * 16 + 8}px`)
const guideLeft = computed(() => `${props.depth * 16 + 14}px`)

const isSelected = computed(() => {
  if (!props.selectedPath || props.node.isDirectory) return false
  return props.node.path === props.selectedPath
})

const isDropTarget = computed(() => {
  return props.node.isDirectory && props.dropTargetPath === props.node.path
})

const fileIcon = computed(() => {
  if (props.node.isDirectory) return null
  return getFileIcon(props.node.name)
})

const fileParts = computed(() => {
  if (props.node.isDirectory) return null
  return getFileStemAndExt(props.node.name)
})

function onClick() {
  if (props.node.isDirectory) {
    emit('toggle', props.node)
  } else {
    emit('open', props.node)
  }
}

function onContext(e: MouseEvent) {
  emit('contextmenu', e, props.node)
}

const gitInfo = computed(() => {
  const status = props.node.gitStatus
  if (!status || status === 'clean') return null
  return props.gitStatusStyle[status]
})
</script>

<template>
  <div>
    <!-- Node row -->
    <div
      class="tree-row"
      :class="{
        'tree-row--folder': node.isDirectory,
        'tree-row--file': !node.isDirectory,
        'tree-row--selected': isSelected,
        'tree-row--drop-target': isDropTarget,
      }"
      :style="{ paddingLeft }"
      :data-path="node.path"
      :data-is-dir="node.isDirectory ? '1' : undefined"
      @click="onClick"
      @contextmenu.prevent="onContext"
    >
      <!-- Expand caret (directories only) -->
      <span
        v-if="node.isDirectory"
        class="tree-caret"
        :class="{ 'tree-caret--open': node.expanded }"
      >
        <svg width="10" height="10" viewBox="0 0 10 10" fill="currentColor">
          <path d="M3 2l4 3-4 3z" />
        </svg>
      </span>

      <!-- File icon (files only — folders have no icon, just caret + name) -->
      <span v-if="!node.isDirectory && fileIcon" class="tree-icon">
        <svg
          width="14" height="14" viewBox="0 0 24 24" fill="none"
          :stroke="fileIcon.color" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"
          v-html="fileIcon.svg"
        />
      </span>

      <!-- Name -->
      <span v-if="node.isDirectory" class="tree-name tree-name--folder">
        {{ node.name }}
      </span>
      <span v-else class="tree-name tree-name--file">
        <span class="tree-name-stem">{{ fileParts?.stem }}</span><span class="tree-name-ext">{{ fileParts?.ext }}</span>
      </span>

      <!-- Git status -->
      <span
        v-if="gitInfo"
        class="tree-git"
        :class="gitInfo.color"
      >
        {{ gitInfo.letter }}
      </span>
    </div>

    <!-- Children (expanded directories) with indent guide -->
    <div
      v-if="node.isDirectory && node.expanded && node.children"
      class="tree-children"
      :style="{ '--guide-left': guideLeft }"
    >
      <CanvasSidebarFileTreeNode
        v-for="child in node.children"
        :key="child.path"
        :node="child"
        :depth="depth + 1"
        :git-status-style="gitStatusStyle"
        :selected-path="selectedPath"
        :drop-target-path="dropTargetPath"
        @toggle="emit('toggle', $event)"
        @open="emit('open', $event)"
        @contextmenu="(event, n) => emit('contextmenu', event, n)"
        @new-file="emit('new-file', $event)"
      />
    </div>
  </div>
</template>

<style scoped>
.tree-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 2px 0;
  cursor: default;
  position: relative;
  border-right: 3px solid transparent;
  transition: background-color 0.15s, border-color 0.15s;
}

/* Folder rows */
.tree-row--folder {
  color: var(--qc-text-muted);
  padding-top: 2px;
  padding-bottom: 2px;
  transition: color 0.2s;
}

.tree-row--folder:hover {
  color: var(--qc-text);
}

/* File rows */
.tree-row--file:hover {
  background-color: color-mix(in srgb, var(--qc-text) 6%, transparent);
}

.tree-row--file:hover .tree-name-stem {
  font-weight: 600;
}

/* Selected file */
.tree-row--selected {
  background-color: color-mix(in srgb, var(--qc-text) 10%, transparent);
  border-right-color: var(--qc-accent, #6e8efb);
}

/* Drop target folder */
.tree-row--drop-target {
  background-color: color-mix(in srgb, var(--qc-accent, #6e8efb) 15%, transparent);
  outline: 1px dashed color-mix(in srgb, var(--qc-accent, #6e8efb) 50%, transparent);
  outline-offset: -1px;
  border-radius: 4px;
}


.tree-row--selected .tree-name-stem {
  font-weight: 600;
}

/* Caret */
.tree-caret {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 12px;
  flex-shrink: 0;
  opacity: 0.5;
  transition: transform 0.15s ease, opacity 0.15s;
}

.tree-caret--open {
  transform: rotate(90deg);
}

.tree-row--folder:hover .tree-caret {
  opacity: 0.8;
}

/* Icon (files only) */
.tree-icon {
  display: flex;
  align-items: center;
  flex-shrink: 0;
}

/* Name */
.tree-name {
  font-family: var(--qc-font-mono, 'SF Mono', 'Cascadia Code', 'Fira Code', 'JetBrains Mono', monospace);
  font-size: 0.8rem;
  font-weight: 400;
  letter-spacing: 0.02em;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
}

.tree-name--folder {
  color: var(--qc-text);
  font-weight: 500;
  flex: 1;
}

.tree-name--file {
  color: color-mix(in srgb, var(--qc-text) 82%, transparent);
  flex: 1;
}

.tree-name-stem {
  transition: font-weight 0.1s;
}

.tree-name-ext {
  opacity: 0.38;
}

/* Git status */
.tree-git {
  font-family: var(--qc-font-mono, monospace);
  font-size: 0.6rem;
  font-weight: 700;
  flex-shrink: 0;
  margin-right: 6px;
  letter-spacing: 0.02em;
}

/* Indent guide lines */
.tree-children {
  position: relative;
}

.tree-children::after {
  content: '';
  position: absolute;
  left: var(--guide-left, 14px);
  top: 0;
  bottom: 4px;
  width: 1px;
  background: var(--qc-text);
  opacity: 0.07;
  pointer-events: none;
}
</style>
