<script setup lang="ts">
/** The right panel of a fund's pages: what the fund is, and each member's robustness verdict. */
import { invoke } from '@tauri-apps/api/core'
import { useFundsStore } from '#systems/stores/funds'
import { useSystemsStore } from '#systems/stores/systems'
import { useActiveView } from '#systems/composables/useActiveView'
import { FUND_REBALANCES, methodLabel, money, usesPerformance } from '#systems/utils/funds'

interface Verdict { status: string; grade: string; score: number | null }
interface Run { kind: string; status: string; system?: { id?: string }; verdict?: Verdict }

const funds = useFundsStore()
const systems = useSystemsStore()
const { fundId } = useActiveView()
const fund = computed(() => funds.byId(fundId.value))
const runs = ref<Run[]>([])
const verdictError = ref('')

function verdictOf(systemId: string) {
  const verdict = runs.value.find(r => r.kind === 'robustness' && r.status === 'done' && r.verdict && r.system?.id === systemId)?.verdict
  if (!verdict) return { label: 'Not checked', tone: 'qs-fund-side__muted' }
  const grade = `${verdict.grade}${verdict.score != null ? ` · ${Math.round(verdict.score)}` : ''}`
  if (verdict.status === 'robust') return { label: `Robust · ${grade}`, tone: 'qs-fund-side__ok' }
  return { label: `${verdict.status === 'incomplete' ? 'Incomplete' : 'Not robust'} · ${grade}`, tone: 'qs-fund-side__bad' }
}
const members = computed(() => (fund.value?.members ?? []).map(id => {
  const system = systems.byId(id)
  return { id, name: system?.short?.trim() || system?.name || 'Deleted system', removed: !system, verdict: system ? verdictOf(id) : null }
}))

async function refresh() {
  try {
    runs.value = await invoke<Run[]>('plugin:script|research_list')
    verdictError.value = ''
  } catch (e) {
    verdictError.value = String(e)
  }
}
onMounted(refresh)
onActivated(refresh)
</script>

<template>
  <div v-if="fund" class="qs-fund-side">
    <section class="qs-fund-side__section">
      <span class="label">Fund</span>
      <span class="qs-fund-side__name" :title="fund.name">{{ fund.name }}</span>
      <div class="qs-fund-side__row"><span>Amount</span><span class="mono">{{ money(fund.amount) }}</span></div>
      <div class="qs-fund-side__row"><span>Method</span><span>{{ methodLabel(fund.method) }}</span></div>
      <div v-if="usesPerformance(fund.method)" class="qs-fund-side__row"><span>Lookback</span><span class="mono">{{ fund.lookbackDays }} days</span></div>
      <div v-if="usesPerformance(fund.method)" class="qs-fund-side__row"><span>Cap per system</span><span class="mono">{{ fund.capPct }}%</span></div>
      <div class="qs-fund-side__row"><span>Rebalance</span><span>{{ FUND_REBALANCES.find(r => r.id === fund?.rebalance)?.label }}</span></div>
    </section>
    <section class="qs-fund-side__section">
      <span class="label">Systems</span>
      <div v-for="m in members" :key="m.id" class="qs-fund-side__member">
        <span class="qs-fund-side__member-name" :class="{ 'qs-fund-side__muted': m.removed }" :title="m.name">{{ m.name }}</span>
        <span v-if="m.verdict" class="qs-fund-side__verdict" :class="m.verdict.tone">{{ m.verdict.label }}</span>
      </div>
      <p v-if="verdictError" class="qs-fund-side__note" role="alert">Verdicts could not be read: {{ verdictError }}</p>
    </section>
    <p class="qs-fund-side__note">A fund plans and evaluates. It places no orders.</p>
  </div>
</template>

<style scoped>
.qs-fund-side { display: flex; flex-direction: column; gap: 18px; padding: 18px 16px; }
.qs-fund-side__section { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
.qs-fund-side__name { overflow-wrap: anywhere; font-size: 13px; color: var(--qs-text); }
.qs-fund-side__row { display: flex; align-items: center; justify-content: space-between; gap: 12px; font-size: 12px; color: var(--qs-text-secondary); }
.qs-fund-side__member { display: flex; flex-direction: column; gap: 1px; min-width: 0; font-size: 12px; }
.qs-fund-side__member-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qs-text-secondary); }
.qs-fund-side__verdict { font-size: 11px; }
.qs-fund-side__ok { color: var(--qs-success, var(--qs-accent)); }
.qs-fund-side__bad { color: var(--qs-error); }
.qs-fund-side__muted { color: var(--qs-text-muted); }
.qs-fund-side__note { margin: 0; font-size: 12px; line-height: 1.5; color: var(--qs-text-muted); overflow-wrap: anywhere; }
</style>
