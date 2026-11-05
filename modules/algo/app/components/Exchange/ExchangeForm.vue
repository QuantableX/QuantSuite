<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
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
const DEX_PROVIDERS: ExchangeProvider[] = ['uniswap', 'jupiter', 'hyperliquid', 'lighter']

const name = ref('')
const exchangeType = ref<ExchangeType>('cex')
const provider = ref<ExchangeProvider>('binance')
const apiKey = ref('')
const apiSecret = ref('')
const passphrase = ref('')
const walletAddress = ref('')
const privateKey = ref('')
const rpcEndpoint = ref('')
const subAccount = ref('')
const maxValue = ref<number | null>(null)
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
/** Hyperliquid: an API wallet that trades for your main address but cannot withdraw (PLAN-EXECUTION-VENUES §8). */
const isHyperliquid = computed(() => exchangeType.value === 'dex' && provider.value === 'hyperliquid')
/** Lighter (Venues 5/5): an API key that trades for an account index; withdrawals only to the account's own L1 address. */
const isLighter = computed(() => exchangeType.value === 'dex' && provider.value === 'lighter')
const accountIndex = ref('')
const apiKeyIndex = ref('')
/** Jupiter: a Solana wallet QuantSuite generates; never an imported key (§8). */
const isJupiter = computed(() => exchangeType.value === 'dex' && provider.value === 'jupiter')

/** The generated wallet's key, shown once after a confirmed export. */
const exportStep = ref<'idle' | 'confirm' | 'shown'>('idle')
const exportConfirm = ref('')
const exportedKey = ref('')
const exportError = ref<string | null>(null)
const solanaAddress = ref('')

async function loadWallet() {
  solanaAddress.value = ''
  if (!props.exchange || props.exchange.provider !== 'jupiter') return
  try {
    const w = await invoke<{ address: string }>('plugin:algo|solana_wallet_status', { exchange_id: props.exchange.id })
    solanaAddress.value = w.address
  } catch {
    // The address is optional here; the list shows the error.
  }
}

async function exportKey() {
  if (!props.exchange) return
  exportError.value = null
  try {
    const r = await invoke<{ address: string; secret: string }>('plugin:algo|wallet_export_key', {
      exchange_id: props.exchange.id,
      confirm: exportConfirm.value.trim(),
    })
    exportedKey.value = r.secret
    exportStep.value = 'shown'
  } catch (e) {
    exportError.value = String(e)
  }
}
/** Binance has a spot testnet, Bybit a demo-trading environment, Hyperliquid a testnet; the other venues have none. */
const supportsSandbox = computed(
  () => (exchangeType.value === 'cex' && (provider.value === 'binance' || provider.value === 'bybit')) || isHyperliquid.value || isLighter.value,
)
const sandboxLabel = computed(() =>
  provider.value === 'binance' ? 'Binance spot testnet' : provider.value === 'bybit' ? 'Bybit demo trading' : provider.value === 'lighter' ? 'Lighter testnet' : 'Hyperliquid testnet',
)

function capitalize(s: string): string {
  return s.charAt(0).toUpperCase() + s.slice(1)
}

