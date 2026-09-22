<script setup lang="ts">
import type { Exchange, ExchangeConfig, ExchangeType, ExchangeProvider, ConnectionResult } from '#algo/types'

const props = withDefaults(
  defineProps<{
    exchange?: Exchange
    visible?: boolean
    /** The last connection test's outcome, shown inline under the form. */
    testResult?: ConnectionResult | null
    isTesting?: boolean
  }>(),
  { exchange: undefined, visible: undefined, testResult: null, isTesting: false },
)

const emit = defineEmits<{
  submit: [config: ExchangeConfig]
  close: []
  /** Test what is in the form now — stored keys when the key fields are blank while editing. */
  testConnection: [config: ExchangeConfig]
}>()

const CEX_PROVIDERS: ExchangeProvider[] = ['binance', 'bybit', 'okx', 'coinbase', 'kraken', 'kucoin']
const DEX_PROVIDERS: ExchangeProvider[] = ['uniswap', 'jupiter', 'hyperliquid']

const name = ref('')
const exchangeType = ref<ExchangeType>('cex')
const provider = ref<ExchangeProvider>('binance')
const apiKey = ref('')
const apiSecret = ref('')
const passphrase = ref('')
const walletAddress = ref('')
const privateKey = ref('')
const rpcEndpoint = ref('')
const sandbox = ref(false)

const showApiKey = ref(false)
const showApiSecret = ref(false)
const showPrivateKey = ref(false)

const isEditing = computed(() => !!props.exchange)
const modalTitle = computed(() => isEditing.value ? 'Edit Exchange' : 'Add Exchange')

const providerOptions = computed(() =>
  exchangeType.value === 'cex' ? CEX_PROVIDERS : DEX_PROVIDERS
)

const needsPassphrase = computed(() => provider.value === 'okx' || provider.value === 'kucoin')
/** Binance has a spot testnet, Bybit a demo-trading environment; the other venues have none. */
const supportsSandbox = computed(() => exchangeType.value === 'cex' && (provider.value === 'binance' || provider.value === 'bybit'))
const sandboxLabel = computed(() => (provider.value === 'binance' ? 'Binance spot testnet' : 'Bybit demo trading'))

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1)
}

function providerLabel(p: ExchangeProvider): string {
  const name = capitalize(p)
  return exchangeType.value === 'dex' ? `${name} (stored credentials only)` : name
}

function resetForm() {
  name.value = ''
  exchangeType.value = 'cex'
  provider.value = 'binance'
  apiKey.value = ''
  apiSecret.value = ''
  passphrase.value = ''
  walletAddress.value = ''
  privateKey.value = ''
  rpcEndpoint.value = ''
  sandbox.value = false
  showApiKey.value = false
  showApiSecret.value = false
  showPrivateKey.value = false
}

function prefillForm() {
  if (props.exchange) {
    name.value = props.exchange.name
    exchangeType.value = props.exchange.exchange_type
    provider.value = props.exchange.provider
    sandbox.value = props.exchange.sandbox
  } else {
    resetForm()
  }
}

// Reset the provider only on user interaction — a watch on exchangeType would
// also fire after prefillForm() and clobber the edited exchange's provider.
function selectType(type: ExchangeType) {
  exchangeType.value = type
  provider.value = type === 'cex' ? 'binance' : 'uniswap'
}

function onBackdropClick(e: MouseEvent) {
  if ((e.target as HTMLElement).classList.contains('modal-backdrop')) {
    emit('close')
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    emit('close')
  }
}

watch(() => props.visible, (val) => {
  if (val !== false) {
    prefillForm()
    window.addEventListener('keydown', onKeydown)
  } else {
    window.removeEventListener('keydown', onKeydown)
  }
})

onUnmounted(() => {
  window.removeEventListener('keydown', onKeydown)
})

function buildConfig(): ExchangeConfig {
  const config: ExchangeConfig = {
    name: name.value,
    exchange_type: exchangeType.value,
    provider: provider.value,
  }

  if (exchangeType.value === 'cex') {
    if (apiKey.value) config.api_key = apiKey.value
    if (apiSecret.value) config.api_secret = apiSecret.value
    if (passphrase.value) config.passphrase = passphrase.value
    config.sandbox = supportsSandbox.value && sandbox.value
  } else {
    if (walletAddress.value) config.wallet_address = walletAddress.value
    if (privateKey.value) config.private_key = privateKey.value
    if (rpcEndpoint.value) config.rpc_endpoint = rpcEndpoint.value
  }

  return config
}

function handleSubmit() {
  emit('submit', buildConfig())
}

