<script setup lang="ts">
import { Layers, ChartNoAxesCombined, Cpu, Code2 } from 'lucide-vue-next'

withDefaults(defineProps<{ appId: string; size?: number }>(), { size: 18 })

// One app identity for the navigation rail and Home cards.
const icons = {
  quantzen: Layers,
  quantview: ChartNoAxesCombined,
  quantagent: Cpu,
  quantspace: Code2,
}
</script>

<template>
  <component
    :is="icons[appId as keyof typeof icons]"
    v-if="appId in icons"
    :size="size"
    :stroke-width="1.5"
    aria-hidden="true"
  />
  <span v-else aria-hidden="true">Q</span>
</template>
