<script setup lang="ts">
/**
 * What one switch and holding cost on the system's account
 * (PLAN-EXECUTION-VENUES §5.1), from the live books and the account's fee
 * tier: both routes of the switch (a direct pair or through the quote) and
 * the one the execution core would take, plus 30 days of holding.
 */
import { invoke } from '@tauri-apps/api/core'

const props = withDefaults(defineProps<{ exchangeId: string | null; value: number; market?: 'spot' | 'perp' }>(), { market: 'spot' })

interface Estimate { trades: number; fees: number; spread: number; total: number; complete: boolean }
interface Preview {
  value: number; venue: string; taker_fee: number; fee_source: string; holding: number; days: number; total: number
  funding_per_8h: number | null
  switch: { chosen: 'via_quote' | 'direct'; via_quote: Estimate; direct: Estimate | null; direct_pair: string | null; reason: string }
}

const from = ref('BTC')
const to = ref('ETH')
const busy = ref(false)
const preview = ref<Preview | null>(null)
const error = ref<string | null>(null)

function usd(n: number) {
  return `${n.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })}$`
}

async function run() {
  if (!props.exchangeId) return
  busy.value = true
  error.value = null
  try {
    preview.value = await invoke<Preview>('plugin:algo|venue_cost_preview', {
      exchange_id: props.exchangeId,
      from: from.value.trim().toUpperCase(),
      to: to.value.trim().toUpperCase(),
      value_usd: props.value > 0 ? props.value : 1000,
      market: props.market,
      days: 30,
    })
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <div class="qs-cost">
    <div class="qs-cost__line">
      <span class="qs-cost__label">Cost preview</span>
      <input v-model="from" class="input qs-cost__coin" aria-label="From coin">
      <span>→</span>
      <input v-model="to" class="input qs-cost__coin" aria-label="To coin">
      <button class="btn" :disabled="!exchangeId || busy" @click="run">{{ busy ? 'Reading the books…' : 'Preview' }}</button>
    </div>
    <p v-if="error" class="qs-cost__warn">{{ error }}</p>
    <template v-if="preview">
      <p class="qs-cost__text">
        {{ usd(preview.value) }} on {{ preview.venue }}, taker {{ (preview.taker_fee * 100).toFixed(3) }} % ({{ preview.fee_source }}):
      </p>
      <ul class="qs-cost__list">
        <li :class="{ 'qs-cost__chosen': preview.switch.chosen === 'via_quote' }">
          Through the quote, 2 trades: fees {{ usd(preview.switch.via_quote.fees) }} + spread {{ usd(preview.switch.via_quote.spread) }} = {{ usd(preview.switch.via_quote.total) }}
        </li>
        <li v-if="preview.switch.direct" :class="{ 'qs-cost__chosen': preview.switch.chosen === 'direct' }">
          Direct on {{ preview.switch.direct_pair }}, 1 trade: fees {{ usd(preview.switch.direct.fees) }} + spread {{ usd(preview.switch.direct.spread) }} = {{ usd(preview.switch.direct.total) }}
        </li>
        <li v-if="preview.funding_per_8h != null">
          Holding {{ preview.days }} days on perp: {{ usd(preview.holding) }} funding at {{ (preview.funding_per_8h * 100).toFixed(4) }} % per 8 hours
        </li>
        <li v-else>Holding {{ preview.days }} days on spot: {{ usd(preview.holding) }}</li>
      </ul>
      <p class="qs-cost__text">The engine would go {{ preview.switch.chosen === 'direct' ? 'direct' : 'through the quote' }} — {{ preview.switch.reason }}. Total {{ usd(preview.total) }}.</p>
    </template>
  </div>
</template>

<style scoped>
.qs-cost { display: flex; flex-direction: column; gap: 6px; }
.qs-cost__line { display: flex; flex-wrap: wrap; align-items: center; gap: var(--qss-gap-tight); font-size: var(--qss-text-label); }
.qs-cost__label { font-weight: 600; color: var(--qs-text); white-space: nowrap; }
.qs-cost__coin { width: 70px; }
.qs-cost__text { font-size: 12px; color: var(--qs-text-secondary); }
.qs-cost__list { margin: 0; padding-left: 16px; font-size: 12px; color: var(--qs-text-secondary); }
.qs-cost__chosen { color: var(--qs-text); font-weight: 600; }
.qs-cost__warn { font-size: 12px; color: var(--qs-warning); }
</style>
