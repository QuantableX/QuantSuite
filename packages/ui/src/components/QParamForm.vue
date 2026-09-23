<script setup lang="ts">
/**
 * An indicator's parameters as inputs, generated from its parameter schema
 * (smithery TrendIndicator.schema(), like TradingView's indicator inputs).
 * `modelValue` holds every parameter; `base` is the chosen version's
 * parameters — a changed field is marked, and "Reset to version" brings
 * them back. Shared by QuantSystems and QuantScript.
 */
import { computed, useId } from 'vue'
import type { SmitheryParamSchema } from '@quantsuite/core'
import { RotateCcw } from 'lucide-vue-next'
import { paramProblems, type ParamValues } from '../paramForm'

const props = defineProps<{
  schema: Record<string, SmitheryParamSchema>
  modelValue: ParamValues
  /** The version's own parameters; omitted = nothing to reset to. */
  base?: ParamValues
  disabled?: boolean
}>()

const emit = defineEmits<{ 'update:modelValue': [value: ParamValues] }>()
const uid = useId()

const names = computed(() => Object.keys(props.schema))
const problems = computed(() => paramProblems(props.schema, props.modelValue))
const changed = computed(() =>
  props.base ? names.value.filter(n => JSON.stringify(props.modelValue[n]) !== JSON.stringify(props.base![n])) : [],
)

function set(name: string, value: unknown) {
  emit('update:modelValue', { ...props.modelValue, [name]: value })
}

function setNumber(name: string, raw: string) {
  set(name, raw.trim() === '' ? null : Number(raw))
}

function setList(name: string, raw: string) {
  set(name, raw.split(',').map(s => s.trim()).filter(Boolean).map(Number))
}

function setChoice(name: string, index: number) {
  set(name, props.schema[name]!.choices?.[index])
}

function choiceIndex(name: string): number {
  return (props.schema[name]!.choices ?? []).findIndex(c => JSON.stringify(c) === JSON.stringify(props.modelValue[name]))
}

function reset() {
  if (props.base) emit('update:modelValue', { ...props.base })
}

function resetOne(name: string) {
  if (props.base) set(name, props.base[name])
}

function hint(entry: SmitheryParamSchema): string {
  const parts: string[] = []
  if (entry.help) parts.push(entry.help)
  if (entry.tested) parts.push(`tested ${entry.tested[0]}–${entry.tested[1]}`)
  if (entry.min != null || entry.max != null) parts.push(`allowed ${entry.min ?? '…'}–${entry.max ?? '…'}`)
  return parts.join(' · ')
}
</script>

