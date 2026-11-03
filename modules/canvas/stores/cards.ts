import { defineStore } from 'pinia'
import { ref } from 'vue'
import { listKanbanCards, onKanbanChanged, type KanbanCard } from '@quantsuite/core'

/**
 * The kanban cards the canvas shows as nodes (round 2, card 09), per board.
 * Loaded when a card window needs its board; kept current by the kanban
 * bus — no polling.
 */
export const useCardsStore = defineStore('canvas/cards', () => {
  const byBoard = ref<Record<string, Record<string, KanbanCard>>>({})
  const loading = new Set<string>()
  let off: (() => void) | null = null

  async function load(boardId: string): Promise<void> {
    if (!boardId || loading.has(boardId)) return
    loading.add(boardId)
    try {
      const cards = await listKanbanCards(boardId)
      byBoard.value[boardId] = Object.fromEntries(cards.map((c) => [c.id, c]))
    } catch {
      /* the board is unreachable; the node keeps its saved title */
    } finally {
      loading.delete(boardId)
    }
    off ??= onKanbanChanged(() => {
      for (const id of Object.keys(byBoard.value)) void load(id)
    })
  }

  function card(boardId: string, cardId: string): KanbanCard | null {
    if (!byBoard.value[boardId]) void load(boardId)
    return byBoard.value[boardId]?.[cardId] ?? null
  }

  return { byBoard, load, card }
})
