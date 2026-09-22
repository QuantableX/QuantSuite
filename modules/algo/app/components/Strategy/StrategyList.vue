<script setup lang="ts">
import type { Strategy } from '#algo/types'
import { formatDate } from '#algo/utils/format'

defineProps<{
  strategies: Strategy[]
  activeId: string | null
}>()

const emit = defineEmits<{
  select: [id: string]
  create: []
  delete: [id: string]
}>()

function handleDelete(e: Event, id: string) {
  e.stopPropagation()
  emit('delete', id)
}
</script>

<template>
  <div class="strategy-list">
    <button class="btn btn-sm new-btn" @click="emit('create')">
      + New Strategy
    </button>

    <div v-if="strategies.length === 0" class="empty">
      No strategies yet
    </div>

    <div v-else class="list">
      <button
        v-for="strat in strategies"
        :key="strat.id"
        class="item"
        :class="{ active: activeId === strat.id }"
        @click="emit('select', strat.id)"
      >
        <div class="item-content">
          <div class="item-name">{{ strat.name }}</div>
          <div v-if="strat.description" class="item-desc">
            {{ strat.description }}
          </div>
          <div class="item-date">
            {{ formatDate(strat.updated_at) }}
          </div>
        </div>
        <button
          class="delete-btn"
          aria-label="Delete strategy"
          @click="handleDelete($event, strat.id)"
        >
          &times;
        </button>
      </button>
    </div>
  </div>
</template>

<style scoped>
.strategy-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.new-btn {
  width: 100%;
}

.empty {
  font-size: 13px;
  color: var(--qa-text-muted);
  text-align: center;
  padding: 24px 0;
}

.list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 10px;
  border-radius: 6px;
  background: none;
  border: none;
  border-left: 2px solid transparent;
  cursor: pointer;
  text-align: left;
  color: var(--qa-text);
  transition: background var(--qa-transition), border-color var(--qa-transition);
}

.item:hover {
  background: var(--qa-bg-hover);
}

.item.active {
  background: var(--qa-bg-hover);
  border-left-color: var(--qa-accent);
}

.item-content {
  flex: 1;
  min-width: 0;
}

.item-name {
  font-size: 14px;
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.item-desc {
  font-size: 12px;
  color: var(--qa-text-muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-top: 2px;
}

.item-date {
  font-size: 11px;
  color: var(--qa-text-muted);
  margin-top: 2px;
}

.delete-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border-radius: 4px;
  border: none;
  background: none;
  color: var(--qa-text-muted);
  font-size: 16px;
  line-height: 1;
  cursor: pointer;
  opacity: 0;
  transition: opacity var(--qa-transition), background var(--qa-transition), color var(--qa-transition);
}

.item:hover .delete-btn {
  opacity: 1;
}

.delete-btn:hover {
  background: var(--qa-error);
  color: #fff;
}
</style>
