<script setup lang="ts">
/** A fund's settings, for a new fund and an existing one. */
import { useSystemsStore } from '#systems/stores/systems'
import { useFundsStore, emptyFund } from '#systems/stores/funds'
import { FUND_LOOKBACKS, FUND_METHODS, FUND_REBALANCES, KELLY_FRACTIONS, money, usesPerformance } from '#systems/utils/funds'
import type { Fund } from '#systems/types'

const props = defineProps<{ fund?: Fund }>()
const emit = defineEmits<{ saved: [fund: Fund]; cancel: [] }>()
const systems = useSystemsStore()
const funds = useFundsStore()

function fresh(): Fund {
  return props.fund ? JSON.parse(JSON.stringify(props.fund)) : emptyFund()
}
const form = ref<Fund>(fresh())
const filter = ref('')
const busy = ref(false)
const error = ref<string | null>(null)
watch(() => props.fund, () => { form.value = fresh(); error.value = null })

// A system deleted since is shown nowhere here: saving drops it.
const known = computed(() => form.value.members.filter(id => systems.byId(id)))
const removed = computed(() => form.value.members.length - known.value.length)
const listed = computed(() => {
  const term = filter.value.trim().toLowerCase()
  const ready = systems.systems.filter(s => s.status === 'ready')
  return term ? ready.filter(s => `${s.name} ${s.short ?? ''}`.toLowerCase().includes(term)) : ready
})
const method = computed(() => FUND_METHODS.find(m => m.id === form.value.method))
const fixedTotal = computed(() => known.value.reduce((sum, id) => sum + (Number(form.value.fixedPct[id]) || 0), 0))
const equalShare = computed(() => known.value.length ? form.value.amount / known.value.length : 0)

function toggle(id: string, on: boolean) {
  form.value.members = on ? [...known.value, id] : known.value.filter(m => m !== id)
}

