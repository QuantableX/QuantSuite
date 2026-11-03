/**
 * Scheduled prompts (round 3, card 03): the words for a rule and a time,
 * and a new schedule's starting values. The engine is in the crate
 * (crate/src/schedules.rs); times are local wall-clock times.
 */
import type { Recurrence, Schedule } from '#pilot/types'

/** Monday-first, as the crate counts (1 = Monday … 7 = Sunday). */
export const DAYS = [
  { n: 1, short: 'Mon' },
  { n: 2, short: 'Tue' },
  { n: 3, short: 'Wed' },
  { n: 4, short: 'Thu' },
  { n: 5, short: 'Fri' },
  { n: 6, short: 'Sat' },
  { n: 7, short: 'Sun' },
]

/** `2026-11-04T08:00` → "Nov 4, 08:00". */
export function localLabel(at: string): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})T(\d{2}:\d{2})$/.exec(at.trim())
  if (!m) return at
  const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]))
  return `${d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })}, ${m[4]}`
}

/** "Every day at 08:00", "Weekdays at 08:00", "Mon, Wed at 18:30", "Every 6 hours from Nov 4, 08:00", "Once, Nov 4, 08:00". */
export function describeRecurrence(r: Recurrence): string {
  switch (r.kind) {
    case 'once':
      return `Once, ${localLabel(r.at)}`
    case 'daily':
      return `Every day at ${r.time}`
    case 'weekdays':
      return `Weekdays at ${r.time}`
    case 'days': {
      const names = DAYS.filter((d) => r.days.includes(d.n)).map((d) => d.short)
      if (names.length === 7) return `Every day at ${r.time}`
      return `${names.join(', ') || 'No day'} at ${r.time}`
    }
    case 'hours':
      return `Every ${r.every === 1 ? 'hour' : `${r.every} hours`} from ${localLabel(r.from)}`
  }
}

const pad = (n: number) => String(n).padStart(2, '0')
const hm = (d: Date) => `${pad(d.getHours())}:${pad(d.getMinutes())}`
const dayStart = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()

/** When a slot is, for a list: "Today 08:00", "Tomorrow 08:00", "Tue 08:00" (this week), "Nov 12, 08:00". */
export function whenLabel(ms: number, now = Date.now()): string {
  const d = new Date(ms)
  const days = Math.round((dayStart(d) - dayStart(new Date(now))) / 86_400_000)
  if (days === 0) return `Today ${hm(d)}`
  if (days === 1) return `Tomorrow ${hm(d)}`
  if (days === -1) return `Yesterday ${hm(d)}`
  if (days > 1 && days < 7) return `${d.toLocaleDateString('en-US', { weekday: 'short' })} ${hm(d)}`
  return `${d.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })}, ${hm(d)}`
}

/** "in 4m", "in 3h 20m", "in 2d" — how far off a slot is. */
export function inLabel(ms: number, now = Date.now()): string {
  const m = Math.max(0, Math.round((ms - now) / 60_000))
  if (m < 1) return 'now'
  if (m < 60) return `in ${m}m`
  const h = Math.floor(m / 60)
  if (h < 24) return m % 60 ? `in ${h}h ${m % 60}m` : `in ${h}h`
  return `in ${Math.round(h / 24)}d`
}

/** A local date and time for a datetime-local field: `YYYY-MM-DDTHH:MM`. */
export function localStamp(ms: number): string {
  const d = new Date(ms)
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${hm(d)}`
}

/** A rule of another kind that keeps the time the user already set. */
export function switchKind(r: Recurrence, kind: Recurrence['kind'], now = Date.now()): Recurrence {
  const time = 'time' in r ? r.time : 'at' in r ? r.at.slice(11) : 'from' in r ? r.from.slice(11) : '09:00'
  const nextHour = localStamp(Math.ceil((now + 60_000) / 3_600_000) * 3_600_000)
  const stamp = (t: string) => {
    // The next time this clock time comes round.
    const today = `${localStamp(now).slice(0, 11)}${t}`
    return new Date(today).getTime() > now ? today : localStamp(new Date(today).getTime() + 86_400_000)
  }
  switch (kind) {
    case 'once':
      return { kind, at: /^\d{2}:\d{2}$/.test(time) ? stamp(time) : nextHour }
    case 'daily':
    case 'weekdays':
      return { kind, time }
    case 'days':
      return { kind, days: r.kind === 'days' ? r.days : [1, 2, 3, 4, 5], time }
    case 'hours':
      return { kind, every: r.kind === 'hours' ? r.every : 6, from: /^\d{2}:\d{2}$/.test(time) ? stamp(time) : nextHour }
  }
}

/** A new schedule's starting values (the thread's when it comes from "Schedule this…"). */
export function blankSchedule(base: Partial<Schedule> = {}): Schedule {
  return {
    id: '',
    name: '',
    prompt: '',
    contextId: 'general',
    provider: '',
    model: '',
    effort: '',
    mode: '',
    instance: '',
    recurrence: { kind: 'weekdays', time: '09:00' },
    enabled: true,
    missed: 'skip',
    target: 'new',
    threadId: null,
    lastSlot: null,
    createdAt: 0,
    updatedAt: 0,
    nextDue: null,
    lastRun: null,
    ...base,
  }
}

/** A run's word for the history. */
export function runLabel(status: string, manual: boolean): string {
  const word: Record<string, string> = { started: 'Ran', skipped: 'Skipped', missed: 'Missed', failed: 'Failed', starting: 'Starting' }
  return `${word[status] ?? status}${manual && status === 'started' ? ' (run now)' : ''}`
}

/** A typed time as 24-hour `HH:MM`: "8" → "08:00", "830" → "08:30", "8:5" → "08:05", "18.45" → "18:45"; null if it is none. */
export function normalizeTime(raw: string): string | null {
  const s = raw.trim().replace(/[.h]/gi, ':')
  let h: number
  let m: number
  const colon = /^(\d{1,2}):(\d{1,2})$/.exec(s)
  if (colon) {
    h = Number(colon[1])
    m = Number(colon[2])
  } else if (/^\d{1,4}$/.test(s)) {
    if (s.length <= 2) {
      h = Number(s)
      m = 0
    } else {
      h = Number(s.slice(0, s.length - 2))
      m = Number(s.slice(-2))
    }
  } else return null
  if (h > 23 || m > 59) return null
  return `${pad(h)}:${pad(m)}`
}
