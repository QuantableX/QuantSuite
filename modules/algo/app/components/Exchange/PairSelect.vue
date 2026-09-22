<script setup lang="ts">
/**
 * The pair dropdown — every place the module asks for a trading pair
 * (PLAN-QUANTALGO §5). Its options are what the chosen connected exchange
 * lists right now, fetched from that exchange's public market metadata.
 * The filter box narrows a long list (Binance lists thousands of pairs);
 * the value is always one of the options.
 */
import { useExchangeStore } from '#algo/stores/exchange'

const props = withDefaults(
  defineProps<{
    modelValue: string
    exchangeId: string | null | undefined
    /** Picked when the list loads and the current value is not in it. */
    preferred?: string | null
    id?: string
    small?: boolean
  }>(),
  { preferred: null, id: undefined, small: false },
)

const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

const exchangeStore = useExchangeStore()

const pairs = ref<string[]>([])
const isLoading = ref(false)
const error = ref<string | null>(null)
const filter = ref('')
let loadGen = 0

const filtered = computed(() => {
  const needle = filter.value.trim().toUpperCase()
  const list = needle ? pairs.value.filter((p) => p.includes(needle)) : pairs.value
  // The selected pair stays visible even when the filter would hide it.
  if (props.modelValue && !list.includes(props.modelValue) && pairs.value.includes(props.modelValue)) {
    return [props.modelValue, ...list]
  }
  return list
})

async function load(exchangeId: string | null | undefined, force = false) {
  const gen = ++loadGen
  if (!exchangeId) {
    pairs.value = []
    error.value = null
    if (props.modelValue) emit('update:modelValue', '')
    return
  }
  isLoading.value = true
  error.value = null
  try {
    const loaded = await exchangeStore.loadPairs(exchangeId, force)
    if (gen !== loadGen) return
    pairs.value = loaded
    if (!loaded.includes(props.modelValue)) {
      const next = props.preferred && loaded.includes(props.preferred)
        ? props.preferred
        : (loaded[0] ?? '')
      emit('update:modelValue', next)
    }
  } catch (err) {
    if (gen !== loadGen) return
    pairs.value = []
    error.value = String(err)
    if (props.modelValue) emit('update:modelValue', '')
  } finally {
    if (gen === loadGen) isLoading.value = false
  }
}

watch(() => props.exchangeId, (id) => { void load(id) }, { immediate: true })

function onChange(e: Event) {
  emit('update:modelValue', (e.target as HTMLSelectElement).value)
}
</script>

<template>
  <div class="pair-select">
    <input
      v-if="pairs.length > 12"
      v-model="filter"
      class="input pair-select__filter"
      :class="{ 'input-sm': small }"
      type="text"
      placeholder="Filter pairs…"
      spellcheck="false"
    />
    <select
      :id="id"
      class="input"
      :class="{ 'input-sm': small }"
      :value="modelValue"
      :disabled="!exchangeId || isLoading || !!error || pairs.length === 0"
      @change="onChange"
    >
      <option v-if="!exchangeId" value="">Select an exchange first</option>
      <option v-else-if="isLoading" value="">Loading pairs…</option>
      <option v-else-if="error" value="">Pairs unavailable</option>
      <option v-else-if="pairs.length === 0" value="">No pairs listed</option>
      <option v-for="p in filtered" :key="p" :value="p">{{ p }}</option>
    </select>
    <p v-if="error" class="pair-select__error">{{ error }}</p>
    <p v-else-if="pairs.length" class="pair-select__meta">
      {{ pairs.length.toLocaleString() }} pairs listed
      <button type="button" class="pair-select__refresh" @click="load(exchangeId, true)">refresh</button>
    </p>
  </div>
</template>

<style scoped>
.pair-select {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.pair-select__filter {
  font-family: ui-monospace, 'Cascadia Code', 'JetBrains Mono', Menlo, Consolas, monospace;
}

.pair-select__error {
  font-size: 11px;
  color: var(--qa-error);
  line-height: 1.4;
}

.pair-select__meta {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
  color: var(--qa-text-muted);
}

.pair-select__refresh {
  background: none;
  border: none;
  padding: 0;
  font-size: 11px;
  color: var(--qa-text-secondary);
  cursor: pointer;
  text-decoration: underline;
}

.pair-select__refresh:hover {
  color: var(--qa-text);
}

.input-sm {
  padding: 5px 10px;
  font-size: 11px;
}
</style>
