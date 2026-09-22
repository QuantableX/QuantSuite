<script setup lang="ts">
import { useConfigStore } from '#systems/stores/config'
import { useIndicatorOptions } from '#systems/composables/useIndicatorOptions'
import SystemsSettingsMarketFilterSelect from '#systems/components/Settings/MarketFilterSelect.vue'
import type { Cadence, PriceSource, RankingSource, RunConfig, TrendKind } from '#systems/types'

const props = defineProps<{ systemId: string }>()
const config = useConfigStore()

const cfg = computed(() => config.get(props.systemId))
const { catalog, scoreTrack, options: memberOptions, trendOptions } = useIndicatorOptions(() => cfg.value)

const trend = computed(() => cfg.value.indicator.trend ?? 'ema_cross')
const members = computed(() => cfg.value.indicator.aggregate ?? [])
// The EMA band's lengths matter when the EMA cross is the signal or votes
// in the aggregate.
const emaInUse = computed(() => trend.value === 'ema_cross' || (trend.value === 'aggregate' && members.value.includes('ema_cross')))

function setAggregate(aggregate: TrendKind[]) {
  const current = config.get(props.systemId)
  config.update(props.systemId, { indicator: { ...current.indicator, aggregate } })
}

function set<K extends keyof RunConfig>(key: K, value: RunConfig[K]) {
  config.update(props.systemId, { [key]: value } as Partial<RunConfig>)
}

function setEma<K extends keyof RunConfig['indicator']['emaCross']>(
  key: K,
  value: RunConfig['indicator']['emaCross'][K],
) {
  const current = config.get(props.systemId)
  config.update(props.systemId, {
    indicator: {
      ...current.indicator,
      emaCross: { ...current.indicator.emaCross, [key]: value },
    },
  })
}

function setTrend(value: TrendKind) {
  const current = config.get(props.systemId)
  config.update(props.systemId, {
    indicator: { ...current.indicator, trend: value },
  })
}

const cadences: Cadence[] = ['1m', '1h', '4h', '12h', 'daily', 'weekly', 'monthly']
const sources: RankingSource[] = ['auto', 'cmc', 'local']
const priceSources: PriceSource[] = ['close', 'open', 'high', 'low', 'hl2', 'hlc3', 'ohlc4']

const feePct = computed({
  get: () => +(cfg.value.feeRate * 100).toFixed(4),
  set: v => set('feeRate', (Number(v) || 0) / 100),
})
const slipPct = computed({
  get: () => +(cfg.value.slippageRate * 100).toFixed(4),
  set: v => set('slippageRate', (Number(v) || 0) / 100),
})
</script>

