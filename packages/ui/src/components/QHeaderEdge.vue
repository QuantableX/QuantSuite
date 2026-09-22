<script setup lang="ts">
/**
 * A corner wedge in the module header — the full-height, 51px square that sits
 * hard against the logo cap or the actions cap and toggles the panel below it.
 *
 * QuantCanvas' shape, shared since 2026-08-26 (user decision) so QuantCode and
 * QuantConsole carry the same two edges. It is square and borderless on purpose:
 * the wedge reads as part of the header's frame, not as a button floating in it,
 * which is what keeps the eye on the search field in the middle.
 *
 * The header's centre slot is `align-items: stretch`, so `height: 100%` here
 * spans the whole bar.
 */
withDefaults(
  defineProps<{
    /** Which cap it leans against — decides the side its divider sits on. */
    side: 'left' | 'right'
    /** The panel it controls is open. */
    active?: boolean
    label?: string
  }>(),
  { active: false, label: undefined }
)

defineEmits<{ (e: 'click'): void }>()
</script>

<template>
  <button
    class="qhe"
    :class="[`qhe--${side}`, { 'is-active': active }]"
    :title="label"
    :aria-label="label"
    :aria-pressed="active"
    style="-webkit-app-region: no-drag"
    @click="$emit('click')"
  >
    <slot />
  </button>
</template>

<style scoped>
.qhe {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 51px;
  height: 100%;
  border: none;
  background: transparent;
  color: var(--qss-text-muted);
  cursor: pointer;
  transition: color 150ms ease, background-color 150ms ease;
}

.qhe--left {
  border-right: 1px solid var(--qss-border);
}
.qhe--right {
  border-left: 1px solid var(--qss-border);
}

.qhe:hover {
  color: var(--qss-text);
  background: var(--qss-bg-raised);
}

/* Open is the resting state, so it is NOT highlighted — the wedge lights up on
   hover only. Marking "open" would leave the header permanently lit, since both
   panels are open almost all the time. */
.qhe.is-active {
  color: var(--qss-text-secondary);
}

.qhe :deep(svg) {
  width: 22px;
  height: 22px;
}
</style>
