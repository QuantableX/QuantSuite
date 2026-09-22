import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import type { Note, NotePatch, NoteSummary, NoteTreeNode, PropertyValue } from '#notes/types'

/**
 * The note collection.
 *
 * `list` is the whole collection, loaded once and patched in place — every
 * view renders from it, so a table, a board and the sidebar tree all show the
 * same rows without three round trips. Only the open note carries its TipTap
 * document; summaries stay light enough that holding all of them is cheaper
 * than re-querying per view switch.
 */
export const useNotesStore = defineStore('notes/notes', () => {
  const list = ref<NoteSummary[]>([])
  const archived = ref<NoteSummary[]>([])
  const openNote = ref<Note | null>(null)
  const loading = ref(false)
  const dirty = ref(false)

  /** The sidebar's hierarchy. Notes with a missing parent surface at the root
   * rather than vanishing — a dangling parentId must not hide a note. */
  const tree = computed<NoteTreeNode[]>(() => {
    const nodes = new Map<string, NoteTreeNode>()
    for (const note of list.value) nodes.set(note.id, { ...note, children: [] })

    const roots: NoteTreeNode[] = []
    for (const node of nodes.values()) {
      const parent = node.parentId ? nodes.get(node.parentId) : undefined
      if (parent) parent.children.push(node)
      else roots.push(node)
    }

    const sort = (items: NoteTreeNode[]) => {
      items.sort((a, b) => a.sortIndex - b.sortIndex)
      for (const item of items) sort(item.children)
    }
    sort(roots)
    return roots
  })

  const byId = computed(() => {
    const map = new Map<string, NoteSummary>()
    for (const note of list.value) map.set(note.id, note)
    return map
  })

  function replace(summary: NoteSummary) {
    const at = list.value.findIndex((n) => n.id === summary.id)
    if (summary.isArchived) {
      if (at !== -1) list.value.splice(at, 1)
      return
    }
    if (at === -1) list.value.push(summary)
    else list.value[at] = summary
    if (openNote.value?.id === summary.id) {
      // Keep the document: the summary is authoritative for everything else,
      // but a metadata refresh must not clobber unsaved editor content.
      openNote.value = { ...openNote.value, ...summary }
    }
  }

  async function loadList() {
    loading.value = true
    try {
      list.value = await invoke<NoteSummary[]>('plugin:notes|list_notes', { includeArchived: false })
    } finally {
      loading.value = false
    }
  }

  async function loadArchived() {
    const all = await invoke<NoteSummary[]>('plugin:notes|list_notes', { includeArchived: true })
    archived.value = all.filter((n) => n.isArchived)
  }

  async function open(id: string) {
    openNote.value = await invoke<Note>('plugin:notes|get_note', { id })
    dirty.value = false
    return openNote.value
  }

  function close() {
    openNote.value = null
    dirty.value = false
  }

  async function create(options: { title?: string; parentId?: string | null; properties?: Record<string, PropertyValue> } = {}) {
    const summary = await invoke<NoteSummary>('plugin:notes|create_note', {
      title: options.title ?? null,
      parentId: options.parentId ?? null,
      properties: options.properties ?? null,
    })
    list.value.push(summary)
    return summary
  }

  async function updateMeta(id: string, patch: NotePatch) {
    replace(await invoke<NoteSummary>('plugin:notes|update_note_meta', { id, patch }))
  }

  async function setProperty(id: string, propertyId: string, value: PropertyValue) {
    // Optimistic: a select dropdown or a checkbox must not wait a round trip
    // to redraw, and the authoritative row replaces this a tick later.
    const local = byId.value.get(id)
    if (local) {
      const next = { ...local.properties }
      if (value === null || value === undefined) delete next[propertyId]
      else next[propertyId] = value
      replace({ ...local, properties: next })
    }
    replace(await invoke<NoteSummary>('plugin:notes|set_note_property', { id, propertyId, value }))
  }

  async function saveContent(id: string, content: Record<string, any>) {
    const summary = await invoke<NoteSummary>('plugin:notes|save_note_content', { id, content })
    replace(summary)
    if (openNote.value?.id === id) openNote.value = { ...openNote.value, ...summary, content }
    dirty.value = false
  }

  async function move(id: string, parentId: string | null, sortIndex: number) {
    replace(await invoke<NoteSummary>('plugin:notes|move_note', { id, parentId, sortIndex }))
    // A move can reparent a whole subtree; the cheap correct thing is a reload.
    await loadList()
  }

  async function archive(id: string) {
    await invoke<boolean>('plugin:notes|archive_note', { id })
    if (openNote.value?.id === id) close()
    await loadList()
  }

  async function restore(id: string) {
    await invoke<boolean>('plugin:notes|restore_note', { id })
    await Promise.all([loadList(), loadArchived()])
  }

  async function remove(id: string) {
    await invoke<boolean>('plugin:notes|delete_note', { id })
    if (openNote.value?.id === id) close()
    await Promise.all([loadList(), loadArchived()])
  }

  async function emptyTrash() {
    const removed = await invoke<number>('plugin:notes|empty_trash')
    await Promise.all([loadList(), loadArchived()])
    return removed
  }

  function reset() {
    list.value = []
    archived.value = []
    close()
  }

  return {
    list,
    archived,
    openNote,
    loading,
    dirty,
    tree,
    byId,
    loadList,
    loadArchived,
    open,
    close,
    create,
    updateMeta,
    setProperty,
    saveContent,
    move,
    archive,
    restore,
    remove,
    emptyTrash,
    reset,
  }
})
