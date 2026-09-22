import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import type {
  CalendarEvent,
  EventInput,
  EventPatch,
  Occurrence,
  SeriesScope,
} from '#plan/types'
import { spannedDayKeys } from '#plan/utils/datetime'

/**
 * The occurrences currently on screen.
 *
 * The store never holds "all events": it holds the expansion of the visible
 * range, refetched when the range moves. Recurrence lives in the crate, so a
 * month is one call — see `list_occurrences`.
 */
export const useEventsStore = defineStore('plan/events', () => {
  const occurrences = ref<Occurrence[]>([])
  const loading = ref(false)
  const selected = ref<Occurrence | null>(null)

  /** The range currently loaded, so a redundant navigation is a no-op. */
  const rangeFrom = ref<string | null>(null)
  const rangeTo = ref<string | null>(null)

  // Range loads are async and the user can page faster than they return; only
  // the newest may write.
  let requestSeq = 0

  /**
   * Occurrences grouped by local day key — what every view indexes into.
   *
   * A timed occurrence lands in EVERY day it touches, not just the one it
   * starts on: 23:00–06:00 is an evening on one day and a morning on the next,
   * and the second day has to be able to find it. The views clamp their own
   * half of it (see `daySegment`).
   *
   * All-day events stay keyed on their first day — the all-day strip lays a
   * multi-day span out by column, not by bucket.
   */
  const byDay = computed(() => {
    const map = new Map<string, Occurrence[]>()
    const push = (key: string, occ: Occurrence) => {
      const bucket = map.get(key)
      if (bucket) bucket.push(occ)
      else map.set(key, [occ])
    }
    for (const occ of occurrences.value) {
      if (occ.isAllDay) {
        const key = (occ.startsOn ?? '').slice(0, 10)
        if (key) push(key, occ)
        continue
      }
      for (const key of spannedDayKeys(occ)) push(key, occ)
    }
    return map
  })

  async function loadRange(from: Date, to: Date, force = false) {
    const fromIso = from.toISOString()
    const toIso = to.toISOString()
    if (!force && rangeFrom.value === fromIso && rangeTo.value === toIso) return

    const seq = ++requestSeq
    loading.value = true
    try {
      const rows = await invoke<Occurrence[]>('plugin:plan|list_occurrences', {
        from: fromIso,
        to: toIso,
        includeHidden: false,
      })
      if (seq !== requestSeq) return
      occurrences.value = rows
      rangeFrom.value = fromIso
      rangeTo.value = toIso
    } catch {
      // Browser development has no Tauri bridge; an empty grid is the honest
      // result there, not a thrown error the whole layout has to catch.
      if (seq === requestSeq) occurrences.value = []
    } finally {
      if (seq === requestSeq) loading.value = false
    }
  }

  /** Re-run the last range — after any write. */
  async function refresh() {
    if (!rangeFrom.value || !rangeTo.value) return
    await loadRange(new Date(rangeFrom.value), new Date(rangeTo.value), true)
  }

  async function getEvent(id: string) {
    return await invoke<CalendarEvent>('plugin:plan|get_event', { id })
  }

  async function create(event: EventInput) {
    const created = await invoke<CalendarEvent>('plugin:plan|create_event', { event })
    await refresh()
    return created
  }

  async function update(id: string, patch: EventPatch) {
    const updated = await invoke<CalendarEvent>('plugin:plan|update_event', { id, patch })
    await refresh()
    return updated
  }

  async function remove(id: string) {
    await invoke<boolean>('plugin:plan|delete_event', { id })
    if (selected.value?.eventId === id) selected.value = null
    await refresh()
  }

  /**
   * Apply a change to one instance of a series, at the scope the user picked.
   *
   * `this`      — an override keyed on the instance's original start.
   * `following` — cut the old series the day before, then start a new one.
   * `all`       — edit the series itself.
   */
  async function applyToSeries(occ: Occurrence, scope: SeriesScope, patch: EventPatch) {
    if (!occ.isRecurring || scope === 'all') {
      await update(occ.eventId, patch)
      return
    }

    if (scope === 'this') {
      await invoke<boolean>('plugin:plan|override_occurrence', {
        eventId: occ.eventId,
        occurrenceStart: occ.occurrenceStart,
        kind: 'moved',
        patch,
      })
      await refresh()
      return
    }

    // 'following': the tail becomes its own series so the head keeps its history.
    const original = await getEvent(occ.eventId)
    await invoke<boolean>('plugin:plan|truncate_series', {
      eventId: occ.eventId,
      occurrenceStart: occ.occurrenceStart,
    })
    await create({
      calendarId: patch.calendarId ?? original.calendarId,
      title: patch.title ?? original.title,
      description: patch.description ?? original.description,
      location: patch.location ?? original.location,
      startsAt: patch.startsAt ?? occ.startsAt,
      endsAt: patch.endsAt ?? occ.endsAt,
      startsOn: patch.startsOn ?? occ.startsOn,
      endsOn: patch.endsOn ?? occ.endsOn,
      isAllDay: patch.isAllDay ?? occ.isAllDay,
      tz: original.tz,
      rrule: patch.rrule ?? original.rrule,
      color: patch.color ?? original.color,
    })
  }

  async function deleteOccurrence(occ: Occurrence, scope: SeriesScope) {
    if (!occ.isRecurring || scope === 'all') {
      await remove(occ.eventId)
      return
    }
    if (scope === 'this') {
      await invoke<boolean>('plugin:plan|override_occurrence', {
        eventId: occ.eventId,
        occurrenceStart: occ.occurrenceStart,
        kind: 'cancelled',
        patch: null,
      })
    } else {
      await invoke<boolean>('plugin:plan|truncate_series', {
        eventId: occ.eventId,
        occurrenceStart: occ.occurrenceStart,
      })
    }
    if (selected.value?.occurrenceStart === occ.occurrenceStart) selected.value = null
    await refresh()
  }

  return {
    occurrences,
    loading,
    selected,
    byDay,
    loadRange,
    refresh,
    getEvent,
    create,
    update,
    remove,
    applyToSeries,
    deleteOccurrence,
  }
})
