import type { LaidOutLink, LaidOutNode, SankeyData, SankeyLayout } from '#finance/types'

/**
 * Laying out a Sankey — nodes into layers, bands into ribbons.
 *
 * A pure function over its input, so it can be checked against invented data
 * without rendering anything. Hand-written rather than `d3-sankey`: the
 * algorithm is a hundred lines and pulling in d3 modules for it would put them
 * in a bundle nothing else needs.
 *
 * The steps:
 *
 *   1. Bucket the nodes by `layer` and drop empty layers, so a month with no
 *      subcategories does not leave a blank third column.
 *   2. Scale every layer against the SAME cents-per-pixel — the widest layer
 *      sets it. Scaling each layer to its own height is the classic mistake:
 *      it makes a small layer look as big as the income and the picture lies.
 *   3. Stack within a layer in the order the crate returned (value-descending),
 *      which is what keeps a household budget from looking like spaghetti.
 *      Crossing minimisation is overkill here.
 *   4. Draw each link as a CLOSED path — top edge across, bottom edge back —
 *      so the ribbon can be thick at one end and thin at the other. A stroked
 *      line cannot do that, and that varying thickness is the whole point.
 *   5. Stack the endpoints on each node's edge with a running offset, in and
 *      out kept separate, so a node's ribbons never overlap each other.
 */

export interface SankeyLayoutOptions {
  width: number
  height: number
  nodeWidth?: number
  /** Vertical gap between two nodes in the same layer. */
  nodePadding?: number
  padding?: { top: number; right: number; bottom: number; left: number }
}

export function layoutSankey(data: SankeyData, opts: SankeyLayoutOptions): SankeyLayout {
  const nodeWidth = opts.nodeWidth ?? 14
  const nodePadding = opts.nodePadding ?? 10
  const pad = opts.padding ?? { top: 16, right: 8, bottom: 16, left: 8 }

  const innerW = Math.max(1, opts.width - pad.left - pad.right)
  const innerH = Math.max(1, opts.height - pad.top - pad.bottom)

  // 1. Layers, empty ones dropped.
  const layerIndex = [...new Set(data.nodes.map((n) => n.layer))].sort((a, b) => a - b)
  const layers = layerIndex.map((layer) => data.nodes.filter((n) => n.layer === layer))
  if (layers.length === 0) return { nodes: [], links: [], width: opts.width, height: opts.height }

  // 2. One scale for every layer, set by the tallest.
  const layerTotals = layers.map((nodes) => nodes.reduce((sum, n) => sum + Math.max(0, n.valueCents), 0))
  const maxTotal = Math.max(1, ...layerTotals)
  const tallestCount = Math.max(...layers.map((l) => l.length))
  const usableH = Math.max(1, innerH - nodePadding * Math.max(0, tallestCount - 1))
  const pxPerCent = usableH / maxTotal

  const columnGap = layers.length > 1 ? (innerW - nodeWidth * layers.length) / (layers.length - 1) : 0

  const placed = new Map<string, LaidOutNode>()
  const nodes: LaidOutNode[] = []

  layers.forEach((layerNodes, column) => {
    const x = pad.left + column * (nodeWidth + columnGap)
    const total = layerTotals[column] ?? 0
    const heights = layerNodes.map((n) => Math.max(1, Math.max(0, n.valueCents) * pxPerCent))
    const stackH = heights.reduce((a, b) => a + b, 0) + nodePadding * Math.max(0, layerNodes.length - 1)
    // Centre a short layer against a tall one, so the hub lines up with the
    // middle of the income rather than sitting at the top.
    let y = pad.top + Math.max(0, (innerH - stackH) / 2)
    void total

    layerNodes.forEach((node, i) => {
      const height = heights[i]!
      const laid: LaidOutNode = { ...node, x, y, width: nodeWidth, height }
      placed.set(node.id, laid)
      nodes.push(laid)
      y += height + nodePadding
    })
  })

  // 5. Running offsets per node edge.
  const outOffset = new Map<string, number>()
  const inOffset = new Map<string, number>()

  const links: LaidOutLink[] = []
  for (const link of data.links) {
    const source = placed.get(link.source)
    const target = placed.get(link.target)
    if (!source || !target) continue

    const thickness = Math.max(1, Math.max(0, link.valueCents) * pxPerCent)
    const so = outOffset.get(source.id) ?? 0
    const to = inOffset.get(target.id) ?? 0
    outOffset.set(source.id, so + thickness)
    inOffset.set(target.id, to + thickness)

    const x1 = source.x + source.width
    const x2 = target.x
    const y1 = source.y + so
    const y2 = target.y + to
    const cx = (x1 + x2) / 2

    // 4. Closed ribbon: out along the top, back along the bottom.
    const path =
      `M ${x1} ${y1} C ${cx} ${y1}, ${cx} ${y2}, ${x2} ${y2} ` +
      `L ${x2} ${y2 + thickness} C ${cx} ${y2 + thickness}, ${cx} ${y1 + thickness}, ${x1} ${y1 + thickness} Z`

    links.push({
      ...link,
      path,
      midX: cx,
      midY: (y1 + y2) / 2 + thickness / 2,
    })
  }

  return { nodes, links, width: opts.width, height: opts.height }
}
