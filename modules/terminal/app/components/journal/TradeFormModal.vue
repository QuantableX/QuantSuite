<script setup lang="ts">
import { X, Star, Plus, Minus } from 'lucide-vue-next'
import type { JournalTrade } from '#terminal/stores/journal'

const props = defineProps<{
  trade?: JournalTrade | null
  closeOnly?: boolean
}>()

const emit = defineEmits<{
  close: []
  save: [data: any]
  closeTrade: [exitPrice: number, fees: number]
}>()

const COMMON_PAIRS = ['BTC/USDT', 'ETH/USDT', 'SOL/USDT', 'BNB/USDT', 'XRP/USDT', 'ADA/USDT', 'AVAX/USDT', 'DOGE/USDT', 'DOT/USDT', 'MATIC/USDT', 'LINK/USDT', 'ARB/USDT']
const STRATEGIES = ['Breakout', 'Mean Reversion', 'Trend Following', 'Scalp', 'Swing', 'DCA', 'Momentum']
const EXCHANGES = ['Binance', 'Bybit', 'OKX', 'Coinbase', 'Kraken', 'Bitget']
const EMOTIONS: Array<{ value: JournalTrade['emotion']; label: string; emoji: string }> = [
  { value: 'confident', label: 'Confident', emoji: '😎' },
  { value: 'neutral', label: 'Neutral', emoji: '😐' },
  { value: 'fearful', label: 'Fearful', emoji: '😰' },
  { value: 'greedy', label: 'Greedy', emoji: '🤑' },
  { value: 'frustrated', label: 'Frustrated', emoji: '😤' },
]

const form = reactive({
  pair: props.trade?.pair ?? 'BTC/USDT',
  side: props.trade?.side ?? 'long' as 'long' | 'short',
  entry: props.trade?.entry ?? null as number | null,
  exit: props.trade?.exit ?? null as number | null,
  size: props.trade?.size ?? null as number | null,
  fees: props.trade?.fees ?? 0,
  strategy: props.trade?.strategy ?? '',
  exchange: props.trade?.exchange ?? 'Binance',
  notes: props.trade?.notes ?? '',
  tags: [...(props.trade?.tags ?? [])],
  emotion: props.trade?.emotion ?? 'neutral' as JournalTrade['emotion'],
  rating: props.trade?.rating ?? 3,
  date: props.trade?.date ?? new Date().toISOString().slice(0, 10),
  status: props.trade?.status ?? 'open' as 'open' | 'closed',
})

const closeExitPrice = ref<number | null>(null)
const closeExtraFees = ref(0)
const tagInput = ref('')
const errors = ref<Record<string, string>>({})

function addTag() {
  const tag = tagInput.value.trim().toLowerCase()
  if (tag && !form.tags.includes(tag)) {
    form.tags.push(tag)
  }
  tagInput.value = ''
}

function removeTag(tag: string) {
  form.tags = form.tags.filter(t => t !== tag)
}

function validate(): boolean {
  errors.value = {}
  if (!form.pair) errors.value.pair = 'Required'
  if (!form.entry || form.entry <= 0) errors.value.entry = 'Valid price required'
  if (!form.size || form.size <= 0) errors.value.size = 'Valid size required'
  if (form.exit !== null && form.exit <= 0) errors.value.exit = 'Valid price required'
  return Object.keys(errors.value).length === 0
}

function validateClose(): boolean {
  errors.value = {}
  if (!closeExitPrice.value || closeExitPrice.value <= 0) errors.value.closeExit = 'Valid exit price required'
  return Object.keys(errors.value).length === 0
}

function handleSave() {
  if (!validate()) return
  const status = form.exit !== null ? 'closed' : 'open'
  emit('save', { ...form, status })
}

function handleClose() {
  if (!validateClose()) return
  emit('closeTrade', closeExitPrice.value!, closeExtraFees.value)
}

function onOverlayClick(e: MouseEvent) {
  if ((e.target as HTMLElement).classList.contains('modal-overlay')) {
    emit('close')
  }
}
</script>

