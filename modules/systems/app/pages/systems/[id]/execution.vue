<script setup lang="ts">
/**
 * Execution (PLAN-AUTO-EXECUTION §2.3): the system trades by itself after the
 * daily close — Off, Paper or Live on a connected account. The settings live
 * in QuantAlgo's database, not in the system's config, so the saved system
 * stays as it is. Runs list every night's decision, orders and cost.
 */
definePageMeta({ layout: 'systems', path: '/algo/manual/:id/execution' })

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useSystemsStore } from '#systems/stores/systems'
import { useServerLink } from '@quantsuite/core'
import SystemsSettingsCostPreview from '#systems/components/Settings/CostPreview.vue'

type Mode = 'off' | 'paper' | 'live'
interface Settings {
  mode: Mode
  exchange_id: string | null
  market: string
  /** Perp: isolated leverage, 1x default, capped in QuantAlgo Settings (Venues 4/5). */
  leverage: number
  capital: { kind: 'fixed' | 'percent' | 'whole'; value: number }
  run_time: string
  retry_hours: number
  coverage_policy: string
  max_order_value: number | null
  daily_turnover_cap: number | null
  paper_fee_pct: number
  paper_slippage_pct: number
}
interface Verdict { run_id: string; status: string; robust: boolean; finished_at: string }
interface View { system_id: string; settings: Settings; armed: boolean; armed_at: string | null; ack_not_robust: boolean; next_run: string | null; paper_runs_done: number; verdict: Verdict | null }
interface Exchange { id: string; name: string; provider: string; sandbox: boolean; key_storage?: string }
interface Check { name: string; ok: boolean; detail: string }
interface OrderReport { status: string; filled_qty: number; avg_price: number; quote_amount: number; fee_asset: string; fee_amount: number }
interface Order { client_order_id: string; leg: string; side: string; qty: number; state: string; report: OrderReport | null; error: string | null; instrument: { symbol: string } }
interface Run {
  id: string; status: string; mode: string; close_date: string | null; target: { kind: string; asset?: string }
  fees: number; cost: number; errors: string[]; notes: string[]; dust: string[]; started_at: string
  route: { chosen: string; reason: string } | null
}
interface RunView { run: Run; orders: Order[]; summary: string }
interface Coverage { venue: string; as_of: string; coins: string[]; tradable: string[]; missing: string[] }

const route = useRoute()
const systems = useSystemsStore()
const systemId = computed(() => route.params.id as string)
const system = computed(() => systems.byId(systemId.value))

const view = ref<View | null>(null)
const form = ref<Settings | null>(null)
const exchanges = ref<Exchange[]>([])
const runs = ref<RunView[]>([])
const openRun = ref<string | null>(null)
const error = ref<string | null>(null)
const saving = ref(false)
const savedAt = ref<string | null>(null)
const checks = ref<Check[]>([])
const arming = ref(false)
const ackNotRobust = ref(false)
const coverage = ref<Coverage | null>(null)
const coverageBusy = ref(false)

const tradableVenues = computed(() => exchanges.value.filter(e => ['binance', 'bybit', 'hyperliquid', 'jupiter', 'lighter'].includes(e.provider.toLowerCase())))
const account = computed(() => exchanges.value.find(e => e.id === form.value?.exchange_id) ?? null)
/** Perp exists on Hyperliquid and Lighter accounts. */
const hasPerp = computed(() => ['hyperliquid', 'lighter'].includes(account.value?.provider.toLowerCase() ?? ''))
/** What "USD" is on the account: USDC on Hyperliquid and Jupiter, USDT on Binance and Bybit. */
const quote = computed(() => (['hyperliquid', 'jupiter', 'lighter'].includes(account.value?.provider.toLowerCase() ?? '') ? 'USDC' : 'USDT'))
watch(hasPerp, (perp) => {
  if (!perp && form.value?.market === 'perp') form.value.market = 'spot'
})
watch(() => form.value?.market, (market) => {
  if (form.value && market !== 'perp') form.value.leverage = 1
})

