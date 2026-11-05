<script setup lang="ts">
/**
 * The server PIN prompt (PLAN-SERVER-CLIENT §4.1 layer 6, card Server 5/7).
 * The seam (app/seam/core.ts) asks for it with a `qs:server-pin` event when
 * the server answers an `external` call — arming live execution, starting a
 * live bot, closing positions, changing a key — with "step-up required". The
 * PIN goes to the server once and stays valid there for 10 minutes.
 */
interface Ask {
  message: string
  resolve: (pin: string | null) => void
}

const ask = ref<Ask | null>(null)
const pin = ref('')
const input = ref<HTMLInputElement | null>(null)

function onAsk(e: Event) {
  const detail = (e as CustomEvent<Ask>).detail
  if (!detail?.resolve) return
  e.preventDefault()
  // One prompt at a time: a second request waits for no PIN.
  if (ask.value) return detail.resolve(null)
  pin.value = ''
  ask.value = detail
  void nextTick(() => input.value?.focus())
}

function close(value: string | null) {
  ask.value?.resolve(value)
  ask.value = null
  pin.value = ''
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'Escape') close(null)
}

onMounted(() => window.addEventListener('qs:server-pin', onAsk))
onUnmounted(() => {
  window.removeEventListener('qs:server-pin', onAsk)
  close(null)
})
</script>

<template>
  <div v-if="ask" class="qsp-overlay" @mousedown.self="close(null)" @keydown="onKey">
    <form class="qsp-box" @submit.prevent="close(pin)">
      <p class="qsp-title">Server PIN</p>
      <p class="qsp-hint">
        This changes trading or keys on the server. Enter the server's PIN; it stays valid on this PC for 10 minutes.
      </p>
      <input
        ref="input"
        v-model="pin"
        class="qsp-input"
        type="password"
        inputmode="numeric"
        autocomplete="off"
        placeholder="6–12 digits"
      >
      <div class="qsp-actions">
        <button type="button" class="qsu-btn" @click="close(null)">Cancel</button>
        <button type="submit" class="qsu-btn qsu-btn--primary" :disabled="pin.length < 6">Continue</button>
      </div>
    </form>
  </div>
</template>

<style scoped>
.qsp-overlay { position: fixed; inset: 0; z-index: 1000; display: flex; align-items: center; justify-content: center; background: rgb(0 0 0 / 0.45); }
.qsp-box { width: 360px; padding: 18px 20px; border-radius: 12px; border: 1px solid var(--qss-border); background: var(--qss-bg-card); color: var(--qss-text); display: flex; flex-direction: column; gap: 10px; }
.qsp-title { margin: 0; font-size: 14px; font-weight: 600; }
.qsp-hint { margin: 0; font-size: 12px; color: var(--qss-text-secondary); }
.qsp-input { padding: 8px 10px; border-radius: 8px; border: 1px solid var(--qss-border); background: var(--qss-bg); color: var(--qss-text); font: inherit; font-size: 14px; letter-spacing: 0.15em; }
.qsp-actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
