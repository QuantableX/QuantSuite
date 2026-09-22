<script setup lang="ts">
/**
 * The link graph, Obsidian-style: d3-force for the physics, one <canvas> for
 * the paint (SVG stops scaling long before a vault does). Pointer events
 * only — in-app dragging via HTML5 DnD is banned in this codebase.
 *
 * Two rules keep it fast, also over RDP where every full-canvas repaint is
 * expensive:
 *
 * 1. **The layout settles offline.** The simulation is ticked synchronously
 *    until it cools, then the graph is fitted to the view and painted once.
 *    No timer runs while nothing is being dragged; a data refresh with the
 *    same nodes and links repaints without touching the layout, one with
 *    new nodes seeds them next to their neighbours and re-settles quietly.
 * 2. **Painting is on demand and batched.** Hover, pan and zoom ask for one
 *    repaint per animation frame, a drag for at most ~30 a second; edges go
 *    into one path, nodes are grouped by colour, labels are culled to the
 *    viewport and fade in with the zoom.
 * 3. **The simulation never runs on a timer.** A dragged node follows the
 *    pointer by itself; when it is dropped the neighbourhood relaxes in a
 *    short burst of frames and the map is still again.
 *
 * Ghost nodes (`missing`) render hollow: link targets no memory answers to.
 * Compact maps open on click; immersive maps select first and open on double-click.
 * Double-click on empty space fits the graph to the view again.
 */
import {
  forceCollide,
  forceLink,
  forceManyBody,
  forceSimulation,
  forceX,
  forceY,
  type ForceLink,
  type Simulation,
  type SimulationLinkDatum,
} from 'd3-force'
import type { GraphEdge, GraphNode } from '#memory/types'
import { DEFAULT_GRAPH_ZOOM, MIN_GRAPH_ZOOM, MAX_GRAPH_ZOOM, stepGraphZoom } from '#memory/utils/graph'

interface SimNode extends GraphNode {
  x: number
  y: number
  vx: number
  vy: number
  fx?: number | null
  fy?: number | null
  /** Painted radius, computed once per rebuild. */
  r: number
  /** Fill (or stroke, for ghosts), computed once per restyle. */
  fill: string
  degree: number
}

interface SimEdge extends SimulationLinkDatum<SimNode> {
  count: number
}

const props = withDefaults(
  defineProps<{
    nodes: GraphNode[]
    edges: GraphEdge[]
    /** Ring-highlight this node (the local graph's center). */
    focusId?: string | null
    /** Draw titles from this zoom level on; 0 = always. */
    labelThreshold?: number
    /** Node fill per scope (gigabrain view) — absent scopes use the default. */
    scopeColors?: Record<string, string> | null
    /** Full-screen constellation treatment; the document mini-map stays compact. */
    immersive?: boolean
    highlightIds?: string[] | null
    showLabels?: boolean
  }>(),
  { focusId: null, labelThreshold: 0.5, scopeColors: null, immersive: false, highlightIds: null, showLabels: true }
)

const emit = defineEmits<{
  (e: 'open', id: string): void
  (e: 'select', id: string | null): void
  (e: 'zoom', value: number): void
}>()

const wrapper = ref<HTMLElement | null>(null)
const canvas = ref<HTMLCanvasElement | null>(null)

let sim: Simulation<SimNode, SimEdge> | null = null
let simNodes: SimNode[] = []
let simEdges: SimEdge[] = []
let edgeKeys = ''
let fillGroups: Map<string, SimNode[]> = new Map()
let ghosts: SimNode[] = []
/** Neighbour ids per node — hover highlights them. */
let neighbours: Map<string, Set<string>> = new Map()
/** Nodes by falling degree — hubs win when labels compete for space. */
let labelOrder: SimNode[] = []
/** Screen-pixel width of a title at the base font, measured once. */
const labelWidths = new Map<string, number>()
let resizeObserver: ResizeObserver | null = null
let laidOut = false
const savedPositions = new Map<string, { x: number; y: number }>()

// View transform (pan/zoom) and interaction state.
let scale = DEFAULT_GRAPH_ZOOM
let tx = 0
let ty = 0
/** Once the user panned or zoomed, resizes stop re-fitting the view. */
let userMoved = false
let dragging: SimNode | null = null
let panning = false
let moved = 0
let lastX = 0
let lastY = 0
let hovered: SimNode | null = null
let frame = 0

const colors = {
  node: '#5b9bd5',
  ghost: '#6e6e7a',
  edge: '#47474f',
  label: '#9a9aa5',
  focus: '#60a5fa',
}

