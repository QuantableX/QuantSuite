<script setup lang="ts">
/**
 * A note's icon, wherever a note is listed: the stroke symbol for a known key,
 * the raw glyph for a pre-switch row that still holds an emoji, and the
 * document symbol when a note has no icon at all.
 *
 * Stroke width is a prop rather than fixed: the same path drawn at 34px with
 * the 1.7 the chrome uses everywhere else reads as a marker, not a line.
 */
import { noteIconDef, DEFAULT_NOTE_ICON } from '#notes/utils/noteIcons'

const props = withDefaults(
  defineProps<{
    /** The note's stored icon: an icon key, a legacy glyph, or nothing. */
    icon?: string | null
    size?: number
    stroke?: number
    /** false renders nothing when the note has no icon of its own. */
    fallback?: boolean
  }>(),
  { icon: null, size: 16, stroke: 1.7, fallback: true },
)

const symbol = computed(() => {
  const known = noteIconDef(props.icon)
  if (known) return known
  // A glyph from before the switch stays a glyph; only a note with no icon at
  // all falls back to the document symbol.
  if (props.icon) return undefined
  return props.fallback ? noteIconDef(DEFAULT_NOTE_ICON) : undefined
})

const glyph = computed(() => (symbol.value ? '' : (props.icon ?? '')))
</script>

<template>
  <svg
    v-if="symbol"
    class="qn-icon"
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    :stroke-width="stroke"
    stroke-linecap="round"
    stroke-linejoin="round"
    role="img"
    :aria-label="symbol.label"
  >
    <path :d="symbol.d" />
  </svg>
  <span v-else-if="glyph" class="qn-icon qn-icon--glyph" :style="{ fontSize: `${size}px` }">{{ glyph }}</span>
</template>

<style scoped>
.qn-icon {
  flex-shrink: 0;
  display: block;
}

.qn-icon--glyph {
  line-height: 1;
}
</style>
