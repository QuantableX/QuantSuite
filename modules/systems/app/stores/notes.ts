import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'

/** The user's own notes per strategy, in the core settings (scope `systems`). */
const NOTES_SCOPE = 'systems'
const NOTES_KEY = 'strategyNotes'
const NOTES_DEV_KEY = 'qs-systems-strategy-notes'

export interface StrategyNote { text: string; updatedAt: string }
interface NotesDocument { version: 1; notes: Record<string, StrategyNote> }

function native() { return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window }

function readNotes(raw: unknown): NotesDocument {
  const notes: Record<string, StrategyNote> = {}
  const source = raw && typeof raw === 'object' ? (raw as { notes?: unknown }).notes : null
  if (source && typeof source === 'object') {
    for (const [id, note] of Object.entries(source as Record<string, unknown>)) {
      const text = (note as StrategyNote | null)?.text
      if (typeof text === 'string' && text) notes[id] = { text, updatedAt: String((note as StrategyNote).updatedAt ?? '') }
    }
  }
  return { version: 1, notes }
}
async function read(): Promise<NotesDocument> {
  const raw = native()
    ? await invoke('plugin:qs|get_setting', { scope: NOTES_SCOPE, key: NOTES_KEY })
    : JSON.parse(localStorage.getItem(NOTES_DEV_KEY) ?? 'null')
  return readNotes(raw)
}
async function write(value: NotesDocument) {
  if (native()) await invoke('plugin:qs|set_setting', { scope: NOTES_SCOPE, key: NOTES_KEY, value })
  else localStorage.setItem(NOTES_DEV_KEY, JSON.stringify(value))
}

export const useStrategyNotesStore = defineStore('systems/notes', () => {
  const notes = ref<Record<string, StrategyNote>>({})
  const loaded = ref(false)
  const error = ref<string | null>(null)
  const saving = ref(false)

  async function load(force = false) {
    if (loaded.value && !force) return
    try {
      notes.value = (await read()).notes
      loaded.value = true
      error.value = null
    } catch (e) { error.value = String(e instanceof Error ? e.message : e) }
  }

  function text(systemId: string): string { return notes.value[systemId]?.text ?? '' }

  /** Save one strategy's notes on the latest document, so another window's notes stay. Empty text removes them. */
  async function save(systemId: string, value: string): Promise<boolean> {
    saving.value = true
    try {
      const doc = await read()
      if (value.trim()) doc.notes[systemId] = { text: value, updatedAt: new Date().toISOString() }
      else delete doc.notes[systemId]
      await write(doc)
      notes.value = doc.notes
      loaded.value = true
      error.value = null
      return true
    } catch (e) {
      error.value = String(e instanceof Error ? e.message : e)
      return false
    } finally { saving.value = false }
  }

  return { notes, loaded, error, saving, load, text, save }
})