function readColors() {
  if (!wrapper.value) return
  const style = getComputedStyle(wrapper.value)
  const read = (name: string, fallback: string) => style.getPropertyValue(name).trim() || fallback
  colors.node = read('--qm-graph-node', colors.node)
  colors.ghost = read('--qm-graph-node-ghost', colors.ghost)
  colors.edge = read('--qm-graph-edge', colors.edge)
  colors.label = read('--qm-text-secondary', colors.label)
  colors.focus = read('--qm-link', colors.focus)
}

function radiusOf(node: GraphNode): number {
  if (node.missing) return 4
  return 4 + Math.min(10, Math.sqrt(node.incoming + node.outgoing) * 1.7)
}

function size(): { w: number; h: number } {
  // A hidden host reports 0×0; lay out into a sane box and fit on resize.
  return { w: wrapper.value?.clientWidth || 600, h: wrapper.value?.clientHeight || 400 }
}

// ── Simulation ───────────────────────────────────────────────────────────

/** Colours and groups depend on theme and scope tints — cheap to redo. */
function restyle() {
  fillGroups = new Map()
  ghosts = []
  for (const node of simNodes) {
    const tint = props.scopeColors?.[node.scope]
    node.fill = tint ?? (node.missing ? colors.ghost : colors.node)
    if (node.missing) {
      ghosts.push(node)
    } else {
      const group = fillGroups.get(node.fill)
      if (group) group.push(node)
      else fillGroups.set(node.fill, [node])
    }
  }
}

function makeSim(w: number, h: number): Simulation<SimNode, SimEdge> {
  const degreeOf = (end: SimNode | string | number) =>
    typeof end === 'object' ? end.degree : 1
  const scopes = [...new Set(simNodes.map((n) => n.scope))].sort()
  const center = (node: SimNode, axis: 'x' | 'y') => {
    if (!props.immersive || scopes.length < 2) return axis === 'x' ? w / 2 : h / 2
    const angle = scopes.indexOf(node.scope) / scopes.length * Math.PI * 2 - Math.PI / 2
    return axis === 'x' ? w / 2 + Math.cos(angle) * w * 0.25 : h / 2 + Math.sin(angle) * h * 0.24
  }
  return forceSimulation<SimNode, SimEdge>(simNodes)
    .stop()
    .alphaDecay(0.03)
    .velocityDecay(0.45)
    .force(
      'link',
      forceLink<SimNode, SimEdge>(simEdges)
        .id((d) => d.id)
        // Hubs get room; leaves sit close to what they hang off.
        .distance((e) => (props.immersive ? 76 : 42) + 5 * Math.min(10, Math.sqrt(degreeOf(e.source) + degreeOf(e.target))))
        .strength((e) => props.immersive ? 0.22 : 1 / Math.max(1, Math.min(degreeOf(e.source), degreeOf(e.target))))
    )
    .force(
      'charge',
      forceManyBody<SimNode>()
        .strength((d) => (d.missing ? -60 : -130 - 18 * Math.min(10, d.degree)))
        .distanceMax(520)
        .theta(0.9)
    )
    // Gentle pull towards the middle instead of forceCenter: it keeps
    // disconnected components and orphans in a ring around the clusters
    // rather than letting the charge blow them to the edges. Loners are
    // pulled a little harder than hubs, which have links holding them.
    .force('x', forceX<SimNode>((d) => center(d, 'x')).strength((d) => (d.degree ? 0.035 : 0.07)))
    .force('y', forceY<SimNode>((d) => center(d, 'y')).strength((d) => (d.degree ? 0.035 : 0.07)))
    .force(
      'collide',
      forceCollide<SimNode>()
        .radius((d) => d.r + 5)
        .strength(0.8)
        .iterations(2)
    )
}

/** Tick synchronously until the simulation cools — no timer, no repaints. */
function settle(alpha: number) {
  if (!sim) return
  const maxTicks = simNodes.length > 1500 ? 120 : 320
  sim.alphaDecay(0.03).alpha(alpha)
  for (let i = 0; i < maxTicks && sim.alpha() > sim.alphaMin(); i++) sim.tick()
}

