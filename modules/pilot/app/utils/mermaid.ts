/**
 * Mermaid diagrams (parity 13): rendered once per source and theme, cached,
 * and put into a message's HTML after it is sanitised — the Markdown
 * sanitiser drops <style>, which Mermaid's SVG needs, so a diagram goes in
 * through its own stricter cleaning instead (no scripts, no foreign objects,
 * no links or resources outside the SVG).
 *
 * Streaming: an unfinished fence stays readable source; a finished one is
 * rendered once and every later repaint takes it from the cache, so only the
 * new diagram is ever rendered. Mermaid itself loads the first time a diagram
 * appears. Too large or too slow diagrams fail into an explanation, with the
 * source still there.
 */
import { ref } from 'vue'

export const SOURCE_MAX = 20_000
export const LINES_MAX = 1_000
const RENDER_MS = 8_000
const CACHE_MAX = 64

export interface MermaidRender {
  svg?: string
  error?: string
}

const cache = new Map<string, MermaidRender>()
const pending = new Set<string>()
/** Bumped when a diagram finished rendering: messages repaint. */
export const mermaidVersion = ref(0)
/** Diagrams the user switched to their source. */
export const showingSource = new Set<string>()

/** djb2 over the source and theme, hex. */
export function hashOf(src: string, theme: string): string {
  let h = 5381
  const s = `${theme}\u0000${src}`
  for (let i = 0; i < s.length; i++) h = ((h << 5) + h + s.charCodeAt(i)) >>> 0
  return h.toString(16).padStart(8, '0') + s.length.toString(16)
}

export function currentTheme(): 'dark' | 'light' {
  return typeof document !== 'undefined' && !document.documentElement.classList.contains('dark') ? 'light' : 'dark'
}

/** Why a source is not rendered at all, if it is too large. */
export function tooLarge(src: string): string | null {
  if (src.length > SOURCE_MAX) return `This diagram is ${Math.round(src.length / 1000)}k characters; diagrams up to ${SOURCE_MAX / 1000}k are drawn.`
  if (src.split('\n').length > LINES_MAX) return `This diagram has more than ${LINES_MAX} lines; it is shown as source.`
  return null
}

/** An SVG with nothing that runs or reaches outside it. */
export function cleanSvg(svg: string): string {
  const doc = new DOMParser().parseFromString(svg, 'image/svg+xml')
  const root = doc.documentElement
  if (!root || root.nodeName.toLowerCase() !== 'svg') return ''
  root.querySelectorAll('script, foreignObject, iframe, object, embed, image, use[href^="http"], use[*|href^="http"]').forEach((el) => el.remove())
  root.querySelectorAll('a').forEach((a) => {
    // A link becomes its contents.
    while (a.firstChild) a.parentNode?.insertBefore(a.firstChild, a)
    a.remove()
  })
  root.querySelectorAll('*').forEach((el) => {
    for (const attr of [...el.attributes]) {
      const name = attr.name.toLowerCase()
      const value = attr.value.trim().toLowerCase()
      if (name.startsWith('on')) el.removeAttribute(attr.name)
      else if ((name === 'href' || name === 'xlink:href') && !value.startsWith('#')) el.removeAttribute(attr.name)
      else if (name === 'style' && /url\(\s*['"]?(?!#)/.test(value)) el.removeAttribute(attr.name)
    }
  })
  root.querySelectorAll('style').forEach((s) => {
    s.textContent = (s.textContent ?? '').replace(/@import[^;]+;/g, '').replace(/url\(\s*['"]?(?!#)[^)]*\)/g, 'none')
  })
  // Natural size: the message scrolls a wide diagram instead of shrinking its text.
  const vb = (root.getAttribute('viewBox') ?? '').split(/[\s,]+/).map(Number)
  if (vb.length === 4 && vb[2]! > 0 && vb[3]! > 0) {
    root.setAttribute('width', String(Math.ceil(vb[2]!)))
    root.setAttribute('height', String(Math.ceil(vb[3]!)))
    root.style.removeProperty('max-width')
  }
  root.setAttribute('role', 'img')
  return new XMLSerializer().serializeToString(root)
}

let mermaid: any = null
let loading: Promise<any> | null = null
let queue: Promise<void> = Promise.resolve()
let seq = 0

async function load(theme: 'dark' | 'light') {
  if (!mermaid) {
    loading ??= import('mermaid').then((m) => (mermaid = (m as any).default ?? m))
    await loading
  }
  const cs = getComputedStyle(document.documentElement)
  const v = (name: string, fallback: string) => cs.getPropertyValue(name).trim() || fallback
  mermaid.initialize({
    startOnLoad: false,
    securityLevel: 'strict',
    theme: theme === 'dark' ? 'dark' : 'default',
    maxTextSize: SOURCE_MAX,
    maxEdges: 500,
    htmlLabels: false,
    flowchart: { htmlLabels: false },
    fontFamily: v('--qss-font-sans', 'system-ui, sans-serif'),
    themeVariables: theme === 'dark' ? { background: 'transparent', fontSize: '13px', primaryTextColor: v('--qss-text', '#d4d4d8'), lineColor: v('--qss-text-secondary', '#9a9aa5') } : { fontSize: '13px' },
  })
  return mermaid
}

/** The cached result, starting a render when there is none. */
export function diagram(src: string, theme: 'dark' | 'light' = currentTheme()): { hash: string; state: 'ready' | 'pending' | 'error'; error?: string } {
  const hash = hashOf(src, theme)
  const hit = cache.get(hash)
  if (hit?.svg) return { hash, state: 'ready' }
  if (hit?.error) return { hash, state: 'error', error: hit.error }
  const big = tooLarge(src)
  if (big) {
    cache.set(hash, { error: big })
    return { hash, state: 'error', error: big }
  }
  if (!pending.has(hash)) {
    pending.add(hash)
    queue = queue.then(() => render(hash, src, theme))
  }
  return { hash, state: 'pending' }
}

async function render(hash: string, src: string, theme: 'dark' | 'light') {
  try {
    const m = await load(theme)
    const id = `qp-mm-${++seq}`
    const timeout = new Promise<never>((_, reject) => setTimeout(() => reject(new Error(`drawing took longer than ${RENDER_MS / 1000} s`)), RENDER_MS))
    const { svg } = (await Promise.race([m.render(id, src), timeout])) as { svg: string }
    const clean = cleanSvg(svg)
    remember(hash, clean ? { svg: clean } : { error: 'The diagram could not be drawn safely.' })
  } catch (e) {
    const msg = String((e as Error)?.message ?? e).split('\n').slice(0, 3).join(' ').slice(0, 300)
    remember(hash, { error: `Not a diagram Mermaid can draw: ${msg}` })
    // Mermaid leaves an error element behind in the body.
    document.querySelectorAll('body > [id^="dqp-mm-"], body > svg[id^="qp-mm-"]').forEach((el) => el.remove())
  } finally {
    pending.delete(hash)
    mermaidVersion.value++
  }
}

function remember(hash: string, r: MermaidRender) {
  cache.set(hash, r)
  while (cache.size > CACHE_MAX) cache.delete(cache.keys().next().value as string)
}

export function svgOf(hash: string): string | undefined {
  return cache.get(hash)?.svg
}

/** Put cached diagrams into sanitised HTML. */
export function fillDiagrams(html: string): string {
  return html.replace(/%%QPMM:([0-9a-f]+)%%/g, (_m, h: string) => cache.get(h)?.svg ?? '')
}
