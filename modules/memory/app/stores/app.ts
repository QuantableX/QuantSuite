/**
 * UI state only — panels and the editor's view mode. Persisted in
 * localStorage: chrome preferences are per machine, not vault data.
 */
import { defineStore } from 'pinia'
import { parseStarred } from '#memory/utils/library'

export type ViewMode = 'edit' | 'split' | 'preview'

function readBool(key: string, fallback: boolean): boolean {
  try {
    const raw = localStorage.getItem(key)
    return raw === null ? fallback : raw === '1'
  } catch {
    return fallback
  }
}

function readMode(key: string): ViewMode {
  try {
    const raw = localStorage.getItem(key)
    return raw === 'edit' || raw === 'preview' || raw === 'split' ? raw : 'split'
  } catch {
    return 'split'
  }
}

function readStarred(): string[] {
  try { return parseStarred(localStorage.getItem('qm-starred')) } catch { return [] }
}
function readLibraryLayout(): 'cards' | 'list' {
  try { return localStorage.getItem('qm-library-layout') === 'list' ? 'list' : 'cards' } catch { return 'cards' }
}

export const useAppStore = defineStore('memory/app', {
  state: () => ({
    sidebarLeftOpen: readBool('qm-sidebar-left', true),
    sidebarRightOpen: readBool('qm-sidebar-right', true),
    viewMode: readMode('qm-view-mode'),
    createOpen: false,
    createTitle: '',
    createScope: 'general',
    starred: readStarred(),
    libraryLayout: readLibraryLayout(),
  }),
  actions: {
    toggleStar(id: string) {
      this.starred = this.starred.includes(id) ? this.starred.filter(value => value !== id) : [...this.starred, id]
      try { localStorage.setItem('qm-starred', JSON.stringify(this.starred)) } catch { /* private mode */ }
    },
    setLibraryLayout(layout: 'cards' | 'list') {
      this.libraryLayout = layout
      try { localStorage.setItem('qm-library-layout', layout) } catch { /* private mode */ }
    },
    startCreate(scope: string, title = '') {
      this.createScope = scope
      this.createTitle = title
      this.createOpen = true
    },
    toggleSidebar(side: 'left' | 'right') {
      if (side === 'left') {
        this.sidebarLeftOpen = !this.sidebarLeftOpen
        try { localStorage.setItem('qm-sidebar-left', this.sidebarLeftOpen ? '1' : '0') } catch { /* private mode */ }
      } else {
        this.sidebarRightOpen = !this.sidebarRightOpen
        try { localStorage.setItem('qm-sidebar-right', this.sidebarRightOpen ? '1' : '0') } catch { /* private mode */ }
      }
    },
    setViewMode(mode: ViewMode) {
      this.viewMode = mode
      try { localStorage.setItem('qm-view-mode', mode) } catch { /* private mode */ }
    },
  },
})
