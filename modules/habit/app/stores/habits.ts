import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import type { Check, DayState, Habit, HabitPatch, StatsOut } from '#habit/types'
import { todayKey, weekdayOf } from '#habit/utils/dates'

/**
 * The habits, the selected year's checks, and that year's statistics.
 *
 * Checks live in a Set of `habitId|day` keys — the grid asks "is this cell
 * ticked" a few thousand times per render and a Set answers in O(1).
 * Writes go through the plugin and update the Set optimistically; `load()`
 * reconciles whenever the backend announces a change from elsewhere.
 */
export const useHabitsStore = defineStore('habit/habits', () => {
  const habits = ref<Habit[]>([])
  const year = ref(new Date().getFullYear())
  const checkKeys = ref<Set<string>>(new Set())
  const stats = ref<StatsOut | null>(null)
  const loaded = ref(false)

  /** The day the left panel is checking off. Starts on today. */
  const selectedDay = ref(todayKey())

  /** The habit modal: closed, creating, or editing one habit. */
  const editor = ref<{ mode: 'create' } | { mode: 'edit'; id: string } | null>(null)

  const currentYear = computed(() => new Date().getFullYear())

  const key = (habitId: string, day: string) => `${habitId}|${day}`

  async function load() {
    try {
      const [list, checks, statsOut] = await Promise.all([
        invoke<Habit[]>('plugin:habit|list_habits'),
        invoke<Check[]>('plugin:habit|year_checks', { year: year.value }),
        invoke<StatsOut>('plugin:habit|stats', { year: year.value }),
      ])
      habits.value = list
      checkKeys.value = new Set(checks.map((c) => key(c.habitId, c.onDay)))
      stats.value = statsOut
      loaded.value = true
    } catch {
      /* browser development — the module renders empty */
    }
  }

  async function setYear(value: number) {
    year.value = value
    await load()
  }

  function selectDay(day: string) {
    selectedDay.value = day
  }

  function openCreate() {
    editor.value = { mode: 'create' }
  }

  function openEdit(id: string) {
    editor.value = { mode: 'edit', id }
  }

  function closeEditor() {
    editor.value = null
  }

  function isChecked(habitId: string, day: string): boolean {
    return checkKeys.value.has(key(habitId, day))
  }

  /**
   * What one cell is: before the habit's life, in the future, paused, open
   * or checked. Mirrors the crate's eligibility rules so the grid never
   * offers a click the backend would reject.
   */
  function dayState(habit: Habit, day: string): DayState {
    if (day < habit.startedOn) return 'before'
    if (day > todayKey()) return 'future'
    for (const p of habit.pauses) {
      if (p.fromOn <= day && (p.toOn === null || day <= p.toOn)) return 'paused'
    }
    if (((habit.daysMask >> weekdayOf(day)) & 1) === 0) return 'untracked'
    return isChecked(habit.id, day) ? 'checked' : 'open'
  }

  /** One day's completion across every habit that was trackable on it. */
  function dayProgress(day: string): { eligible: number; done: number; pct: number } {
    let eligible = 0
    let done = 0
    for (const habit of habits.value) {
      const state = dayState(habit, day)
      if (state === 'open') eligible += 1
      else if (state === 'checked') {
        eligible += 1
        done += 1
      }
    }
    return { eligible, done, pct: eligible > 0 ? Math.round((done / eligible) * 100) : 0 }
  }

  async function toggle(habit: Habit, day: string) {
    const state = dayState(habit, day)
    if (state !== 'open' && state !== 'checked') return
    const checked = state === 'open'
    const k = key(habit.id, day)
    // Optimistic — the grid must not lag behind a click.
    const next = new Set(checkKeys.value)
    if (checked) next.add(k)
    else next.delete(k)
    checkKeys.value = next
    try {
      await invoke('plugin:habit|set_check', { id: habit.id, day, checked })
      stats.value = await invoke<StatsOut>('plugin:habit|stats', { year: year.value })
    } catch {
      await load() // the backend disagreed — reconcile
    }
  }

  async function create(name: string, daysMask?: number) {
    await invoke<Habit>('plugin:habit|create_habit', { name, daysMask })
    await load()
  }

  async function update(id: string, patch: HabitPatch) {
    await invoke<Habit>('plugin:habit|update_habit', { id, patch })
    await load()
  }

  async function pause(id: string) {
    await invoke<Habit>('plugin:habit|pause_habit', { id })
    await load()
  }

  async function resume(id: string) {
    await invoke<Habit>('plugin:habit|resume_habit', { id })
    await load()
  }

  async function remove(id: string) {
    await invoke('plugin:habit|delete_habit', { id })
    await load()
  }

  /**
   * Drop a habit at a new list position (index into the list WITHOUT the
   * dragged habit). One write: the new sortIndex lands between the
   * neighbours' — the backend orders by sortIndex.
   */
  async function reorder(id: string, toIndex: number) {
    const rest = habits.value.filter((h) => h.id !== id)
    const clamped = Math.max(0, Math.min(toIndex, rest.length))
    const before = rest[clamped - 1]
    const after = rest[clamped]
    let sortIndex: number
    if (!before && !after) return
    else if (!before) sortIndex = after!.sortIndex - 1
    else if (!after) sortIndex = before.sortIndex + 1
    else sortIndex = (before.sortIndex + after.sortIndex) / 2
    // Reorder the local list immediately — a drop that snaps back and then
    // jumps a beat later reads as broken.
    const dragged = habits.value.find((h) => h.id === id)
    if (dragged) {
      dragged.sortIndex = sortIndex
      habits.value = [...rest.slice(0, clamped), dragged, ...rest.slice(clamped)]
    }
    await invoke('plugin:habit|update_habit', { id, patch: { sortIndex } })
    await load()
  }

  return {
    habits,
    year,
    currentYear,
    stats,
    loaded,
    selectedDay,
    editor,
    load,
    setYear,
    selectDay,
    openCreate,
    openEdit,
    closeEditor,
    isChecked,
    dayState,
    dayProgress,
    toggle,
    create,
    update,
    pause,
    resume,
    remove,
    reorder,
  }
})
