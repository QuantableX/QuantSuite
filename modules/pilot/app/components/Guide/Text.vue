<script setup lang="ts">
/** A guide's sentence with its `code` spans as inline code (round 3, card 04). */
import { computed } from 'vue'

const props = defineProps<{ text: string }>()
const parts = computed(() => {
  const bits = props.text.split('`')
  // An odd backtick count leaves the last one as plain text.
  if (bits.length % 2 === 0) bits[bits.length - 2] += `\`${bits.pop()}`
  return bits.map((text, i) => ({ text, code: i % 2 === 1 && text.length > 0 }))
})
</script>

<template>
  <span><template v-for="(p, i) in parts" :key="i"><code v-if="p.code" class="qp-guide-code">{{ p.text }}</code><template v-else>{{ p.text }}</template></template></span>
</template>

<style scoped>
.qp-guide-code { padding: 0 4px; border-radius: 4px; background: var(--qss-bg-card, #292930); font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 0.92em; overflow-wrap: anywhere; }
</style>
