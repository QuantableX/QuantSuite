<script setup lang="ts">
/**
 * A keyboard-first option list for the composer's pickers: Up/Down move,
 * Home/End jump, Enter picks, Ctrl+1..9 picks directly, Escape closes.
 * Disabled rows say why. The list takes focus when it opens.
 */
import { nextTick, onMounted, ref, watch } from 'vue'
import { Check } from 'lucide-vue-next'
import type { MenuOption } from '#pilot/utils/models'

const props = defineProps<{ options: MenuOption[]; selected: string; title?: string }>()
const emit = defineEmits<{ pick: [id: string]; close: [] }>()

const list = ref<HTMLElement | null>(null)
const active = ref(0)

function firstEnabled(from: number, dir: 1 | -1): number {
  const n = props.options.length
  for (let i = 0; i < n; i++) {
    const at = (from + dir * i + n * 2) % n
    if (!props.options[at]?.disabled) return at
  }
  return -1
}

watch(
  () => props.options,
  () => {
    const sel = props.options.findIndex((o) => o.id === props.selected)
    active.value = sel >= 0 ? sel : firstEnabled(0, 1)
  },
  { immediate: true }
)

function pick(i: number) {
  const o = props.options[i]
  if (!o || o.disabled) return
  emit('pick', o.id)
}

function onKey(e: KeyboardEvent) {
  const n = props.options.length
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    active.value = firstEnabled(active.value + 1, 1)
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    active.value = firstEnabled(active.value - 1 + n, -1)
  } else if (e.key === 'Home') {
    e.preventDefault()
    active.value = firstEnabled(0, 1)
  } else if (e.key === 'End') {
    e.preventDefault()
    active.value = firstEnabled(n - 1, -1)
  } else if (e.key === 'Enter' || e.key === ' ') {
    e.preventDefault()
    pick(active.value)
  } else if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    emit('close')
  } else if ((e.ctrlKey || e.metaKey) && /^[1-9]$/.test(e.key)) {
    e.preventDefault()
    pick(Number(e.key) - 1)
  }
}

onMounted(() => void nextTick(() => list.value?.focus()))
</script>

<template>
  <div ref="list" class="qp-menu" role="listbox" tabindex="0" :aria-label="title" @keydown="onKey">
    <div v-if="title" class="qp-menu-title">{{ title }}</div>
    <button
      v-for="(o, i) in options"
      :key="o.id"
      class="qp-menu-row"
      role="option"
      tabindex="-1"
      :aria-selected="o.id === selected"
      :class="{ active: i === active, disabled: o.disabled }"
      :title="o.disabled ? o.reason : o.description"
      @mouseenter="!o.disabled && (active = i)"
      @click="pick(i)"
    >
      <span class="qp-menu-check"><Check v-if="o.id === selected" :size="13" /></span>
      <span class="qp-menu-text">
        <span class="qp-menu-label">{{ o.label }}</span>
        <span v-if="o.disabled && o.reason" class="qp-menu-desc">{{ o.reason }}</span>
        <span v-else-if="o.description" class="qp-menu-desc">{{ o.description }}</span>
      </span>
      <span v-if="i < 9" class="qp-menu-key">Ctrl+{{ i + 1 }}</span>
    </button>
  </div>
</template>

<style scoped>
.qp-menu {
  width: max-content;
  min-width: 220px;
  max-width: 380px;
  max-height: 440px;
  overflow-y: auto;
  padding: 4px;
  border-radius: 10px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 10px 28px rgba(0, 0, 0, 0.4);
  outline: none;
  animation: qp-menu-in 0.17s ease-out;
}
@keyframes qp-menu-in {
  from { opacity: 0; transform: translateY(4px) scale(0.98); }
  to { opacity: 1; transform: none; }
}
@media (prefers-reduced-motion: reduce) {
  .qp-menu { animation: qp-menu-fade 0.12s linear; }
  @keyframes qp-menu-fade { from { opacity: 0; } to { opacity: 1; } }
}
.qp-menu-title { padding: 6px 8px 4px; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-menu-row {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  width: 100%;
  padding: 6px 8px 6px 4px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  text-align: left;
  cursor: pointer;
}
.qp-menu-row.active { background: var(--qss-bg-hover, #313139); }
.qp-menu-row.disabled { opacity: 0.45; cursor: default; }
.qp-menu-check { width: 16px; flex-shrink: 0; display: inline-flex; justify-content: center; padding-top: 1px; color: var(--qss-accent, #7aa2f7); }
.qp-menu-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
.qp-menu-label { font-size: 12.5px; }
.qp-menu-desc { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); line-height: 1.4; max-width: 300px; }
.qp-menu-key { flex-shrink: 0; font-size: 10.5px; color: var(--qss-text-muted, #6e6e7a); padding-top: 2px; opacity: 0; }
.qp-menu-row.active .qp-menu-key { opacity: 1; }
</style>