/** The account's perp positions with their liquidation distance (Venues 4/5). */
interface PerpPosition {
  coin: string; side: string; size: number; entry_price: number; mark_price: number | null
  liquidation_price: number | null; liquidation_distance: number | null; leverage: number | null; warn: boolean
}
const positions = ref<PerpPosition[]>([])
async function loadPositions() {
  positions.value = []
  if (!view.value || view.value.settings.market !== 'perp' || !view.value.settings.exchange_id) return
  try {
    positions.value = await invoke<PerpPosition[]>('plugin:algo|venue_perp_positions', { exchange_id: view.value.settings.exchange_id })
  } catch {
    // Shown as no positions; the account's own page names the error.
  }
}
watch(() => view.value?.settings.exchange_id + ':' + view.value?.settings.market, loadPositions)
function price(n: number | null) {
  return n == null ? '—' : n.toLocaleString('en-US', { maximumFractionDigits: n < 10 ? 4 : 1 })
}
const dirty = computed(() => JSON.stringify(form.value) !== JSON.stringify(view.value?.settings))
const status = computed(() => {
  const v = view.value
  if (!v || v.settings.mode === 'off') return { label: 'Off', tone: 'muted' }
  if (v.settings.mode === 'paper') return { label: 'Paper', tone: 'ok' }
  return v.armed ? { label: 'Live · armed', tone: 'live' } : { label: 'Live · not armed', tone: 'warn' }
})

function money(n: number | null | undefined) {
  if (n == null || !Number.isFinite(n)) return '—'
  return `${n.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}$`
}
function utc(iso: string | null) {
  if (!iso) return '—'
  const d = new Date(iso)
  return `${d.toISOString().slice(0, 10)} ${d.toISOString().slice(11, 16)} UTC`
}
function decision(r: Run) {
  return r.target.kind === 'hold' ? r.target.asset : 'USD'
}

async function load() {
  error.value = null
  try {
    const [v, ex] = await Promise.all([
      invoke<View>('plugin:algo|manual_execution_get', { system_id: systemId.value }),
      invoke<Exchange[]>('plugin:algo|list_exchanges'),
    ])
    view.value = v
    form.value = JSON.parse(JSON.stringify(v.settings))
    exchanges.value = ex
    ackNotRobust.value = v.ack_not_robust
    checks.value = []
    coverage.value = null
    await loadRuns()
  } catch (e) {
    error.value = String(e)
  }
}

async function loadRuns() {
  try {
    runs.value = await invoke<RunView[]>('plugin:algo|manual_execution_runs', { system_id: systemId.value, limit: 60 })
  } catch (e) {
    error.value = String(e)
  }
}

async function save() {
  if (!form.value) return
  saving.value = true
  error.value = null
  try {
    view.value = await invoke<View>('plugin:algo|manual_execution_save', { system_id: systemId.value, settings: form.value })
    form.value = JSON.parse(JSON.stringify(view.value.settings))
    checks.value = []
    savedAt.value = new Date().toLocaleTimeString('en-US', { hourCycle: 'h23' })
    setTimeout(() => { savedAt.value = null }, 4000)
  } catch (e) {
    error.value = String(e)
  } finally {
    saving.value = false
  }
}

// On a PC that uses a server, arming live needs a Full device (card Server 5/7);
// the server then asks for its PIN.
const link = useServerLink()

async function arm() {
  arming.value = true
  error.value = null
  try {
    const res = await invoke<{ armed: boolean; checks: Check[] }>('plugin:algo|manual_execution_arm', { system_id: systemId.value, ack_not_robust: ackNotRobust.value })
    checks.value = res.checks
    view.value = await invoke<View>('plugin:algo|manual_execution_get', { system_id: systemId.value })
  } catch (e) {
    error.value = String(e)
  } finally {
    arming.value = false
  }
}

async function disarm() {
  try {
    view.value = await invoke<View>('plugin:algo|manual_execution_disarm', { system_id: systemId.value })
    checks.value = []
  } catch (e) {
    error.value = String(e)
  }
}

async function checkCoverage() {
  if (!form.value?.exchange_id) return
  coverageBusy.value = true
  coverage.value = null
  try {
    coverage.value = await invoke<Coverage>('plugin:algo|manual_execution_coverage', { system_id: systemId.value, exchange_id: form.value.exchange_id })
  } catch (e) {
    error.value = String(e)
  } finally {
    coverageBusy.value = false
  }
}

function numberOrNull(v: string) {
  const n = Number(v)
  return v.trim() === '' || !Number.isFinite(n) ? null : n
}

let unlisten: UnlistenFn | null = null
onMounted(async () => {
  await load()
  unlisten = await listen<{ systemId: string }>('algo:manual-run', (e) => {
    if (e.payload.systemId === systemId.value) void load()
  })
})
onUnmounted(() => { unlisten?.() })
watch(systemId, () => { void load() })
</script>

