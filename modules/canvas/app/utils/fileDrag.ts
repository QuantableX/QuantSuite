/**
 * A file dragged from the explorer onto the canvas. The explorer's row drag
 * uses pointer events (HTML5 drag and drop is unreliable in WebView2), sets
 * this while it runs, and on release over the canvas asks the canvas to open
 * the file there ('qc-open-file-on-canvas' with the release point).
 */
import { ref } from 'vue'

export const fileDrag = ref<{ path: string; overCanvas: boolean } | null>(null)

/** Whether a screen point is on the canvas (its windows included), not on the explorer or a panel beside it. */
export function isOverCanvas(x: number, y: number): boolean {
  return !!document.elementFromPoint(x, y)?.closest('[data-qc-canvas]')
}
