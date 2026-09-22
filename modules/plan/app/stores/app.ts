import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import type { AppSettings, ViewKind } from '#plan/types'
import { addDays, addMonths, startOfDay, startOfWeek } from '#plan/utils/datetime'

const defaultSettings: AppSettings = {
  defaultView: 'week',
  weekStartsOn: 1,
  slotMinutes: 15,
  timeFormat: '24h',
  timezone: 'local',
  showWeekends: true,
  sidebarLeftOpen: true,
  sidebarRightOpen: true,
}

/** UI state, the visible range, and persisted preferences. */
export const useAppStore = defineStore('plan/app', () => {
  const settings = ref<AppSettings>({ ...defaultSettings })
  const view = ref<ViewKind>('week')
  /** The day the view is anchored on — the cursor, not "today". */
  const cursor = ref<Date>(startOfDay(new Date()))
  const editorOpen = ref(false)

  const sidebarLeftOpen = computed(() => settings.value.sidebarLeftOpen)
  const sidebarRightOpen = computed(() => settings.value.sidebarRightOpen)

  /**
   * The days the current view renders.
   *
   * Month view returns a fixed six-week grid so the layout does not jump by a
   * row between months; agenda returns the next 30 days from the cursor.
   */
  const days = computed<Date[]>(() => {
    const start = cursor.value
    switch (view.value) {
      case 'day':
        return [start]
      case 'week': {
        const first = startOfWeek(start, settings.value.weekStartsOn)
        const all = Array.from({ length: 7 }, (_, i) => addDays(first, i))
        return settings.value.showWeekends ? all : all.filter((d) => d.getDay() !== 0 && d.getDay() !== 6)
      }
      case 'month': {
        const firstOfMonth = new Date(start.getFullYear(), start.getMonth(), 1)
        const gridStart = startOfWeek(firstOfMonth, settings.value.weekStartsOn)
        return Array.from({ length: 42 }, (_, i) => addDays(gridStart, i))
      }
      case 'agenda':
        return Array.from({ length: 30 }, (_, i) => addDays(start, i))
      default:
        return [start]
    }
  })

  /** The window to ask the crate for — a day wider than the grid on each side,
   * so an event that starts just outside it still draws its overlap. */
  const range = computed(() => {
    const list = days.value
    const from = addDays(list[0] ?? cursor.value, -1)
    const to = addDays(list[list.length - 1] ?? cursor.value, 2)
    return { from, to }
  })

  const title = computed(() => {
    const list = days.value
    switch (view.value) {
      case 'day':
        return cursor.value.toLocaleDateString(undefined, {
          weekday: 'long',
          day: 'numeric',
          month: 'long',
          year: 'numeric',
        })
      case 'week': {
        const first = list[0]!
        const last = list[list.length - 1]!
        // Inside one month only the bare day number leads. It is built by hand
        // rather than by dropping `month` from the options: day+year without a
        // month is not a format Intl has, and it falls back to nonsense like
        // "2026 (day: 30)".
        const left =
          first.getMonth() === last.getMonth()
            ? String(first.getDate())
            : first.toLocaleDateString(undefined, { day: 'numeric', month: 'short' })
        const right = last.toLocaleDateString(undefined, {
          day: 'numeric',
          month: 'short',
          year: 'numeric',
        })
        return `${left} – ${right}`
      }
      case 'month':
        return cursor.value.toLocaleDateString(undefined, { month: 'long', year: 'numeric' })
      case 'agenda':
        return `From ${cursor.value.toLocaleDateString(undefined, { day: 'numeric', month: 'long' })}`
      default:
        return ''
    }
  })

  function setView(next: ViewKind) {
    view.value = next
  }

  function setCursor(date: Date) {
    cursor.value = startOfDay(date)
  }

  function today() {
    cursor.value = startOfDay(new Date())
  }

  /** One step forward or back, in the unit the current view shows. */
  function step(direction: 1 | -1) {
    switch (view.value) {
      case 'day':
        cursor.value = addDays(cursor.value, direction)
        break
      case 'week':
        cursor.value = addDays(cursor.value, 7 * direction)
        break
      case 'month':
        cursor.value = addMonths(cursor.value, direction)
        break
      case 'agenda':
        cursor.value = addDays(cursor.value, 30 * direction)
        break
    }
  }

  async function loadSettings() {
    try {
      const loaded = await invoke<AppSettings>('plugin:plan|get_app_settings')
      settings.value = { ...defaultSettings, ...loaded }
      view.value = settings.value.defaultView
    } catch {
      settings.value = { ...defaultSettings }
    }
  }

  async function saveSettings() {
    settings.value = await invoke<AppSettings>('plugin:plan|update_app_settings', {
      settings: settings.value,
    })
  }

  function toggleSidebar(side: 'left' | 'right') {
    if (side === 'left') settings.value.sidebarLeftOpen = !settings.value.sidebarLeftOpen
    else settings.value.sidebarRightOpen = !settings.value.sidebarRightOpen
  }

  return {
    settings,
    view,
    cursor,
    editorOpen,
    sidebarLeftOpen,
    sidebarRightOpen,
    days,
    range,
    title,
    setView,
    setCursor,
    today,
    step,
    loadSettings,
    saveSettings,
    toggleSidebar,
  }
})
