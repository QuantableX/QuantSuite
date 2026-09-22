<script setup lang="ts">
import { useAppStore } from '#systems/stores/app'

const app = useAppStore()

const providers = [
  { id: 'cmc', label: 'CoinMarketCap snapshots', note: 'Historical top-N rankings' },
  { id: 'local', label: 'Local reconstruction', note: 'CoinGecko market-cap fallback' },
  { id: 'ccxt', label: 'CCXT / Binance', note: 'Daily OHLCV candles' },
  { id: 'coingecko', label: 'CoinGecko market chart', note: 'OHLCV fallback' },
]

const engineRunning = computed(() => app.engineStatus.status === 'running')
</script>

<template>
  <div class="card qs-prov">
    <div class="qs-prov__head">
      <h3 class="qs-prov__title">Data Providers</h3>
      <span class="pill">{{ engineRunning ? 'engine up' : 'engine down' }}</span>
    </div>
    <ul class="qs-prov__list">
      <li v-for="p in providers" :key="p.id" class="qs-prov__item">
        <span class="qs-prov__dot" :class="engineRunning ? 'qs-prov__dot--ok' : 'qs-prov__dot--idle'" />
        <div class="qs-prov__meta">
          <span class="qs-prov__label">{{ p.label }}</span>
          <span class="qs-prov__note">{{ p.note }}</span>
        </div>
      </li>
    </ul>
  </div>
</template>

<style scoped>
.qs-prov {
  padding: 12px 14px;
}

.qs-prov__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 10px;
}

.qs-prov__title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
}

.qs-prov__list {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.qs-prov__item {
  display: flex;
  align-items: flex-start;
  gap: 10px;
}

.qs-prov__dot {
  margin-top: 5px;
  width: 8px;
  height: 8px;
  border-radius: 999px;
  flex-shrink: 0;
}

.qs-prov__dot--ok {
  background: var(--qs-success);
}

.qs-prov__dot--idle {
  background: var(--qs-text-muted);
}

.qs-prov__meta {
  display: flex;
  flex-direction: column;
}

.qs-prov__label {
  font-size: 12px;
  color: var(--qs-text);
}

.qs-prov__note {
  font-size: 11px;
  color: var(--qs-text-muted);
}
</style>
