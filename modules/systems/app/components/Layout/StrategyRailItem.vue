<script setup lang="ts">
import { strategyIcon } from '#systems/utils/strategyIcons'
import type { SystemMeta } from '#systems/types'
defineProps<{ strategy: SystemMeta; active: boolean; shortcut: number }>()
defineEmits<{ select: [] }>()
</script>

<template>
  <button class="qs-strategy" :class="{ 'qs-strategy--active': active }" :disabled="strategy.status !== 'ready'"
    :data-strategy-id="strategy.id"
    :aria-current="active ? 'page' : undefined"
    :title="`${strategy.name}${shortcut < 9 ? ` — Ctrl+${shortcut + 1}` : ''}`" @click="$emit('select')">
    <component :is="strategyIcon(strategy).component" :size="16" :stroke-width="1.7" aria-hidden="true" />
    <span class="qs-strategy__name">{{ strategy.short?.trim() || strategy.name }}</span>
    <span v-if="strategy.status !== 'ready'" class="qs-strategy__short">soon</span>
  </button>
</template>

<style scoped>
.qs-strategy { display: flex; align-items: center; gap: 8px; width: 100%; min-width: 0; padding: 9px 8px; border: 0; border-radius: 6px; background: transparent; color: var(--qs-text-secondary); text-align: left; cursor: pointer; }
.qs-strategy:hover, .qs-strategy--active { background: var(--qs-bg-hover); color: var(--qs-text); }
.qs-strategy--active { box-shadow: inset 2px 0 var(--qs-accent); }
.qs-strategy:focus-visible { outline: 2px solid var(--qs-accent); outline-offset: -2px; }
.qs-strategy:disabled { opacity: .5; cursor: not-allowed; }
.qs-strategy:enabled { cursor: grab; }
.qs-strategy:enabled:active { cursor: grabbing; }
.qs-strategy svg { flex-shrink: 0; color: var(--qs-accent); }
.qs-strategy__name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; }
.qs-strategy__short { flex-shrink: 0; max-width: 40px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 9px; color: var(--qs-text-muted); }
</style>
