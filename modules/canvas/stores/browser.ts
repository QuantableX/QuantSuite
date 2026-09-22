import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type { BrowserHistoryEntry, BrowserBookmark, SearchEngine } from '../shared/types'

// The one cap: addHistoryEntry trims to it, and disk mirrors memory. Capping
// again on save would silently drop what a previous session already persisted —
// and searchHistory promises the whole list, not the newest slice.
const MAX_HISTORY_ENTRIES = 10000

// Navigation arrives in bursts (redirects, link-chasing); five seconds folds a
// burst into a single write. Accepted limit: closing the app inside that window
// loses the navigation that scheduled the pending write.
const HISTORY_SAVE_DEBOUNCE_MS = 5000

export const useBrowserStore = defineStore('canvas/browser', () => {
  // ---- State ----
  const history = ref<BrowserHistoryEntry[]>([])
  const bookmarks = ref<BrowserBookmark[]>([])
  const searchEngine = ref<SearchEngine>(
    (typeof localStorage !== 'undefined' && localStorage.getItem('qc-search-engine') as SearchEngine) || 'google'
  )
  const loaded = ref(false)

  // ---- Actions ----

  function addHistoryEntry(entry: BrowserHistoryEntry): void {
    // Deduplicate: skip if same URL was visited within last 5 seconds
    const recent = history.value[0]
    if (recent && recent.url === entry.url) {
      const diff = new Date(entry.visitedAt).getTime() - new Date(recent.visitedAt).getTime()
      if (diff < 5000) return
    }
    history.value.unshift(entry)
    // Cap at 10000 entries
    if (history.value.length > MAX_HISTORY_ENTRIES) {
      history.value = history.value.slice(0, MAX_HISTORY_ENTRIES)
    }
    debouncedSave()
  }

  function clearHistory(): void {
    history.value = []
    saveNow()
  }

  function searchHistory(query: string): BrowserHistoryEntry[] {
    const q = query.toLowerCase()
    return history.value.filter(
      e => e.url.toLowerCase().includes(q) || e.title.toLowerCase().includes(q)
    )
  }

  function addBookmark(bookmark: BrowserBookmark): void {
    // Don't add duplicate URLs
    if (bookmarks.value.some(b => b.url === bookmark.url)) return
    bookmarks.value.push(bookmark)
    saveNow()
  }

  function removeBookmark(id: string): void {
    const index = bookmarks.value.findIndex(b => b.id === id)
    if (index !== -1) {
      bookmarks.value.splice(index, 1)
      saveNow()
    }
  }

  function isBookmarked(url: string): boolean {
    return bookmarks.value.some(b => b.url === url)
  }

  function getBookmarkByUrl(url: string): BrowserBookmark | undefined {
    return bookmarks.value.find(b => b.url === url)
  }

  function getBookmarkFolders(): string[] {
    const folders = new Set<string>()
    bookmarks.value.forEach(b => { if (b.folder) folders.add(b.folder) })
    return Array.from(folders)
  }

  function setSearchEngine(engine: SearchEngine): void {
    searchEngine.value = engine
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('qc-search-engine', engine)
    }
  }

  // ---- Persistence via Tauri ----

  let saveTimeout: ReturnType<typeof setTimeout> | null = null

  function debouncedSave(): void {
    if (saveTimeout) clearTimeout(saveTimeout)
    saveTimeout = setTimeout(() => {
      saveTimeout = null
      saveToDisk()
    }, HISTORY_SAVE_DEBOUNCE_MS)
  }

  // Bookmark edits and "clear history" are rare and deliberate — no reason to
  // leave them five seconds away from disk behind the navigation debounce.
  function saveNow(): void {
    if (saveTimeout) {
      clearTimeout(saveTimeout)
      saveTimeout = null
    }
    void saveToDisk()
  }

  async function saveToDisk(): Promise<void> {
    try {
      const data = JSON.stringify({
        history: history.value,
        bookmarks: bookmarks.value,
      })
      await invoke('plugin:canvas|save_browser_data', { data })
    } catch (error) {
      console.error('Failed to save browser data:', error)
    }
  }

  async function loadFromDisk(): Promise<void> {
    if (loaded.value) return
    try {
      const raw = await invoke<string>('plugin:canvas|load_browser_data')
      const data = JSON.parse(raw)
      if (Array.isArray(data.history)) history.value = data.history
      if (Array.isArray(data.bookmarks)) bookmarks.value = data.bookmarks
      loaded.value = true
    } catch (error) {
      console.error('Failed to load browser data:', error)
      loaded.value = true
    }
  }

  return {
    // State
    history,
    bookmarks,
    searchEngine,
    loaded,
    // Actions
    addHistoryEntry,
    clearHistory,
    searchHistory,
    addBookmark,
    removeBookmark,
    isBookmarked,
    getBookmarkByUrl,
    getBookmarkFolders,
    setSearchEngine,
    loadFromDisk,
    saveToDisk,
  }
})