<template>
  <div class="card qs-form">
    <h2 class="qs-form__heading">Run Configuration</h2>

    <div class="qs-form__section qs-form__section--universe">
      <span class="qs-form__legend">Coin Selection</span>
      <div class="qs-form__grid">
        <div>
          <label class="label">Top N</label>
          <input class="input mono" type="number" min="2" max="1000"
            :value="cfg.topN" @input="set('topN', Math.max(2, Number(($event.target as HTMLInputElement).value) || 0))" />
        </div>
        <div>
          <label class="label">Exclude Top N</label>
          <input class="input mono" type="number" min="0" max="999"
            :value="cfg.excludeTopN" @input="set('excludeTopN', Math.max(0, Number(($event.target as HTMLInputElement).value) || 0))" />
        </div>
        <div>
          <label class="label">Rotation Cadence</label>
          <select class="select" :value="cfg.cadence" @change="set('cadence', ($event.target as HTMLSelectElement).value as Cadence)">
            <option v-for="c in cadences" :key="c" :value="c">{{ c }}</option>
          </select>
        </div>
        <div>
          <label class="label">Ranking Source</label>
          <select class="select" :value="cfg.rankingSource" @change="set('rankingSource', ($event.target as HTMLSelectElement).value as RankingSource)">
            <option v-for="s in sources" :key="s" :value="s">{{ s }}</option>
          </select>
        </div>
      </div>
      <div class="qs-form__checks">
        <label class="qs-check">
          <input type="checkbox" :checked="cfg.excludeStablecoins" @change="set('excludeStablecoins', ($event.target as HTMLInputElement).checked)" />
          <span>Exclude stablecoins</span>
        </label>
        <label class="qs-check">
          <input type="checkbox" :checked="cfg.excludeWrapped" @change="set('excludeWrapped', ($event.target as HTMLInputElement).checked)" />
          <span>Exclude wrapped</span>
        </label>
        <label class="qs-check">
          <input type="checkbox" :checked="cfg.includeUsd" @change="set('includeUsd', ($event.target as HTMLInputElement).checked)" />
          <span>Include USD / cash leg</span>
        </label>
      </div>
    </div>

    <div class="qs-form__section qs-form__section--window">
      <span class="qs-form__legend">Window</span>
      <div class="qs-form__grid">
        <div>
          <label class="label">Start Date</label>
          <input class="input mono" type="date" :value="cfg.startDate" @change="set('startDate', ($event.target as HTMLInputElement).value)" />
        </div>
        <div>
          <label class="label">End Date</label>
          <input class="input mono" type="date" :value="cfg.endDate" @change="set('endDate', ($event.target as HTMLInputElement).value)" />
        </div>
      </div>
    </div>

    <div class="qs-form__section qs-form__section--signals">
      <span class="qs-form__legend">Trend Signal</span>
      <div class="qs-form__grid qs-form__grid--indicator">
        <div>
          <label class="label">Indicator</label>
          <SystemsSettingsIndicatorSelect
            :model-value="trend"
            :options="trendOptions"
            @update:model-value="setTrend($event as TrendKind)"
          />
        </div>
        <div v-if="trend === 'aggregate'">
          <label class="label">Members</label>
          <SystemsBacktestCompareSelect
            class="qs-form__members"
            label="Members"
            title="The indicators whose signals are averaged"
            empty-hint="Pick indicators to average"
            unit="member"
            unit-plural="members"
            :model-value="members"
            :options="memberOptions"
            @update:model-value="setAggregate"
          />
        </div>
      </div>
      <p v-if="trend === 'aggregate'" class="qs-form__hint">
        Averages each member’s +1/−1 signal on the same pair: bullish above 0, bearish below 0.
        Ties keep the previous verdict; warming-up members do not vote.
      </p>
      <p v-else-if="trend !== 'ema_cross'" class="qs-form__hint">
        Uses the Smithery defaults. Scores refer to {{ scoreTrack }} candles; historical references are not fresh certification.
        Validate the indicator on the system’s asset ratios and portfolio, including trading costs.
      </p>
      <p v-if="catalog.loading" class="qs-form__hint">Loading Smithery indicators…</p>
      <p v-if="catalog.error" class="qs-form__hint" role="alert">
        Smithery could not be loaded: {{ catalog.error }}
        <button class="btn btn--sm" type="button" @click="catalog.load()">Retry</button>
      </p>
      <div class="qs-form__checks">
        <label class="qs-check">
          <input type="checkbox" :checked="cfg.marketFilter" @change="set('marketFilter', ($event.target as HTMLInputElement).checked)" />
          <span>Higher filter: TOTAL</span>
        </label>
      </div>
      <p class="qs-form__hint">
        Positions only while TOTAL (the ranked top-N, weighted by market cap) is bullish; otherwise USD.
        An independent filter applies to all variants and live evaluation, without changing coin rankings.
      </p>
      <SystemsSettingsMarketFilterSelect v-if="cfg.marketFilter" :config="cfg"
        @update:model-value="set('marketIndicator', $event)" />
      <div v-if="emaInUse" class="qs-form__grid qs-form__grid--three">
        <div>
          <label class="label">Source</label>
          <select class="select" :value="cfg.indicator.emaCross.src" @change="setEma('src', ($event.target as HTMLSelectElement).value as PriceSource)">
            <option v-for="p in priceSources" :key="p" :value="p">{{ p }}</option>
          </select>
        </div>
        <div>
          <label class="label">Fast Length</label>
          <input class="input mono" type="number" min="1" :value="cfg.indicator.emaCross.fastLength"
            @input="setEma('fastLength', Math.max(1, Number(($event.target as HTMLInputElement).value) || 0))" />
        </div>
        <div>
          <label class="label">Slow Length</label>
          <input class="input mono" type="number" min="1" :value="cfg.indicator.emaCross.slowLength"
            @input="setEma('slowLength', Math.max(1, Number(($event.target as HTMLInputElement).value) || 0))" />
        </div>
      </div>
    </div>

    <div class="qs-form__section qs-form__section--costs">
      <span class="qs-form__legend">Costs &amp; Rate Limit</span>
      <div class="qs-form__grid">
        <div>
          <label class="label">Fee (%)</label>
          <input class="input mono" type="number" step="0.01" min="0" v-model.number="feePct" />
        </div>
        <div>
          <label class="label">Slippage (%)</label>
          <input class="input mono" type="number" step="0.01" min="0" v-model.number="slipPct" />
        </div>
        <div>
          <label class="label">API delay (s)</label>
          <input class="input mono" type="number" step="0.1" min="0" :value="cfg.minRequestInterval"
            @input="set('minRequestInterval', Math.max(0, Number(($event.target as HTMLInputElement).value) || 0))" />
        </div>
      </div>
      <p class="qs-form__hint">
        Delay applies only to uncached API requests. Increase it if you hit rate limits.
      </p>
    </div>
  </div>
