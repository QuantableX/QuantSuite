import type {
  NoteSummary,
  Property,
  PropertyValue,
  View,
  ViewFilter,
  ViewSort,
} from '#notes/types'

/**
 * Turning the collection into the rows one view shows: filter, then sort.
 *
 * Every view calls this so a filter means the same thing in a table as on a
 * board. It is deliberately synchronous over the already-loaded list rather
 * than a query — the whole collection is in memory, and re-querying per view
 * switch is what made the old tool-per-table model feel slow.
 */

function isEmpty(value: PropertyValue | undefined): boolean {
  if (value === null || value === undefined || value === '') return true
  if (Array.isArray(value)) return value.length === 0
  return false
}

function asText(value: PropertyValue | undefined): string {
  if (value === null || value === undefined) return ''
  if (Array.isArray(value)) return value.join(' ')
  return String(value)
}

function matches(note: NoteSummary, filter: ViewFilter): boolean {
  const value = note.properties[filter.propertyId]
  switch (filter.operator) {
    case 'is_empty':
      return isEmpty(value)
    case 'is_not_empty':
      return !isEmpty(value)
    case 'is':
      if (Array.isArray(value)) return value.includes(String(filter.value))
      return value === filter.value
    case 'is_not':
      if (Array.isArray(value)) return !value.includes(String(filter.value))
      return value !== filter.value
    case 'contains':
      return asText(value).toLowerCase().includes(String(filter.value ?? '').toLowerCase())
    case 'before':
      return !isEmpty(value) && asText(value) < String(filter.value ?? '')
    case 'after':
      return !isEmpty(value) && asText(value) > String(filter.value ?? '')
    default:
      return true
  }
}

/**
 * Compare two notes on one property.
 *
 * Empty always sorts last regardless of direction — a note with no due date
 * belongs at the bottom whether the list runs soonest-first or latest-first,
 * which is not what a plain string compare would do.
 */
function compare(a: NoteSummary, b: NoteSummary, sort: ViewSort, property?: Property): number {
  const left = a.properties[sort.propertyId]
  const right = b.properties[sort.propertyId]
  const leftEmpty = isEmpty(left)
  const rightEmpty = isEmpty(right)
  if (leftEmpty && rightEmpty) return 0
  if (leftEmpty) return 1
  if (rightEmpty) return -1

  let result: number
  if (property?.kind === 'number') {
    result = Number(left) - Number(right)
  } else if (property?.kind === 'checkbox') {
    result = Number(Boolean(left)) - Number(Boolean(right))
  } else if (property?.kind === 'select') {
    // Select sorts by the option's position in the schema, not alphabetically:
    // "To do, In progress, Done" is an order the user chose, and A-Z destroys it.
    const options = property.config.options ?? []
    result = options.findIndex((o) => o.id === left) - options.findIndex((o) => o.id === right)
  } else {
    result = asText(left).localeCompare(asText(right))
  }
  return sort.direction === 'desc' ? -result : result
}

export function rowsForView(
  notes: NoteSummary[],
  view: View | null,
  propertyById: Map<string, Property>,
): NoteSummary[] {
  if (!view) return notes

  const filters = view.config.filters ?? []
  const rows = filters.length ? notes.filter((note) => filters.every((f) => matches(note, f))) : notes.slice()

  const sorts = view.config.sorts ?? []
  if (sorts.length === 0) {
    return rows.sort((a, b) => a.sortIndex - b.sortIndex)
  }

  return rows.sort((a, b) => {
    for (const sort of sorts) {
      const result = compare(a, b, sort, propertyById.get(sort.propertyId))
      if (result !== 0) return result
    }
    return a.sortIndex - b.sortIndex
  })
}

export interface NoteGroup {
  /** Option id, or `null` for the "no value" group. */
  key: string | null
  label: string
  color: string
  notes: NoteSummary[]
}

/**
 * Group rows by a select property — the board's columns.
 *
 * Every option gets a column even when empty, because an empty "Done" column
 * is a drop target and a missing one is a dead end. The no-value group leads,
 * the way Notion's "No Status" does.
 */
export function groupByProperty(
  rows: NoteSummary[],
  property: Property | undefined,
): NoteGroup[] {
  if (!property) return [{ key: null, label: 'All notes', color: 'slate', notes: rows }]

  const options = property.config.options ?? []
  const groups: NoteGroup[] = [
    { key: null, label: `No ${property.name.toLowerCase()}`, color: 'slate', notes: [] },
    ...options.map((o) => ({ key: o.id, label: o.name, color: o.color, notes: [] as NoteSummary[] })),
  ]
  const index = new Map(groups.map((g) => [g.key, g]))

  for (const note of rows) {
    const raw = note.properties[property.id]
    const key = typeof raw === 'string' && raw !== '' ? raw : null
    // A value pointing at a deleted option lands in "no value" rather than
    // creating a phantom column nobody can drop into.
    ;(index.get(key) ?? index.get(null)!).notes.push(note)
  }
  return groups
}
