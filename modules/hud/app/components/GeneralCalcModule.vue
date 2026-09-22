<template>
  <div class="gc-module">
    <div class="gc-tabs">
      <button
        :class="{ active: mode === 'calculator' }"
        @click="mode = 'calculator'"
      >
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          style="vertical-align: middle; margin-right: 3px"
        >
          <rect x="4" y="2" width="16" height="20" rx="2" />
          <line x1="8" y1="6" x2="16" y2="6" />
          <line x1="8" y1="10" x2="10" y2="10" />
          <line x1="14" y1="10" x2="16" y2="10" />
          <line x1="8" y1="14" x2="10" y2="14" />
          <line x1="14" y1="14" x2="16" y2="14" />
          <line x1="8" y1="18" x2="16" y2="18" />
        </svg>
        Calculator
      </button>
      <button
        :class="{ active: mode === 'converter' }"
        @click="mode = 'converter'"
      >
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          style="vertical-align: middle; margin-right: 3px"
        >
          <polyline points="17 1 21 5 17 9" />
          <path d="M3 11V9a4 4 0 0 1 4-4h14" />
          <polyline points="7 23 3 19 7 15" />
          <path d="M21 13v2a4 4 0 0 1-4 4H3" />
        </svg>
        Converter
      </button>
    </div>
    <div v-if="mode === 'calculator'" class="gc-panel">
      <div class="calc-display">
        <div class="calc-expr">{{ expression || "&nbsp;" }}</div>
        <input
          ref="calcInput"
          class="calc-result-input"
          :value="display"
          @keydown="onKeyDown"
          @input="onTypedInput"
          spellcheck="false"
          autocomplete="off"
        />
      </div>
      <div class="calc-grid">
        <button
          v-for="key in calcKeys"
          :key="key"
          :class="['calc-key', keyClass(key)]"
          @click="pressKey(key)"
        >
          {{ key }}
        </button>
      </div>
      <!-- Recent Calculations -->
      <div v-if="history.length" class="calc-history">
        <div class="calc-history-header">
          <span class="calc-history-title">Recent</span>
          <button class="btn btn-ghost btn-sm" @click="clearHistory">
            Clear
          </button>
        </div>
        <div class="calc-history-list">
          <div
            v-for="entry in history"
            :key="entry.id"
            class="calc-history-row"
            @click="replayHistoryEntry(entry)"
            title="Click to load expression"
          >
            <div class="calc-history-info">
              <span class="calc-history-expr">{{ entry.expression }}</span>
              <span class="calc-history-res">= {{ entry.result }}</span>
            </div>
            <button
              class="calc-history-del"
              @click.stop="removeHistoryEntry(entry.id)"
              title="Remove"
            >
              ✕
            </button>
          </div>
        </div>
      </div>
    </div>
    <div v-if="mode === 'converter'" class="gc-panel">
      <div class="conv-row">
        <select
          v-model="category"
          class="input sm"
          @change="setCategory(category)"
        >
          <option v-for="cat in categories" :key="cat" :value="cat">
            {{ cat }}
          </option>
        </select>
      </div>
      <div class="conv-fields">
        <div class="conv-field">
          <select v-model="fromUnit" class="input sm">
            <option v-for="u in currentUnits" :key="u" :value="u">
              {{ u }}
            </option>
          </select>
          <input
            v-model.number="fromValue"
            class="input"
            type="number"
            @input="convertUnits"
          />
        </div>
        <button class="btn btn-ghost swap-btn" @click="swapUnits">⇄</button>
        <div class="conv-field">
          <select v-model="toUnit" class="input sm">
            <option v-for="u in currentUnits" :key="u" :value="u">
              {{ u }}
            </option>
          </select>
          <input :value="toValue" class="input" readonly />
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { useGeneralCalc } from '#hud/composables/useGeneralCalc'
const {
  mode,
  display,
  expression,
  hasResult,
  history,
  pressKey,
  clearHistory,
  removeHistoryEntry,
  replayHistoryEntry,
  category,
  fromUnit,
  toUnit,
  fromValue,
  toValue,
  convertUnits,
  swapUnits,
  setCategory,
  categories,
  currentUnits,
} = useGeneralCalc();