function providerLabel(p: ExchangeProvider): string {
  const name = capitalize(p)
  return exchangeType.value === 'dex' && !['hyperliquid', 'jupiter', 'lighter'].includes(p) ? `${name} (stored credentials only)` : name
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
  subAccount.value = ''
  accountIndex.value = ''
  apiKeyIndex.value = ''
  maxValue.value = null
  exportStep.value = 'idle'
  exportConfirm.value = ''
  exportedKey.value = ''
  exportError.value = null
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
    exportStep.value = 'idle'
    exportConfirm.value = ''
    exportedKey.value = ''
    exportError.value = null
    loadWallet()
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
    if (walletAddress.value) config.wallet_address = walletAddress.value.trim()
    if (privateKey.value) config.private_key = privateKey.value.trim()
    if (isHyperliquid.value) {
      if (subAccount.value) config.sub_account = subAccount.value.trim()
      config.sandbox = sandbox.value
    } else if (rpcEndpoint.value) {
      config.rpc_endpoint = rpcEndpoint.value
    }
    if (isLighter.value) {
      // Account index and API key index travel in sub_account / api_key.
      delete config.wallet_address
      if (accountIndex.value) config.sub_account = accountIndex.value.trim()
      if (apiKeyIndex.value) config.api_key = apiKeyIndex.value.trim()
      config.sandbox = sandbox.value
    }
    if (isJupiter.value) {
      // The wallet is generated server-side; only settings travel.
      delete config.wallet_address
      delete config.private_key
      if (apiKey.value) config.api_key = apiKey.value.trim()
      if (maxValue.value != null && maxValue.value > 0) config.max_value = maxValue.value
    }
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
    <div v-if="visible !== false" class="modal-backdrop" data-module="algo" @click="onBackdropClick">
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
            <div v-if="isHyperliquid" class="form-hint hl-guide">
              <p>
                QuantSuite trades through an <strong>API wallet</strong>: it can place and cancel orders for your
                account but cannot withdraw or transfer. Your main wallet's key or seed phrase is never needed.
              </p>
              <ol class="guide">
                <li>In Hyperliquid's app open <em>More → API</em> and create an API wallet (one per sub-account; never reuse an old one).</li>
                <li>Approve it with your main wallet. Copy its private key — Hyperliquid shows it once.</li>
                <li>Paste the key and your main address here. A sub-account address is optional.</li>
              </ol>
              <p>
                Spot and perp hold separate USDC. Moving USDC between them is signed by your main wallet, so you do
                that in Hyperliquid's app.
              </p>
            </div>
            <div v-else-if="isLighter" class="form-hint hl-guide">
              <p>
                QuantSuite trades through a Lighter <strong>API key</strong>. It places and cancels orders; it can also
                withdraw, but only to your account's own L1 address. Sending funds anywhere else needs your wallet's key,
                which QuantSuite never asks for.
              </p>
              <ol class="guide">
                <li>In Lighter's app open <em>Settings → API keys</em> and create a key on an index from 2 to 254 (one per system or bot; 0 and 1 are Lighter's own).</li>
                <li>Copy the key's private key and note your account index (a sub-account has its own).</li>
                <li>Enter both indexes and the private key here.</li>
              </ol>
              <p>Standard Lighter accounts trade without fees. Perp positions use isolated margin, 1x unless you choose more.</p>
            </div>
            <div v-else-if="isJupiter" class="form-hint hl-guide">
              <p>
                QuantSuite <strong>generates a new Solana wallet</strong> when you save and shows its address. Fund it with
                the trading capital only: its key can do everything, so it never holds more than the cap below. There is
                no import — never enter a seed phrase anywhere.
              </p>
              <p>Swaps go through Jupiter with a slippage and price-impact cap; a small SOL balance pays the network fees.</p>
            </div>
            <p v-else-if="exchangeType === 'dex'" class="form-note">
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
          <template v-if="isJupiter">
            <div v-if="isEditing" class="form-group">
              <label class="label">Wallet address</label>
              <input class="input mono" type="text" :value="solanaAddress || '…'" readonly />
            </div>
            <div class="form-group">
              <label class="label">Max value on this wallet (USD)</label>
              <input
                v-model.number="maxValue"
                class="input"
                type="number"
                min="0"
                step="any"
                :placeholder="isEditing ? 'Leave blank to keep the stored cap' : 'e.g. 1000 — arming is refused above it'"
              />
            </div>
            <div class="form-group">
              <label class="label">RPC URL <span class="optional">(optional)</span></label>
              <input
                v-model="rpcEndpoint"
                class="input"
                type="text"
                :placeholder="isEditing ? 'Leave blank to keep the stored RPC' : 'https://… — blank: the public Solana RPC (rate-limited)'"
                autocomplete="off"
              />
            </div>
            <div class="form-group">
              <label class="label">Jupiter API key <span class="optional">(optional — keyless allows one request every 2 seconds)</span></label>
              <input v-model="apiKey" class="input" type="password" :placeholder="isEditing ? 'Leave blank to keep the stored key' : 'From portal.jup.ag'" autocomplete="off" />
            </div>
            <div v-if="isEditing" class="form-group">
              <label class="label">Wallet key</label>
              <button v-if="exportStep === 'idle'" type="button" class="btn" @click="exportStep = 'confirm'">Export the key…</button>
              <div v-else-if="exportStep === 'confirm'" class="export-box">
                <p class="form-note">Anyone with this key can move everything on the wallet. Type the wallet address to show it.</p>
                <input v-model="exportConfirm" class="input mono" type="text" placeholder="Wallet address" autocomplete="off" />
                <div class="export-actions">
                  <button type="button" class="btn" @click="exportStep = 'idle'">Cancel</button>
                  <button type="button" class="btn btn-primary" :disabled="exportConfirm.trim() !== solanaAddress" @click="exportKey">Show the key</button>
                </div>
                <p v-if="exportError" class="form-note">{{ exportError }}</p>
              </div>
              <div v-else class="export-box">
                <input class="input mono" type="text" :value="exportedKey" readonly />
                <p class="form-hint">Phantom and the Solana CLI import this format. Close the form when you are done.</p>
              </div>
            </div>
          </template>
          <template v-else-if="isLighter">
            <div class="form-group">
              <label class="label">Account index</label>
              <input v-model="accountIndex" class="input mono" type="text" inputmode="numeric" :placeholder="isEditing ? 'Leave blank to keep the stored index' : 'e.g. 281474976710654'" autocomplete="off" />
            </div>
            <div class="form-group">
              <label class="label">API key index</label>
              <input v-model="apiKeyIndex" class="input mono" type="text" inputmode="numeric" :placeholder="isEditing ? 'Leave blank to keep the stored index' : '2 – 254'" autocomplete="off" />
            </div>
            <div class="form-group">
              <label class="label">API key private key</label>
              <div class="input-reveal">
                <input
                  v-model="privateKey"
                  class="input"
                  :type="showPrivateKey ? 'text' : 'password'"
                  :placeholder="isEditing ? 'Leave blank to keep the stored key' : '0x… (40 bytes)'"
                  autocomplete="off"
                />
                <button type="button" class="reveal-btn" @click="showPrivateKey = !showPrivateKey">{{ showPrivateKey ? 'Hide' : 'Show' }}</button>
              </div>
            </div>
            <div class="form-group">
              <label class="checkbox-row">
                <input v-model="sandbox" type="checkbox" />
                <span>Testnet — {{ sandboxLabel }}</span>
              </label>
              <p class="form-hint">An API key on the testnet (testnet.app.lighter.xyz). Balances, prices and orders all come from the testnet.</p>
            </div>
          </template>
          <template v-else-if="exchangeType === 'dex'">
            <div class="form-group">
              <label class="label">{{ isHyperliquid ? 'Main wallet address' : 'Wallet Address' }}</label>
              <input
                v-model="walletAddress"
                class="input"
                type="text"
                :placeholder="isEditing ? 'Leave blank to keep the stored address' : '0x...'"
                autocomplete="off"
              />
            </div>

            <div class="form-group">
              <label class="label">{{ isHyperliquid ? 'API wallet private key' : 'Private Key' }}</label>
              <div class="input-reveal">
                <input
                  v-model="privateKey"
                  class="input"
                  :type="showPrivateKey ? 'text' : 'password'"
                  :placeholder="isEditing ? 'Leave blank to keep the stored key' : 'Enter private key'"
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

            <div v-if="isHyperliquid" class="form-group">
              <label class="label">Sub-account address <span class="optional">(optional)</span></label>
              <input
                v-model="subAccount"
                class="input"
                type="text"
                :placeholder="isEditing ? 'Leave blank to keep the stored sub-account' : '0x... — orders act on this sub-account'"
                autocomplete="off"
              />
            </div>

            <div v-if="isHyperliquid" class="form-group">
              <label class="checkbox-row">
                <input v-model="sandbox" type="checkbox" />
                <span>Testnet — {{ sandboxLabel }}</span>
              </label>
              <p class="form-hint">
                An API wallet approved on the testnet. Balances, prices and orders all come from the testnet.
              </p>
            </div>

            <div v-else class="form-group">
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
              :disabled="(exchangeType === 'dex' && !isHyperliquid && !isLighter && !(isJupiter && isEditing)) || isTesting"
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

.export-box {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.export-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

.hl-guide p {
  margin: 0 0 6px;
}

.hl-guide .guide {
  margin: 0 0 6px;
  padding-left: 18px;
}

.hl-guide .guide li {
  margin-bottom: 2px;
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