</template>

<style scoped>
.qs-form {
  min-width: 0;
  padding: 16px 20px 20px;
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  grid-template-areas: 'heading heading' 'universe signals' 'window signals' 'costs signals';
  column-gap: 24px;
  align-content: start;
}

.qs-form__heading {
  grid-area: heading;
  margin: 0 0 8px;
  font-size: 13px;
  font-weight: 600;
}

.qs-form__section--universe { grid-area: universe; }
.qs-form__section--window { grid-area: window; }
.qs-form__section--signals { grid-area: signals; }
.qs-form__section--costs { grid-area: costs; }

.qs-form__section {
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px 0;
  border-top: 1px solid var(--qs-border-subtle);
}

.qs-form__section:last-child {
  padding-bottom: 0;
}

.qs-form__legend {
  font-size: 11px;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--qs-accent);
}

.qs-form__hint {
  margin: 0;
  font-size: 12px;
  line-height: 1.5;
  color: var(--qs-text-muted);
}

.qs-form__grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px 12px;
}

.qs-form__grid > * { min-width: 0; }
.qs-form__grid--indicator { grid-template-columns: minmax(0, 1fr); }
.qs-form__grid--three { grid-template-columns: repeat(3, minmax(0, 1fr)); }
.qs-form__section--costs .qs-form__grid { grid-template-columns: repeat(3, minmax(0, 1fr)); }

.qs-form__checks {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.qs-form__checks .qs-check { padding: 4px 10px; }

/* The member picker fills its grid cell like the inputs beside it. */
.qs-form__members :deep(.qs-cmp__trigger) {
  width: 100%;
  justify-content: space-between;
  padding: 8px 12px;
  font-size: inherit;
}

.qs-form__members :deep(.qs-cmp__panel) {
  left: 0;
  right: auto;
}

/* .qs-check (the checkbox chip) lives in the module stylesheet: the backtest
   header uses the same chip for the higher filter. */

@media (max-width: 720px) {
  .qs-form {
    grid-template-columns: minmax(0, 1fr);
    grid-template-areas: 'heading' 'universe' 'window' 'signals' 'costs';
  }

  .qs-form__grid {
    grid-template-columns: 1fr;
  }
}
</style>
