import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import type { Calendar, CalendarColor } from '#plan/types'

export const useCalendarsStore = defineStore('plan/calendars', () => {
  const list = ref<Calendar[]>([])

  const byId = computed(() => {
    const map = new Map<string, Calendar>()
    for (const c of list.value) map.set(c.id, c)
    return map
  })

  const visibleIds = computed(() => new Set(list.value.filter((c) => c.isVisible).map((c) => c.id)))

  const defaultCalendar = computed(() => list.value.find((c) => c.isDefault) ?? list.value[0] ?? null)

  async function load() {
    list.value = await invoke<Calendar[]>('plugin:plan|list_calendars')
  }

  async function create(name: string, color: CalendarColor) {
    const calendar = await invoke<Calendar>('plugin:plan|create_calendar', { name, color })
    list.value.push(calendar)
    return calendar
  }

  async function update(
    id: string,
    patch: { name?: string; color?: CalendarColor; isVisible?: boolean; sortIndex?: number },
  ) {
    // Optimistic: toggling a calendar off has to redraw the grid immediately,
    // not a round trip later.
    const at = list.value.findIndex((c) => c.id === id)
    if (at !== -1) list.value[at] = { ...list.value[at]!, ...patch }
    const updated = await invoke<Calendar>('plugin:plan|update_calendar', {
      id,
      name: patch.name ?? null,
      color: patch.color ?? null,
      isVisible: patch.isVisible ?? null,
      sortIndex: patch.sortIndex ?? null,
    })
    if (at !== -1) list.value[at] = updated
    return updated
  }

  async function remove(id: string) {
    await invoke<boolean>('plugin:plan|delete_calendar', { id })
    list.value = list.value.filter((c) => c.id !== id)
  }

  return { list, byId, visibleIds, defaultCalendar, load, create, update, remove }
})
