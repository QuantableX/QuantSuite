<script setup lang="ts">
/**
 * New strategy from one indicator version: a name and the version's
 * parameters, adjustable (QParamForm, packages/ui). QuantAlgo checks the
 * values against the registry's schema and freezes them into the
 * RegimeTrend strategy's `indicator_params`; the strategy opens there.
 */
import { paramDiff, paramProblems, type ParamValues } from '@quantsuite/ui'
import { useForgeStore } from '#script/stores/forge'
import type { IndicatorInfo } from '#script/types'

const props = defineProps<{ indicator: IndicatorInfo }>()
const emit = defineEmits<{ close: [] }>()

const forge = useForgeStore()
const name = ref('')
const values = ref<ParamValues>({ ...props.indicator.params })
const dialog = ref<HTMLElement | null>(null)
const nameInput = ref<HTMLInputElement | null>(null)
const previousFocus = typeof document !== 'undefined' ? document.activeElement as HTMLElement | null : null

const schema = computed(() => props.indicator.schema ?? {})
const overrides = computed(() => paramDiff(values.value, props.indicator.params))
const custom = computed(() => Object.keys(overrides.value).length > 0)
const problems = computed(() => Object.entries(paramProblems(schema.value, values.value)))
const busy = computed(() => forge.creating !== null)
const defaultName = computed(() => `Regime ${props.indicator.name}${custom.value ? ' (custom)' : ''}`)

async function create() {
  if (busy.value || problems.value.length) return
  const strategy = await forge.createStrategy(props.indicator.key, {
    name: name.value.trim() || null,
    params: custom.value ? overrides.value : null,
  })
  if (strategy) emit('close')
}

function close() {
  if (!busy.value) emit('close')
}

onMounted(() => {
  forge.createError = null
  nameInput.value?.focus()
})
onUnmounted(() => previousFocus?.focus())
function trapFocus(e: KeyboardEvent) {
  if (e.key !== 'Tab') return
  const elements = Array.from(dialog.value?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled)') ?? []).filter((el) => el.getClientRects().length)
  const first = elements[0], last = elements.at(-1)
  if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last?.focus() }
  else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first?.focus() }
}
</script>

<template>
  <div class="qsc-modal-overlay" @mousedown.self="close">
    <div ref="dialog" class="qsc-modal qsf-strategy-modal" role="dialog" aria-modal="true" aria-label="New strategy" @keydown.esc.stop="close" @keydown="trapFocus">
      <header class="qsc-modal-head">
        <h2 class="qsc-modal-title">New strategy</h2>
        <button class="qsc-icon-btn" aria-label="Close dialog" title="Close" :disabled="busy" @click="close">
          <svg width="12" height="12" viewBox="0 0 14 14" fill="none" aria-hidden="true">
            <path d="M3 3l8 8M11 3l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
          </svg>
        </button>
      </header>

      <p class="qsf-strategy-intro">
        A RegimeTrend strategy in QuantAlgo on <strong>{{ indicator.name }}</strong> (<span class="mono qsf-strategy-key">{{ indicator.key }}</span>): long while the regime is up.
        Pick the version that matches the bot's timeframe.
      </p>

      <label class="qsc-field">
        <span class="qsc-field-label">Strategy name</span>
        <input ref="nameInput" v-model="name" :disabled="busy" class="qsc-input" :placeholder="defaultName" @keydown.enter="create" />
      </label>

      <div class="qsc-field">
        <span class="qsc-field-label">Parameters</span>
        <div class="qsf-strategy-params">
          <QParamForm v-model="values" :schema="schema" :base="indicator.params" :disabled="busy" />
        </div>
        <span class="qsc-field-hint">Frozen into the strategy. Changed values leave the version's evidence behind; validate them in the forge.</span>
      </div>

      <p v-if="forge.createError" class="qsc-note is-error" role="alert">{{ forge.createError }}</p>

      <footer class="qsc-modal-foot">
        <button class="qsc-btn" :disabled="busy" @click="close">Cancel</button>
        <button class="qsc-btn is-primary" :disabled="busy || problems.length > 0" @click="create">{{ busy ? 'Creating…' : 'Create strategy' }}</button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.qsf-strategy-modal {
  width: 520px;
  max-width: 100%;
}
.qsf-strategy-intro {
  margin: 0;
  font-size: 12.5px;
  line-height: 1.5;
  color: var(--qss-text-secondary);
}
.qsf-strategy-key {
  font-size: 11.5px;
  color: var(--qss-text-muted);
}
.qsf-strategy-params {
  max-height: 360px;
  overflow-y: auto;
  padding: 8px 10px;
  border: 1px solid var(--qss-border-subtle);
  border-radius: 8px;
}
</style>
