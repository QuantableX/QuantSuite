/**
 * UI state only — the two panels. localStorage: chrome preferences are per
 * machine, not session data.
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

export const useAppStore = defineStore('pilot/app', {
  state: () => ({
    sidebarLeftOpen: readBool('qp-sidebar-left', true),
    sidebarRightOpen: readBool('qp-sidebar-right', true),
  }),
  actions: {
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
