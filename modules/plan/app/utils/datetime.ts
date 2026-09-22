/**
 * Local-date helpers. Every date the calendar reasons about is a *local* date;
 * UTC only appears at the IPC boundary.
 *
 * The one rule to keep: **never `new Date("2026-09-01")`.** A bare date string
 * parses as UTC midnight, which is the previous day for everyone west of
 * Greenwich — the single most common calendar bug there is. Use `fromKey`.
 */

/** `YYYY-MM-DD` for a local date. */
export function toKey(date: Date): string {
  const m = `${date.getMonth() + 1}`.padStart(2, '0')
  const d = `${date.getDate()}`.padStart(2, '0')
  return `${date.getFullYear()}-${m}-${d}`
}

/** A local Date at midnight from `YYYY-MM-DD`, built from the parts. */
export function fromKey(key: string): Date {
  const [y, m, d] = key.slice(0, 10).split('-').map(Number)
  return new Date(y ?? 1970, (m ?? 1) - 1, d ?? 1)
}

export function startOfDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate())
}

export function addDays(date: Date, days: number): Date {
  const out = new Date(date)
  out.setDate(out.getDate() + days)
  return out
}

export function addMonths(date: Date, months: number): Date {
  const out = new Date(date.getFullYear(), date.getMonth() + months, 1)
  // Keep the day where it exists, clamp where it does not — this is for
  // *navigation* ("next month"), not for recurrence, where skipping is right.
  const lastDay = new Date(out.getFullYear(), out.getMonth() + 1, 0).getDate()
  out.setDate(Math.min(date.getDate(), lastDay))
  return out
}

/** `weekStartsOn`: 0 = Sunday, 1 = Monday. */
export function startOfWeek(date: Date, weekStartsOn = 1): Date {
  const day = date.getDay()
  const diff = (day - weekStartsOn + 7) % 7
  return addDays(startOfDay(date), -diff)
}

export function isSameDay(a: Date, b: Date): boolean {
  return (
    a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate()
  )
}

export function isToday(date: Date): boolean {
  return isSameDay(date, new Date())
}

export function minutesSinceMidnight(date: Date): number {
  return date.getHours() * 60 + date.getMinutes()
}

/** Round to the nearest slot — what a drag on the grid snaps to. */
export function snapMinutes(minutes: number, slot: number): number {
  return Math.round(minutes / slot) * slot
}

export function formatTime(date: Date, format: '24h' | '12h' = '24h'): string {
  return date.toLocaleTimeString(undefined, {
    hour: format === '24h' ? '2-digit' : 'numeric',
    minute: '2-digit',
    hour12: format === '12h',
  })
}

export function formatHour(hour: number, format: '24h' | '12h' = '24h'): string {
  if (format === '12h') {
    const suffix = hour < 12 ? 'AM' : 'PM'
    const h = hour % 12 === 0 ? 12 : hour % 12
    return `${h} ${suffix}`
  }
  return `${`${hour}`.padStart(2, '0')}:00`
}

export function formatDay(date: Date, opts: Intl.DateTimeFormatOptions = {}): string {
  return date.toLocaleDateString(undefined, opts)
}

/** A local Date at `minutes` past midnight on `day`. */
export function atMinutes(day: Date, minutes: number): Date {
  const out = startOfDay(day)
  out.setMinutes(minutes)
  return out
}

/**
 * The start of an occurrence as a local Date.
 *
 * Timed occurrences carry a full ISO string with an offset, which `new Date`
 * parses correctly. All-day ones carry a bare date and must go through
 * `fromKey` — that split is the whole reason this helper exists.
 */
export function occurrenceStart(occ: { startsAt: string | null; startsOn: string | null }): Date {
  if (occ.startsAt) return new Date(occ.startsAt)
  if (occ.startsOn) return fromKey(occ.startsOn)
  return new Date()
}

export function occurrenceEnd(occ: {
  startsAt: string | null
  endsAt: string | null
  startsOn: string | null
  endsOn: string | null
}): Date {
  if (occ.endsAt) return new Date(occ.endsAt)
  if (occ.endsOn) return fromKey(occ.endsOn)
  return occurrenceStart(occ)
}

/** The four timestamp columns every occurrence carries. */
export interface OccurrenceTimes {
  startsAt: string | null
  endsAt: string | null
  startsOn: string | null
  endsOn: string | null
}

/**
 * How an occurrence sits inside ONE day.
 *
 * An event from 23:00 to 06:00 is one event but two blocks: the evening of one
 * day and the morning of the next. Every view that draws or labels a day needs
 * the same four facts about it, so they are worked out once, here.
 */
export function daySegment(
  occ: OccurrenceTimes,
  day: Date,
): { start: Date; end: Date; continuesBefore: boolean; continuesAfter: boolean } {
  const start = occurrenceStart(occ)
  const end = occurrenceEnd(occ)
  const dayStart = startOfDay(day)
  const dayEnd = addDays(dayStart, 1)
  return {
    start,
    end,
    continuesBefore: start < dayStart,
    // An end exactly at midnight closes this day rather than opening the next.
    continuesAfter: end > dayEnd,
  }
}

/**
 * Every local day key an occurrence touches — one for most events, more for
 * anything crossing midnight.
 *
 * The guard is not paranoia: a corrupt row with a decade-long span would
 * otherwise loop until the tab dies.
 */
export function spannedDayKeys(occ: OccurrenceTimes, limit = 400): string[] {
  if (!occ.startsAt) return [toKey(occurrenceStart(occ))]
  const end = occurrenceEnd(occ)
  const keys: string[] = []
  let day = startOfDay(occurrenceStart(occ))
  for (let i = 0; i < limit; i++) {
    keys.push(toKey(day))
    const next = addDays(day, 1)
    if (end <= next) break
    day = next
  }
  return keys
}

/** The browser's IANA zone, for stamping newly created events. */
export function localZone(): string {
  try {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || 'UTC'
  } catch {
    return 'UTC'
  }
}
