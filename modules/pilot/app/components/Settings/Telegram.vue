<script setup lang="ts">
/**
 * Settings → Telegram bot (round 4, card 2): talk to an agent from Telegram.
 * Off by default. The token goes to the vault and is never shown again; the
 * backend only answers one private chat, linked with a one-time code.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { pilot } from '#pilot/utils/invoke'

interface Pairing { code: string; secondsLeft: number; link: string | null; qr: string | null }
interface Status {
  enabled: boolean
  running: boolean
  error: string | null
  hasToken: boolean
  hasPin: boolean
  botName: string | null
  linkedTo: string | null
  pairing: Pairing | null
}

const status = ref<Status | null>(null)
const notice = ref('')
const busy = ref(false)
const token = ref('')
const pin = ref('')
let timer: ReturnType<typeof setInterval> | null = null

const leftText = computed(() => {
  const s = status.value?.pairing?.secondsLeft ?? 0
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`
})

async function run(command: string, args: Record<string, unknown> = {}) {
  busy.value = true
  notice.value = ''
  try {
    const next = await pilot<Status>(`plugin:pilot|${command}`, args)
    if (next) status.value = next
  } catch (e) {
    notice.value = String(e)
  } finally {
    busy.value = false
  }
}

const load = () => run('pilot_telegram_status')
async function saveToken() {
  await run('pilot_telegram_set_token', { token: token.value })
  if (!notice.value) token.value = ''
}
async function setPin() {
  busy.value = true
  notice.value = ''
  try {
    await pilot('plugin:pilot|pilot_phone_set_pin', { pin: pin.value })
    pin.value = ''
  } catch (e) {
    notice.value = String(e)
  } finally {
    busy.value = false
  }
  await load()
}
const turn = (enabled: boolean) => run('pilot_telegram_set', { enabled })
const pair = () => run('pilot_telegram_pair')
const cancelPair = () => run('pilot_telegram_pair_cancel')
const unlink = () => run('pilot_telegram_unlink')
const forget = () => run('pilot_telegram_forget_token')

/** The code's countdown and the chat showing up, while the page is open. */
watch(
  () => status.value?.pairing?.code,
  (code) => {
    if (timer) clearInterval(timer)
    timer = null
    if (code) {
      timer = setInterval(async () => {
        const next = await pilot<Status>('plugin:pilot|pilot_telegram_status')
        if (next) status.value = next
      }, 3000)
    }
  },
)

onMounted(load)
onBeforeUnmount(() => {
  if (timer) clearInterval(timer)
})
</script>

<template>
  <div class="qp-settings">
    <p class="qp-settings-desc">
      Message your own Telegram bot and an agent starts working. It alerts you when an agent waits for you, with buttons to answer, and sends the final answer back. It reads from Telegram outward, so nothing on this PC is opened to the internet.
    </p>
    <p class="qp-settings-desc qp-warn">
      Your messages and what the agent answers pass through Telegram's servers and are not end-to-end encrypted, and a question that asks for a secret is left for the PC. Leave this off if your code must not go there.
    </p>

    <div v-if="status" class="qp-gh" :class="status.running && !status.error ? 'ok' : status.error ? 'bad' : ''">
      <span class="qp-dot" />
      <span v-if="status.error">{{ status.error }}</span>
      <span v-else-if="status.running">On<template v-if="status.botName"> as <code>@{{ status.botName }}</code></template></span>
      <span v-else>Off</span>
      <button v-if="status.running" class="qp-btn" :disabled="busy" @click="turn(false)">Turn off</button>
    </div>

    <template v-if="status">
      <h3 class="qp-settings-title">1. Your bot</h3>
      <p class="qp-settings-desc">
        In Telegram, message <code>@BotFather</code>, send <code>/newbot</code> and follow its questions. Paste the token it gives you here. It goes to the vault and is never shown again.
      </p>
      <div class="qp-row">
        <input v-model="token" class="qp-input qp-grow" type="password" autocomplete="off" :placeholder="status.hasToken ? 'A token is saved. Paste another to replace it.' : '123456789:AAE...'" aria-label="Bot token" @keydown.enter="saveToken">
        <button class="qp-btn" :disabled="busy || !token" @click="saveToken">Save token</button>
        <span v-if="status.hasToken" class="qp-ok">Saved</span>
      </div>

      <h3 class="qp-settings-title">2. A PIN</h3>
      <p class="qp-settings-desc">
        Anything that makes an agent act needs it: send <code>/unlock</code> and the PIN, and the bot deletes that message. It stays valid for 10 minutes. It is the same PIN as phone access.
      </p>
      <div class="qp-row">
        <input v-model="pin" class="qp-input" type="password" inputmode="numeric" autocomplete="off" placeholder="6 to 12 digits" aria-label="PIN" @keydown.enter="setPin">
        <button class="qp-btn" :disabled="busy || !pin" @click="setPin">{{ status.hasPin ? 'Change PIN' : 'Set PIN' }}</button>
        <span v-if="status.hasPin" class="qp-ok">Set</span>
      </div>

      <div v-if="!status.running" class="qp-row">
        <button class="qp-btn qp-primary" :disabled="busy || !status.hasToken || !status.hasPin" @click="turn(true)">Turn on</button>
        <span v-if="!status.hasToken || !status.hasPin" class="qp-settings-notice">It needs the token and the PIN first.</span>
      </div>

      <template v-if="status.running">
        <h3 class="qp-settings-title">3. Link your chat</h3>
        <div v-if="status.linkedTo" class="qp-gh ok">
          <span class="qp-dot" />
          <span>Linked to {{ status.linkedTo }}. Every other chat is ignored.</span>
          <button class="qp-btn" :disabled="busy" @click="unlink">Unlink</button>
        </div>
        <div v-else-if="status.pairing" class="qp-pair">
          <!-- eslint-disable-next-line vue/no-v-html -- the QR SVG comes from our own backend and holds no outside text -->
          <div v-if="status.pairing.qr" class="qp-qr" aria-label="QR code that opens the bot" v-html="status.pairing.qr" />
          <div class="qp-pair-text">
            <p class="qp-settings-desc">Scan this with your phone, or send your bot:</p>
            <code class="qp-code">/start {{ status.pairing.code }}</code>
            <p class="qp-settings-desc">Valid for {{ leftText }}, and works once.</p>
            <button class="qp-btn" :disabled="busy" @click="cancelPair">Cancel</button>
          </div>
        </div>
        <button v-else class="qp-btn qp-self" :disabled="busy" @click="pair">Link a chat</button>
      </template>

      <template v-if="status.hasToken">
        <button class="qp-btn qp-self qp-danger" :disabled="busy" @click="forget">Turn off and delete the token</button>
      </template>
    </template>

    <p v-if="notice" class="qp-settings-notice qp-bad">{{ notice }}</p>
  </div>