function handleTest() {
  emit('testConnection', buildConfig())
}
</script>

<template>
  <Teleport to="body">
    <div v-if="visible !== false" class="modal-backdrop" @click="onBackdropClick">
      <div class="modal-content">
        <div class="modal-header">
          <h3 class="modal-title">{{ modalTitle }}</h3>
          <button class="close-btn" @click="emit('close')">&times;</button>
        </div>

        <form class="form-body" @submit.prevent="handleSubmit">
          <!-- Name -->
          <div class="form-group">
            <label class="label">Name</label>
            <input
              v-model="name"
              class="input"
              type="text"
              placeholder="My Exchange"
              required
            />
          </div>

          <!-- Type toggle -->
          <div class="form-group">
            <label class="label">Type</label>
            <div class="type-toggle">
              <button
                type="button"
                class="toggle-option"
                :class="{ active: exchangeType === 'cex' }"
                @click="selectType('cex')"
              >
                CEX
              </button>
              <button
                type="button"
                class="toggle-option"
                :class="{ active: exchangeType === 'dex' }"
                @click="selectType('dex')"
              >
                DEX
              </button>
            </div>
          </div>

          <!-- Provider -->
          <div class="form-group">
            <label class="label">Provider</label>
            <select v-model="provider" class="input">
              <option
                v-for="p in providerOptions"
                :key="p"
                :value="p"
              >
                {{ providerLabel(p) }}
              </option>
            </select>
            <p v-if="exchangeType === 'dex'" class="form-note">
              DEX credentials can be stored here, but deploy and pair discovery are not supported yet.
            </p>
            <p v-else class="form-hint">
              Pairs, candles and paper bots use this provider's public market data.
              API keys are optional — they are needed for balances and live trading.
            </p>
          </div>

          <!-- CEX fields -->
          <template v-if="exchangeType === 'cex'">
            <div class="form-group">
              <label class="label">API Key <span class="optional">(optional)</span></label>
              <div class="input-reveal">
                <input
                  v-model="apiKey"
                  class="input"
                  :type="showApiKey ? 'text' : 'password'"
                  :placeholder="isEditing ? 'Leave blank to keep the stored key' : 'Enter API key'"
                  autocomplete="off"
                />
                <button
                  type="button"
                  class="reveal-btn"
                  @click="showApiKey = !showApiKey"
                >
                  {{ showApiKey ? 'Hide' : 'Show' }}
                </button>
              </div>
            </div>

            <div class="form-group">
              <label class="label">API Secret <span class="optional">(optional)</span></label>
              <div class="input-reveal">
                <input
                  v-model="apiSecret"
                  class="input"
                  :type="showApiSecret ? 'text' : 'password'"
                  :placeholder="isEditing ? 'Leave blank to keep the stored secret' : 'Enter API secret'"
                  autocomplete="off"
                />
                <button
                  type="button"
                  class="reveal-btn"
                  @click="showApiSecret = !showApiSecret"
                >
                  {{ showApiSecret ? 'Hide' : 'Show' }}
                </button>
              </div>
            </div>

            <div class="form-group">
              <label class="label">
                Passphrase
                <span class="optional">{{ needsPassphrase ? '(required by this provider)' : '(optional)' }}</span>
              </label>
              <input
                v-model="passphrase"
                class="input"
                type="password"
                placeholder="Exchange passphrase"
                autocomplete="off"
              />
            </div>

            <div v-if="supportsSandbox" class="form-group">
              <label class="checkbox-row">
                <input v-model="sandbox" type="checkbox" />
                <span>Sandbox — {{ sandboxLabel }}</span>
              </label>
              <p class="form-hint">
                Use keys from the sandbox environment. Balances, the connection test and live orders go
                there; market data still comes from the production exchange.
              </p>
            </div>
          </template>

          <!-- DEX fields -->
          <template v-if="exchangeType === 'dex'">
            <div class="form-group">
              <label class="label">Wallet Address</label>
              <input
                v-model="walletAddress"
                class="input"
                type="text"
                placeholder="0x..."
                autocomplete="off"
              />
            </div>

            <div class="form-group">
              <label class="label">Private Key</label>
              <div class="input-reveal">
                <input
                  v-model="privateKey"
                  class="input"
                  :type="showPrivateKey ? 'text' : 'password'"
                  placeholder="Enter private key"
                  autocomplete="off"
                />
                <button
                  type="button"
                  class="reveal-btn"
                  @click="showPrivateKey = !showPrivateKey"
                >
                  {{ showPrivateKey ? 'Hide' : 'Show' }}
                </button>
              </div>
            </div>

            <div class="form-group">
              <label class="label">RPC Endpoint <span class="optional">(optional)</span></label>
              <input
                v-model="rpcEndpoint"
                class="input"
                type="text"
                placeholder="https://..."
                autocomplete="off"
              />
            </div>
          </template>

          <!-- Connection test result -->
          <div
            v-if="isTesting || testResult"
            class="test-result"
            :class="{
              'test-result--ok': testResult?.success,
              'test-result--fail': testResult && !testResult.success,
            }"
          >
            <template v-if="isTesting">Testing connection…</template>
            <template v-else-if="testResult">
              <span class="test-result__icon">{{ testResult.success ? '✓' : '✗' }}</span>
              <span class="test-result__msg">{{ testResult.message }}</span>
              <span v-if="testResult.latency_ms != null" class="test-result__latency mono">
                {{ testResult.latency_ms }} ms
              </span>
            </template>
          </div>

          <!-- Actions -->
          <div class="form-actions">
            <button
              type="button"
              class="btn"
              :disabled="exchangeType === 'dex' || isTesting"
              @click="handleTest"
            >
              {{ isTesting ? 'Testing…' : 'Test Connection' }}
            </button>
            <div class="actions-right">
              <button type="button" class="btn" @click="emit('close')">Cancel</button>
              <button type="submit" class="btn btn-primary">Save</button>
            </div>
          </div>
        </form>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.modal-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.5);
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: center;
}

