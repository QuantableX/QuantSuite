<script setup lang="ts">
/**
 * QuantFlow's calendar toolbar: date navigation and calendar view tabs.
 */
import { useAppStore } from '#plan/stores/app'
import type { ViewKind } from '#plan/types'

const app = useAppStore()

/** 24×24 stroke paths per view, per PLAN-V3 §3 — no emoji, no glyphs. */
const VIEWS: Array<{ kind: ViewKind; label: string; icon: string; key: string }> = [
  { kind: 'day', label: 'Day', icon: 'M6 4h12v16H6z M6 9h12', key: 'D' },
  { kind: 'week', label: 'Week', icon: 'M3 5h18v14H3z M3 9h18 M9 9v10 M15 9v10', key: 'W' },
  { kind: 'month', label: 'Month', icon: 'M4 5h16v15H4z M4 10h16 M9 10v10 M14 10v10 M4 15h16', key: 'M' },
  { kind: 'agenda', label: 'Agenda', icon: 'M4 6h3 M10 6h10 M4 12h3 M10 12h10 M4 18h3 M10 18h10', key: 'A' },
]
</script>

<template>
  <div class="qp-toolbar">
    <div class="qp-hd">
      <div class="qp-hd__nav">
        <button class="qp-hd__today" @click="app.today()">Today</button>
        <button class="qp-hd__arrow" aria-label="Previous" @click="app.step(-1)">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="m15 6-6 6 6 6" />
          </svg>
        </button>
        <button class="qp-hd__arrow" aria-label="Next" @click="app.step(1)">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path d="m9 6 6 6-6 6" />
          </svg>
        </button>
        <span class="qp-hd__title">{{ app.title }}</span>
      </div>

      <div class="qp-hd__views">
        <button
          v-for="entry in VIEWS"
          :key="entry.kind"
          class="qp-hd__view"
          :class="{ 'is-active': app.view === entry.kind }"
          :aria-pressed="app.view === entry.kind"
          :title="`${entry.label} (${entry.key})`"
          @click="app.setView(entry.kind)"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path :d="entry.icon" />
          </svg>
          <span>{{ entry.label }}</span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qp-toolbar { display: flex; flex-shrink: 0; min-height: 78px; border-bottom: 1px solid var(--qp-border-subtle); }
.qp-hd {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 16px 24px;
  flex-wrap: wrap;
}

.qp-hd__nav {
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 4px;
}

.qp-hd__today {
  padding: 4px 11px;
  border: 1px solid var(--qp-border);
  border-radius: var(--qp-radius);
  background: var(--qp-bg-card);
  color: var(--qp-text-secondary);
  font-size: 12px;
  cursor: pointer;
}

.qp-hd__today:hover {
  border-color: var(--qp-accent);
  color: var(--qp-text);
}

.qp-hd__arrow {
  display: grid;
  place-items: center;
  width: 30px;
  height: 30px;
  border: none;
  border-radius: var(--qp-radius);
  background: transparent;
  color: var(--qp-text-muted);
  cursor: pointer;
}

.qp-hd__arrow:hover {
  background: var(--qp-bg-hover);
  color: var(--qp-text);
}

.qp-hd__title {
  margin-left: 8px;
  overflow: hidden;
  color: var(--qp-text);
  font-size: 22px;
  font-weight: 500;
  letter-spacing: -.03em;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qp-hd__views {
  display: flex;
  flex-shrink: 0;
  gap: 2px;
  padding: 3px;
  border: 1px solid var(--qp-border);
  border-radius: 10px;
  background: var(--qp-bg-raised);
}

.qp-hd__view {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 6px 10px;
  border: none;
  border-radius: var(--qp-radius);
  background: transparent;
  color: var(--qp-text-muted);
  font-size: 12px;
  cursor: pointer;
}

.qp-hd__view:hover {
  background: var(--qp-bg-hover);
  color: var(--qp-text-secondary);
}

.qp-hd__view.is-active {
  background: var(--qp-bg-card);
  color: var(--qp-text);
}
</style>