<template>
  <div class="modal-overlay fixed inset-0 z-50 flex items-center justify-center p-4" style="background-color: rgba(0,0,0,0.6); backdrop-filter: blur(4px)" @mousedown="onOverlayClick">
    <div class="w-full max-w-lg max-h-[90vh] overflow-y-auto rounded-lg" style="background-color: var(--surface-1); border: 1px solid var(--border)">
      <!-- Header -->
      <div class="flex items-center justify-between px-4 py-3" style="border-bottom: 1px solid var(--border)">
        <h3 class="text-sm font-bold" style="color: var(--text-primary)">
          {{ closeOnly ? 'Close Trade' : (trade ? 'Edit Trade' : 'Log New Trade') }}
        </h3>
        <button class="p-1 rounded hover:brightness-125" style="color: var(--muted)" @click="emit('close')">
          <X class="w-4 h-4" />
        </button>
      </div>

      <!-- Close Trade Mode -->
      <div v-if="closeOnly" class="p-4 space-y-4">
        <div class="card p-3">
          <div class="text-xs" style="color: var(--text-secondary)">
            {{ trade?.pair }} · {{ trade?.side === 'long' ? 'Long' : 'Short' }} · Entry ${{ trade?.entry?.toLocaleString() }}
          </div>
        </div>
        <div>
          <label class="text-[11px] font-medium mb-1 block" style="color: var(--text-secondary)">Exit Price</label>
          <input v-model.number="closeExitPrice" type="number" step="any" placeholder="0.00" class="w-full px-3 py-2 rounded text-xs font-mono" style="background-color: var(--surface-2); border: 1px solid var(--border); color: var(--text-primary); outline: none" />
          <span v-if="errors.closeExit" class="text-[10px]" style="color: var(--negative)">{{ errors.closeExit }}</span>
        </div>
        <div>
          <label class="text-[11px] font-medium mb-1 block" style="color: var(--text-secondary)">Additional Fees</label>
          <input v-model.number="closeExtraFees" type="number" step="any" placeholder="0.00" class="w-full px-3 py-2 rounded text-xs font-mono" style="background-color: var(--surface-2); border: 1px solid var(--border); color: var(--text-primary); outline: none" />
        </div>
        <div class="flex gap-2 justify-end pt-2">
          <button class="px-4 py-2 rounded text-xs font-medium" style="background-color: var(--surface-2); color: var(--text-secondary)" @click="emit('close')">Cancel</button>
          <button class="px-4 py-2 rounded text-xs font-medium" style="background-color: var(--accent); color: #fff" @click="handleClose">Close Trade</button>
        </div>
      </div>

      <!-- Full Form -->
      <div v-else class="p-4 space-y-4">
        <!-- Row: Pair + Date -->
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="text-[11px] font-medium mb-1 block" style="color: var(--text-secondary)">Pair</label>
            <select v-model="form.pair" class="w-full px-3 py-2 rounded text-xs" style="background-color: var(--surface-2); border: 1px solid var(--border); color: var(--text-primary); outline: none">
              <option v-for="p in COMMON_PAIRS" :key="p" :value="p">{{ p }}</option>
            </select>
            <span v-if="errors.pair" class="text-[10px]" style="color: var(--negative)">{{ errors.pair }}</span>
          </div>
          <div>
            <label class="text-[11px] font-medium mb-1 block" style="color: var(--text-secondary)">Date</label>
            <input v-model="form.date" type="date" class="w-full px-3 py-2 rounded text-xs" style="background-color: var(--surface-2); border: 1px solid var(--border); color: var(--text-primary); outline: none" />
          </div>
        </div>

        <!-- Side Toggle -->
        <div>
          <label class="text-[11px] font-medium mb-1 block" style="color: var(--text-secondary)">Side</label>
          <div class="flex gap-2">
            <button
              class="flex-1 py-2 rounded text-xs font-semibold transition-all"
              :style="{
                backgroundColor: form.side === 'long' ? 'var(--positive)' : 'var(--surface-2)',
                color: form.side === 'long' ? '#fff' : 'var(--text-secondary)',
              }"
              @click="form.side = 'long'"
            >Long</button>
            <button
              class="flex-1 py-2 rounded text-xs font-semibold transition-all"
              :style="{
                backgroundColor: form.side === 'short' ? 'var(--negative)' : 'var(--surface-2)',
                color: form.side === 'short' ? '#fff' : 'var(--text-secondary)',
              }"
              @click="form.side = 'short'"
            >Short</button>
          </div>
        </div>

        <!-- Row: Entry + Exit + Size -->
        <div class="grid grid-cols-3 gap-3">
          <div>
            <label class="text-[11px] font-medium mb-1 block" style="color: var(--text-secondary)">Entry Price</label>
            <input v-model.number="form.entry" type="number" step="any" placeholder="0.00" class="w-full px-3 py-2 rounded text-xs font-mono" style="background-color: var(--surface-2); border: 1px solid var(--border); color: var(--text-primary); outline: none" />
            <span v-if="errors.entry" class="text-[10px]" style="color: var(--negative)">{{ errors.entry }}</span>
          </div>
          <div>
            <label class="text-[11px] font-medium mb-1 block" style="color: var(--text-secondary)">Exit Price</label>
            <input v-model.number="form.exit" type="number" step="any" placeholder="Open" class="w-full px-3 py-2 rounded text-xs font-mono" style="background-color: var(--surface-2); border: 1px solid var(--border); color: var(--text-primary); outline: none" />
            <span v-if="errors.exit" class="text-[10px]" style="color: var(--negative)">{{ errors.exit }}</span>
          </div>
          <div>
            <label class="text-[11px] font-medium mb-1 block" style="color: var(--text-secondary)">Size</label>
            <input v-model.number="form.size" type="number" step="any" placeholder="0" class="w-full px-3 py-2 rounded text-xs font-mono" style="background-color: var(--surface-2); border: 1px solid var(--border); color: var(--text-primary); outline: none" />
            <span v-if="errors.size" class="text-[10px]" style="color: var(--negative)">{{ errors.size }}</span>
          </div>
        </div>

        <!-- Row: Fees + Strategy + Exchange -->
        <div class="grid grid-cols-3 gap-3">
          <div>
            <label class="text-[11px] font-medium mb-1 block" style="color: var(--text-secondary)">Fees</label>
            <input v-model.number="form.fees" type="number" step="any" placeholder="0.00" class="w-full px-3 py-2 rounded text-xs font-mono" style="background-color: var(--surface-2); border: 1px solid var(--border); color: var(--text-primary); outline: none" />
          </div>
          <div>
            <label class="text-[11px] font-medium mb-1 block" style="color: var(--text-secondary)">Strategy</label>
            <select v-model="form.strategy" class="w-full px-3 py-2 rounded text-xs" style="background-color: var(--surface-2); border: 1px solid var(--border); color: var(--text-primary); outline: none">
              <option value="">None</option>
              <option v-for="s in STRATEGIES" :key="s" :value="s">{{ s }}</option>
            </select>
          </div>
          <div>
            <label class="text-[11px] font-medium mb-1 block" style="color: var(--text-secondary)">Exchange</label>
            <select v-model="form.exchange" class="w-full px-3 py-2 rounded text-xs" style="background-color: var(--surface-2); border: 1px solid var(--border); color: var(--text-primary); outline: none">
              <option v-for="e in EXCHANGES" :key="e" :value="e">{{ e }}</option>
            </select>
          </div>
        </div>

        <!-- Emotion -->
        <div>
          <label class="text-[11px] font-medium mb-1 block" style="color: var(--text-secondary)">Emotion</label>
          <div class="flex gap-2">
            <button
              v-for="em in EMOTIONS"
              :key="em.value"
              class="flex-1 py-2 rounded text-xs text-center transition-all"
              :style="{
                backgroundColor: form.emotion === em.value ? 'var(--accent)' : 'var(--surface-2)',
                color: form.emotion === em.value ? '#fff' : 'var(--text-secondary)',
              }"
              :title="em.label"
              @click="form.emotion = em.value"
            >
              {{ em.emoji }}
            </button>
          </div>
        </div>

        <!-- Rating -->
        <div>
          <label class="text-[11px] font-medium mb-1 block" style="color: var(--text-secondary)">Trade Quality ({{ form.rating }}/5)</label>
          <div class="flex gap-1">
            <button
              v-for="n in 5"
              :key="n"
              class="p-1 transition-all"
              @click="form.rating = n"
            >
              <Star
                class="w-5 h-5"
                :style="{ color: n <= form.rating ? 'var(--warning)' : 'var(--surface-3)', fill: n <= form.rating ? 'var(--warning)' : 'transparent' }"
              />
            </button>
          </div>
        </div>

        <!-- Notes -->
        <div>
          <label class="text-[11px] font-medium mb-1 block" style="color: var(--text-secondary)">Notes</label>
          <textarea v-model="form.notes" rows="3" placeholder="Trade rationale, observations..." class="w-full px-3 py-2 rounded text-xs resize-none" style="background-color: var(--surface-2); border: 1px solid var(--border); color: var(--text-primary); outline: none" />
        </div>

        <!-- Tags -->
        <div>
          <label class="text-[11px] font-medium mb-1 block" style="color: var(--text-secondary)">Tags</label>
          <div class="flex flex-wrap gap-1 mb-2">
            <span
              v-for="tag in form.tags"
              :key="tag"
              class="flex items-center gap-1 text-[10px] font-medium px-2 py-0.5 rounded"
              style="background-color: var(--surface-3); color: var(--text-secondary)"
            >
              {{ tag }}
              <button class="hover:brightness-150" @click="removeTag(tag)">
                <X class="w-2.5 h-2.5" />
              </button>
            </span>
          </div>
          <div class="flex gap-2">
            <input
              v-model="tagInput"
              type="text"
              placeholder="Add tag..."
              class="flex-1 px-3 py-1.5 rounded text-xs"
              style="background-color: var(--surface-2); border: 1px solid var(--border); color: var(--text-primary); outline: none"
              @keydown.enter.prevent="addTag"
            />
            <button class="px-3 py-1.5 rounded text-xs" style="background-color: var(--surface-2); color: var(--text-secondary)" @click="addTag">
              <Plus class="w-3 h-3" />
            </button>
          </div>
        </div>

        <!-- Actions -->
        <div class="flex gap-2 justify-end pt-2" style="border-top: 1px solid var(--border)">
          <button class="px-4 py-2 rounded text-xs font-medium" style="background-color: var(--surface-2); color: var(--text-secondary)" @click="emit('close')">Cancel</button>
          <button class="px-4 py-2 rounded text-xs font-medium" style="background-color: var(--accent); color: #fff" @click="handleSave">
            {{ trade ? 'Update Trade' : 'Log Trade' }}
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
