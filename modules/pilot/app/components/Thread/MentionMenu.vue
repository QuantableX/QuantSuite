<script setup lang="ts">
/**
 * The composer's `@` / `/` / `$` menu (card 9): it opens above the box
 * while the caret sits in a trigger; the box keeps focus and its keys
 * (Up/Down, Enter or Tab picks, Escape closes) drive the menu.
 */
import { nextTick, ref, watch, type Component } from 'vue'
import { Bot, File, Folder, Loader2, MessagesSquare, Slash, Sparkles } from 'lucide-vue-next'

export interface MenuRow {
  key: string
  label: string
  detail?: string
  badge?: string
  icon: 'file' | 'folder' | 'thread' | 'skill' | 'command' | 'agent'
}

const props = defineProps<{ rows: MenuRow[]; active: number; loading: boolean; title: string; empty: string }>()
const emit = defineEmits<{ pick: [index: number]; hover: [index: number] }>()

const ICONS: Record<MenuRow['icon'], Component> = { file: File, folder: Folder, thread: MessagesSquare, skill: Sparkles, command: Slash, agent: Bot }
const list = ref<HTMLElement | null>(null)

watch(
  () => props.active,
  async (i) => {
    await nextTick()
    list.value?.querySelector<HTMLElement>(`[data-row="${i}"]`)?.scrollIntoView({ block: 'nearest' })
  }
)
</script>

<template>
  <div class="qp-mm" role="listbox" :aria-label="title" @mousedown.prevent>
    <div class="qp-mm-head">
      <span>{{ title }}</span>
      <Loader2 v-if="loading" class="qp-mm-spin" :size="12" />
      <span class="qp-mm-keys">↑↓ · Enter · Esc</span>
    </div>
    <div ref="list" class="qp-mm-list">
      <div v-if="!rows.length && !loading" class="qp-mm-empty">{{ empty }}</div>
      <button
        v-for="(r, i) in rows"
        :key="r.key"
        class="qp-mm-row"
        role="option"
        tabindex="-1"
        :data-row="i"
        :aria-selected="i === active"
        :class="{ on: i === active }"
        @mouseenter="emit('hover', i)"
        @click="emit('pick', i)"
      >
        <component :is="ICONS[r.icon]" class="qp-mm-icon" :size="13" />
        <span class="qp-mm-label">{{ r.label }}</span>
        <span v-if="r.detail" class="qp-mm-detail">{{ r.detail }}</span>
        <span v-if="r.badge" class="qp-mm-badge">{{ r.badge }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.qp-mm {
  position: absolute;
  left: 0;
  right: 0;
  bottom: calc(100% + 6px);
  z-index: 30;
  display: flex;
  flex-direction: column;
  max-height: 320px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 10px;
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.45);
  overflow: hidden;
}
.qp-mm-head {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 10px;
  border-bottom: 1px solid var(--qss-border-subtle, #35353d);
  font-size: 11px;
  color: var(--qss-text-muted, #6e6e7a);
}
.qp-mm-keys { margin-left: auto; }
.qp-mm-spin { animation: qp-mm-spin 900ms linear infinite; }
@keyframes qp-mm-spin { to { transform: rotate(360deg); } }
.qp-mm-list { overflow-y: auto; padding: 4px; }
.qp-mm-empty { padding: 10px; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); }
.qp-mm-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  min-width: 0;
  padding: 5px 8px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12.5px;
  text-align: left;
  cursor: pointer;
}
.qp-mm-row.on { background: var(--qss-bg-hover, #313139); }
.qp-mm-icon { flex-shrink: 0; color: var(--qss-text-secondary, #9a9aa5); }
.qp-mm-label { flex-shrink: 0; max-width: 55%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-mm-detail { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-mm-badge {
  flex-shrink: 0;
  padding: 0 6px;
  border-radius: 8px;
  background: var(--qss-bg-card, #292930);
  font-size: 10.5px;
  color: var(--qss-text-secondary, #9a9aa5);
}
</style>
