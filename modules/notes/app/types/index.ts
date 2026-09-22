/**
 * QuantNotes' shared types — the mirror of the crate's serde structs.
 *
 * One collection of notes, one property schema over them, and a set of views
 * that render the same notes differently. There is no workspace: the module
 * owns a single database (see `crate/src/lib.rs`).
 */

export type Theme = 'dark' | 'light' | 'system'

/** How a property value is stored and which editor a view renders for it. */
export type PropertyKind =
  | 'text'
  | 'number'
  | 'select'
  | 'multi_select'
  | 'date'
  | 'checkbox'
  | 'url'

export type ViewKind = 'table' | 'board' | 'list' | 'gallery' | 'calendar'

/** The palette a select option can pick from — keys into `--qn-tag-*`. */
export type OptionColor =
  | 'slate'
  | 'red'
  | 'amber'
  | 'green'
  | 'blue'
  | 'purple'
  | 'pink'

export interface SelectOption {
  id: string
  name: string
  color: OptionColor
}

export interface PropertyConfig {
  options?: SelectOption[]
}

export interface Property {
  id: string
  name: string
  kind: PropertyKind
  config: PropertyConfig
  sortIndex: number
  createdAt: string
}

/**
 * A property value, keyed by property id on the note.
 *
 * `text`/`url`/`date` are strings (dates are `YYYY-MM-DD`), `number` a number,
 * `checkbox` a boolean, `select` an option id, `multi_select` an array of them.
 * An absent key is empty — nothing writes `null`.
 */
export type PropertyValue = string | number | boolean | string[] | null

export type PropertyBag = Record<string, PropertyValue>

// ─── Views ──────────────────────────────────────────────────────────────

export type FilterOperator =
  | 'is'
  | 'is_not'
  | 'contains'
  | 'is_empty'
  | 'is_not_empty'
  | 'before'
  | 'after'

export interface ViewFilter {
  propertyId: string
  operator: FilterOperator
  value?: PropertyValue
}

export interface ViewSort {
  propertyId: string
  direction: 'asc' | 'desc'
}

export interface ViewConfig {
  /** Board view: the select property whose options become the columns. */
  groupBy?: string
  /** Calendar view: the date property that places a note on a day. */
  dateBy?: string
  filters?: ViewFilter[]
  sorts?: ViewSort[]
  /** Property ids the view shows, in order. */
  visible?: string[]
  cardSize?: 'small' | 'medium' | 'large'
}

export interface View {
  id: string
  name: string
  kind: ViewKind
  config: ViewConfig
  sortIndex: number
  createdAt: string
  updatedAt: string
}

// ─── Notes ──────────────────────────────────────────────────────────────

/** A note without its document — what every view renders a row from. */
export interface NoteSummary {
  id: string
  parentId: string | null
  title: string
  icon: string | null
  cover: string | null
  properties: PropertyBag
  wordCount: number
  sortIndex: number
  isArchived: boolean
  createdAt: string
  updatedAt: string
}

export interface Note extends NoteSummary {
  content: Record<string, any>
}

export interface NotePatch {
  title?: string
  icon?: string | null
  cover?: string | null
  parentId?: string | null
  sortIndex?: number
  isArchived?: boolean
}

export interface NoteTreeNode extends NoteSummary {
  children: NoteTreeNode[]
}

// ─── Search / backlinks ─────────────────────────────────────────────────

export interface SearchHit {
  noteId: string
  title: string
  snippet: string
}

export interface Backlink {
  noteId: string
  title: string
  icon: string | null
}

// ─── Settings ───────────────────────────────────────────────────────────

export interface AppSettings {
  theme: Theme
  fontSize: number
  defaultNoteFont: 'sans' | 'serif' | 'mono'
  smartQuotes: boolean
  autoformat: boolean
  spellcheckLanguage: string
  defaultViewId: string
  sidebarLeftOpen: boolean
  sidebarRightOpen: boolean
  focusMode: boolean
}
