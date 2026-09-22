<script setup lang="ts">
/**
 * One note, open: cover, icon, title, its properties, then the document.
 *
 * The property block sits between the title and the body the way Notion's
 * does — the same values the table and the board edit, in the same cells, so
 * there is one editor per property kind in the whole module.
 */
import { Editor, EditorContent } from '@tiptap/vue-3'
import StarterKit from '@tiptap/starter-kit'
import { markRaw, shallowRef } from 'vue'
import { useNotesStore } from '#notes/stores/notes'
import { useSchemaStore } from '#notes/stores/schema'
import { useDebouncedSave } from '#notes/composables/useDebouncedSave'
import type { Backlink } from '#notes/types'
import { NOTE_ICONS } from '#notes/utils/noteIcons'
import { invoke } from '@tauri-apps/api/core'

const props = defineProps<{ noteId: string }>()

const notes = useNotesStore()
const schema = useSchemaStore()
const debouncer = useDebouncedSave(700)

const editor = shallowRef<Editor | null>(null)
const titleDraft = ref('')
const backlinks = ref<Backlink[]>([])
let hydratedId: string | null = null

function fallbackContent() {
  return { type: 'doc', content: [{ type: 'paragraph', content: [] }] }
}

const note = computed(() => notes.openNote)
const saveState = computed(() => (notes.dirty ? 'Saving…' : 'Saved'))
const updatedLabel = computed(() => {
  const value = note.value?.updatedAt
  if (!value) return ''
  return new Date(value).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
})

watch(
  () => notes.openNote,
  (current) => {
    if (!current) {
      editor.value?.destroy()
      editor.value = null
      hydratedId = null
      return
    }

    titleDraft.value = current.title

    if (editor.value) {
      // A metadata refresh swaps openNote for the server row, whose content
      // can be older than what the debounced save has not flushed yet —
      // re-hydrate on a real note switch only.
      if (current.id === hydratedId) return
      hydratedId = current.id
      editor.value.commands.setContent(current.content ?? fallbackContent(), false)
      return
    }

    hydratedId = current.id
    editor.value = markRaw(
      new Editor({
        content: current.content ?? fallbackContent(),
        extensions: [StarterKit.configure({ heading: { levels: [1, 2, 3] } })],
        editorProps: { attributes: { class: 'qn-ne__doc tiptap' } },
        onUpdate: ({ editor: ed }) => {
          const open = notes.openNote
          if (!open) return
          const content = ed.getJSON()
          notes.dirty = true
          debouncer.schedule(`note-${open.id}`, async () => {
            await notes.saveContent(open.id, content as Record<string, any>)
          })
        },
      }),
    )
  },
  { immediate: true },
)

watch(
  () => props.noteId,
  async (id) => {
    if (!id) return
    if (id !== notes.openNote?.id) await notes.open(id)
    backlinks.value = await invoke<Backlink[]>('plugin:notes|list_backlinks', { id })
  },
  { immediate: true },
)

async function commitTitle() {
  const current = note.value
  if (!current) return
  const next = titleDraft.value.trim() || 'Untitled'
  if (next === current.title) return
  await notes.updateMeta(current.id, { title: next })
}

const iconMenu = ref(false)

async function setIcon(icon: string | null) {
  iconMenu.value = false
  if (note.value) await notes.updateMeta(note.value.id, { icon })
}

function toggleHeading(level: 1 | 2 | 3) {
  editor.value?.chain().focus().toggleHeading({ level }).run()
}

function toggleMark(action: 'bold' | 'italic' | 'strike') {
  const chain = editor.value?.chain().focus()
  if (!chain) return
  if (action === 'bold') chain.toggleBold().run()
  else if (action === 'italic') chain.toggleItalic().run()
  else chain.toggleStrike().run()
}

function toggleList(type: 'bullet' | 'ordered') {
  const chain = editor.value?.chain().focus()
  if (!chain) return
  if (type === 'bullet') chain.toggleBulletList().run()
  else chain.toggleOrderedList().run()
}

function toggleBlock(type: 'blockquote' | 'code') {
  const chain = editor.value?.chain().focus()
  if (!chain) return
  if (type === 'blockquote') chain.toggleBlockquote().run()
  else chain.toggleCodeBlock().run()
}

onBeforeUnmount(() => {
  editor.value?.destroy()
  editor.value = null
})
</script>

