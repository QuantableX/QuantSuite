<script setup lang="ts">
/**
 * Exposure per venue (PLAN-EXECUTION-VENUES §10): what every bot and Manual
 * system holds on each account, from their own books — spreading over
 * venues is the protection against one venue failing.
 */
import { invoke } from '@tauri-apps/api/core'

interface Exposure { exchange_id: string; name: string; holdings: [string, string, number, number | null][]; cost_basis: number }

const props = withDefaults(defineProps<{ compact?: boolean }>(), { compact: false })
const rows = ref<Exposure[]>([])
const error = ref<string | null>(null)

async function load() {
  try {
    rows.value = await invoke<Exposure[]>('plugin:algo|venue_exposure')
  } catch (e) {
    error.value = String(e)
  }
}

function qty(n: number) {
  return n.toLocaleString('en-US', { maximumFractionDigits: n < 1 ? 6 : 2 })
}

onMounted(load)
onActivated(load)
defineExpose({ load })
</script>

<template>
  <div class="exposure">
    <p v-if="error" class="text-error exposure__note">{{ error }}</p>
    <p v-else-if="!rows.length" class="exposure__note">Nothing held by bots or Manual systems yet.</p>
    <div v-for="r in rows" :key="r.exchange_id" class="exposure__venue">
      <div class="exposure__head">
        <span class="exposure__name">{{ r.name }}</span>
        <span class="exposure__count">{{ r.holdings.length }} holding{{ r.holdings.length === 1 ? '' : 's' }}</span>
      </div>
      <ul v-if="!props.compact" class="exposure__list">
        <li v-for="(h, i) in r.holdings" :key="i">
          <span class="mono">{{ qty(h[2]) }} {{ h[1] }}</span>
          <span class="exposure__owner">{{ h[0] }}</span>
        </li>
      </ul>
    </div>
  </div>
</template>

<style scoped>
.exposure { display: flex; flex-direction: column; gap: 8px; }
.exposure__note { font-size: 12px; color: var(--qa-text-muted); }
.exposure__venue { display: flex; flex-direction: column; gap: 4px; }
.exposure__head { display: flex; justify-content: space-between; font-size: 12px; }
.exposure__name { font-weight: 600; color: var(--qa-text); }
.exposure__count { color: var(--qa-text-muted); }
.exposure__list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; font-size: 12px; }
.exposure__list li { display: flex; justify-content: space-between; gap: 8px; }
.exposure__owner { color: var(--qa-text-muted); }
</style>