function rebuild() {
  const { w, h } = size()
  for (const n of simNodes) savedPositions.set(n.id, { x: n.x, y: n.y })
  // Retain filtered-out positions without an unbounded cache on long sessions.
  while (savedPositions.size > 10000) savedPositions.delete(savedPositions.keys().next().value!)
  const previous = new Map(simNodes.map((n) => [n.id, n]))
  const ids = new Set(props.nodes.map((n) => n.id))
  const edges = props.edges.filter((e) => ids.has(e.source) && ids.has(e.target))
  const nextEdgeKeys = edges.map((e) => `${e.source}>${e.target}:${e.count}`).sort().join('\n')
  const sameStructure =
    laidOut &&
    previous.size === ids.size &&
    props.nodes.every((n) => previous.has(n.id)) &&
    nextEdgeKeys === edgeKeys

  const degree = new Map<string, number>()
  for (const e of edges) {
    degree.set(e.source, (degree.get(e.source) ?? 0) + 1)
    degree.set(e.target, (degree.get(e.target) ?? 0) + 1)
  }

  // Keep positions across data refreshes — a re-scan must not shuffle the map.
  const fresh: SimNode[] = []
  simNodes = props.nodes.map((n, i) => {
    const old = previous.get(n.id)
    const position = old ?? savedPositions.get(n.id)
    const node: SimNode = {
      ...n,
      x: position?.x ?? w / 2,
      y: position?.y ?? h / 2,
      vx: 0,
      vy: 0,
      fx: old?.fx ?? null,
      fy: old?.fy ?? null,
      r: radiusOf(n),
      fill: '',
      degree: degree.get(n.id) ?? 0,
    }
    if (!position) {
      // Phyllotaxis around the middle: evenly spread, no overlap at start.
      const angle = i * 2.3999632
      const radius = 14 * Math.sqrt(i)
      node.x += Math.cos(angle) * radius
      node.y += Math.sin(angle) * radius
      fresh.push(node)
    }
    return node
  })
  const byId = new Map(simNodes.map((n) => [n.id, n]))
  simEdges = edges.map((e) => ({ source: e.source, target: e.target, count: e.count }))
  edgeKeys = nextEdgeKeys
  neighbours = new Map()
  for (const e of edges) {
    if (!neighbours.has(e.source)) neighbours.set(e.source, new Set())
    if (!neighbours.has(e.target)) neighbours.set(e.target, new Set())
    neighbours.get(e.source)!.add(e.target)
    neighbours.get(e.target)!.add(e.source)
  }
  labelOrder = [...simNodes].sort((a, b) => b.degree - a.degree)
  if (dragging) dragging = byId.get(dragging.id) ?? null
  if (hovered) hovered = byId.get(hovered.id) ?? null
  restyle()

  if (sameStructure && sim) {
    // Titles or counts changed, the map did not: repaint, keep the layout.
    sim.nodes(simNodes)
    sim.force<ForceLink<SimNode, SimEdge>>('link')?.links(simEdges)
    scheduleDraw()
    return
  }

  // New nodes start next to their neighbours, not in the middle of the map.
  if (laidOut && fresh.length) {
    const freshIds = new Set(fresh.map((n) => n.id))
    for (const node of fresh) {
      let sx = 0
      let sy = 0
      let count = 0
      for (const e of edges) {
        const other =
          e.source === node.id ? e.target : e.target === node.id ? e.source : null
        if (other === null || freshIds.has(other)) continue
        const neighbour = byId.get(other)
        if (!neighbour) continue
        sx += neighbour.x
        sy += neighbour.y
        count++
      }
      if (count) {
        node.x = sx / count + (Math.random() - 0.5) * 30
        node.y = sy / count + (Math.random() - 0.5) * 30
      }
    }
  }

  sim?.stop()
  sim = makeSim(w, h)
  settle(laidOut ? 0.35 : 1)
  if (!laidOut || !userMoved) props.immersive ? resetView() : fit()
  laidOut = true
  scheduleDraw()
}

/** Scale and centre the view so every node is visible. */
function fit(members = simNodes) {
  if (!members.length) return
  const { w, h } = size()
  let minX = Infinity
  let minY = Infinity
  let maxX = -Infinity
  let maxY = -Infinity
  for (const n of members) {
    minX = Math.min(minX, n.x - n.r)
    minY = Math.min(minY, n.y - n.r)
    maxX = Math.max(maxX, n.x + n.r)
    maxY = Math.max(maxY, n.y + n.r)
  }
  const pad = props.immersive ? 120 : 40
  const bw = maxX - minX + pad * 2
  const bh = maxY - minY + pad * 2
  const availableWidth = w
  const fitted = Math.min(1.6, Math.max(MIN_GRAPH_ZOOM, Math.min(availableWidth / bw, (h - (props.immersive ? 160 : 0)) / bh)))
  scale = props.immersive ? Math.max(MIN_GRAPH_ZOOM, Math.floor(fitted * 20) / 20) : fitted
  tx = availableWidth / 2 - ((minX + maxX) / 2) * scale
  ty = h / 2 - ((minY + maxY) / 2) * scale
  emit('zoom', scale)
}

