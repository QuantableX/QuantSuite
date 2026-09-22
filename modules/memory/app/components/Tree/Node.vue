<script setup lang="ts">
/**
 * One folder of the vault tree, Obsidian-style — visually the same language
 * as QuantSpace's file explorer (canvas FileTreeNode): rotating caret, 16px
 * indent with guide lines, mono filenames with the extension dimmed, and an
 * accent edge on the open memory. Data comes from the index, not the
 * filesystem — the rows ARE memories, so clicking opens by id.
 */
import type { VaultFolder } from '#memory/types'

const props = defineProps<{
  folder: VaultFolder
  depth: number
  /** Collapsed folder paths — folders default to open. */
  collapsed: ReadonlySet<string>
  activeId: string | null
  /** The synthetic top node renders only its children, no row of its own. */
  root?: boolean
}>()

const emit = defineEmits<{
  (e: 'open', id: string): void
  (e: 'toggle', path: string): void
}>()

const open = computed(() => !props.collapsed.has(props.folder.path))
/** A workspace group in the gigabrain view — a scope, not a directory. */
const isScopeGroup = computed(
  () => props.folder.path.startsWith('scope:') && !props.folder.path.includes('/')
)
const rowPad = computed(() => `${props.depth * 16 + 8}px`)
const childPad = computed(() => `${(props.depth + (props.root ? 0 : 1)) * 16 + 8}px`)
const guideLeft = computed(() => `${props.depth * 16 + 14}px`)
const childDepth = computed(() => (props.root ? props.depth : props.depth + 1))

</script>

<template>
  <div>
    <button
      v-if="!root"
      :aria-expanded="open"
      class="qm-tree-row qm-tree-row--folder"
      :class="{ 'qm-tree-row--scope': isScopeGroup }"
      :style="{ paddingLeft: rowPad }"
      @click="emit('toggle', folder.path)"
    >
      <span class="qm-tree-caret" :class="{ 'qm-tree-caret--open': open }">
        <svg width="10" height="10" viewBox="0 0 10 10" fill="currentColor">
          <path d="M3 2l4 3-4 3z" />
        </svg>
      </span>
      <span class="qm-tree-name qm-tree-name--folder">{{ folder.name }}</span>
      <span class="qm-tree-count">{{ folder.files.length }}</span>
    </button>

    <div
      v-if="root || open"
      class="qm-tree-children"
      :class="{ 'qm-tree-children--root': root }"
      :style="root ? undefined : { '--qm-guide-left': guideLeft }"
    >
      <MemoryTreeNode
        v-for="sub in folder.folders"
        :key="sub.path"
        :folder="sub"
        :depth="childDepth"
        :collapsed="collapsed"
        :active-id="activeId"
        @open="emit('open', $event)"
        @toggle="emit('toggle', $event)"
      />

      <button
        v-for="file in folder.files"
        :key="file.id"
        class="qm-tree-row qm-tree-row--file"
        :class="{ 'qm-tree-row--active': file.id === activeId }"
        :style="{ paddingLeft: childPad }"
        :title="file.relPath"
        :aria-current="file.id === activeId ? 'page' : undefined"
        @click="emit('open', file.id)"
      >
        <span class="qm-tree-icon">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
            <polyline points="14 2 14 8 20 8" />
          </svg>
        </span>
        <span class="qm-tree-name qm-tree-name--file">
          <span class="qm-tree-stem">{{ file.title }}</span>
        </span>
        <span v-if="file.incomingLinks + file.outgoingLinks > 0" class="qm-tree-count">
          {{ file.incomingLinks + file.outgoingLinks }}
        </span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.qm-tree-row {
  width: 100%;
  border: 0;
  background: transparent;
  text-align: left;
  display: flex;
  align-items: center;
  gap: 6px;
  padding-top: 6px;
  padding-bottom: 6px;
  padding-right: 6px;
  position: relative;
  border-right: 3px solid transparent;
  cursor: pointer;
  transition: background-color 0.15s, border-color 0.15s, color 0.2s;
}

.qm-tree-row--folder {
  color: var(--qm-text-muted);
}
.qm-tree-row--folder:hover {
  color: var(--qm-text);
}

/* A scope root — the QuantSpace explorer's root row, verbatim: mono,
   uppercase, with the tree guide-lined beneath it. */
.qm-tree-row--scope {
  margin-top: 2px;
  padding-top: 4px;
  padding-bottom: 4px;
  color: var(--qm-text-muted);
}
.qm-tree-row--scope .qm-tree-name--folder {
  font-size: 11px;
  font-weight: 600;
  color: var(--qm-text);
  letter-spacing: 0.02em;
}

.qm-tree-row--file:hover {
  background-color: color-mix(in srgb, var(--qm-text) 6%, transparent);
}
.qm-tree-row--file:hover .qm-tree-stem {
  font-weight: 600;
}

.qm-tree-row--active {
  background-color: color-mix(in srgb, var(--qm-text) 10%, transparent);
  border-right-color: var(--qm-link);
}
.qm-tree-row--active .qm-tree-stem {
  font-weight: 600;
}

.qm-tree-caret {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 12px;
  flex-shrink: 0;
  opacity: 0.5;
  transition: transform 0.15s ease, opacity 0.15s;
}
.qm-tree-caret--open {
  transform: rotate(90deg);
}
.qm-tree-row--folder:hover .qm-tree-caret {
  opacity: 0.8;
}

.qm-tree-icon {
  display: flex;
  align-items: center;
  flex-shrink: 0;
  color: var(--qm-text-muted);
}

.qm-tree-name {
  font-size: 11px;
  letter-spacing: 0.02em;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  min-width: 0;
  flex: 1;
}

.qm-tree-name--folder {
  color: var(--qm-text);
  font-weight: 500;
}

.qm-tree-name--file {
  color: color-mix(in srgb, var(--qm-text) 82%, transparent);
}

.qm-tree-ext {
  opacity: 0.38;
}
.qm-tree-row:focus-visible { outline: 1px solid var(--qm-link); outline-offset: -1px; }

.qm-tree-count {
  flex-shrink: 0;
  font-size: 10px;
  color: var(--qm-text-muted);
  opacity: 0.7;
}

/* Indent guide line, exactly the explorer's. */
.qm-tree-children {
  position: relative;
}
.qm-tree-children:not(.qm-tree-children--root)::after {
  content: '';
  position: absolute;
  left: var(--qm-guide-left, 14px);
  top: 0;
  bottom: 4px;
  width: 1px;
  background: var(--qm-text);
  opacity: 0.07;
  pointer-events: none;
}
</style>
