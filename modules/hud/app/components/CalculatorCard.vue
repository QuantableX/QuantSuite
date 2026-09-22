<template>
  <div class="card calculator-card">
    <div class="card-title">
      <svg
        width="16"
        height="16"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        style="margin-right: 6px; vertical-align: middle"
      >
        <rect x="4" y="2" width="16" height="20" rx="2" />
        <line x1="8" y1="6" x2="16" y2="6" />
        <line x1="8" y1="10" x2="16" y2="10" />
        <line x1="8" y1="14" x2="16" y2="14" />
        <line x1="8" y1="18" x2="16" y2="18" />
      </svg>
      Calculator
    </div>

    <div class="input-row">
      <label>Capital ($)</label>
      <input
        class="input"
        type="number"
        :value="inputs.capital"
        @input="update('capital', $event)"
      />
    </div>

    <div class="input-row">
      <label>Risk (%)</label>
      <input
        class="input"
        type="number"
        step="0.1"
        :value="inputs.riskPercent"
        @input="update('riskPercent', $event)"
      />
    </div>

    <div class="input-row">
      <label>Leverage</label>
      <input
        class="input"
        type="number"
        :value="inputs.leverage"
        @input="update('leverage', $event)"
      />
    </div>

    <div class="input-row">
      <label>Maker Fee (%)</label>
      <input
        class="input"
        type="number"
        step="0.01"
        :value="inputs.makerFee"
        @input="update('makerFee', $event)"
      />
    </div>

    <div class="input-row">
      <label>Taker Fee (%)</label>
      <input
        class="input"
        type="number"
        step="0.01"
        :value="inputs.takerFee"
        @input="update('takerFee', $event)"
      />
    </div>

    <div class="input-row">
      <label>Entry Fee</label>
      <div class="toggle-group">
        <button
          class="toggle-btn"
          :class="{ active: inputs.entryFeeType === 'maker' }"
          @click="update('entryFeeType', 'maker')"
        >
          Maker
        </button>
        <button
          class="toggle-btn"
          :class="{ active: inputs.entryFeeType === 'taker' }"
          @click="update('entryFeeType', 'taker')"
        >
          Taker
        </button>
      </div>
    </div>

    <div class="input-row">
      <label>Exit Fee</label>
      <div class="toggle-group">
        <button
          class="toggle-btn"
          :class="{ active: inputs.exitFeeType === 'maker' }"
          @click="update('exitFeeType', 'maker')"
        >
          Maker
        </button>
        <button
          class="toggle-btn"
          :class="{ active: inputs.exitFeeType === 'taker' }"
          @click="update('exitFeeType', 'taker')"
        >
          Taker
        </button>
      </div>
    </div>

    <button class="btn btn-primary calculate-btn" @click="$emit('calculate')">
      Calculate
    </button>
  </div>
</template>

<script setup lang="ts">
import type { CalculatorInputs } from "#hud/composables/useCalculator";

const props = defineProps<{
  inputs: CalculatorInputs;
}>();

const emit = defineEmits<{
  "update:inputs": [inputs: CalculatorInputs];
  calculate: [];
}>();

function update(key: keyof CalculatorInputs, eventOrValue: Event | string) {
  let value: any;
  if (typeof eventOrValue === "string") {
    value = eventOrValue;
  } else {
    const target = eventOrValue.target as HTMLInputElement;
    value =
      key === "entryFeeType" || key === "exitFeeType"
        ? target.value
        : parseFloat(target.value) || 0;
  }
  emit("update:inputs", { ...props.inputs, [key]: value });
}
</script>

<style scoped>
.calculator-card {
  margin: 8px 0;
}

.card-title {
  font-size: 16px;
  font-weight: 700;
  text-align: center;
  margin-bottom: 10px;
}

.input-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin: 5px 0;
}

.input-row label {
  font-size: 13px;
  color: var(--text-secondary);
}

.input-row .input {
  width: 100px;
  /* Hide native number spinners */
  -moz-appearance: textfield;
  appearance: textfield;
}

/* Hide spinners for Chrome, Safari, Edge, Opera */
.input-row .input::-webkit-outer-spin-button,
.input-row .input::-webkit-inner-spin-button {
  -webkit-appearance: none;
  margin: 0;
}

.toggle-group {
  display: flex;
  gap: 4px;
}

.toggle-btn {
  padding: 4px 12px;
  font-size: 12px;
  background: var(--input-bg);
  border: 1px solid var(--border-color);
  border-radius: 4px;
  color: var(--text-secondary);
  cursor: pointer;
  transition: all 0.15s ease;
}

.toggle-btn:hover {
  border-color: var(--accent-blue);
}

.toggle-btn.active {
  background: var(--accent-blue);
  border-color: var(--accent-blue);
  color: white;
}

.calculate-btn {
  width: 100%;
  margin-top: 10px;
}
</style>
