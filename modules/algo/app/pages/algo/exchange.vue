<script setup lang="ts">
definePageMeta({ layout: 'algo' })

import { useExchangeStore } from '#algo/stores/exchange'
import type { Exchange, ExchangeConfig, ConnectionResult } from '#algo/types'

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
    if (config.api_key || config.api_secret) {
      testResult.value = await exchangeStore.testCredentials(config)
    } else if (editingExchange.value) {
      testResult.value = await exchangeStore.testConnection(editingExchange.value.id)
    } else {
      testResult.value = {
        success: false,
        message: 'Enter an API key and secret to test them — both are optional for paper trading and backtests.',
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

onMounted(async () => {
  await exchangeStore.load()
})
</script>

<template>
  <div class="exchange-page">
    <!-- Header -->
    <div class="exchange-page__header">
      <h2 class="exchange-page__title">Exchange Connections</h2>
      <button class="btn btn-primary" @click="openAddForm">Add Exchange</button>
    </div>

    <!-- Loading State -->
    <div v-if="exchangeStore.isLoading && !exchangeStore.exchanges.length" class="exchange-page__loading">
      <p class="text-muted">Loading exchanges...</p>
    </div>

    <!-- Empty State -->
    <div v-else-if="!exchangeStore.exchanges.length" class="exchange-page__empty card">
      <p class="empty-state__title">No exchanges connected</p>
      <p class="empty-state__desc text-muted">
        Connect a centralized or decentralized exchange to start trading.
      </p>
      <button class="btn btn-primary" @click="openAddForm">Connect Exchange</button>
    </div>

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
              <AlgoExchangeBalanceDisplay :balances="exchangeStore.balances" />
            </div>
          </div>
        </div>

        <!-- No Selection State -->
        <div v-else class="exchange-page__detail">
          <div class="no-selection card">
            <p class="text-muted">Select an exchange to view balances and details.</p>
          </div>
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
  height: 100%;
  overflow-y: auto;
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.exchange-page__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  flex-shrink: 0;
}

.exchange-page__title {
  font-size: 16px;
  font-weight: 600;
  color: var(--qa-text);
}

.exchange-page__loading {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 200px;
  font-size: 14px;
}

/* Empty State */
.exchange-page__empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  padding: 48px 24px;
  text-align: center;
}

.empty-state__title {
  font-size: 15px;
  font-weight: 600;
  color: var(--qa-text);
}

.empty-state__desc {
  font-size: 13px;
  max-width: 360px;
}

/* Content Layout */
.exchange-page__content {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 20px;
  flex: 1;
  min-height: 0;
}

@media (max-width: 900px) {
  .exchange-page__content {
    grid-template-columns: 1fr;
  }
}

.exchange-page__list {
  min-height: 0;
  overflow-y: auto;
}

.exchange-page__detail {
  min-height: 0;
  overflow-y: auto;
}

/* Detail Panel */
.detail-panel__header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  margin-bottom: 20px;
}

.detail-panel__info {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.detail-panel__name {
  font-size: 16px;
  font-weight: 600;
  color: var(--qa-text);
}

.detail-panel__meta {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 13px;
}

.detail-panel__provider {
  text-transform: capitalize;
}

.detail-panel__status {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--qa-text-secondary);
}

.detail-panel__actions {
  display: flex;
  gap: 6px;
}

.detail-panel__balances {
  border-top: 1px solid var(--qa-border-subtle);
  padding-top: 16px;
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

/* No selection */
.no-selection {
  display: flex;
  align-items: center;
  justify-content: center;
  min-height: 200px;
  font-size: 13px;
}
</style>
