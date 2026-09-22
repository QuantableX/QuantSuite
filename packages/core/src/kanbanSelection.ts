/** UI board selection shared by the drawer and MCP; never changes workspace.active. */
const STORAGE_KEY = 'qss-kanban-board'
let selected: string | null = null
const listeners = new Set<(id: string) => void>()

function valid(id: unknown): id is string {
  return typeof id === 'string' && (id === 'general' || id.startsWith('core:workspace:'))
}

export function getSelectedKanbanBoard(): string | null {
  try {
    const saved = localStorage.getItem(STORAGE_KEY)
    if (valid(saved)) selected = saved
  } catch { /* UI selection still works without storage. */ }
  return selected
}

export function selectKanbanBoard(id: string): void {
  if (!valid(id) || id === selected) return
  selected = id
  try { localStorage.setItem(STORAGE_KEY, id) } catch { /* private mode */ }
  listeners.forEach(listener => listener(id))
}

function onStorage(event: StorageEvent) {
  if (event.key === STORAGE_KEY && valid(event.newValue)) selectKanbanBoard(event.newValue)
}

export function onKanbanBoardSelected(listener: (id: string) => void): () => void {
  if (!listeners.size && typeof window !== 'undefined') window.addEventListener('storage', onStorage)
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
    if (!listeners.size && typeof window !== 'undefined') window.removeEventListener('storage', onStorage)
  }
}
