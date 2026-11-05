<script setup lang="ts">
definePageMeta({ layout: 'algo' })

import { invoke } from '@tauri-apps/api/core'
import { useExchangeStore } from '#algo/stores/exchange'
import type { Exchange, ExchangeConfig, ConnectionResult, VaultStatus } from '#algo/types'

const exchangeStore = useExchangeStore()

// UI state
const showForm = ref(false)
const editingExchange = ref<Exchange | null>(null)
const isDeleting = ref(false)
const balanceError = ref<string | null>(null)
const testResult = ref<ConnectionResult | null>(null)
const isTesting = ref(false)

async function handleRefreshBalances(exchangeId: string) {
  balanceError.value = null
  try {
    await exchangeStore.refreshBalances(exchangeId)
  } catch (err) {
    // refreshBalances rethrows — without this the click handler dies silently.
    console.error('Failed to refresh exchange balances:', err)
    balanceError.value = String(err)
  }
}

async function handleSelect(exchangeId: string) {
  exchangeStore.setActive(exchangeId)
  await handleRefreshBalances(exchangeId)
}

async function handleDelete(exchangeId: string) {
  const exchange = exchangeStore.exchanges.find((item) => item.id === exchangeId)
  if (!exchange) return

  const confirmed = window.confirm(
    `Are you sure you want to remove "${exchange.name}"? This cannot be undone.`,
  )
  if (!confirmed) return

  isDeleting.value = true
  try {
    await exchangeStore.delete(exchange.id)
  } catch (err) {
    console.error('Failed to delete exchange:', err)
  } finally {
    isDeleting.value = false
  }
}

async function handleSubmit(config: ExchangeConfig) {
  try {
    if (editingExchange.value) {
      await exchangeStore.update(editingExchange.value.id, config)
    } else {
      await exchangeStore.add(config)
    }
    showForm.value = false
    editingExchange.value = null
  } catch (err) {
    console.error('Failed to save exchange:', err)
  }
}

// Tests what the form holds: freshly typed keys are probed as they are (no
// save), blank key fields while editing mean "the stored ones". The result
// renders inside the form (PLAN-QUANTALGO §5) — no alerts.
async function handleTestConnection(config: ExchangeConfig) {
  isTesting.value = true
  testResult.value = null
  try {
    // A Solana wallet is generated on save: only the stored one can be tested.
    const typed = config.provider !== 'jupiter'
      && !!(config.api_key || config.api_secret || config.private_key || config.wallet_address)
    if (typed) {
      testResult.value = await exchangeStore.testCredentials(config)
    } else if (editingExchange.value) {
      testResult.value = await exchangeStore.testConnection(editingExchange.value.id)
    } else {
      testResult.value = {
        success: false,
        message: config.provider === 'jupiter'
          ? 'QuantSuite generates the wallet when you save; test it afterwards.'
          : 'Enter an API key and secret to test them — both are optional for paper trading and backtests.',
      }
    }
  } catch (err) {
    console.error('Failed to test exchange connection:', err)
    testResult.value = { success: false, message: String(err) }
  } finally {
    isTesting.value = false
  }
}

function openAddForm() {
  editingExchange.value = null
  testResult.value = null
  showForm.value = true
}

function openEditForm(exchange: Exchange) {
  editingExchange.value = exchange
  testResult.value = null
  showForm.value = true
}

function closeForm() {
  showForm.value = false
  editingExchange.value = null
  testResult.value = null
}

// The secret vault (PLAN-AUTO-EXECUTION §2.1). Asked only when the list says
// it is locked: on a machine with an OS keystore it never is.
const vault = ref<VaultStatus | null>(null)
const vaultPassphrase = ref('')
const isUnlocking = ref(false)

async function loadVaultStatus() {
  if (!exchangeStore.exchanges.some((ex) => ex.key_storage === 'locked')) {
    vault.value = null
    return
  }
  try {
    vault.value = await invoke<VaultStatus>('plugin:qs|vault_status')
  } catch (err) {
    vault.value = { state: 'error', source: null, secrets: 0, message: String(err) }
  }
}

