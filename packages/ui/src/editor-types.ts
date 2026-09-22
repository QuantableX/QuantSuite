/**
 * Types the shared code editor exchanges with its hosts.
 *
 * They live beside the component rather than inside it: a `<script setup>`
 * block cannot export, and both QuantCanvas and QuantCode need to name these
 * in their own stores.
 */

/** One diagnostic from Monaco, flattened for a problems panel — or one a
 *  host computed itself and hands back to the editor (`diagnostics` prop). */
export interface EditorMarker {
  severity: 'error' | 'warning' | 'info' | 'hint'
  message: string
  line: number
  column: number
  /** Where the span ends, 1-based; absent = to the end of `line`. */
  endLine?: number
  endColumn?: number
  /** Which checker produced it — `ts`, `json`, `css`, `python`. */
  source?: string
  /** The file it belongs to; markers from several files share one panel. */
  path: string
}

/** Where the caret is, 1-based, as Monaco counts. */
export interface EditorPosition {
  line: number
  column: number
}