/** Default framing centers the layout at a true 100%; fitting is explicit. */
function resetView() {
  const { w, h } = size()
  const minX = simNodes.length ? Math.min(...simNodes.map(n => n.x)) : w / 2
  const maxX = simNodes.length ? Math.max(...simNodes.map(n => n.x)) : w / 2
  const minY = simNodes.length ? Math.min(...simNodes.map(n => n.y)) : h / 2
  const maxY = simNodes.length ? Math.max(...simNodes.map(n => n.y)) : h / 2
  scale = DEFAULT_GRAPH_ZOOM
  tx = w / 2 - (minX + maxX) / 2
  ty = h / 2 + (props.immersive ? 30 : 0) - (minY + maxY) / 2
  userMoved = false
  emit('zoom', scale)
  scheduleDraw()
}

// ── Painting ─────────────────────────────────────────────────────────────

function scheduleDraw() {
  if (frame) return
  frame = requestAnimationFrame(() => {
    frame = 0
    draw()
  })
}

/** At most ~30 repaints a second while dragging, with a trailing draw so
 *  the last pointer position always lands. */
const DRAG_INTERVAL_MS = 33
let lastDragDraw = 0
let trailingDraw = 0
function throttledDraw() {
  const now = performance.now()
  const due = DRAG_INTERVAL_MS - (now - lastDragDraw)
  if (due <= 0) {
    if (trailingDraw) {
      clearTimeout(trailingDraw)
      trailingDraw = 0
    }
    lastDragDraw = now
    scheduleDraw()
    return
  }
  if (trailingDraw) return
  trailingDraw = window.setTimeout(() => {
    trailingDraw = 0
    lastDragDraw = performance.now()
    scheduleDraw()
  }, due)
}

/** After a drop: let the neighbourhood settle around the moved node in a
 *  short burst — a handful of frames, several ticks each, then silence. */
let relaxFrame = 0
function relax() {
  if (!sim) return
  stopRelax()
  if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
    settle(0.3)
    scheduleDraw()
    return
  }
  sim.alphaDecay(0.06).alpha(0.3)
  const step = () => {
    relaxFrame = 0
    if (!sim) return
    for (let i = 0; i < 6 && sim.alpha() > sim.alphaMin(); i++) sim.tick()
    draw()
    if (sim.alpha() > sim.alphaMin()) relaxFrame = requestAnimationFrame(step)
  }
  relaxFrame = requestAnimationFrame(step)
}

function stopRelax() {
  if (relaxFrame) cancelAnimationFrame(relaxFrame)
  relaxFrame = 0
}