async function unlockVault() {
  if (!vaultPassphrase.value) return
  isUnlocking.value = true
  try {
    vault.value = await invoke<VaultStatus>('plugin:qs|vault_unlock', { passphrase: vaultPassphrase.value })
    if (vault.value.state === 'ready') {
      vaultPassphrase.value = ''
      await exchangeStore.load()
      await loadVaultStatus()
    }
  } catch (err) {
    vault.value = { state: 'error', source: null, secrets: 0, message: String(err) }
  } finally {
    isUnlocking.value = false
  }
}

onMounted(async () => {
  await exchangeStore.load()
  await loadVaultStatus()
})
</script>

<template>
  <div class="exchange-page">
    <!-- Header -->
    <div class="exchange-page__header">
      <h2 class="exchange-page__title">Exchange Connections</h2>
      <button class="btn btn-primary" @click="openAddForm">Add Exchange</button>
    </div>

    <!-- Secret vault locked (only without an OS keystore, or when it failed) -->
    <div v-if="vault && vault.state !== 'ready'" class="vault-notice card">
      <p class="vault-notice__text">{{ vault.message ?? 'The secret vault is not open.' }}</p>
      <form
        v-if="vault.state === 'needs-passphrase'"
        class="vault-notice__form"
        @submit.prevent="unlockVault"
      >
        <input
          v-model="vaultPassphrase"
          type="password"
          class="input"
          placeholder="Vault passphrase"
          autocomplete="off"
        >
        <button class="btn btn-sm btn-primary" type="submit" :disabled="isUnlocking || !vaultPassphrase">
          {{ isUnlocking ? 'Opening...' : 'Open vault' }}
        </button>
      </form>
    </div>

    <!-- Loading State -->
    <div v-if="exchangeStore.isLoading && !exchangeStore.exchanges.length" class="exchange-page__loading">
      <p class="text-muted">Loading exchanges...</p>
    </div>

    <!-- Empty State -->
    <QEmptyState
      v-else-if="!exchangeStore.exchanges.length"
      compact
      title="No exchanges connected"
      description="Connect a centralized or decentralized exchange to start trading."
    >
      <button class="btn btn-primary" @click="openAddForm">Connect Exchange</button>
    </QEmptyState>

    <!-- Exchange Content -->
    <template v-else>
      <div class="exchange-page__content">
        <!-- Exchange List -->
        <div class="exchange-page__list">
          <AlgoExchangeList
            :exchanges="exchangeStore.exchanges"
            @select="handleSelect"
            @delete="handleDelete"
          />
          <div class="card exposure-card">
            <h3 class="exposure-card__title">Exposure per venue</h3>
            <AlgoExchangeExposurePanel />
          </div>
        </div>

        <!-- Selected Exchange Detail -->
        <div v-if="exchangeStore.activeExchange" class="exchange-page__detail">
          <div class="detail-panel card">
            <div class="detail-panel__header">
              <div class="detail-panel__info">
                <h3 class="detail-panel__name">{{ exchangeStore.activeExchange.name }}</h3>
                <div class="detail-panel__meta">
                  <span
                    class="pill"
                    :class="exchangeStore.activeExchange.exchange_type === 'cex' ? 'pill--cex' : 'pill--dex'"
                  >
                    {{ exchangeStore.activeExchange.exchange_type.toUpperCase() }}
                  </span>
                  <span class="text-muted detail-panel__provider">
                    {{ exchangeStore.activeExchange.provider }}
                  </span>
                  <span class="detail-panel__status">
                    <span
                      class="status-dot"
                      :class="exchangeStore.activeExchange.is_active ? 'status-dot--active' : 'status-dot--inactive'"
                    />
                    {{ exchangeStore.activeExchange.is_active ? 'Active' : 'Inactive' }}
                  </span>
                </div>
              </div>
              <div class="detail-panel__actions">
                <button class="btn btn-sm" @click="openEditForm(exchangeStore.activeExchange!)">
                  Edit
                </button>
                <button
                  class="btn btn-sm"
                  @click="handleRefreshBalances(exchangeStore.activeExchange!.id)"
                >
                  Refresh
                </button>
              </div>
            </div>

            <!-- Balance Display -->
            <div class="detail-panel__balances">
              <p v-if="balanceError" class="text-error">{{ balanceError }}</p>
              <AlgoExchangeBalanceDisplay class="detail-panel__display" :balances="exchangeStore.balances" />
            </div>
          </div>
        </div>

        <!-- No Selection State -->
        <div v-else class="exchange-page__detail">
          <p class="no-selection text-muted">Select an exchange to view balances and details.</p>
        </div>
      </div>
    </template>

    <!-- Exchange Form Modal -->
    <AlgoExchangeForm
      :visible="showForm"
      :exchange="editingExchange ?? undefined"
      :test-result="testResult"
      :is-testing="isTesting"
      @submit="handleSubmit"
      @test-connection="handleTestConnection"
      @close="closeForm"
    />
  </div>