<template>
  <div class="qs-exec">
    <header class="qs-exec__head">
      <h1 class="qs-exec__title">{{ system?.name }} · Execution</h1>
      <span class="qs-exec__status" :data-tone="status.tone">{{ status.label }}</span>
      <span v-if="view?.next_run" class="qs-exec__next">Next run {{ utc(view.next_run) }}</span>
      <div class="qs-exec__actions">
        <Transition name="fade"><span v-if="savedAt" class="qs-exec__saved">Saved {{ savedAt }}</span></Transition>
        <button class="btn btn-primary" :disabled="!form || saving || !dirty" @click="save">{{ saving ? 'Saving…' : 'Save execution' }}</button>
      </div>
    </header>
    <div v-if="error" class="qs-exec__error" role="alert">{{ error }}</div>

    <div v-if="form" class="qs-exec__grid">
      <section class="card qs-exec__settings" aria-label="Execution settings">
        <label><span class="label">Mode</span>
          <select v-model="form.mode" class="select">
            <option value="off">Off — signals only, you trade by hand</option>
            <option value="paper">Paper — simulated fills at the real book</option>
            <option value="live">Live — real orders after arming</option>
          </select>
        </label>
        <label><span class="label">Account</span>
          <select v-model="form.exchange_id" class="select">
            <option :value="null">Choose a connected account</option>
            <option v-for="e in tradableVenues" :key="e.id" :value="e.id">{{ e.name }} ({{ e.provider }}{{ e.sandbox ? ', sandbox' : '' }})</option>
          </select>
        </label>
        <label><span class="label">Market</span>
          <select v-model="form.market" class="select">
            <option value="spot">Spot</option>
            <option value="perp" :disabled="!hasPerp">Perp — isolated, long and short{{ hasPerp ? '' : ' (Hyperliquid and Lighter accounts)' }}</option>
          </select>
        </label>
        <label v-if="form.market === 'perp'"><span class="label">Leverage (x, isolated — capped in QuantAlgo Settings)</span>
          <input v-model.number="form.leverage" class="input" type="number" min="1" step="1">
        </label>
        <div class="qs-exec__pair">
          <label><span class="label">Capital</span>
            <select v-model="form.capital.kind" class="select">
              <option value="fixed">Fixed amount ({{ quote }})</option>
              <option value="percent">% of the {{ quote }} balance</option>
              <option value="whole">The whole (sub-)account</option>
            </select>
          </label>
          <label v-if="form.capital.kind !== 'whole'"><span class="label">{{ form.capital.kind === 'fixed' ? 'Amount' : 'Percent' }}</span>
            <input v-model.number="form.capital.value" class="input" type="number" min="0" step="any">
          </label>
        </div>
        <div class="qs-exec__pair">
          <label><span class="label">Run time (UTC)</span><input v-model="form.run_time" class="input" placeholder="00:15"></label>
          <label><span class="label">Retry window (hours after the close)</span><input v-model.number="form.retry_hours" class="input" type="number" min="1" max="23"></label>
        </div>
        <div class="qs-exec__pair">
          <label><span class="label">Max order value ({{ quote }}, optional)</span>
            <input class="input" type="number" min="0" step="any" :value="form.max_order_value ?? ''" @input="form.max_order_value = numberOrNull(($event.target as HTMLInputElement).value)">
          </label>
          <label><span class="label">Daily turnover cap ({{ quote }}, optional)</span>
            <input class="input" type="number" min="0" step="any" :value="form.daily_turnover_cap ?? ''" @input="form.daily_turnover_cap = numberOrNull(($event.target as HTMLInputElement).value)">
          </label>
        </div>
        <div v-if="form.mode === 'paper'" class="qs-exec__pair">
          <label><span class="label">Paper fee (%)</span><input v-model.number="form.paper_fee_pct" class="input" type="number" min="0" max="1" step="0.01"></label>
          <label><span class="label">Paper slippage (%)</span><input v-model.number="form.paper_slippage_pct" class="input" type="number" min="0" max="1" step="0.01"></label>
        </div>
        <p class="qs-exec__hint">
          Runs after the daily close at 00:00 UTC; the run waits for every coin's closed candle and the day's ranking until the window ends, then skips. It never trades on old data.
          One sub-account per system is recommended: a system only trades the capital and coins it owns.
        </p>
        <div class="qs-exec__coverage">
          <button class="btn" :disabled="!form.exchange_id || coverageBusy" @click="checkCoverage">{{ coverageBusy ? 'Checking…' : 'Check coverage' }}</button>
          <span v-if="coverage" :class="coverage.missing.length ? 'qs-exec__warn' : 'qs-exec__ok'">
            {{ coverage.tradable.length }} / {{ coverage.coins.length }} coins tradable on {{ coverage.venue }}
            <template v-if="coverage.missing.length"> — missing {{ coverage.missing.join(', ') }}: a pick of these is held as USD (backtest ≠ live)</template>
          </span>
        </div>
      </section>

      <section class="card qs-exec__arm" aria-label="Live arming">
        <h2 class="qs-exec__h2">Live</h2>
        <p class="qs-exec__line">Robustness:
          <strong v-if="view?.verdict" :class="view.verdict.robust ? 'qs-exec__ok' : 'qs-exec__warn'">{{ view.verdict.status }}</strong>
          <strong v-else class="qs-exec__warn">no verdict</strong>
        </p>
        <p class="qs-exec__line">Completed paper runs: <strong>{{ view?.paper_runs_done ?? 0 }}</strong></p>
        <label class="qs-exec__tick"><input v-model="ackNotRobust" type="checkbox"> I know this system is not robust</label>
        <div class="qs-exec__buttons">
          <button
            class="btn btn-primary"
            :disabled="arming || dirty || view?.settings.mode !== 'live' || view?.armed || !link.canTrade.value"
            :title="link.canTrade.value ? '' : 'This PC may not trade on the server (its role there).'"
            @click="arm"
          >{{ arming ? 'Checking…' : 'Arm Live' }}</button>
          <button class="btn" :disabled="!view?.armed" @click="disarm">Disarm</button>
        </div>
        <p v-if="view?.armed" class="qs-exec__line qs-exec__live">Armed {{ utc(view.armed_at) }}: the next run trades real money.</p>
        <ul v-if="checks.length" class="qs-exec__checks">
          <li v-for="c in checks" :key="c.name" :class="c.ok ? 'qs-exec__ok' : 'qs-exec__warn'">{{ c.ok ? '✓' : '✗' }} {{ c.detail }}</li>
        </ul>
        <p class="qs-exec__hint">Arming needs Live saved, a completed paper run, a working account and a robust verdict (or the tick). Stopping never sells.</p>
        <SystemsSettingsCostPreview :exchange-id="form.exchange_id" :market="form.market === 'perp' ? 'perp' : 'spot'" :value="form.capital.kind === 'fixed' ? form.capital.value : 1000" />
        <div v-if="view?.settings.market === 'perp'" class="qs-exec__positions">
          <h2 class="qs-exec__h2">Perp positions on the account</h2>
          <p v-if="!positions.length" class="qs-exec__hint">None open.</p>
          <ul v-else class="qs-exec__checks">
            <li v-for="p in positions" :key="p.coin + p.side" :class="p.warn ? 'qs-exec__warn' : ''">
              {{ p.coin }} {{ p.side }} {{ p.size }} at {{ price(p.entry_price) }}{{ p.leverage ? `, ${p.leverage}x` : '' }} —
              liquidation {{ price(p.liquidation_price) }}<template v-if="p.liquidation_distance != null"> ({{ (p.liquidation_distance * 100).toFixed(1) }} % from the mark)</template>
            </li>
          </ul>
          <p class="qs-exec__hint">Funding is booked every hour into the journal and the system's cash.</p>
        </div>
      </section>

      <section class="card qs-exec__runs" aria-label="Runs">
        <h2 class="qs-exec__h2">Runs</h2>
        <p v-if="!runs.length" class="qs-exec__hint">No run yet. The first one starts at the next run time.</p>
        <div class="qs-exec__table">
          <table v-if="runs.length">
            <thead><tr><th>Close</th><th>Mode</th><th>Decision</th><th>Orders</th><th class="num">Fees</th><th class="num">Cost</th><th>Status</th></tr></thead>
            <tbody>
              <template v-for="r in runs" :key="r.run.id">
                <tr class="qs-exec__row" @click="openRun = openRun === r.run.id ? null : r.run.id">
                  <td>{{ r.run.close_date ?? r.run.started_at.slice(0, 10) }}</td>
                  <td>{{ r.run.mode }}</td>
                  <td>{{ decision(r.run) }}</td>
                  <td>{{ r.orders.filter(o => o.report && o.report.filled_qty > 0).length }}</td>
                  <td class="num">{{ money(r.run.fees) }}</td>
                  <td class="num">{{ money(r.run.cost) }}</td>
                  <td :class="r.run.status === 'done' ? 'qs-exec__ok' : 'qs-exec__warn'">{{ r.run.status.replace('_', ' ') }}</td>
                </tr>
                <tr v-if="openRun === r.run.id" class="qs-exec__detail">
                  <td colspan="7">
                    <p>{{ r.summary }}</p>
                    <p v-if="r.run.route">Route: {{ r.run.route.chosen === 'direct' ? 'direct pair' : 'through the quote' }} — {{ r.run.route.reason }}</p>
                    <ul>
                      <li v-for="o in r.orders" :key="o.client_order_id">
                        {{ o.leg }} · {{ o.side }} {{ o.qty }} {{ o.instrument.symbol }} ·
                        <template v-if="o.report">{{ o.report.status }}, {{ o.report.filled_qty }} at {{ o.report.avg_price }} (fee {{ o.report.fee_amount }} {{ o.report.fee_asset }})</template>
                        <template v-else>{{ o.state }}{{ o.error ? ` — ${o.error}` : '' }}</template>
                      </li>
                    </ul>
                    <p v-for="(e, i) in r.run.errors" :key="`e${i}`" class="qs-exec__warn">{{ e }}</p>
                    <p v-for="(n, i) in [...r.run.notes, ...r.run.dust]" :key="`n${i}`" class="qs-exec__note">{{ n }}</p>
                  </td>
                </tr>
              </template>
            </tbody>
          </table>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.qs-exec { display: flex; flex-direction: column; gap: 14px; width: 100%; height: 100%; min-height: 0; }
