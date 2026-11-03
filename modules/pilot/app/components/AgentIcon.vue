<script setup lang="ts">
/**
 * An agent's logo (round 2, card 01) wherever QuantPilot names an agent:
 * thread rows, the composer, the pickers, settings, board, usage, the
 * QuantCanvas chat window. Built-in agents show their bundled mark (no
 * network), custom agents their chosen image or their letter, an unknown
 * or still-loading identity a neutral "?" — never another agent's logo.
 * `status` draws a ring around the logo instead of replacing it: working
 * (turning arc), waiting, limited, error.
 */
import { computed } from 'vue'
import { SquareTerminal } from 'lucide-vue-next'
import { usePilotStore } from '#pilot/stores/pilot'
import { agentGlyph } from '#pilot/utils/agentIcons'

const FILES = import.meta.glob('../assets/agents/*.{svg,png}', { eager: true, query: '?url', import: 'default' }) as Record<string, string>
const URLS: Record<string, string> = Object.fromEntries(Object.entries(FILES).map(([path, url]) => [path.slice(path.lastIndexOf('/') + 1), url]))

const props = withDefaults(
  defineProps<{
    /** Adapter id; `''` = a terminal row without an agent; null/undefined = not known yet. */
    agent: string | null | undefined
    size?: number
    /** `idle` (or null) draws no ring. */
    status?: 'working' | 'waiting' | 'limited' | 'error' | 'idle' | null
    /** Tooltip and accessible name; defaults to the agent's name. */
    label?: string | null
    /** Next to its visible name: hidden from assistive tech, no tooltip. */
    decorative?: boolean
    /** A custom agent still being edited (not in the status yet): its letter comes from `label`. */
    custom?: boolean
  }>(),
  { size: 14, status: null, label: null, decorative: false, custom: false },
)

const store = usePilotStore()
const adapter = computed(() => (props.agent ? store.adapters.find((a) => a.id === props.agent) : undefined))
const glyph = computed(() =>
  agentGlyph(props.agent, {
    label: adapter.value?.label ?? (props.custom ? props.label : null),
    image: props.agent ? store.agentImages[props.agent] : null,
    known: Boolean(adapter.value) || props.custom,
  }),
)
const src = computed(() => {
  const g = glyph.value
  if (g.kind === 'image') return g.src
  if (g.kind === 'mark') return URLS[g.mark.file] ?? null
  return null
})
const mono = computed(() => glyph.value.kind === 'mark' && glyph.value.mark.tone === 'mono')
const maskStyle = computed(() => ({ maskImage: `url("${src.value}")` }))
const ring = computed(() => (props.status && props.status !== 'idle' ? props.status : null))
const letter = computed(() => (glyph.value.kind === 'letter' ? glyph.value.letter : '?'))
const name = computed(() => {
  if (props.label) return props.label
  if (props.agent === '') return 'Terminal'
  if (props.agent == null) return 'Agent not known yet'
  return store.adapterLabel(props.agent)
})
const STATUS: Record<string, string> = { working: 'working', waiting: 'needs you', limited: 'usage limit', error: 'failed' }
const title = computed(() => (ring.value ? `${name.value} — ${STATUS[ring.value]}` : name.value))
</script>

<template>
  <span
    class="qp-aicon"
    :class="ring ? ['qp-aicon-ring', `is-${ring}`] : null"
    :style="{ '--qp-aicon-size': `${size}px`, '--qp-aicon-gap': `${size <= 12 ? 2 : 3}px` }"
    :role="decorative ? undefined : 'img'"
    :aria-label="decorative ? undefined : title"
    :aria-hidden="decorative ? 'true' : undefined"
    :title="decorative ? undefined : title"
    :data-agent="agent ?? ''"
    :data-glyph="glyph.kind"
  >
    <span v-if="src && mono" class="qp-aicon-mono" :style="maskStyle" />
    <img v-else-if="src" class="qp-aicon-img" :src="src" alt="" draggable="false" />
    <SquareTerminal v-else-if="glyph.kind === 'terminal'" class="qp-aicon-term" :size="size" :stroke-width="1.75" />
    <span v-else class="qp-aicon-letter" :class="{ unknown: glyph.kind === 'unknown' }">{{ letter }}</span>
  </span>
</template>

<style scoped>
@property --qp-aicon-turn {
  syntax: '<angle>';
  inherits: false;
  initial-value: 0deg;
}

.qp-aicon {
  position: relative;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: var(--qp-aicon-size);
  height: var(--qp-aicon-size);
  line-height: 1;
  vertical-align: middle;
}
.qp-aicon-img,
.qp-aicon-mono {
  display: block;
  width: 100%;
  height: 100%;
  border-radius: 3px;
}
.qp-aicon-img { object-fit: contain; user-select: none; -webkit-user-drag: none; }
.qp-aicon-mono {
  background: currentColor;
  mask-size: contain;
  mask-repeat: no-repeat;
  mask-position: center;
}
.qp-aicon-term { color: var(--qss-text-secondary, #9a9aa5); }
.qp-aicon-letter {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 100%;
  height: 100%;
  border-radius: 4px;
  background: color-mix(in srgb, currentColor 18%, transparent);
  font-family: var(--qss-font-sans, system-ui, sans-serif);
  font-size: calc(var(--qp-aicon-size) * 0.62);
  font-weight: 700;
}
.qp-aicon-letter.unknown { color: var(--qss-text-muted, #6e6e7a); font-weight: 600; }

/* The status ring sits around the logo (3 px out, 2 px at small sizes), so the mark stays whole. */
.qp-aicon-ring::after {
  content: '';
  position: absolute;
  inset: calc(var(--qp-aicon-gap, 3px) * -1);
  border-radius: calc(var(--qp-aicon-gap, 3px) + 3px);
  border: 1.5px solid var(--qp-aicon-ring, transparent);
  pointer-events: none;
}
.qp-aicon-ring.is-waiting { --qp-aicon-ring: var(--qss-warning, #d29a3f); }
.qp-aicon-ring.is-limited { --qp-aicon-ring: var(--qss-warning, #d29a3f); }
.qp-aicon-ring.is-limited::after { border-style: dashed; }
.qp-aicon-ring.is-error { --qp-aicon-ring: var(--qss-error, #f87171); }
.qp-aicon-ring.is-working::after {
  border-color: transparent;
  background: conic-gradient(
      from var(--qp-aicon-turn),
      var(--qss-success, #34d399) 0 34%,
      color-mix(in srgb, var(--qss-success, #34d399) 26%, transparent) 34% 100%
    )
    border-box;
  mask: linear-gradient(#000 0 0) padding-box exclude, linear-gradient(#000 0 0);
  -webkit-mask: linear-gradient(#000 0 0) padding-box, linear-gradient(#000 0 0);
  -webkit-mask-composite: xor;
  animation: qp-aicon-turn 1.1s linear infinite;
}
@keyframes qp-aicon-turn {
  to { --qp-aicon-turn: 360deg; }
}
@media (prefers-reduced-motion: reduce) {
  .qp-aicon-ring.is-working::after { animation: none; }
}
</style>