.modal-content {
  background: var(--qa-bg-card);
  border: 1px solid var(--qa-border);
  border-radius: var(--qa-radius-lg);
  padding: 24px;
  max-width: 500px;
  width: 90%;
  max-height: 85vh;
  overflow-y: auto;
}

.modal-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 20px;
}

.modal-title {
  font-size: 16px;
  font-weight: 600;
  color: var(--qa-text);
}

.close-btn {
  background: none;
  border: none;
  color: var(--qa-text-muted);
  font-size: 22px;
  line-height: 1;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 4px;
  transition: all var(--qa-transition);
}

.close-btn:hover {
  color: var(--qa-text);
  background: var(--qa-bg-hover);
}

.form-body {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.form-group {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.optional {
  font-weight: 400;
  color: var(--qa-text-muted);
  text-transform: none;
  letter-spacing: 0;
}

.form-note {
  margin: 4px 0 0;
  font-size: 12px;
  line-height: 1.4;
  color: var(--qa-warning);
}

.form-hint {
  margin: 4px 0 0;
  font-size: 12px;
  line-height: 1.4;
  color: var(--qa-text-muted);
}

.checkbox-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--qa-text);
  cursor: pointer;
}

.checkbox-row input {
  accent-color: var(--qa-accent);
}

.type-toggle {
  display: flex;
  border: 1px solid var(--qa-border);
  border-radius: var(--qa-radius);
  overflow: hidden;
}

.toggle-option {
  flex: 1;
  padding: 8px;
  font-size: 13px;
  font-weight: 600;
  text-align: center;
  background: var(--qa-bg-input);
  color: var(--qa-text-muted);
  border: none;
  cursor: pointer;
  transition: all var(--qa-transition);
}

.toggle-option:first-child {
  border-right: 1px solid var(--qa-border);
}

.toggle-option.active {
  background: var(--qa-bg-hover);
  color: var(--qa-text);
}

.toggle-option:hover:not(.active) {
  background: var(--qa-bg-hover);
}

.input-reveal {
  position: relative;
  display: flex;
  align-items: center;
}

.input-reveal .input {
  padding-right: 60px;
}

.reveal-btn {
  position: absolute;
  right: 8px;
  background: none;
  border: none;
  color: var(--qa-text-muted);
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.04em;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 3px;
  transition: color var(--qa-transition);
}

.reveal-btn:hover {
  color: var(--qa-text);
}

.test-result {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  font-size: 12px;
  border: 1px solid var(--qa-border);
  border-radius: var(--qa-radius);
  background: var(--qa-bg-hover);
  color: var(--qa-text-secondary);
}

.test-result--ok {
  color: var(--qa-success);
  border-color: color-mix(in srgb, var(--qa-success) 35%, transparent);
}

.test-result--fail {
  color: var(--qa-error);
  border-color: color-mix(in srgb, var(--qa-error) 35%, transparent);
}

.test-result__icon {
  flex-shrink: 0;
}

.test-result__msg {
  flex: 1;
  min-width: 0;
  line-height: 1.4;
}

.test-result__latency {
  flex-shrink: 0;
  color: var(--qa-text-muted);
}

.form-actions {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 8px;
}

.actions-right {
  display: flex;
  gap: 8px;
}
</style>