</template>

<style scoped>
.exchange-page {
  container-type: inline-size;
  height: 100%;
  overflow-y: auto;
  padding: var(--qss-page-edge);
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
}

.exchange-page__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--qss-gap);
  flex-shrink: 0;
}

.exchange-page__title {
  font-size: var(--qss-text-title);
  font-weight: 600;
  letter-spacing: -.01em;
  color: var(--qa-text);
}

.vault-notice {
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap-tight);
  border-left: 3px solid var(--qa-warning);
  flex-shrink: 0;
}

.vault-notice__text {
  font-size: var(--qss-text-section);
  color: var(--qa-text);
}

.vault-notice__form {
  display: flex;
  gap: var(--qss-gap-tight);
  align-items: center;
}

.vault-notice__form .input {
  max-width: 280px;
}

.exchange-page__loading {
  font-size: var(--qss-text-section);
}

/* The list beside its detail; each scrolls on its own. */
.exchange-page__content {
  display: grid;
  grid-template-columns: minmax(300px, 380px) minmax(0, 1fr);
  gap: var(--qss-gap);
  flex: 1 1 0;
  min-height: 420px;
}

.exchange-page__list {
  min-height: 0;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
}

.exchange-page__detail {
  min-width: 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
}

/* Detail Panel */
.detail-panel {
  flex: 1 1 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap-tight);
}

.detail-panel__header {
  flex-shrink: 0;
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--qss-gap-tight) var(--qss-gap);
}

.detail-panel__info {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: var(--qss-gap-tight) var(--qss-gap);
}

.detail-panel__name {
  font-size: var(--qss-text-section);
  font-weight: 600;
  color: var(--qa-text);
}

.detail-panel__meta {
  display: flex;
  align-items: center;
  gap: var(--qss-gap-tight);
  font-size: var(--qss-text-label);
}

.detail-panel__provider {
  text-transform: capitalize;
}

.detail-panel__status {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: var(--qss-text-label);
  color: var(--qa-text-secondary);
}

.detail-panel__actions {
  display: flex;
  gap: 6px;
}

.exposure-card {
  flex-shrink: 0;
}

.exposure-card__title {
  font-size: var(--qss-text-section);
  font-weight: 600;
  color: var(--qa-text);
  margin-bottom: var(--qss-gap-tight);
}

.detail-panel__balances {
  flex: 1 1 0;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap-tight);
  border-top: 1px solid var(--qa-border-subtle);
  padding-top: var(--qss-gap);
}

.detail-panel__display {
  flex: 1 1 0;
  min-height: 0;
}

/* Pill variants */
.pill--cex {
  color: var(--qa-accent);
  border-color: var(--qa-accent);
}

.pill--dex {
  color: var(--qa-accent);
  border-color: var(--qa-accent);
}

/* Status dot */
.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}

.status-dot--active {
  background: var(--qa-accent);
}

.status-dot--inactive {
  background: var(--qa-text-muted);
}

/* No selection: a hint level with the list, no frame */
.no-selection {
  padding: var(--qss-panel-pad) 0;
  font-size: var(--qss-text-section);
}

@container (max-width: 700px) {
  .exchange-page__content {
    flex: none;
    grid-template-columns: minmax(0, 1fr);
    align-content: start;
  }

  .exchange-page__list {
    overflow-y: visible;
  }

  .detail-panel {
    min-height: 320px;
  }
}
</style>
