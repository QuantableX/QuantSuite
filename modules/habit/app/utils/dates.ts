/**
 * Day arithmetic for the year page. Everything is a LOCAL calendar day —
 * `Date` objects are built with local-time constructors only, and keys are
 * `YYYY-MM-DD`. No UTC anywhere: a habit is done "today" in the user's day.
 */

const pad = (n: number) => String(n).padStart(2, '0')

/** A local date as its day key, YYYY-MM-DD. */
export function dayKey(date: Date): string {
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`
}

export function todayKey(): string {
  return dayKey(new Date())
}

/** ISO 8601 week number (Monday-based; week 1 holds the first Thursday). */
export function isoWeek(date: Date): number {
  const d = new Date(date.getFullYear(), date.getMonth(), date.getDate())
  // Shift to the Thursday of this week; its year is the ISO year.
  const dow = (d.getDay() + 6) % 7 // Mo=0 … Su=6
  d.setDate(d.getDate() - dow + 3)
  const firstThursday = new Date(d.getFullYear(), 0, 4)
  const firstDow = (firstThursday.getDay() + 6) % 7
  firstThursday.setDate(firstThursday.getDate() - firstDow + 3)
  return 1 + Math.round((d.getTime() - firstThursday.getTime()) / (7 * 86_400_000))
}

export interface DayRow {
  key: string
  date: Date
  /** 0 = January. */
  month: number
  /** ISO calendar week. */
  week: number
  /** Mo=0 … Su=6. */
  weekday: number
}

/** Every day of one year, in order. */
export function daysOfYear(year: number): DayRow[] {
  const rows: DayRow[] = []
  const d = new Date(year, 0, 1)
  while (d.getFullYear() === year) {
    rows.push({
      key: dayKey(d),
      date: new Date(d),
      month: d.getMonth(),
      week: isoWeek(d),
      weekday: (d.getDay() + 6) % 7,
    })
    d.setDate(d.getDate() + 1)
  }
  return rows
}

export const MONTHS = [
  'January', 'February', 'March', 'April', 'May', 'June',
  'July', 'August', 'September', 'October', 'November', 'December',
] as const

export const WEEKDAYS = ['Mo', 'Tu', 'We', 'Th', 'Fr', 'Sa', 'Su'] as const

/** "01. Mo" — the row label: date first, then the weekday. */
export function rowLabel(row: DayRow): string {
  return `${pad(row.date.getDate())}. ${WEEKDAYS[row.weekday]}`
}

/** A day key back to a local Date. */
export function keyToDate(key: string): Date {
  const [y, m, d] = key.split('-').map(Number)
  return new Date(y!, m! - 1, d!)
}

/** A day key's weekday, Mo=0 … Su=6 — the bit index in a habit's daysMask. */
export function weekdayOf(key: string): number {
  return (keyToDate(key).getDay() + 6) % 7
}

/** "Mo, 01. September 2026" — the selected-day headline. */
export function longDayLabel(key: string): string {
  const d = keyToDate(key)
  return `${WEEKDAYS[(d.getDay() + 6) % 7]}, ${pad(d.getDate())}. ${MONTHS[d.getMonth()]} ${d.getFullYear()}`
}
