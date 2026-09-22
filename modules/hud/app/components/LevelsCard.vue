<template>
  <div class="card levels-card">
    <div class="card-header">
      <span
        class="direction-indicator"
        :class="{ long: isLong, short: !isLong }"
      >
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          style="margin-right: 4px; vertical-align: middle"
        >
          <polyline
            :points="
              isLong
                ? '22 12 18 12 15 3 9 21 6 12 2 12'
                : '22 12 18 12 15 21 9 3 6 12 2 12'
            "
          />
        </svg>
        {{ isLong ? "LONG" : "SHORT" }}
      </span>
    </div>

    <div class="level-row">
      <label class="level-label">Entry</label>
      <input
        class="input level-input"
        type="text"
        :value="formatPrice(levels.entry)"
        @input="updateLevel('entry', $event)"
      />
      <button
        class="btn btn-icon btn-ghost"
        @click="$emit('copy', String(levels.entry))"
      >
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <rect x="9" y="9" width="13" height="13" rx="2" />
          <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
        </svg>
      </button>
    </div>

    <div class="level-row">
      <label class="level-label">TP</label>
      <input
        class="input level-input"
        type="text"
        :value="formatPrice(levels.tp)"
        @input="updateLevel('tp', $event)"
      />
      <button
        class="btn btn-icon btn-ghost"
        @click="$emit('copy', String(levels.tp))"
      >
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <rect x="9" y="9" width="13" height="13" rx="2" />
          <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
        </svg>
      </button>
    </div>

    <div class="level-row">
      <label class="level-label">SL</label>
      <input
        class="input level-input"
        type="text"
        :value="formatPrice(levels.sl)"
        @input="updateLevel('sl', $event)"
      />
      <button
        class="btn btn-icon btn-ghost"
        @click="$emit('copy', String(levels.sl))"
      >
        <svg
          width="14"
          height="14"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
        >
          <rect x="9" y="9" width="13" height="13" rx="2" />
          <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
        </svg>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
interface Levels {
  entry: number;
  tp: number;
  sl: number;
}

const props = defineProps<{
  isLong: boolean;
  levels: Levels;
}>();

const emit = defineEmits<{
  "update:levels": [levels: Levels];
  copy: [value: string];
}>();

function formatPrice(price: number): string {
  if (!price) return "";
  return price.toString();
}

function updateLevel(key: keyof Levels, event: Event) {
  const target = event.target as HTMLInputElement;
  const value = parseFloat(target.value.replace(/,/g, "")) || 0;
  emit("update:levels", { ...props.levels, [key]: value });
}
</script>

<style scoped>
.levels-card {
  margin: 8px 0;
}

.card-header {
  text-align: center;
  margin-bottom: 8px;
}

.direction-indicator {
  font-size: 16px;
  font-weight: 700;
}

.direction-indicator.long {
  color: var(--accent-green);
}

.direction-indicator.short {
  color: var(--accent-red);
}

.level-row {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 5px 0;
}

.level-label {
  width: 50px;
  font-size: 14px;
  font-weight: 500;
}

.level-input {
  flex: 1;
}
</style>