</template>

<style scoped>
.qp-settings { display: flex; flex-direction: column; gap: 10px; font-size: 12.5px; color: var(--qss-text, #e8e8ec); }
.qp-settings-title { margin: 6px 0 0; font-size: 13px; font-weight: 600; }
.qp-settings-desc { margin: 0; color: var(--qss-text-secondary, #9a9aa5); line-height: 1.45; }
.qp-settings-desc code, .qp-gh code { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 12px; }
.qp-settings-notice { color: var(--qss-text-secondary, #9a9aa5); font-size: 12px; }
.qp-warn { color: var(--qss-warning, #fbbf24); }
.qp-gh { display: flex; align-items: center; gap: 8px; padding: 8px 10px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; }
.qp-gh > span:nth-child(2) { flex: 1; min-width: 0; overflow-wrap: anywhere; }
.qp-dot { flex: none; width: 7px; height: 7px; border-radius: 50%; background: var(--qss-border, #47474f); }
.qp-gh.ok .qp-dot { background: var(--qss-success, #34d399); }
.qp-gh.bad .qp-dot { background: var(--qss-error, #f87171); }
.qp-row { display: flex; align-items: center; gap: 8px; }
.qp-grow { flex: 1; min-width: 0; }
.qp-input { background: var(--qss-bg, #18181e); border: 1px solid var(--qss-border, #47474f); border-radius: 6px; padding: 6px 8px; color: inherit; font: inherit; outline: none; }
.qp-input:focus { border-color: var(--qss-accent, #a0a0a8); }
.qp-input::placeholder { color: var(--qss-text-muted, #6e6e7a); }
.qp-btn { flex: none; padding: 5px 10px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: none; color: inherit; font: inherit; cursor: pointer; }
.qp-btn:hover:not(:disabled) { background: var(--qss-bg-hover, rgba(255, 255, 255, 0.06)); }
.qp-btn:disabled { opacity: 0.5; cursor: default; }
.qp-primary { background: var(--qss-text, #e8e8ec); border-color: var(--qss-text, #e8e8ec); color: var(--qss-bg, #18181e); font-weight: 600; }
.qp-primary:hover:not(:disabled) { background: var(--qss-text, #e8e8ec); }
.qp-danger { color: var(--qss-error, #f87171); }
.qp-self { align-self: flex-start; }
.qp-ok { color: var(--qss-success, #34d399); }
.qp-bad { color: var(--qss-error, #f87171); }
.qp-pair { display: flex; gap: 16px; padding: 12px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; }
.qp-qr { flex: none; width: 168px; height: 168px; padding: 8px; border-radius: 6px; background: #fff; }
.qp-qr :deep(svg) { display: block; width: 100%; height: 100%; }
.qp-pair-text { display: flex; flex-direction: column; align-items: flex-start; gap: 8px; min-width: 0; }
.qp-code { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 18px; font-weight: 600; font-variant-numeric: tabular-nums; }
</style>
