import { nextTick, onBeforeUnmount, onDeactivated, ref, type Ref } from 'vue'
import { reorderWorkspace, type Workspace } from '@quantsuite/core'

/** Pointer gestures keep workspace sorting inside the WebView2 event loop. */
export function useWorkspaceReorder(workspaces: Ref<Workspace[]>, refresh: () => Promise<void>) {
  const list = ref<HTMLElement | null>(null)
  const draggedId = ref<string | null>(null)
  const drop = ref<{ id: string; edge: 'before' | 'after'; beforeId: string | null } | null>(null)
  const saving = ref(false)
  const error = ref('')
  const announcement = ref('')
  let press: { id: string; x: number; y: number; pointerId: number; el: HTMLElement } | null = null
  let pointer = { x: 0, y: 0 }
  let frame = 0
  let suppressClick = false

  function updateDrop() {
    drop.value = null
    const root = list.value
    const moving = workspaces.value.find(w => w.id === draggedId.value)
    if (!root || !moving) return
    const hit = document.elementFromPoint(pointer.x, pointer.y)?.closest<HTMLElement>('[data-workspace-id]')
    if (!hit || !root.contains(hit)) return
    const group = workspaces.value.filter(w => w.pinned === moving.pinned)
    const index = group.findIndex(w => w.id === hit.dataset.workspaceId)
    const target = group[index]
    if (!target || target.id === moving.id) return
    const rect = hit.getBoundingClientRect()
    const edge = pointer.y < rect.y + rect.height / 2 ? 'before' : 'after'
    drop.value = {
      id: target.id, edge,
      beforeId: edge === 'before' ? target.id : group.slice(index + 1).find(w => w.id !== moving.id)?.id ?? null,
    }
  }

  function autoScroll() {
    const root = list.value
    if (!root || !draggedId.value) return
    const rect = root.getBoundingClientRect()
    if (pointer.x >= rect.left && pointer.x <= rect.right && pointer.y >= rect.top && pointer.y <= rect.bottom) {
      const step = pointer.y < rect.top + 28 ? -7 : pointer.y > rect.bottom - 28 ? 7 : 0
      if (step) { root.scrollTop += step; updateDrop() }
    }
    frame = requestAnimationFrame(autoScroll)
  }

  function cleanup() {
    const previous = press
    press = null
    draggedId.value = null
    drop.value = null
    cancelAnimationFrame(frame)
    window.removeEventListener('pointermove', move)
    window.removeEventListener('pointerup', finish)
    window.removeEventListener('pointercancel', cancel)
    window.removeEventListener('keydown', escape)
    window.removeEventListener('blur', cancel)
    if (previous?.el.hasPointerCapture(previous.pointerId)) previous.el.releasePointerCapture(previous.pointerId)
  }

  function ignoreDragClick() {
    suppressClick = true
  }

  function cancel() {
    if (draggedId.value) ignoreDragClick()
    cleanup()
  }

  function escape(event: KeyboardEvent) {
    if (event.key === 'Escape') { event.preventDefault(); cancel() }
  }

  function start(event: PointerEvent, id: string) {
    if (event.button !== 0 || !event.isPrimary || saving.value) return
    cleanup()
    suppressClick = false
    error.value = ''
    press = { id, x: event.clientX, y: event.clientY, pointerId: event.pointerId, el: event.currentTarget as HTMLElement }
    press.el.setPointerCapture(event.pointerId)
    window.addEventListener('pointermove', move, { passive: false })
    window.addEventListener('pointerup', finish)
    window.addEventListener('pointercancel', cancel)
    window.addEventListener('keydown', escape)
    window.addEventListener('blur', cancel)
  }

  function move(event: PointerEvent) {
    if (!press || event.pointerId !== press.pointerId) return
    pointer = { x: event.clientX, y: event.clientY }
    if (!draggedId.value) {
      if (Math.hypot(pointer.x - press.x, pointer.y - press.y) < 5) return
      draggedId.value = press.id
      frame = requestAnimationFrame(autoScroll)
    }
    event.preventDefault()
    updateDrop()
  }

  async function save(id: string, beforeId: string | null) {
    if (saving.value) return
    saving.value = true
    error.value = ''
    announcement.value = ''
    try {
      await reorderWorkspace(id, beforeId)
      await refresh()
      announcement.value = `Moved ${workspaces.value.find(w => w.id === id)?.name ?? 'workspace'}.`
      await nextTick()
      list.value?.querySelector<HTMLElement>(`[data-workspace-id="${CSS.escape(id)}"]`)?.focus({ preventScroll: true })
    } catch (e) {
      error.value = `Could not save workspace order: ${String(e)}`
    } finally { saving.value = false }
  }

  function finish(event: PointerEvent) {
    if (!press || event.pointerId !== press.pointerId) return
    const id = draggedId.value
    pointer = { x: event.clientX, y: event.clientY }
    updateDrop()
    const target = drop.value
    if (id) ignoreDragClick()
    cleanup()
    if (id && target) void save(id, target.beforeId)
  }

  function click(event: MouseEvent, id: string, select: (id: string) => unknown) {
    if (suppressClick) { suppressClick = false; event.preventDefault(); return }
    void select(id)
  }

  function keydown(event: KeyboardEvent, id: string) {
    if (!event.altKey || !['ArrowUp', 'ArrowDown'].includes(event.key)) return
    event.preventDefault()
    const moving = workspaces.value.find(w => w.id === id)
    if (!moving || saving.value) return
    const group = workspaces.value.filter(w => w.pinned === moving.pinned)
    const index = group.findIndex(w => w.id === id)
    if (event.key === 'ArrowUp' && index > 0) void save(id, group[index - 1]!.id)
    if (event.key === 'ArrowDown' && index < group.length - 1) void save(id, group[index + 2]?.id ?? null)
  }

  onBeforeUnmount(cleanup)
  onDeactivated(cleanup)
  return { list, draggedId, drop, saving, error, announcement, start, click, keydown, cancel }
}
