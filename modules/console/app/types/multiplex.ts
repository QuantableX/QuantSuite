/**
 * The multiplexer's shape (docs/PLAN-CONSOLE.md §5, phase P3).
 *
 * Tabs hold a **tree**, not a list: a split can be split again, which is what
 * "recursive splits" means and what a flat row/column array cannot express. The
 * tree is the store's state; the layout component only renders it and reports
 * where a divider was dragged.
 *
 * Ids: `PaneNode.id` identifies a node *in the tree* (a divider reports against
 * it), while `paneKey` identifies the **session slot** a leaf shows. They are
 * separate because a leaf can move around the tree — a split rearranges nodes
 * while the shell behind the pane keeps running.
 */

export type SplitDirection = 'row' | 'col'

export interface PaneLeaf {
  kind: 'leaf'
  id: string
  /** The session slot this leaf renders. */
  paneKey: string
}

export interface PaneSplit {
  kind: 'split'
  id: string
  /** `row` splits left/right, `col` splits top/bottom. */
  dir: SplitDirection
  children: PaneNode[]
  /**
   * One fraction per child, summing to 1. Fractions rather than pixels: the
   * window resizes, and a stored pixel width silently stops matching the box it
   * was measured in.
   */
  sizes: number[]
}

export type PaneNode = PaneLeaf | PaneSplit

/**
 * A tab's name, derived once for everyone who has to print it.
 *
 * Only the name: session state — integration, exit, what is running — is read per
 * *pane* from the slots themselves, because that is the grain the group list shows
 * it at. Rolling the same flags up per tab as well means recomputing them for
 * every tab on every delta, to render nothing.
 */
export interface ConsoleTabView {
  key: string
  /** User-visible title; renamed titles win over the derived one. */
  title: string
}
