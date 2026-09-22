import type { Occurrence, PlacedOccurrence } from '#plan/types'
import { daySegment, minutesSinceMidnight } from '#plan/utils/datetime'

/**
 * Placing overlapping events side by side in a day column — the one piece of
 * real geometry in the time grid.
 *
 * A pure function over its input, so it is cheap to test and does not need a
 * rendered grid to reason about.
 *
 * The algorithm, per column:
 *
 *   1. Sort by start; on a tie the longer event first, so the big block is the
 *      one that gets column 0 and the short ones stack to its right.
 *   2. Group into **clusters**: a run of events where each one starts before
 *      the cluster's running maximum end. Two events that merely touch
 *      (10:00–11:00 and 11:00–12:00) are not in the same cluster.
 *   3. Inside a cluster assign columns greedily — the first column whose last
 *      event has already ended.
 *   4. Width is `1 / columnsInCluster`, left is `column * width`.
 *
 * Step 5 is what makes it readable rather than merely correct: later columns
 * are widened past their slot and layered above, the way Google Calendar does
 * it. Three exact thirds are legible; five exact fifths are not, and the
 * overlap tells the eye which block is on top.
 */

/** Minutes below which a block would render as an unreadable sliver. */
const MIN_DURATION = 15

/** How far a column may bleed into the one to its right. */
const BLEED = 0.7

const DAY_MINUTES = 24 * 60

/**
 * `day` is which column is being laid out, and it is not optional: an event
 * from 23:00 to 06:00 appears in two columns and is a different block in each.
 * Without it the morning half would be drawn at 23:00 on the wrong day.
 */
export function layoutDay(occurrences: Occurrence[], day: Date): PlacedOccurrence[] {
  const timed = occurrences.filter((o) => !o.isAllDay)
  if (timed.length === 0) return []

  const items = timed
    .map((occurrence) => {
      const seg = daySegment(occurrence, day)
      // Each half is clamped to its own column: the evening runs to the bottom
      // of one day, the morning from the top of the next.
      const startMinutes = seg.continuesBefore ? 0 : minutesSinceMidnight(seg.start)
      // An end of exactly midnight is the bottom of THIS day, not the top of
      // the next — 1440, where `minutesSinceMidnight` would read 0.
      const endsAtMidnight = !seg.continuesAfter && minutesSinceMidnight(seg.end) === 0
      const rawEnd =
        seg.continuesAfter || endsAtMidnight ? DAY_MINUTES : minutesSinceMidnight(seg.end)
      return {
        occurrence,
        continuesBefore: seg.continuesBefore,
        continuesAfter: seg.continuesAfter,
        startMinutes,
        endMinutes: Math.min(DAY_MINUTES, Math.max(rawEnd, startMinutes + MIN_DURATION)),
      }
    })
    .sort((a, b) => a.startMinutes - b.startMinutes || b.endMinutes - a.endMinutes)

  const placed: PlacedOccurrence[] = []
  let cluster: typeof items = []
  let clusterEnd = -1

  const flush = () => {
    if (cluster.length === 0) return

    // Greedy column assignment inside the cluster.
    const columnEnds: number[] = []
    const columnOf = new Map<number, number>()
    cluster.forEach((item, index) => {
      let column = columnEnds.findIndex((end) => end <= item.startMinutes)
      if (column === -1) {
        column = columnEnds.length
        columnEnds.push(item.endMinutes)
      } else {
        columnEnds[column] = item.endMinutes
      }
      columnOf.set(index, column)
    })

    const columns = columnEnds.length
    const slot = 1 / columns
    cluster.forEach((item, index) => {
      const column = columnOf.get(index) ?? 0
      const isLast = column === columns - 1
      placed.push({
        occurrence: item.occurrence,
        continuesBefore: item.continuesBefore,
        continuesAfter: item.continuesAfter,
        startMinutes: item.startMinutes,
        endMinutes: item.endMinutes,
        left: column * slot,
        // The last column stops at the edge; the others bleed right and sit
        // above their neighbour.
        width: isLast ? slot : slot * (1 + BLEED),
        zIndex: column + 1,
      })
    })

    cluster = []
    clusterEnd = -1
  }

  for (const item of items) {
    if (cluster.length > 0 && item.startMinutes >= clusterEnd) flush()
    cluster.push(item)
    clusterEnd = Math.max(clusterEnd, item.endMinutes)
  }
  flush()

  return placed
}