async function save() {
  if (busy.value) return
  busy.value = true
  error.value = null
  try {
    emit('saved', await funds.save({ ...form.value, members: known.value }))
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <form class="qs-fund-form" @submit.prevent="save">
    <section class="card qs-fund-form__panel">
      <label class="qs-fund-form__wide"><span class="label">Name</span><input v-model="form.name" class="input" required maxlength="80" placeholder="My fund" :disabled="busy" /></label>
      <label><span class="label">Amount ($)</span><input v-model.number="form.amount" class="input" type="number" min="0" step="any" required :disabled="busy" /></label>
      <label><span class="label">Rebalance</span>
        <select v-model="form.rebalance" class="select" :disabled="busy"><option v-for="r in FUND_REBALANCES" :key="r.id" :value="r.id">{{ r.label }}</option></select>
      </label>
      <label class="qs-fund-form__wide"><span class="label">Split method</span>
        <select v-model="form.method" class="select" :disabled="busy"><option v-for="m in FUND_METHODS" :key="m.id" :value="m.id">{{ m.label }}</option></select>
      </label>
      <label v-if="usesPerformance(form.method)"><span class="label">Lookback</span>
        <select v-model.number="form.lookbackDays" class="select" :disabled="busy"><option v-for="d in FUND_LOOKBACKS" :key="d" :value="d">{{ d }} days</option></select>
      </label>
      <label v-if="usesPerformance(form.method)"><span class="label">Cap per system (%)</span><input v-model.number="form.capPct" class="input" type="number" min="1" max="100" step="1" :disabled="busy" /></label>
      <label v-if="form.method === 'kelly'"><span class="label">Kelly fraction</span>
        <select v-model.number="form.kellyFraction" class="select" :disabled="busy"><option v-for="k in KELLY_FRACTIONS" :key="k.value" :value="k.value">{{ k.label }}</option></select>
      </label>
      <label><span class="label">Live start</span><input v-model="form.liveStartDate" class="input" type="date" :disabled="busy" /></label>
      <p class="qs-fund-form__hint">
        {{ method?.hint }}
        <template v-if="form.method === 'equal' && known.length">{{ money(equalShare) }} per system.</template>
        <template v-if="form.method === 'fixed'">{{ fixedTotal }}% assigned.</template>
        <template v-if="usesPerformance(form.method)">Until a full lookback exists the split is Equal. A system that lost money over the lookback gets nothing, except under Inverse volatility.</template>
      </p>
    </section>

    <section class="card qs-fund-form__systems">
      <div class="qs-fund-form__systems-head">
        <span class="label">Systems ({{ known.length }})</span>
        <input v-model="filter" class="input qs-fund-form__filter" placeholder="Filter" aria-label="Filter systems" />
      </div>
      <div class="qs-fund-form__list">
        <div v-for="s in listed" :key="s.id" class="qs-fund-form__system">
          <label class="qs-check">
            <input type="checkbox" :checked="known.includes(s.id)" :disabled="busy" @change="toggle(s.id, ($event.target as HTMLInputElement).checked)" />
            <span :title="s.name">{{ s.short?.trim() ? `${s.short} · ${s.name}` : s.name }}</span>
          </label>
          <input v-if="form.method === 'fixed' && known.includes(s.id)" v-model.number="form.fixedPct[s.id]" class="input qs-fund-form__share" type="number" min="0" max="100" step="any" placeholder="%" :aria-label="`Share of ${s.name} in percent`" :disabled="busy" />
        </div>
        <p v-if="!listed.length" class="qs-fund-form__hint">No system matches.</p>
      </div>
      <p v-if="removed" class="qs-fund-form__hint">{{ removed }} deleted {{ removed === 1 ? 'system' : 'systems' }} will be dropped from this fund on saving.</p>
    </section>

    <p v-if="error" class="qs-fund-form__error" role="alert">{{ error }}</p>
    <div class="qs-fund-form__actions">
      <slot name="actions" />
      <button type="button" class="btn" :disabled="busy" @click="emit('cancel')">Cancel</button>
      <button class="btn btn-primary" :disabled="busy">{{ busy ? 'Saving…' : 'Save fund' }}</button>
    </div>
  </form>
</template>

<style scoped>
.qs-fund-form { display: flex; flex-direction: column; flex: 1; gap: var(--qss-gap); min-height: 0; }
.qs-fund-form__panel { flex-shrink: 0; display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: var(--qss-gap); padding: var(--qss-panel-pad); }
.qs-fund-form__panel label { display: flex; flex-direction: column; min-width: 0; }
.qs-fund-form__wide { grid-column: span 2; }
.qs-fund-form__hint { grid-column: 1 / -1; margin: 0; color: var(--qs-text-muted); font-size: var(--qss-text-label); line-height: 1.5; }
.qs-fund-form__systems { flex: 1; display: flex; flex-direction: column; gap: var(--qss-gap-tight); min-height: 120px; padding: var(--qss-panel-pad); }
.qs-fund-form__systems-head { flex-shrink: 0; display: flex; align-items: center; justify-content: space-between; gap: var(--qss-gap); }
.qs-fund-form__filter { max-width: 220px; }
.qs-fund-form__list { flex: 1; min-height: 0; overflow-y: auto; display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); align-content: start; gap: 2px var(--qss-gap); }
.qs-fund-form__system { display: flex; align-items: center; justify-content: space-between; gap: 8px; min-width: 0; min-height: 30px; }
.qs-fund-form__system .qs-check { min-width: 0; }
.qs-fund-form__system .qs-check span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qs-fund-form__share { flex: 0 0 72px; width: 72px; }
.qs-fund-form__error { flex-shrink: 0; margin: 0; color: var(--qs-error); font-size: var(--qss-text-label); overflow-wrap: anywhere; }
.qs-fund-form__actions { flex-shrink: 0; display: flex; justify-content: flex-end; flex-wrap: wrap; gap: var(--qss-gap-tight); }
</style>