<template>
  <div v-if="note" class="qn-ne">
    <div class="qn-ne__scroll">
      <div class="qn-ne__shell">
        <div class="qn-ne__identity">
          <div class="qn-ne__icon-wrap">
            <button class="qn-ne__icon" aria-label="Change icon" @click="iconMenu = !iconMenu">
              <NotesNoteIcon :icon="note.icon" :size="36" :stroke="1.4" />
            </button>
            <div v-if="iconMenu" class="qn-ne__icon-menu">
              <button
                v-for="icon in NOTE_ICONS"
                :key="icon.key"
                :class="{ 'is-active': note.icon === icon.key }"
                :title="icon.label"
                @click="setIcon(icon.key)"
              >
                <NotesNoteIcon :icon="icon.key" :size="18" />
              </button>
              <button class="qn-ne__icon-clear" @click="setIcon(null)">Clear</button>
            </div>
          </div>

          <input
            v-model="titleDraft"
            class="qn-ne__title"
            placeholder="Untitled"
            @blur="commitTitle"
            @keydown.enter.prevent="commitTitle"
          />
        </div>

        <!-- The property block: the same cells the table and board edit. -->
        <div class="qn-ne__props">
          <div v-for="property in schema.properties" :key="property.id" class="qn-ne__prop">
            <span class="qn-ne__prop-name">{{ property.name }}</span>
            <NotesViewsPropertyCell
              :note-id="note.id"
              :property="property"
              :value="note.properties[property.id]"
            />
          </div>
        </div>

        <div class="qn-ne__meta">
          <span>{{ saveState }}</span>
          <span>{{ note.wordCount }} words</span>
          <span>Updated {{ updatedLabel }}</span>
        </div>

        <div class="qn-ne__toolbar">
          <button :class="{ 'is-active': editor?.isActive('heading', { level: 1 }) }" @click="toggleHeading(1)">H1</button>
          <button :class="{ 'is-active': editor?.isActive('heading', { level: 2 }) }" @click="toggleHeading(2)">H2</button>
          <button :class="{ 'is-active': editor?.isActive('heading', { level: 3 }) }" @click="toggleHeading(3)">H3</button>
          <span class="qn-ne__sep" />
          <button :class="{ 'is-active': editor?.isActive('bold') }" @click="toggleMark('bold')">Bold</button>
          <button :class="{ 'is-active': editor?.isActive('italic') }" @click="toggleMark('italic')">Italic</button>
          <button :class="{ 'is-active': editor?.isActive('strike') }" @click="toggleMark('strike')">Strike</button>
          <span class="qn-ne__sep" />
          <button :class="{ 'is-active': editor?.isActive('bulletList') }" @click="toggleList('bullet')">Bullets</button>
          <button :class="{ 'is-active': editor?.isActive('orderedList') }" @click="toggleList('ordered')">Numbers</button>
          <button :class="{ 'is-active': editor?.isActive('blockquote') }" @click="toggleBlock('blockquote')">Quote</button>
          <button :class="{ 'is-active': editor?.isActive('codeBlock') }" @click="toggleBlock('code')">Code</button>
        </div>

        <EditorContent v-if="editor" :editor="editor" class="qn-ne__editor" />

        <section v-if="backlinks.length" class="qn-ne__backlinks">
          <h2>Linked from</h2>
          <NuxtLink v-for="link in backlinks" :key="link.noteId" :to="`/notes/n/${link.noteId}`">
            <NotesNoteIcon :icon="link.icon" :size="14" />{{ link.title }}
          </NuxtLink>
        </section>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qn-ne {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--qn-bg);
}

.qn-ne__scroll {
  flex: 1;
  min-height: 0;
  overflow: auto;
}

.qn-ne__shell {
  width: min(820px, calc(100% - 48px));
  margin: 0 auto;
  padding: 40px 0 120px;
}

.qn-ne__identity {
  display: flex;
  align-items: center;
  gap: 12px;
}

.qn-ne__icon-wrap {
  position: relative;
}

.qn-ne__icon {
  display: grid;
  place-items: center;
  width: 46px;
  height: 46px;
  border: none;
  border-radius: 10px;
  background: transparent;
  color: var(--qn-text);
  padding: 0;
  cursor: pointer;
}

.qn-ne__icon:hover {
  background: var(--qn-bg-hover);
}

/* Fixed 32px cells rather than six fractions of a fixed panel width: the cell
   is the hit target for an 18px symbol, and the panel takes its width from the
   grid instead of the grid stretching to a panel. */
.qn-ne__icon-menu {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  z-index: 40;
  display: grid;
  grid-template-columns: repeat(6, 32px);
  gap: 4px;
  padding: 8px;
  border: 1px solid var(--qn-border);
  border-radius: 10px;
  background: var(--qn-bg-sidebar);
  box-shadow: 0 16px 36px rgba(0, 0, 0, 0.35);
}

