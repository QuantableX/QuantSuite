<script setup lang="ts">
/**
 * A 24-hour time field (round 3, card 03): the native time input follows the
 * system's 12-hour clock here; this one is always HH:MM. Typing "830" or
 * "8:30" becomes 08:30 when it is left; anything else is shown as wrong and
 * the last good time stays.
 */
import { ref, watch } from 'vue'
import { normalizeTime } from '#pilot/utils/schedules'

const props = defineProps<{ modelValue: string; label?: string }>()
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

const text = ref(props.modelValue)
const bad = ref(false)
watch(
  () => props.modelValue,
  (v) => {
    if (normalizeTime(text.value) !== v) text.value = v
  },
)
function commit() {
  const t = normalizeTime(text.value)
  bad.value = !t
  if (!t) return
  text.value = t
  if (t !== props.modelValue) emit('update:modelValue', t)
}
function onInput() {
  const t = normalizeTime(text.value)
  bad.value = false
  // A complete time takes effect at once (the summary follows).
  if (t && /^\d{1,2}:\d{2}$/.test(text.value.trim())) emit('update:modelValue', t)
}
</script>

<template>
  <input
    v-model="text"
    class="qp-input qp-timefield"
    :class="{ bad }"
    inputmode="numeric"
    placeholder="HH:MM"
    maxlength="5"
    spellcheck="false"
    :aria-label="label ?? 'Time'"
    :aria-invalid="bad"
    :title="bad ? 'A 24-hour time, e.g. 08:30 or 18:45' : undefined"
    @input="onInput"
    @blur="commit"
    @keydown.enter.prevent="commit"
  />
</template>

<style scoped>
.qp-input {
  background: var(--qss-bg, #18181e);
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12.5px;
  padding: 6px 9px;
  outline: none;
}
.qp-input:focus { border-color: var(--qss-accent, #a0a0a8); }
.qp-input::placeholder { color: var(--qss-text-muted, #6e6e7a); }
.qp-timefield { width: 70px; font-variant-numeric: tabular-nums; text-align: center; }
.qp-timefield.bad { border-color: var(--qss-error, #f87171); }
</style>