.qs-exec__head { display: flex; align-items: center; gap: 12px; flex-shrink: 0; }
.qs-exec__title { font-size: 16px; font-weight: 600; color: var(--qs-text); }
.qs-exec__status { padding: 2px 10px; border: 1px solid var(--qs-border); border-radius: 999px; font-size: 12px; color: var(--qs-text-muted); }
.qs-exec__status[data-tone='ok'] { color: var(--qs-success); }
.qs-exec__status[data-tone='warn'] { color: var(--qs-warning); }
.qs-exec__status[data-tone='live'] { color: var(--qs-error); border-color: color-mix(in srgb, var(--qs-error) 50%, var(--qs-border)); }
.qs-exec__next { font-size: 12px; color: var(--qs-text-muted); }
.qs-exec__actions { margin-left: auto; display: flex; align-items: center; gap: 10px; }
.qs-exec__saved { font-size: 12px; color: var(--qs-success); }
.qs-exec__error { padding: 8px 12px; border: 1px solid color-mix(in srgb, var(--qs-error) 50%, var(--qs-border)); border-radius: var(--qs-radius); color: var(--qs-error); font-size: 13px; flex-shrink: 0; }
.qs-exec__grid { flex: 1 1 auto; min-height: 0; display: grid; grid-template-columns: minmax(0, 1.3fr) minmax(0, 1fr); grid-template-rows: auto minmax(0, 1fr); gap: 14px; }
.qs-exec__settings { grid-row: 1 / span 2; display: flex; flex-direction: column; gap: 10px; padding: 16px; min-height: 0; overflow: auto; }
.qs-exec__settings label { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
.qs-exec__pair { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }
.qs-exec__arm { padding: 16px; display: flex; flex-direction: column; gap: 8px; }
.qs-exec__runs { padding: 16px; display: flex; flex-direction: column; gap: 8px; min-height: 0; }
.qs-exec__table { flex: 1 1 auto; min-height: 0; overflow: auto; }
.qs-exec__table table { width: 100%; border-collapse: collapse; font-size: 12px; }
.qs-exec__table th { text-align: left; color: var(--qs-text-muted); font-weight: 500; padding: 4px 6px; border-bottom: 1px solid var(--qs-border-subtle); position: sticky; top: 0; background: var(--qs-bg-card, var(--qs-bg)); }
.qs-exec__table td { padding: 5px 6px; border-bottom: 1px solid var(--qs-border-subtle); }
.qs-exec__table .num { text-align: right; font-variant-numeric: tabular-nums; }
.qs-exec__row { cursor: pointer; }
.qs-exec__row:hover { background: var(--qs-bg-hover); }
.qs-exec__detail td { color: var(--qs-text-secondary); }
.qs-exec__detail ul { margin: 4px 0; padding-left: 16px; }
.qs-exec__h2 { font-size: 13px; font-weight: 600; color: var(--qs-text); }
.qs-exec__line { font-size: 13px; color: var(--qs-text-secondary); }
.qs-exec__tick { display: flex; align-items: center; gap: 6px; font-size: 13px; color: var(--qs-text-secondary); }
.qs-exec__buttons { display: flex; gap: 8px; }
.qs-exec__checks { margin: 0; padding-left: 0; list-style: none; font-size: 12px; }
.qs-exec__hint { font-size: 12px; color: var(--qs-text-muted); }
.qs-exec__note { font-size: 12px; color: var(--qs-text-muted); }
.qs-exec__coverage { display: flex; align-items: center; gap: 10px; font-size: 12px; }
.qs-exec__ok { color: var(--qs-success); }
.qs-exec__warn { color: var(--qs-warning); }
.qs-exec__live { color: var(--qs-error); }
</style>