<template>
  <div class="qpf">
    <p v-if="!names.length" class="qpf-empty">This indicator has no parameters.</p>
    <div v-for="name in names" :key="name" class="qpf-row" :class="{ 'is-changed': changed.includes(name), 'is-invalid': problems[name] }">
      <label class="qpf-label" :for="`${uid}-${name}`" :title="hint(schema[name]!)">
        <span>{{ schema[name]!.label || name }}</span>
        <span v-if="changed.includes(name)" class="qpf-dot" aria-label="changed" />
      </label>
      <div class="qpf-field">
        <input
          v-if="schema[name]!.type === 'bool'"
          :id="`${uid}-${name}`"
          type="checkbox"
          :checked="modelValue[name] === true"
          :disabled="disabled"
          @change="set(name, ($event.target as HTMLInputElement).checked)"
        />
        <select
          v-else-if="schema[name]!.type === 'choice'"
          :id="`${uid}-${name}`"
          class="qpf-input"
          :value="choiceIndex(name)"
          :disabled="disabled"
          @change="setChoice(name, Number(($event.target as HTMLSelectElement).value))"
        >
          <option v-for="(choice, i) in schema[name]!.choices ?? []" :key="i" :value="i">{{ choice }}</option>
        </select>
        <input
          v-else-if="schema[name]!.type === 'list'"
          :id="`${uid}-${name}`"
          class="qpf-input mono"
          type="text"
          spellcheck="false"
          :value="Array.isArray(modelValue[name]) ? (modelValue[name] as unknown[]).join(', ') : ''"
          :disabled="disabled"
          @change="setList(name, ($event.target as HTMLInputElement).value)"
        />
        <input
          v-else
          :id="`${uid}-${name}`"
          class="qpf-input mono"
          type="number"
          :min="schema[name]!.min"
          :max="schema[name]!.max"
          :step="schema[name]!.step ?? (schema[name]!.type === 'int' ? 1 : 'any')"
          :value="modelValue[name] as number"
          :disabled="disabled"
          @change="setNumber(name, ($event.target as HTMLInputElement).value)"
        />
        <button
          v-if="base && changed.includes(name)"
          type="button"
          class="qpf-reset-one"
          :title="`Back to the version's ${JSON.stringify(base[name])}`"
          :disabled="disabled"
          @click="resetOne(name)"
        ><RotateCcw :size="12" /></button>
      </div>
      <p v-if="problems[name]" class="qpf-problem">{{ problems[name] }}</p>
      <p v-else-if="hint(schema[name]!)" class="qpf-hint">{{ hint(schema[name]!) }}</p>
    </div>
    <div v-if="base && names.length" class="qpf-foot">
      <span class="qpf-count">{{ changed.length ? `${changed.length} changed from the version` : "The version's parameters" }}</span>
      <button type="button" class="qpf-reset" :disabled="disabled || !changed.length" @click="reset">Reset to version</button>
    </div>
  </div>
</template>

<style scoped>
.qpf {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.qpf-empty {
  margin: 0;
  font-size: 12px;
  color: var(--qss-text-muted);
}

.qpf-row {
  display: grid;
  grid-template-columns: minmax(96px, 38%) minmax(0, 1fr);
  align-items: center;
  column-gap: 10px;
  row-gap: 2px;
}

.qpf-label {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  font-size: 12px;
  color: var(--qss-text-secondary);
}

.qpf-label > span:first-child {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.qpf-dot {
  flex-shrink: 0;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--qss-accent);
}

.qpf-field {
  display: flex;
  align-items: center;
  gap: 4px;
  min-width: 0;
}

.qpf-input {
  box-sizing: border-box;
  width: 100%;
  min-width: 0;
  height: 28px;
  padding: 3px 8px;
  border: 1px solid var(--qss-border);
  border-radius: 6px;
  background: var(--qss-bg);
  color: var(--qss-text);
  font-size: 12px;
  outline: none;
}

.qpf-input.mono {
  font-family: var(--qss-font-mono);
  font-variant-numeric: tabular-nums;
}

.qpf-input:focus {
  border-color: var(--qss-accent);
}

.is-invalid .qpf-input {
  border-color: var(--qss-error);
}

.qpf-reset-one {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  width: 24px;
  height: 24px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text-muted);
  cursor: pointer;
}

.qpf-reset-one:hover {
  background: var(--qss-bg-hover);
  color: var(--qss-text);
}

.qpf-hint,
.qpf-problem {
  grid-column: 2;
  margin: 0;
  font-size: 11px;
  line-height: 1.35;
  color: var(--qss-text-muted);
}

.qpf-problem {
  color: var(--qss-error);
}

.qpf-foot {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding-top: 6px;
  border-top: 1px solid var(--qss-border-subtle);
}

.qpf-count {
  font-size: 11px;
  color: var(--qss-text-muted);
}

.qpf-reset {
  border: 1px solid var(--qss-border);
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-secondary);
  padding: 3px 10px;
  font-size: 12px;
  cursor: pointer;
}

.qpf-reset:hover:not(:disabled) {
  border-color: var(--qss-accent);
  color: var(--qss-text);
}

.qpf-reset:disabled {
  opacity: 0.5;
  cursor: default;
}
</style>
