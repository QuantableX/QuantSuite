/**
 * The note icon set — 24×24 stroke paths, per PLAN-V3 §3: no emoji, no glyph
 * characters. Same idiom as the view-kind icons in ViewTabs, so a note icon
 * sits next to the rest of the chrome instead of on top of it.
 *
 * What a note stores in `icon` is one of these keys. A row written before the
 * switch still holds a glyph; NoteIcon renders that as text rather than
 * dropping it, so nobody loses an icon they picked.
 */
export interface NoteIconDef {
  key: string
  label: string
  d: string
}

export const NOTE_ICONS: NoteIconDef[] = [
  { key: 'doc', label: 'Document', d: 'M6 3h8l4 4v14H6z M14 3v4h4 M9 12h6 M9 16h4' },
  { key: 'draft', label: 'Draft', d: 'M4 20v-4L16 4l4 4L8 20z M14 6l4 4' },
  { key: 'idea', label: 'Idea', d: 'M12 3a6 6 0 0 1 3.5 10.9V17h-7v-3.1A6 6 0 0 1 12 3z M9.5 20h5' },
  { key: 'goal', label: 'Goal', d: 'M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0z M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0z M12 12h.01' },
  { key: 'pin', label: 'Pinned', d: 'M9 3h6 M10 3v7L7 14v2h10v-2l-3-4V3 M12 16v5' },
  { key: 'bookmark', label: 'Bookmark', d: 'M6 3h12v18l-6-5-6 5z' },
  { key: 'chart', label: 'Report', d: 'M4 4v16h16 M8 16v-4 M12 16V9 M16 16v-6' },
  { key: 'calendar', label: 'Schedule', d: 'M4 6h16v14H4z M4 10h16 M8 3v4 M16 3v4' },
  { key: 'task', label: 'Task', d: 'M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0z M8 12.5l2.5 2.5L16 9.5' },
  { key: 'urgent', label: 'Urgent', d: 'M13 3L5 14h6l-1 7 8-11h-6z' },
  { key: 'star', label: 'Starred', d: 'M12 4l2.5 5.2 5.5.8-4 3.9 1 5.6-5-2.7-5 2.7 1-5.6-4-3.9 5.5-.8z' },
  { key: 'flag', label: 'Milestone', d: 'M6 3v18 M6 4h12l-2.5 4L18 12H6z' },
  { key: 'tag', label: 'Reference', d: 'M4 4h7l9 9-7 7-9-9V4z M8.5 8.5h.01' },
  { key: 'folder', label: 'Collection', d: 'M4 5h5l2 3h9v11H4z' },
  { key: 'log', label: 'Log', d: 'M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0z M12 7v5.5l3.5 2' },
  { key: 'risk', label: 'Risk', d: 'M12 4L3 20h18z M12 10v4 M12 17h.01' },
  { key: 'link', label: 'Link', d: 'M9 17H7A5 5 0 0 1 7 7h2 M15 7h2a5 5 0 1 1 0 10h-2 M8 12h8' },
  { key: 'code', label: 'Code', d: 'M9 6l-5 6 5 6 M15 6l5 6-5 6' },
]

export const DEFAULT_NOTE_ICON = 'doc'

const BY_KEY = new Map(NOTE_ICONS.map((icon) => [icon.key, icon]))

export function noteIconDef(key: string | null | undefined): NoteIconDef | undefined {
  return key ? BY_KEY.get(key) : undefined
}