.qn-ne__icon-menu button {
  display: grid;
  place-items: center;
  width: 32px;
  height: 32px;
  padding: 0;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qn-text-muted);
  line-height: 1;
  cursor: pointer;
}

.qn-ne__icon-menu button:hover {
  background: var(--qn-bg-hover);
  color: var(--qn-text);
}

.qn-ne__icon-menu button.is-active {
  background: var(--qn-bg-hover);
  color: var(--qn-accent);
}

.qn-ne__icon-clear {
  grid-column: 1 / -1;
  width: auto !important;
  height: 26px !important;
  margin-top: 2px;
  color: var(--qn-text-muted);
  font-size: 12px !important;
}

.qn-ne__title {
  flex: 1;
  min-width: 0;
  border: none;
  background: transparent;
  color: var(--qn-text);
  font-size: clamp(30px, 4vw, 42px);
  font-weight: 800;
  letter-spacing: -0.03em;
  outline: none;
}

.qn-ne__props {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 18px 0 14px;
}

.qn-ne__prop {
  display: grid;
  grid-template-columns: 140px 1fr;
  align-items: center;
  gap: 8px;
}

.qn-ne__prop-name {
  color: var(--qn-text-muted);
  font-size: 12px;
}

.qn-ne__meta {
  display: flex;
  gap: 14px;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--qn-border-subtle);
  color: var(--qn-text-muted);
  font-size: 11px;
}

.qn-ne__toolbar {
  position: sticky;
  top: 0;
  z-index: 3;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 2px;
  margin: 0 0 8px;
  padding: 8px 0;
  background: var(--qn-bg);
}

.qn-ne__toolbar button {
  padding: 5px 9px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qn-text-muted);
  font-size: 12px;
  cursor: pointer;
}

.qn-ne__toolbar button:hover {
  background: var(--qn-bg-hover);
  color: var(--qn-text);
}

.qn-ne__toolbar button.is-active {
  background: var(--qn-bg-card);
  color: var(--qn-text);
}

.qn-ne__sep {
  width: 1px;
  height: 16px;
  margin: 0 5px;
  background: var(--qn-border);
}

.qn-ne__editor {
  min-height: 400px;
}

.qn-ne__backlinks {
  margin-top: 48px;
  padding-top: 16px;
  border-top: 1px solid var(--qn-border-subtle);
}

.qn-ne__backlinks h2 {
  margin: 0 0 8px;
  color: var(--qn-text-muted);
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
}

.qn-ne__backlinks a {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border-radius: 6px;
  color: var(--qn-text-secondary);
  font-size: 13px;
  text-decoration: none;
}

.qn-ne__backlinks a:hover {
  background: var(--qn-bg-hover);
  color: var(--qn-text);
}
</style>

<style>
.qn-ne__doc {
  min-height: 100%;
  font-size: 16px;
  line-height: 1.75;
  outline: none;
}

.qn-ne__doc p {
  margin: 0.35em 0;
}

.qn-ne__doc h1 {
  margin: 1.2em 0 0.3em;
  font-size: 1.9em;
  font-weight: 800;
  letter-spacing: -0.03em;
}

.qn-ne__doc h2 {
  margin: 1.05em 0 0.25em;
  font-size: 1.5em;
  font-weight: 700;
  letter-spacing: -0.02em;
}

.qn-ne__doc h3 {
  margin: 0.9em 0 0.2em;
  font-size: 1.2em;
  font-weight: 700;
}

.qn-ne__doc ul,
.qn-ne__doc ol {
  margin: 0.45em 0;
  padding-left: 1.4em;
}

.qn-ne__doc blockquote {
  margin: 0.8em 0;
  padding-left: 0.9em;
  border-left: 3px solid var(--qn-border);
  color: var(--qn-text-secondary);
}

.qn-ne__doc code {
  padding: 0.08em 0.35em;
  border-radius: 5px;
  background: rgba(255, 255, 255, 0.08);
  font-size: 0.92em;
}

.qn-ne__doc pre {
  margin: 0.7em 0;
  padding: 12px 14px;
  border: 1px solid var(--qn-border);
  border-radius: 10px;
  background: rgba(0, 0, 0, 0.2);
}

.qn-ne__doc pre code {
  padding: 0;
  background: transparent;
}

.qn-ne__doc hr {
  margin: 1.2em 0;
  border: none;
  border-top: 1px solid var(--qn-border);
}
</style>