function draw() {
  const el = canvas.value
  const ctx = el?.getContext('2d')
  if (!el || !ctx) return
  const { w, h } = size()
  const dpr = window.devicePixelRatio || 1
  if (el.width !== Math.round(w * dpr) || el.height !== Math.round(h * dpr)) {
    el.width = Math.round(w * dpr)
    el.height = Math.round(h * dpr)
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
  ctx.clearRect(0, 0, w, h)
  if (props.immersive) {
    // Static, deterministic star field. No animation loop while the map is idle.
    for (let i = 0; i < Math.min(200, w * h / 7000); i++) {
      const x = ((i * 137.508 + 23) % 997) / 997 * w
      const y = ((i * i * 71.31 + 131) % 991) / 991 * h
      ctx.fillStyle = i % 5 === 0 ? '#a99bff' : '#b2c8e5'
      ctx.globalAlpha = i % 7 === 0 ? 0.35 : 0.13
      ctx.fillRect(x, y, i % 7 === 0 ? 1.5 : 1, i % 7 === 0 ? 1.5 : 1)
    }
    ctx.globalAlpha = 1
  }
  ctx.translate(tx, ty)
  ctx.scale(scale, scale)

  if (props.immersive) {
    for (const [fill, group] of fillGroups) {
      const x = group.reduce((sum, n) => sum + n.x, 0) / group.length
      const y = group.reduce((sum, n) => sum + n.y, 0) / group.length
      const radius = Math.max(120, Math.min(450, 60 * Math.sqrt(group.length)))
      const glow = ctx.createRadialGradient(x, y, 0, x, y, radius)
      glow.addColorStop(0, fill)
      glow.addColorStop(1, 'transparent')
      ctx.globalAlpha = 0.065
      ctx.fillStyle = glow
      ctx.fillRect(x - radius, y - radius, radius * 2, radius * 2)
    }
    ctx.globalAlpha = 1
  }

  // The world rectangle on screen, with a margin for radii and labels.
  const margin = 80 / scale
  const left = -tx / scale - margin
  const top = -ty / scale - margin
  const right = (w - tx) / scale + margin
  const bottom = (h - ty) / scale + margin
  const visible = (n: SimNode) => n.x >= left && n.x <= right && n.y >= top && n.y <= bottom

  // Edges: one path per weight class, three strokes at most. While a node
  // is hovered the rest recede and its own links light up.
  const focus = props.focusId ? simNodes.find((n) => n.id === props.focusId) : null
  const active = hovered ?? (props.immersive ? focus : null)
  const near = active ? neighbours.get(active.id) : null
  const highlighted = props.highlightIds ? new Set(props.highlightIds) : null
  const emphasized = (node: SimNode) => (!active || node === active || near?.has(node.id)) && (!highlighted || highlighted.has(node.id))
  const connect = (s: SimNode, t: SimNode) => {
    ctx.moveTo(s.x, s.y)
    if (props.immersive) ctx.quadraticCurveTo((s.x + t.x) / 2 + (t.y - s.y) * 0.09, (s.y + t.y) / 2 - (t.x - s.x) * 0.09, t.x, t.y)
    else ctx.lineTo(t.x, t.y)
  }
  ctx.strokeStyle = props.immersive ? '#8494c6' : colors.edge
  ctx.globalAlpha = props.immersive ? (active || highlighted ? 0.09 : 0.27) : hovered ? 0.3 : 0.75
  for (let weight = 1; weight <= 3; weight++) {
    ctx.beginPath()
    let any = false
    for (const edge of simEdges) {
      const s = edge.source as SimNode
      const t = edge.target as SimNode
      if (Math.min(3, edge.count) !== weight) continue
      if (!visible(s) && !visible(t)) continue
      if (active && (s === active || t === active)) continue
      connect(s, t)
      any = true
    }
    if (!any) continue
    ctx.lineWidth = (props.immersive ? 0.5 + weight * 0.25 : 0.6 + weight * 0.5) / scale
    ctx.stroke()
  }
  if (active) {
    ctx.beginPath()
    for (const edge of simEdges) {
      const s = edge.source as SimNode
      const t = edge.target as SimNode
      if (s !== active && t !== active) continue
      connect(s, t)
    }
    ctx.strokeStyle = props.immersive ? active.fill : colors.focus
    ctx.globalAlpha = 0.8
    ctx.lineWidth = 1.3 / scale
    ctx.stroke()
  }
  ctx.globalAlpha = 1

  if (props.immersive) {
    for (const node of simNodes) {
      if (!visible(node)) continue
      const bright = emphasized(node)
      ctx.globalAlpha = bright ? 1 : 0.17
      if (!node.missing && (simNodes.length <= 700 || node === active || (node.degree >= 4 && scale >= 0.6))) {
        const halo = ctx.createRadialGradient(node.x, node.y, 0, node.x, node.y, node.r * 3.7)
        halo.addColorStop(0, node.fill)
        halo.addColorStop(0.25, node.fill + '40')
        halo.addColorStop(1, 'transparent')
        ctx.fillStyle = halo
        ctx.beginPath()
        ctx.arc(node.x, node.y, node.r * 3.7, 0, Math.PI * 2)
        ctx.fill()
      }
      ctx.beginPath()
      ctx.arc(node.x, node.y, node.r * 0.7, 0, Math.PI * 2)
      ctx.fillStyle = node.fill
      ctx.strokeStyle = node.fill
      ctx.lineWidth = 1 / scale
      if (node.missing) ctx.stroke()
      else {
        ctx.fill()
        ctx.beginPath()
        ctx.arc(node.x - node.r * 0.12, node.y - node.r * 0.12, node.r * 0.24, 0, Math.PI * 2)
        ctx.fillStyle = '#f2f0ff'
        ctx.fill()
      }
    }
    ctx.globalAlpha = 1
  }

  // Compact document maps retain batched, flat nodes.
  if (!props.immersive) {
  for (const [fill, group] of fillGroups) {
    ctx.beginPath()
    for (const node of group) {
      if (!visible(node)) continue
      ctx.moveTo(node.x + node.r, node.y)
      ctx.arc(node.x, node.y, node.r, 0, Math.PI * 2)
    }
    ctx.fillStyle = fill
    ctx.fill()
  }

  // Ghosts: hollow, one stroke per tint.
  if (ghosts.length) {
    ctx.lineWidth = 1.2 / scale
    const byTint = new Map<string, SimNode[]>()
    for (const node of ghosts) {
      if (!visible(node)) continue
      const group = byTint.get(node.fill)
      if (group) group.push(node)
      else byTint.set(node.fill, [node])
    }
    for (const [tint, group] of byTint) {
      ctx.beginPath()
      for (const node of group) {
        ctx.moveTo(node.x + node.r, node.y)
        ctx.arc(node.x, node.y, node.r, 0, Math.PI * 2)
      }
      ctx.strokeStyle = tint
      ctx.stroke()
    }
  }
  }

  // Focus and hover rings.
  for (const node of [focus, hovered]) {
    if (!node) continue
    ctx.beginPath()
    ctx.arc(node.x, node.y, node.r + (props.immersive ? 5 : 2.5) / scale, 0, Math.PI * 2)
    ctx.strokeStyle = props.immersive ? node.fill : colors.focus
    ctx.lineWidth = 1 / scale
    ctx.stroke()
  }

  // Labels: culled to the viewport, faded in with the zoom, and never on
  // top of each other — hubs claim their space first, the hovered node and
  // its neighbours always get theirs.
  if (!props.showLabels) return
  const threshold = props.labelThreshold
  const labelAlpha =
    threshold <= 0 ? 1 : Math.min(1, Math.max(0, (scale - threshold) / 0.3 + 0.25))
  ctx.fillStyle = props.immersive ? '#ccd0e5' : colors.label
  const fontSize = props.immersive ? 13 : 11
  ctx.font = `${fontSize / scale}px sans-serif`
  ctx.textAlign = 'center'
  ctx.textBaseline = 'top'
  const placed = new LabelGrid()
  const placeLabel = (node: SimNode, alpha: number) => {
    const title = node.title.length > 36 ? node.title.slice(0, 34) + '…' : node.title
    const width = labelWidth(ctx, title)
    const sx = node.x * scale + tx
    const sy = (node.y + node.r) * scale + ty + 4
    if (!placed.claim(sx - width / 2 - 2, sy, width + 4, fontSize + 3)) return
    ctx.globalAlpha = alpha
    if (props.immersive) {
      ctx.strokeStyle = '#0c0e19'
      ctx.lineWidth = 3 / scale
      ctx.lineJoin = 'round'
      ctx.strokeText(title, node.x, node.y + node.r + 4 / scale)
    }
    ctx.fillText(title, node.x, node.y + node.r + 4 / scale)
  }
  if (active) {
    placeLabel(active, 1)
    for (const node of labelOrder) {
      if (near?.has(node.id) && visible(node)) placeLabel(node, 1)
    }
  }
  if (scale >= threshold && labelAlpha > 0) {
    for (const node of labelOrder) {
      if (node === active || near?.has(node.id) || !visible(node)) continue
      placeLabel(node, emphasized(node) ? labelAlpha : labelAlpha * 0.25)
    }
  }
  ctx.globalAlpha = 1
}

/** Width of a title in screen pixels at the base font — measured once per
 *  title; labels keep their screen size across zoom levels. */
function labelWidth(ctx: CanvasRenderingContext2D, title: string): number {
  const fontSize = props.immersive ? 13 : 11
  const key = `${fontSize}:${title}`
  let width = labelWidths.get(key)
  if (width === undefined) {
    const font = ctx.font
    ctx.font = `${fontSize}px sans-serif`
    width = ctx.measureText(title).width
    ctx.font = font
    labelWidths.set(key, width)
  }
  return width
}

/** Screen-space occupancy of the labels drawn so far, bucketed so a claim
 *  checks a handful of neighbours instead of every label. */
class LabelGrid {
  private cells = new Map<string, Array<[number, number, number, number]>>()
  private static readonly CELL = 96

  claim(x: number, y: number, w: number, h: number): boolean {
    const c = LabelGrid.CELL
    const x0 = Math.floor(x / c)
    const x1 = Math.floor((x + w) / c)
    const y0 = Math.floor(y / c)
    const y1 = Math.floor((y + h) / c)
    for (let cx = x0; cx <= x1; cx++) {
      for (let cy = y0; cy <= y1; cy++) {
        const bucket = this.cells.get(`${cx},${cy}`)
        if (!bucket) continue
        for (const [bx, by, bw, bh] of bucket) {
          if (x < bx + bw && x + w > bx && y < by + bh && y + h > by) return false
        }
      }
    }
    for (let cx = x0; cx <= x1; cx++) {
      for (let cy = y0; cy <= y1; cy++) {
        const key = `${cx},${cy}`
        const bucket = this.cells.get(key)
        if (bucket) bucket.push([x, y, w, h])
        else this.cells.set(key, [[x, y, w, h]])
      }
    }
    return true
  }
}

// ── Interaction ──────────────────────────────────────────────────────────

function toWorld(event: PointerEvent | WheelEvent | MouseEvent): { x: number; y: number } {
  const rect = canvas.value!.getBoundingClientRect()
  return {
    x: (event.clientX - rect.left - tx) / scale,
    y: (event.clientY - rect.top - ty) / scale,
  }
}

function nodeAt(x: number, y: number): SimNode | null {
  // Last drawn wins the hit — iterate back to front.
  for (let i = simNodes.length - 1; i >= 0; i--) {
    const node = simNodes[i]!
    const r = node.r + 3 / scale
    const dx = node.x - x
    const dy = node.y - y
    if (dx * dx + dy * dy <= r * r) return node
  }
  return null
}

function setCursor(cursor: string) {
  if (canvas.value && canvas.value.style.cursor !== cursor) canvas.value.style.cursor = cursor
}

function onPointerDown(event: PointerEvent) {
  if (!canvas.value || event.button !== 0) return
  canvas.value.setPointerCapture(event.pointerId)
  moved = 0
  lastX = event.clientX
  lastY = event.clientY
  const { x, y } = toWorld(event)
  dragging = nodeAt(x, y)
  if (dragging) {
    stopRelax()
    // The dragged node follows the pointer on its own — no simulation, no
    // full-canvas repaint per tick (over RDP that is the lag). The rest of
    // the map catches up in one short relax when the node is dropped.
    dragging.fx = x
    dragging.fy = y
    setCursor('grabbing')
  } else {
    panning = true
    setCursor('grabbing')
  }
}

function onPointerMove(event: PointerEvent) {
  const dx = event.clientX - lastX
  const dy = event.clientY - lastY
  lastX = event.clientX
  lastY = event.clientY
  if (dragging || panning) moved += Math.abs(dx) + Math.abs(dy)
  if (dragging) {
    const { x, y } = toWorld(event)
    dragging.x = x
    dragging.y = y
    dragging.fx = x
    dragging.fy = y
    throttledDraw()
  } else if (panning) {
    tx += dx
    ty += dy
    userMoved = true
    scheduleDraw()
  } else {
    const { x, y } = toWorld(event)
    const hit = nodeAt(x, y)
    if (hit !== hovered) {
      hovered = hit
      setCursor(hit ? 'pointer' : 'grab')
      scheduleDraw()
    }
  }
}

function onPointerUp(event: PointerEvent) {
  const cancelled = event.type === 'pointercancel'
  if (dragging) {
    const clicked = moved < 4 && !cancelled ? dragging : null
    dragging.fx = null
    dragging.fy = null
    dragging = null
    if (clicked) {
      if (props.immersive) emit('select', clicked.id)
      else emit('open', clicked.id)
    }
    else relax()
    setCursor(hovered ? 'pointer' : 'grab')
  }
  if (panning) {
    if (moved < 4 && !cancelled && props.immersive) emit('select', null)
    panning = false
    setCursor(hovered ? 'pointer' : 'grab')
  }
  if (canvas.value?.hasPointerCapture(event.pointerId)) canvas.value.releasePointerCapture(event.pointerId)
}

function onPointerLeave() {
  if (hovered && !dragging) {
    hovered = null
    scheduleDraw()
  }
}

function onWheel(event: WheelEvent) {
  event.preventDefault()
  if (!event.deltaY) return
  const factor = event.deltaY < 0 ? 1.15 : 1 / 1.15
  const next = props.immersive ? stepGraphZoom(scale, -event.deltaY) : Math.min(MAX_GRAPH_ZOOM, Math.max(MIN_GRAPH_ZOOM, scale * factor))
  const rect = canvas.value!.getBoundingClientRect()
  const cx = event.clientX - rect.left
  const cy = event.clientY - rect.top
  // Zoom around the cursor: keep the world point under it fixed.
  tx = cx - ((cx - tx) / scale) * next
  ty = cy - ((cy - ty) / scale) * next
  scale = next
  emit('zoom', scale)
  userMoved = true
  scheduleDraw()
}

function onDoubleClick(event: MouseEvent) {
  const { x, y } = toWorld(event)
  const node = nodeAt(x, y)
  if (node) {
    if (props.immersive) emit('open', node.id)
    return
  }
  userMoved = true
  fit()
  scheduleDraw()
}

function zoomStep(direction: number) {
  const { w, h } = size()
  const next = stepGraphZoom(scale, direction)
  tx = w / 2 - ((w / 2 - tx) / scale) * next
  ty = h / 2 - ((h / 2 - ty) / scale) * next
  scale = next
  userMoved = true
  emit('zoom', scale)
  scheduleDraw()
}

function focusNode(id: string) {
  const node = simNodes.find((n) => n.id === id)
  if (!node) return
  hovered = null
  const { w, h } = size()
  tx = w / 2 - node.x * scale
  ty = h / 2 + (props.immersive ? 30 : 0) - node.y * scale
  userMoved = true
  emit('zoom', scale)
  scheduleDraw()
}

function onKeydown(event: KeyboardEvent) {
  if (!props.immersive) return
  const directions: Record<string, [number, number]> = { ArrowLeft: [40, 0], ArrowRight: [-40, 0], ArrowUp: [0, 40], ArrowDown: [0, -40] }
  const delta = directions[event.key]
  if (delta) {
    tx += delta[0]; ty += delta[1]; userMoved = true; scheduleDraw()
  } else if (event.key === '+' || event.key === '=') zoomStep(1)
  else if (event.key === '-') zoomStep(-1)
  else if (event.key === '0') resetView()
  else if (event.key.toLowerCase() === 'f') { fit(); userMoved = true; scheduleDraw() }
  else if (event.key === 'Enter' && props.focusId) emit('open', props.focusId)
  else if (event.key === 'Escape') emit('select', null)
  else return
  event.preventDefault()
}

// ── Lifecycle ────────────────────────────────────────────────────────────

onMounted(() => {
  readColors()
  rebuild()
  resizeObserver = new ResizeObserver(() => {
    if (!userMoved) props.immersive ? resetView() : fit()
    scheduleDraw()
  })
  if (wrapper.value) resizeObserver.observe(wrapper.value)
})

watch(
  () => [props.nodes, props.edges] as const,
  () => rebuild()
)
watch(
  () => props.scopeColors,
  () => {
    restyle()
    scheduleDraw()
  }
)
watch(
  () => [props.focusId, props.highlightIds, props.showLabels],
  () => scheduleDraw()
)

onActivated(() => {
  readColors()
  restyle()
  if (!userMoved) props.immersive ? resetView() : fit()
  scheduleDraw()
})
onDeactivated(() => {
  stopRelax()
  if (trailingDraw) clearTimeout(trailingDraw)
  trailingDraw = 0
  if (frame) cancelAnimationFrame(frame)
  frame = 0
})
onBeforeUnmount(() => {
  stopRelax()
  if (trailingDraw) clearTimeout(trailingDraw)
  trailingDraw = 0
  sim = null
  if (frame) cancelAnimationFrame(frame)
  frame = 0
  resizeObserver?.disconnect()
  resizeObserver = null
})

defineExpose({
  fit: () => { userMoved = true; fit(); scheduleDraw() },
  reset: resetView,
  zoomStep,
  focusNode,
  focusScope: (scope: string) => { fit(simNodes.filter((n) => n.scope === scope)); userMoved = true; scheduleDraw() },
})
</script>

<template>
  <div ref="wrapper" class="qm-graph" :class="{ 'qm-graph--immersive': immersive }">
    <canvas
      ref="canvas"
      class="qm-graph-canvas"
      :tabindex="immersive ? 0 : undefined"
      :aria-label="immersive ? 'Memory connection map. Search above to select a memory. Arrow keys pan, plus and minus zoom in 5 percent steps, zero resets to 100 percent, F fits the map, Enter opens the selected memory.' : 'Connected memories'"
      @pointerdown="onPointerDown"
      @pointermove="onPointerMove"
      @pointerup="onPointerUp"
      @pointercancel="onPointerUp"
      @pointerleave="onPointerLeave"
      @dblclick="onDoubleClick"
      @wheel="onWheel"
      @keydown="onKeydown"
    />
  </div>
</template>

<style scoped>
.qm-graph {
  position: relative;
  width: 100%;
  height: 100%;
  min-height: 0;
  overflow: hidden;
}

.qm-graph-canvas {
  display: block;
  width: 100%;
  height: 100%;
  cursor: grab;
  touch-action: none;
}
.qm-graph--immersive {
  background: radial-gradient(ellipse at 28% 38%, #21193740, transparent 55%), radial-gradient(ellipse at 76% 70%, #0c2c3540, transparent 55%), #0b0d16;
}
.qm-graph-canvas:focus-visible { outline: 1px solid #a99bff; outline-offset: -3px; }
</style>
