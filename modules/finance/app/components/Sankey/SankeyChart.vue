<script setup lang="ts">
/**
 * The cash-flow Sankey — hand-drawn inline SVG.
 *
 * The layout is a pure function (`useSankeyLayout`); this component owns only
 * the sizing, the hover state and the labels. It re-measures through a
 * `ResizeObserver` and scales via `viewBox`, never through viewport units —
 * the module fills its container, not the window (ARCHITECTURE.md §9).
 *
 * Amounts and names are rendered as SVG text, not only in the tooltip: a chart
 * that hides its numbers until you hover is useless in a screenshot.
 */
import { useAppStore } from '#finance/stores/app'
import { usePlanStore } from '#finance/stores/plan'
import { layoutSankey } from '#finance/composables/useSankeyLayout'
import type { LaidOutNode } from '#finance/types'
import { formatCents, formatCompact } from '#finance/utils/money'

const app = useAppStore()
const plan = usePlanStore()
const router = useRouter()
function openFund(node: LaidOutNode) {
  if (node.fundId) void router.push({ path: '/finance/funds', query: { fund: node.fundId } })
}

const host = ref<HTMLElement | null>(null)
const size = ref({ width: 900, height: 420 })
const hovered = ref<string | null>(null)

let observer: ResizeObserver | null = null

function measure() {
  const el = host.value
  if (!el) return
  const rect = el.getBoundingClientRect()
  if (rect.width > 0 && rect.height > 0) {
    size.value = { width: Math.round(rect.width), height: Math.round(rect.height) }
  }
}

onMounted(() => {
  measure()
  if (typeof ResizeObserver !== 'undefined' && host.value) {
    observer = new ResizeObserver(measure)
    observer.observe(host.value)
  }
})
onActivated(measure)
onBeforeUnmount(() => {
  observer?.disconnect()
  observer = null
})

const layout = computed(() => {
  const data = plan.sankey
  if (!data || data.nodes.length === 0) return null
  // Labels live outside the node columns, so the drawing area is inset.
  return layoutSankey(data, {
    width: size.value.width,
    height: size.value.height,
    nodeWidth: 12,
    nodePadding: 10,
    padding: { top: 18, right: 130, bottom: 18, left: 110 },
  })
})

/** A node's own links, both directions — what a hover highlights. */
const litLinks = computed(() => {
  const id = hovered.value
  if (!id || !layout.value) return null
  return new Set(
    layout.value.links.filter((l) => l.source === id || l.target === id).map((l) => `${l.source}>${l.target}`),
  )
})

function isLit(source: string, target: string): boolean {
  const lit = litLinks.value
  return !lit || lit.has(`${source}>${target}`)
}

/** Labels sit outside the left column and inside for everything else. */
function labelAnchor(node: LaidOutNode): 'start' | 'end' {
  return node.layer === 0 ? 'end' : 'start'
}

function labelX(node: LaidOutNode): number {
  return node.layer === 0 ? node.x - 8 : node.x + node.width + 8
}

/**
 * Label a band only when it can carry the text.
 *
 * The hub never gets one: it is a pass-through, so its label would repeat the
 * income figure that the left-hand node already states — the same number
 * twice, once in the middle of the picture where it means nothing new.
 */
function showsLabel(node: LaidOutNode): boolean {
  return node.kind !== 'hub' && node.height >= 13
}

const money = (cents: number) => formatCents(cents, app.settings.currency, app.settings.locale)
</script>

