<script setup lang="ts">
/**
 * The sidebar: the collection, the note hierarchy, and the plain routes.
 *
 * There is no workspace switcher — the module is one collection now, so the
 * top of the sidebar is a link to the views rather than a folder picker.
 */
import { useNotesStore } from '#notes/stores/notes'
import { useSchemaStore } from '#notes/stores/schema'

const notes = useNotesStore()
const schema = useSchemaStore()
const route = useRoute()
const router = useRouter()

const treeOpen = ref(true)

async function addRoot() {
  const note = await notes.create({})
  router.push(`/notes/n/${note.id}`)
}

function openView(id: string) {
  schema.setActiveView(id)
  router.push('/notes')
}

const onCollection = computed(() => route.path === '/notes')
</script>

<template>
  <div class="qn-ls">
    <section class="qn-ls__block">
      <NuxtLink to="/notes" class="qn-ls__item" :class="{ 'is-active': onCollection }">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M3 5h18v14H3z M3 10h18 M9 5v14" />
        </svg>
        <span>All notes</span>
        <span class="qn-ls__count">{{ notes.list.length }}</span>
      </NuxtLink>

      <button
        v-for="view in schema.views"
        :key="view.id"
        class="qn-ls__view"
        :class="{ 'is-active': onCollection && view.id === schema.activeView?.id }"
        @click="openView(view.id)"
      >
        {{ view.name }}
      </button>
    </section>

    <section class="qn-ls__block">
      <header class="qn-ls__head">
        <button class="qn-ls__head-btn" @click="treeOpen = !treeOpen">
          <svg width="9" height="9" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" :style="{ transform: treeOpen ? 'rotate(90deg)' : 'none' }" aria-hidden="true">
            <path d="m9 6 6 6-6 6" />
          </svg>
          <span>Pages</span>
        </button>
        <button class="qn-ls__head-add" aria-label="New note" title="New note" @click="addRoot">+</button>
      </header>

      <div v-if="treeOpen" class="qn-ls__tree">
        <NotesNoteTreeNode
          v-for="node in notes.tree"
          :key="node.id"
          :node="node"
          :depth="0"
        />
        <p v-if="!notes.tree.length" class="qn-ls__empty">No notes yet.</p>
      </div>
    </section>

    <section class="qn-ls__block qn-ls__block--tail">
      <!-- Suite notes (PLAN-V2 E3): the shared `notes/sections` document from
           core.db, the same one the shell drawer and the QuantHUD overlay edit.
           Suite-wide, so it is a plain route rather than part of the tree. -->
      <NuxtLink to="/notes/suite" class="qn-ls__item">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M4 5h16v14H4z M8 9h8 M8 13h5" />
        </svg>
        <span>Suite notes</span>
      </NuxtLink>

      <NuxtLink to="/notes/trash" class="qn-ls__item">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
          <path d="M4 7h16 M9 7V5h6v2 M6 7l1 13h10l1-13" />
        </svg>
        <span>Trash</span>
      </NuxtLink>
    </section>
  </div>
</template>

<style scoped>
.qn-ls {
  height: 100%;
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 8px 6px;
  overflow-y: auto;
}

.qn-ls__block {
  display: flex;
  flex-direction: column;
  gap: 1px;
  padding-bottom: 10px;
}

.qn-ls__block--tail {
  margin-top: auto;
  padding-top: 10px;
  border-top: 1px solid var(--qn-border-subtle);
}

.qn-ls__item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: 6px;
  color: var(--qn-text-secondary);
  font-size: 13px;
  text-decoration: none;
}

.qn-ls__item svg {
  flex-shrink: 0;
  width: 15px;
  height: 15px;
}

.qn-ls__item:hover {
  background: var(--qn-bg-hover);
  color: var(--qn-text);
}

.qn-ls__item.is-active,
.qn-ls__item.router-link-exact-active {
  background: var(--qn-bg-card);
  color: var(--qn-text);
}

.qn-ls__count {
  margin-left: auto;
  color: var(--qn-text-muted);
  font-size: 11px;
}

.qn-ls__view {
  padding: 4px 8px 4px 31px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qn-text-muted);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}

.qn-ls__view:hover {
  background: var(--qn-bg-hover);
  color: var(--qn-text-secondary);
}

.qn-ls__view.is-active {
  color: var(--qn-text);
}

.qn-ls__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 6px;
}

.qn-ls__head-btn {
  display: flex;
  align-items: center;
  gap: 5px;
  border: none;
  background: transparent;
  color: var(--qn-text-muted);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  cursor: pointer;
}

.qn-ls__head-btn svg {
  transition: transform var(--qss-dur-fast, 120ms) ease;
}

.qn-ls__head-add {
  width: 18px;
  height: 18px;
  border: none;
  border-radius: 4px;
  background: transparent;
  color: var(--qn-text-muted);
  font-size: 14px;
  line-height: 1;
  cursor: pointer;
}

.qn-ls__head-add:hover {
  background: var(--qn-bg-hover);
  color: var(--qn-text);
}

.qn-ls__empty {
  margin: 0;
  padding: 6px 10px;
  color: var(--qn-text-muted);
  font-size: 12px;
}
</style>
