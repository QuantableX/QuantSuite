<script setup lang="ts">
import { Star, Edit3, Trash2, X as XIcon, Clock } from 'lucide-vue-next'
import type { JournalTrade } from '#terminal/stores/journal'

const props = defineProps<{
  trade: JournalTrade
}>()

const emit = defineEmits<{
  edit: [trade: JournalTrade]
  delete: [id: string]
  close: [trade: JournalTrade]
}>()

const EMOTION_MAP: Record<string, { emoji: string; label: string }> = {
  confident: { emoji: '😎', label: 'Confident' },
  neutral: { emoji: '😐', label: 'Neutral' },
  fearful: { emoji: '😰', label: 'Fearful' },
  greedy: { emoji: '🤑', label: 'Greedy' },
  frustrated: { emoji: '😤', label: 'Frustrated' },
}
</script>

<template>
  <div class="px-4 py-3 space-y-3" style="background-color: var(--surface-1); border-bottom: 1px solid var(--border)">
    <!-- Top Row: Info + Actions -->
    <div class="flex items-start justify-between">
      <div class="space-y-2 flex-1">
        <!-- Strategy & Exchange -->
        <div class="flex items-center gap-3">
          <span v-if="trade.strategy" class="text-[11px] font-medium px-2 py-0.5 rounded" style="background-color: var(--accent); color: #fff">
            {{ trade.strategy }}
          </span>
          <span class="text-[11px]" style="color: var(--text-secondary)">{{ trade.exchange }}</span>
          <span class="text-[11px] font-mono" style="color: var(--muted)">Fees: ${{ trade.fees.toFixed(2) }}</span>
          <span v-if="trade.pnlPercent !== null" class="text-[11px] font-mono font-semibold" :style="{ color: trade.pnlPercent >= 0 ? 'var(--positive)' : 'var(--negative)' }">
            {{ trade.pnlPercent >= 0 ? '+' : '' }}{{ trade.pnlPercent.toFixed(2) }}%
          </span>
        </div>

        <!-- Notes -->
        <p v-if="trade.notes" class="text-xs leading-relaxed" style="color: var(--text-secondary)">{{ trade.notes }}</p>

        <!-- Tags -->
        <div v-if="trade.tags.length" class="flex flex-wrap gap-1">
          <span
            v-for="tag in trade.tags"
            :key="tag"
            class="text-[10px] font-medium px-2 py-0.5 rounded"
            style="background-color: var(--surface-3); color: var(--muted)"
          >
            {{ tag }}
          </span>
        </div>

        <!-- Emotion & Rating -->
        <div class="flex items-center gap-4">
          <div class="flex items-center gap-1.5">
            <span class="text-[11px]" style="color: var(--text-secondary)">Emotion:</span>
            <span class="text-sm">{{ EMOTION_MAP[trade.emotion]?.emoji }}</span>
            <span class="text-[11px]" style="color: var(--text-secondary)">{{ EMOTION_MAP[trade.emotion]?.label }}</span>
          </div>
          <div class="flex items-center gap-1">
            <span class="text-[11px]" style="color: var(--text-secondary)">Quality:</span>
            <Star
              v-for="n in 5"
              :key="n"
              class="w-3 h-3"
              :style="{ color: n <= trade.rating ? 'var(--warning)' : 'var(--surface-3)', fill: n <= trade.rating ? 'var(--warning)' : 'transparent' }"
            />
          </div>
        </div>
      </div>

      <!-- Action Buttons -->
      <div class="flex items-center gap-1 ml-3 shrink-0">
        <button
          v-if="trade.status === 'open'"
          class="flex items-center gap-1 px-2.5 py-1.5 rounded text-[11px] font-medium transition-all hover:brightness-125"
          style="background-color: var(--accent); color: #fff"
          @click="emit('close', trade)"
        >
          <Clock class="w-3 h-3" />
          Close
        </button>
        <button
          class="p-1.5 rounded transition-all hover:brightness-125"
          style="background-color: var(--surface-2); color: var(--text-secondary)"
          @click="emit('edit', trade)"
        >
          <Edit3 class="w-3.5 h-3.5" />
        </button>
        <button
          class="p-1.5 rounded transition-all hover:brightness-125"
          style="background-color: var(--surface-2); color: var(--negative)"
          @click="emit('delete', trade.id)"
        >
          <Trash2 class="w-3.5 h-3.5" />
        </button>
      </div>
    </div>
  </div>
</template>
