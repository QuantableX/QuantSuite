<script setup lang="ts">
/**
 * The exchange dropdown — every place the module asks for an exchange
 * (PLAN-QUANTALGO §5). Its options are the exchanges connected on the
 * Exchange page, nothing else: no provider is ever typed.
 */
import { useExchangeStore } from '#algo/stores/exchange'
import { capitalize } from '#algo/utils/format'

const props = withDefaults(
  defineProps<{
    modelValue: string
    /** Extra option at the top that means "no filter" (journal filters). */
    anyLabel?: string | null
    id?: string
    small?: boolean
  }>(),
  { anyLabel: null, id: undefined, small: false },
)

const emit = defineEmits<{ 'update:modelValue': [value: string] }>()

const exchangeStore = useExchangeStore()
const exchanges = computed(() => exchangeStore.exchanges)
const hasExchanges = computed(() => exchanges.value.length > 0)

function onChange(e: Event) {
  emit('update:modelValue', (e.target as HTMLSelectElement).value)
}
</script>

<template>
  <div class="exchange-select">
    <select
      :id="id"
      class="input"
      :class="{ 'input-sm': small }"
      :value="modelValue"
      :disabled="!hasExchanges"
      @change="onChange"
    >
      <option v-if="anyLabel !== null" value="">{{ anyLabel }}</option>
      <option v-else-if="!hasExchanges" value="">No connected exchanges</option>
      <option v-else-if="!modelValue" value="" disabled>Select exchange</option>
      <option v-for="ex in exchanges" :key="ex.id" :value="ex.id">
        {{ ex.name }} ({{ capitalize(ex.provider) }}{{ ex.sandbox ? ' · sandbox' : '' }})
      </option>
    </select>
    <p v-if="!hasExchanges && anyLabel === null" class="exchange-select__hint">
      Connect an exchange on the Exchange page first.
    </p>
  </div>
</template>

<style scoped>
.exchange-select {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.exchange-select__hint {
  font-size: 11px;
  color: var(--qa-warning);
}

.input-sm {
  padding: 5px 10px;
  font-size: 11px;
}
</style>
