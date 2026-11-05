/**
 * UI state only — the two panels, and Conversation or Board in the main
 * area (card 21). localStorage: chrome preferences are per machine, not
 * session data.
 */
import { defineStore } from 'pinia'
import { usePilotStore } from '#pilot/stores/pilot'

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

export type PilotView = 'conversation' | 'board' | 'github' | 'crew' | 'schedules' | 'deploy'

export const useAppStore = defineStore('pilot/app', {
  state: () => ({
    sidebarLeftOpen: readBool('qp-sidebar-left', true),
    sidebarRightOpen: readBool('qp-sidebar-right', true),
    /** The main area: the open thread's conversation, the kanban board (card 21), GitHub (card 23), the crew graph (round 2, card 08) or the schedules (round 3, card 03). */
    view: (['board', 'github', 'crew', 'schedules', 'deploy'].includes(readText('qp-view')) ? readText('qp-view') : 'conversation') as PilotView,
    /** The board shown: a workspace id or `general`; empty until one was picked. */
    boardId: readText('qp-board'),
    /** The thread open beside the board, and the card selected on it. */
    boardPanel: null as string | null,
    boardCard: null as string | null,
  }),
  actions: {
    /** A tab shows itself: it also leaves the Usage page, which the bars can open from any tab. */
    setView(view: PilotView) {
      this.view = view
      writeText('qp-view', view)
      usePilotStore().page = 'thread'
    },
    /**
     * A thread being opened is shown: from a tab about something else, the
     * conversation takes over. The crew view follows the open thread instead.
     */
    showThread() {
      // Leaving the board by a thread leaves its panel too: coming back must not reopen another thread.
      if (this.view === 'board') this.boardPanel = null
      if (this.view !== 'conversation' && this.view !== 'crew') this.setView('conversation')
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
