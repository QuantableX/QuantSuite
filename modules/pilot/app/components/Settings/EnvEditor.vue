<script setup lang="ts">
/**
 * Environment variables as rows (card 19). Values are hidden until Show —
 * they are often keys. Names are checked like a shell's.
 */
import { computed, ref } from 'vue'
import { X } from 'lucide-vue-next'

const props = defineProps<{ modelValue: Record<string, string> }>()
const emit = defineEmits<{ 'update:modelValue': [value: Record<string, string>] }>()

const rows = ref<{ name: string; value: string }[]>(Object.entries(props.modelValue).map(([name, value]) => ({ name, value })))
const show = ref(false)
const VALID = /^[A-Za-z_][A-Za-z0-9_]*$/
const bad = computed(() => rows.value.filter((r) => r.name.trim() && !VALID.test(r.name.trim())).map((r) => r.name))

function sync() {
  const out: Record<string, string> = {}
  for (const r of rows.value) if (VALID.test(r.name.trim())) out[r.name.trim()] = r.value
  emit('update:modelValue', out)
}
function add() {
  rows.value.push({ name: '', value: '' })
}
function remove(i: number) {
  rows.value.splice(i, 1)
  sync()
}
</script>

<template>
  <div class="qp-env">
    <div v-for="(r, i) in rows" :key="i" class="qp-env-row">
      <input v-model="r.name" class="qp-input qp-env-name" placeholder="NAME" spellcheck="false" autocomplete="off" @input="sync" />
      <input v-model="r.value" class="qp-input qp-env-value" :type="show ? 'text' : 'password'" placeholder="value" spellcheck="false" autocomplete="off" @input="sync" />
      <button class="qp-env-x" :aria-label="`Remove ${r.name || 'variable'}`" @click="remove(i)"><X :size="12" /></button>
    </div>
    <div class="qp-env-foot">
      <button class="qp-env-btn" @click="add">Add variable</button>
      <button v-if="rows.length" class="qp-env-btn" :aria-pressed="show" @click="show = !show">{{ show ? 'Hide values' : 'Show values' }}</button>
      <span v-if="bad.length" class="qp-env-bad">Not a variable name: {{ bad.join(', ') }}</span>
    </div>
  </div>
</template>

<style scoped>
.qp-env { display: flex; flex-direction: column; gap: 5px; }
.qp-env-row { display: grid; grid-template-columns: minmax(120px, 0.8fr) minmax(0, 1.4fr) auto; gap: 6px; align-items: center; }
.qp-input {
  background: var(--qss-bg, #18181e);
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12px;
  padding: 5px 8px;
  outline: none;
  min-width: 0;
}
.qp-input:focus { border-color: var(--qss-accent, #a0a0a8); }
.qp-input::placeholder { color: var(--qss-text-muted, #6e6e7a); }
.qp-env-name { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; }
.qp-env-x { display: inline-flex; padding: 4px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-env-x:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-env-foot { display: flex; align-items: center; gap: 8px; }
.qp-env-btn { padding: 2px 8px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: 6px; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; font-size: 11.5px; cursor: pointer; }
.qp-env-btn:hover { background: var(--qss-bg-card, #292930); color: var(--qss-text, #d4d4d8); }
.qp-env-bad { font-size: 11.5px; color: var(--qss-warning, #d29a3f); }
</style>
