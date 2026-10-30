/**
 * UI state only — the two panels, and Conversation or Board in the main
 * area (card 21). localStorage: chrome preferences are per machine, not
 * session data.
 */
import { defineStore } from 'pinia'

function readBool(key: string, fallback: boolean): boolean {
  try {
    const raw = localStorage.getItem(key)
    return raw === null ? fallback : raw === '1'
  } catch {
    return fallback
  }
}

function readText(key: string): string {
  try {
    return localStorage.getItem(key) ?? ''
  } catch {
    return ''
  }
}
function writeText(key: string, value: string) {
  try { localStorage.setItem(key, value) } catch { /* private mode */ }
}

export const useAppStore = defineStore('pilot/app', {
  state: () => ({
    sidebarLeftOpen: readBool('qp-sidebar-left', true),
    sidebarRightOpen: readBool('qp-sidebar-right', true),
    /** The main area: the open thread's conversation, the kanban board (card 21), or GitHub (card 23). */
    view: (['board', 'github'].includes(readText('qp-view')) ? readText('qp-view') : 'conversation') as 'conversation' | 'board' | 'github',
    /** The board shown: a workspace id or `general`; empty until one was picked. */
    boardId: readText('qp-board'),
    /** The thread open beside the board, and the card selected on it. */
    boardPanel: null as string | null,
    boardCard: null as string | null,
  }),
  actions: {
    setView(view: 'conversation' | 'board' | 'github') {
      this.view = view
      writeText('qp-view', view)
    },
    setBoard(id: string) {
      if (this.boardId !== id) this.boardCard = null
      this.boardId = id
      writeText('qp-board', id)
    },
    toggleSidebar(side: 'left' | 'right') {
      if (side === 'left') {
        this.sidebarLeftOpen = !this.sidebarLeftOpen
        try { localStorage.setItem('qp-sidebar-left', this.sidebarLeftOpen ? '1' : '0') } catch { /* private mode */ }
      } else {
        this.sidebarRightOpen = !this.sidebarRightOpen
        try { localStorage.setItem('qp-sidebar-right', this.sidebarRightOpen ? '1' : '0') } catch { /* private mode */ }
      }
    },
  },
})
