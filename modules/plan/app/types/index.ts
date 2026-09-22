/**
 * QuantPlan's shared types — the mirror of the crate's serde structs.
 *
 * Two shapes of event, deliberately kept apart: a **timed** event carries
 * `startsAt`/`endsAt` as UTC ISO strings, an **all-day** event carries
 * `startsOn`/`endsOn` as bare `YYYY-MM-DD` with `endsOn` INCLUSIVE. Collapsing
 * the two into one timestamp is how calendars end up showing whole-day events
 * on the wrong day.
 */

export type ViewKind = 'day' | 'week' | 'month' | 'agenda'

/** Keys into `--qp-cal-*`; never a hex value, which no theme change survives. */
export type CalendarColor =
  | 'red'
  | 'orange'
  | 'amber'
  | 'lime'
  | 'green'
  | 'mint'
  | 'teal'
  | 'cyan'
  | 'blue'
  | 'indigo'
  | 'purple'
  | 'pink'
  | 'brown'
  | 'slate'

export interface Calendar {
  id: string
  name: string
  color: CalendarColor
  isVisible: boolean
  isDefault: boolean
  sortIndex: number
  createdAt: string
  updatedAt: string
}

export interface CalendarEvent {
  id: string
  calendarId: string
  title: string
  description: string | null
  location: string | null
  startsAt: string | null
  endsAt: string | null
  startsOn: string | null
  endsOn: string | null
  isAllDay: boolean
  tz: string
  rrule: string | null
  color: CalendarColor | null
  createdAt: string
  updatedAt: string
}

export interface EventInput {
  calendarId: string
  title: string
  description?: string | null
  location?: string | null
  startsAt?: string | null
  endsAt?: string | null
  startsOn?: string | null
  endsOn?: string | null
  isAllDay: boolean
  tz?: string | null
  rrule?: string | null
  color?: CalendarColor | null
}

export interface EventPatch {
  calendarId?: string
  title?: string
  description?: string | null
  location?: string | null
  startsAt?: string | null
  endsAt?: string | null
  startsOn?: string | null
  endsOn?: string | null
  isAllDay?: boolean
  tz?: string
  rrule?: string | null
  color?: CalendarColor | null
}

/** One concrete instance, after recurrence expansion and overrides. */
export interface Occurrence {
  eventId: string
  /** The instance's ORIGINAL start — the key an override is stored under. */
  occurrenceStart: string
  startsAt: string | null
  endsAt: string | null
  startsOn: string | null
  endsOn: string | null
  isAllDay: boolean
  title: string
  description: string | null
  location: string | null
  calendarId: string
  color: CalendarColor
  tz: string
  rrule: string | null
  isRecurring: boolean
  isOverride: boolean
}

/** How a change to one instance of a series should be applied. */
export type SeriesScope = 'this' | 'following' | 'all'

export interface Reminder {
  id: string
  eventId: string
  minutesBefore: number
  kind: string
}

export interface FreeSlot {
  startsAt: string
  endsAt: string
}

export interface SearchHit {
  eventId: string
  title: string
  startsAt: string | null
  startsOn: string | null
  calendarId: string
}

export interface AppSettings {
  defaultView: ViewKind
  /** 0 = Sunday, 1 = Monday. */
  weekStartsOn: number
  slotMinutes: number
  timeFormat: '24h' | '12h'
  timezone: string
  showWeekends: boolean
  sidebarLeftOpen: boolean
  sidebarRightOpen: boolean
}

/** An occurrence placed in a day column by the overlap layout. */
export interface PlacedOccurrence {
  occurrence: Occurrence
  /** This block is the tail / head of one that crosses midnight. */
  continuesBefore: boolean
  continuesAfter: boolean
  /** Minutes from midnight, local — clamped to the day being drawn. */
  startMinutes: number
  endMinutes: number
  /** 0..1 fractions of the column width. */
  left: number
  width: number
  zIndex: number
}
