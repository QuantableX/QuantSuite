<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import type { Exchange } from '#algo/types'
import { formatDate, capitalize } from '#algo/utils/format'

const props = defineProps<{
  exchanges: Exchange[]
  activeId?: string
}>()

const emit = defineEmits<{
  select: [id: string]
  delete: [id: string]
}>()

/** A Hyperliquid API wallet as the main account approved it (venue_wallet_status). */
interface WalletStatus { address: string; approved: boolean; valid_until: number | null; days_left: number | null; expiring: boolean }

const wallets = ref<Record<string, { text: string, tone: 'muted' | 'warn' }>>({})

async function loadWallets() {
  for (const ex of props.exchanges.filter(e => e.provider === 'hyperliquid' && e.key_storage === 'vault')) {
    try {
      const s = await invoke<WalletStatus>('plugin:algo|venue_wallet_status', { exchange_id: ex.id })
      wallets.value[ex.id] = !s.approved
        ? { text: 'API wallet not approved by the main address', tone: 'warn' }
        : s.days_left == null
          ? { text: 'API wallet approved, no expiry', tone: 'muted' }
          : { text: `API wallet expires in ${s.days_left} day${s.days_left === 1 ? '' : 's'}${s.expiring ? ', approve a new one' : ''}`, tone: s.expiring ? 'warn' : 'muted' }
    } catch (e) {
      wallets.value[ex.id] = { text: `API wallet status unavailable: ${e}`, tone: 'warn' }
    }
  }
}

watch(() => props.exchanges.map(e => `${e.id}:${e.updated_at}`).join(','), loadWallets, { immediate: true })

/** Where the keys are — never the keys (PLAN-AUTO-EXECUTION §2.1). */
function keyLine(ex: Exchange): { text: string, tone: 'muted' | 'warn' } {
  switch (ex.key_storage) {
    case 'vault':
      return { text: `Keys in vault ${ex.key_fingerprint ?? ''}`.trim(), tone: 'muted' }
    case 'unreadable':
      return { text: 'Saved keys unreadable, enter them again', tone: 'warn' }
    case 'locked':
      return { text: 'Vault locked', tone: 'warn' }
    default:
      return { text: 'No API keys', tone: 'muted' }
  }
}
</script>

<template>
  <div class="exchange-list">
    <div v-if="!exchanges.length" class="empty-state">
      No exchanges configured
    </div>
    <div
      v-for="ex in exchanges"
      :key="ex.id"
      class="exchange-card"
      :class="{ active: ex.id === activeId }"
      @click="emit('select', ex.id)"
    >
      <div class="card-top">
        <div class="card-info">
          <span class="exchange-name">{{ ex.name }}</span>
          <div class="card-badges">
            <span class="pill provider-badge">{{ capitalize(ex.provider) }}</span>
            <span class="pill type-badge">{{ ex.exchange_type.toUpperCase() }}</span>
            <span v-if="ex.sandbox" class="pill sandbox-badge" title="Private calls go to the venue's testnet / demo environment">Sandbox</span>
          </div>
        </div>
        <button
          class="delete-btn"
          title="Delete exchange"
          @click.stop="emit('delete', ex.id)"
        >
          &times;
        </button>
      </div>
      <div class="card-bottom">
        <span class="status-indicator">
          <span class="status-dot" :class="ex.is_active ? 'dot-active' : 'dot-inactive'" />
          <span class="status-text">{{ ex.is_active ? 'Connected' : 'Inactive' }}</span>
        </span>
        <span class="connected-date">{{ formatDate(ex.created_at) }}</span>
      </div>
      <div
        class="card-keys"
        :class="{ 'card-keys--warn': keyLine(ex).tone === 'warn' }"
        title="API keys are kept in the suite's secret vault; QuantSuite never shows them again"
      >
        {{ keyLine(ex).text }}
      </div>
      <div
        v-if="wallets[ex.id]"
        class="card-keys"
        :class="{ 'card-keys--warn': wallets[ex.id]!.tone === 'warn' }"
        title="Hyperliquid API wallets can carry an expiry; QuantSuite warns 7 days ahead"
      >
        {{ wallets[ex.id]!.text }}
      </div>
    </div>
  </div>
</template>

<style scoped>
.exchange-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.exchange-card {
  background: var(--qa-bg-card);
  border: 1px solid var(--qa-border);
  border-radius: var(--qa-radius);
  padding: 12px;
  cursor: pointer;
  transition: all var(--qa-transition);
  border-left: 3px solid transparent;
}

.exchange-card:hover {
  background: var(--qa-bg-hover);
}

.exchange-card.active {
  border-left-color: var(--qa-accent);
}

.card-top {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  margin-bottom: 8px;
}

.card-info {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.exchange-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--qa-text);
}

.card-badges {
  display: flex;
  gap: 6px;
}

.provider-badge {
  font-size: 10px;
}

.type-badge {
  font-size: 10px;
}

.sandbox-badge {
  font-size: 10px;
  color: var(--qa-warning);
}

.delete-btn {
  background: none;
  border: none;
  color: var(--qa-text-muted);
  font-size: 18px;
  line-height: 1;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 4px;
  opacity: 0;
  transition: all var(--qa-transition);
}

.exchange-card:hover .delete-btn {
  opacity: 1;
}

.delete-btn:hover {
  color: var(--qa-error);
  background: var(--qa-bg-hover);
}

.card-bottom {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.status-indicator {
  display: flex;
  align-items: center;
  gap: 6px;
}

.status-dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot-active {
  background: var(--qa-accent);
}

.dot-inactive {
  background: var(--qa-text-muted);
}

.status-text {
  font-size: 12px;
  color: var(--qa-text-secondary);
}

.connected-date {
  font-size: 11px;
  color: var(--qa-text-muted);
}

.card-keys {
  margin-top: 6px;
  font-size: 11px;
  color: var(--qa-text-muted);
}

.card-keys--warn {
  color: var(--qa-warning);
}

.empty-state {
  padding: 32px;
  text-align: center;
  color: var(--qa-text-muted);
  font-size: 13px;
}
</style>