<template>
  <div ref="host" class="qf-sk">
    <svg
      v-if="layout"
      class="qf-sk__svg"
      :viewBox="`0 0 ${layout.width} ${layout.height}`"
      preserveAspectRatio="xMidYMid meet"
      role="group"
      aria-label="Cash-flow diagram"
    >
      <!-- Ribbons first, so nodes and labels sit above them. -->
      <g class="qf-sk__links">
        <path
          v-for="link in layout.links"
          :key="`${link.source}>${link.target}`"
          class="qf-sk__link"
          :class="{ 'is-dim': !isLit(link.source, link.target) }"
          :data-color="link.color"
          :d="link.path"
        >
          <title>{{ link.valueCents ? money(link.valueCents) : '' }}</title>
        </path>
      </g>

      <g class="qf-sk__nodes">
        <g
          v-for="node in layout.nodes"
          :key="node.id"
          class="qf-sk__node"
          :data-fund="node.fundId || undefined"
          :role="node.fundId ? 'link' : undefined"
          :tabindex="node.fundId ? 0 : undefined"
          :aria-label="node.fundId ? `Open ${node.label} fund` : undefined"
          @click="openFund(node)"
          @keydown.enter.prevent="openFund(node)"
          @keydown.space.prevent="openFund(node)"
          :class="{ 'is-dim': hovered && hovered !== node.id && !layout.links.some((l) => (l.source === hovered && l.target === node.id) || (l.target === hovered && l.source === node.id)) }"
          @pointerenter="hovered = node.id"
          @pointerleave="hovered = null"
        >
          <rect
            class="qf-sk__rect"
            :data-color="node.color"
            :x="node.x"
            :y="node.y"
            :width="node.width"
            :height="node.height"
            rx="2"
          >
            <title>{{ node.label }} — {{ money(node.valueCents) }}</title>
          </rect>

          <text
            v-if="showsLabel(node)"
            class="qf-sk__label"
            :x="labelX(node)"
            :y="node.y + node.height / 2"
            :text-anchor="labelAnchor(node)"
            dominant-baseline="middle"
          >
            {{ node.label }}
            <tspan class="qf-sk__value" dx="6">{{ formatCompact(node.valueCents, app.settings.locale) }}</tspan>
          </text>
        </g>
      </g>
    </svg>

    <p v-else class="qf-sk__empty">
      Nothing planned yet. Fill in the Plan and the flow appears here.
    </p>
  </div>
</template>

<style scoped>
.qf-sk {
  width: 100%;
  height: 100%;
  min-height: 0;
}

.qf-sk__svg {
  display: block;
  width: 100%;
  height: 100%;
}

.qf-sk__link {
  fill: var(--qf-flow-expense);
  fill-opacity: 0.45;
  transition: fill-opacity 120ms ease;
}


.qf-sk__link[data-color='income'] { fill: var(--qf-flow-income); }
.qf-sk__link[data-color='saving'] { fill: var(--qf-flow-saving); }
.qf-sk__link[data-color='expense'] { fill: var(--qf-flow-expense); }
.qf-sk__link[data-color='leftover'] { fill: var(--qf-flow-leftover); }

.qf-sk__link.is-dim {
  fill-opacity: 0.1;
}

.qf-sk__node {
  cursor: default;
}
.qf-sk__node[data-fund] { cursor: pointer; }
.qf-sk__node[data-fund] .qf-sk__label { pointer-events: auto; }
.qf-sk__node:focus-visible .qf-sk__rect { stroke: var(--qf-text); stroke-width: 2; }

.qf-sk__rect {
  fill: var(--qf-flow-expense);
  transition: opacity 120ms ease;
}


.qf-sk__rect[data-color='income'] { fill: var(--qf-flow-income); }
.qf-sk__rect[data-color='saving'] { fill: var(--qf-flow-saving); }
.qf-sk__rect[data-color='expense'] { fill: var(--qf-flow-expense); }
.qf-sk__rect[data-color='leftover'] { fill: var(--qf-flow-leftover); }

.qf-sk__node.is-dim {
  opacity: 0.35;
}

.qf-sk__label {
  fill: var(--qf-text-secondary);
  font-size: 11px;
  pointer-events: none;
}

.qf-sk__value {
  fill: var(--qf-text-muted);
  font-size: 10px;
}

.qf-sk__empty {
  display: grid;
  place-items: center;
  height: 100%;
  margin: 0;
  padding: 24px;
  color: var(--qf-text-muted);
  font-size: 13px;
  text-align: center;
}
</style>