const calcInput = ref<HTMLInputElement | null>(null);

const calcKeys = [
  "C",
  "⌫",
  "%",
  "÷",
  "7",
  "8",
  "9",
  "×",
  "4",
  "5",
  "6",
  "-",
  "1",
  "2",
  "3",
  "+",
  "±",
  "0",
  ".",
  "=",
];

function keyClass(k: string) {
  if (["C", "⌫"].includes(k)) return "fn";
  if (["÷", "×", "-", "+", "%"].includes(k)) return "op";
  if (k === "=") return "eq";
  return "";
}

const KEY_MAP: Record<string, string> = {
  Enter: "=",
  Escape: "C",
  Backspace: "⌫",
  "/": "÷",
  "*": "×",
};

function onKeyDown(e: KeyboardEvent) {
  const mapped = KEY_MAP[e.key];
  if (mapped) {
    e.preventDefault();
    pressKey(mapped);
    return;
  }
  // Allow digits, operators, dot, percent directly
  if (/^[0-9.+\-]$/.test(e.key) || e.key === "%") {
    e.preventDefault();
    pressKey(e.key);
    return;
  }
  // Block everything else to keep display in sync
  e.preventDefault();
}

function onTypedInput(e: Event) {
  // Prevent native input from desyncing - display is driven by composable
  const target = e.target as HTMLInputElement;
  target.value = display.value;
}
</script>

<style scoped>
.gc-module {
  padding: 4px 0;
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}
.gc-tabs {
  display: flex;
  gap: 6px;
  margin-bottom: 12px;
}
.gc-tabs button {
  flex: 1;
  padding: 6px 4px;
  font-size: 11px;
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-secondary);
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.15s ease;
}
.gc-tabs button.active {
  background: var(--btn-primary);
  color: var(--text-primary);
  border-color: var(--btn-primary);
}
.gc-panel {
  min-height: 60px;
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}
.calc-display {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 8px;
  padding: 10px 12px;
  margin-bottom: 8px;
  text-align: right;
}
.calc-expr {
  font-size: 11px;
  color: var(--text-secondary);
  min-height: 16px;
}
.calc-result-input {
  width: 100%;
  font-size: 28px;
  font-weight: 700;
  font-family: monospace;
  color: var(--text-primary);
  background: transparent;
  border: none;
  outline: none;
  text-align: right;
  padding: 0;
  caret-color: var(--accent-blue);
}
.calc-grid {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 4px;
}
.calc-key {
  padding: 10px 0;
  font-size: 16px;
  border: 1px solid var(--border-color);
  background: var(--bg-card);
  color: var(--text-primary);
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.1s;
}
.calc-key:hover {
  background: var(--bg-secondary);
}
.calc-key.fn {
  color: var(--accent-red);
}
.calc-key.op {
  color: var(--accent-blue);
}
.calc-key.eq {
  background: var(--accent-green);
  color: #fff;
  border-color: var(--accent-green);
}
.conv-row {
  margin-bottom: 8px;
}
.conv-fields {
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: center;
}
.conv-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  width: 100%;
}
.swap-btn {
  font-size: 18px;
  padding: 2px 8px;
}
.input.sm {
  font-size: 12px;
  padding: 4px 8px;
}
/* History list - matches screenshot/clipboard containerized style */
.calc-history {
  margin-top: 10px;
  display: flex;
  flex-direction: column;
  flex: 1;
  min-height: 0;
}
.calc-history-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 6px;
}
.calc-history-title {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}
.calc-history-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  flex: 1;
  overflow-y: auto;
  min-height: 0;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 4px;
  background: var(--bg-secondary);
}
.calc-history-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 6px 8px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  cursor: pointer;
  transition: background 0.15s;
  flex-shrink: 0;
}
.calc-history-row:hover {
  background: var(--bg-secondary);
}
.calc-history-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.calc-history-expr {
  font-size: 11px;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-family: monospace;
}
.calc-history-res {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
  font-family: monospace;
}
.calc-history-del {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 10px;
  width: 20px;
  height: 20px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 3px;
  flex-shrink: 0;
}
.calc-history-del:hover {
  color: var(--accent-red);
}
</style>
