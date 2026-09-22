<script setup lang="ts">
/**
 * One row of the sidebar hierarchy, recursing into its children.
 *
 * Notion's shape: a disclosure arrow that only appears when there is something
 * to disclose, the icon, the title, and hover actions for "add child" and
 * "move to trash".
 */
import { useNotesStore } from '#notes/stores/notes'
import type { NoteTreeNode } from '#notes/types'

const props = defineProps<{ node: NoteTreeNode; depth: number }>()

const notes = useNotesStore()
const route = useRoute()
const router = useRouter()

const expanded = ref(props.depth === 0)
const isActive = computed(() => route.params.id === props.node.id)

async function addChild() {
  expanded.value = true
  const child = await notes.create({ parentId: props.node.id })
  router.push(`/notes/n/${child.id}`)
}

async function trash() {
  await notes.archive(props.node.id)
  if (isActive.value) router.push('/notes')
}
</script>

<template>
  <div class="qn-tn">
    <div class="qn-tn__row" :class="{ 'is-active': isActive }" :style="{ paddingLeft: `${6 + depth * 13}px` }">
      <button
        v-if="node.children.length"
        class="qn-tn__twisty"
        :class="{ 'is-open': expanded }"
        :aria-label="expanded ? 'Collapse' : 'Expand'"
        @click.stop="expanded = !expanded"
      >
        <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="m9 6 6 6-6 6" />
        </svg>
      </button>
      <span v-else class="qn-tn__twisty qn-tn__twisty--empty" />

      <button class="qn-tn__link" @click="router.push(`/notes/n/${node.id}`)">
        <NotesNoteIcon class="qn-tn__icon" :icon="node.icon" :size="13" />
        <span class="qn-tn__title">{{ node.title || 'Untitled' }}</span>
      </button>

      <span class="qn-tn__actions">
        <button aria-label="Add a nested note" title="Add a nested note" @click.stop="addChild">+</button>
        <button aria-label="Move to trash" title="Move to trash" @click.stop="trash">×</button>
      </span>
    </div>

    <div v-if="expanded && node.children.length">
      <NotesNoteTreeNode
        v-for="child in node.children"
        :key="child.id"
        :node="child"
        :depth="depth + 1"
      />
    </div>
  </div>
</template>

<style scoped>
.qn-tn__row {
  display: flex;
  align-items: center;
  gap: 2px;
  padding-right: 4px;
  border-radius: 6px;
}

.qn-tn__row:hover {
  background: var(--qn-bg-hover);
}

.qn-tn__row.is-active {
  background: var(--qn-bg-card);
}

.qn-tn__twisty {
  display: grid;
  place-items: center;
  flex-shrink: 0;
  width: 15px;
  height: 22px;
  border: none;
  background: transparent;
  color: var(--qn-text-muted);
  cursor: pointer;
  transition: transform var(--qss-dur-fast, 120ms) ease;
}

.qn-tn__twisty.is-open {
  transform: rotate(90deg);
}

.qn-tn__twisty--empty {
  cursor: default;
}

.qn-tn__link {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 2px;
  border: none;
  background: transparent;
  color: var(--qn-text-secondary);
  font-size: 13px;
  text-align: left;
  cursor: pointer;
}

.qn-tn__row.is-active .qn-tn__link {
  color: var(--qn-text);
}

.qn-tn__icon {
  flex-shrink: 0;
}

.qn-tn__title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qn-tn__actions {
  display: flex;
  gap: 1px;
  opacity: 0;
}

.qn-tn__row:hover .qn-tn__actions {
  opacity: 1;
}

.qn-tn__actions button {
  width: 18px;
  height: 18px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qn-text-muted);
  font-size: 13px;
  line-height: 1;
  cursor: pointer;
}

.qn-tn__actions button:hover {
  background: var(--qn-bg-input);
  color: var(--qn-text);
}
</style>
