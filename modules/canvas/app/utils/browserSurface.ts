// QuantCanvas browser windows (rework 2026-11-03): where the live page may
// show. A tab is a native webview drawn above all HTML, so the browser window
// shows it only while nothing lies on top of it; otherwise a picture of the
// page stands in its place in the DOM. Higher canvas windows, the minimap and
// whatever lies outside the visible canvas are cut out of a live page with a
// window region (exact rectangles); anything else on top — a menu, a dialog,
// a drawer, a popover, a toast — is found by hit-testing the page's area
// against the DOM, and the page steps aside for its picture.

/** A screen rectangle in CSS px (DOMRect-like). */
export interface Box {
  left: number
  top: number
  right: number
  bottom: number
}

/** A rectangle local to the page, in device px (what the window region takes). */
export interface ClipRect {
  x: number
  y: number
  w: number
  h: number
}

/** A canvas window's rectangle in canvas units. */
export interface Area {
  x: number
  y: number
  width: number
  height: number
}

export function intersect(a: Box, b: Box): Box | null {
  const left = Math.max(a.left, b.left)
  const top = Math.max(a.top, b.top)
  const right = Math.min(a.right, b.right)
  const bottom = Math.min(a.bottom, b.bottom)
  return right > left && bottom > top ? { left, top, right, bottom } : null
}

export function contains(b: Box, x: number, y: number): boolean {
  return x >= b.left && x < b.right && y >= b.top && y < b.bottom
}

export function overlaps(a: Area, b: Area): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

/**
 * What is cut out of a live page: every cut that overlaps it, and the parts
 * of the page outside the visible canvas, as rectangles local to the page in
 * device px.
 */
export function clipRects(page: Box, cuts: Box[], viewport: Box, dpr: number): ClipRect[] {
  const out: ClipRect[] = []
  const local = (b: Box): ClipRect => ({
    x: Math.round((b.left - page.left) * dpr),
    y: Math.round((b.top - page.top) * dpr),
    w: Math.round((b.right - b.left) * dpr),
    h: Math.round((b.bottom - b.top) * dpr),
  })
  for (const cut of cuts) {
    const hit = intersect(page, cut)
    if (hit) out.push(local(hit))
  }
  // Outside the visible canvas: bands on each side the page reaches past.
  if (page.left < viewport.left) out.push(local({ left: page.left, top: page.top, right: Math.min(viewport.left, page.right), bottom: page.bottom }))
  if (page.right > viewport.right) out.push(local({ left: Math.max(viewport.right, page.left), top: page.top, right: page.right, bottom: page.bottom }))
  if (page.top < viewport.top) out.push(local({ left: page.left, top: page.top, right: page.right, bottom: Math.min(viewport.top, page.bottom) }))
  if (page.bottom > viewport.bottom) out.push(local({ left: page.left, top: Math.max(viewport.bottom, page.top), right: page.right, bottom: page.bottom }))
  return out.filter((r) => r.w > 0 && r.h > 0)
}

/**
 * Points to hit-test over the visible part of the page: a grid plus points
 * just inside its corners, minus what is cut out of the page anyway.
 */
export function probePoints(page: Box, viewport: Box, cuts: Box[], cols = 6, rows = 5): { x: number; y: number }[] {
  const seen = intersect(page, viewport)
  if (!seen) return []
  const w = seen.right - seen.left
  const h = seen.bottom - seen.top
  const pts: { x: number; y: number }[] = []
  for (let r = 0; r < rows; r++) {
    for (let c = 0; c < cols; c++) pts.push({ x: seen.left + ((c + 0.5) * w) / cols, y: seen.top + ((r + 0.5) * h) / rows })
  }
  const inset = Math.min(4, w / 4, h / 4)
  pts.push(
    { x: seen.left + inset, y: seen.top + inset },
    { x: seen.right - inset, y: seen.top + inset },
    { x: seen.left + inset, y: seen.bottom - inset },
    { x: seen.right - inset, y: seen.bottom - inset },
  )
  return pts.filter((p) => !cuts.some((c) => contains(c, p.x, p.y)))
}

/**
 * Is anything else on top of the page? `hit` is `document.elementFromPoint`,
 * `isPage` tells the page's own area (and its picture) from the rest.
 */
export function covered(points: { x: number; y: number }[], hit: (x: number, y: number) => Element | null, isPage: (el: Element) => boolean): boolean {
  for (const p of points) {
    const el = hit(p.x, p.y)
    // Nothing there: outside the suite page (never inside the visible canvas).
    if (el && !isPage(el)) return true
  }
  return false
}

/** The address bar: a URL, a host (https), localhost/ports (http), else a web search. */
export function normalizeAddress(input: string, search = (q: string) => `https://www.google.com/search?q=${encodeURIComponent(q)}`): string {
  const text = input.trim()
  if (!text) return 'about:blank'
  if (/^(https?|about|data|blob):/i.test(text)) return text
  if (/^\d{2,5}$/.test(text)) return `http://localhost:${text}`
  if (!/\s/.test(text) && /^(localhost|\[::1\]|127\.|10\.|192\.168\.|0\.0\.0\.0)/i.test(text)) return `http://${text}`
  if (!/\s/.test(text) && /^[^/?#]+\.[a-z]{2,}(:\d+)?([/?#].*)?$/i.test(text)) return `https://${text}`
  return search(text)
}

/** The tab's name: the page title, else the host, else the address. */
export function tabTitle(title: string | undefined, url: string): string {
  const t = (title ?? '').trim()
  if (t) return t
  try {
    const u = new URL(url)
    return u.host || url
  } catch {
    return url || 'New tab'
  }
}

/** A page zoom step (Ctrl +/-), like browsers: 25 % … 300 %. */
const ZOOM_STEPS = [0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3]
export function zoomStep(current: number, dir: 1 | -1): number {
  if (dir > 0) return ZOOM_STEPS.find((z) => z > current + 0.001) ?? ZOOM_STEPS[ZOOM_STEPS.length - 1]!
  return [...ZOOM_STEPS].reverse().find((z) => z < current - 0.001) ?? ZOOM_STEPS[0]!
}
