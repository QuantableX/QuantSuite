<script setup lang="ts">
/**
 * Settings → Appearance (card 19). The colours follow the suite (dark, by
 * the suite's decision); here: the conversation's text size, how long
 * panels and menus take to open (none while Windows asks for reduced
 * motion), and whether `$…$` renders as formulas. Changes apply at once.
 */
import { computed, onMounted } from 'vue'
import { usePilotStore } from '#pilot/stores/pilot'
import type { PilotSettings } from '#pilot/types'

const store = usePilotStore()
onMounted(async () => {
  if (!store.settings) await store.loadSettings()
})
const s = computed(() => store.settings)
const reduced = typeof matchMedia !== 'undefined' && matchMedia('(prefers-reduced-motion: reduce)').matches

function set(patch: Partial<PilotSettings>) {
  void store.saveSettings(patch, { quiet: true })
}
const SIZES: { id: PilotSettings['textSize']; label: string }[] = [
  { id: 'small', label: 'Small' },
  { id: 'default', label: 'Default' },
  { id: 'large', label: 'Large' },
  { id: 'larger', label: 'Larger' },
]
</script>

<template>
  <div v-if="s" class="qp-settings" data-module="pilot">
    <p class="qp-settings-desc">The colours follow the suite. The rest applies at once.</p>
    <div class="qp-row">
      <span class="qp-row-k">Conversation text</span>
      <span class="qp-seg" role="radiogroup" aria-label="Conversation text size">
        <button v-for="z in SIZES" :key="z.id" role="radio" class="qp-seg-btn" :class="{ on: s.textSize === z.id }" :aria-checked="s.textSize === z.id" @click="set({ textSize: z.id })">{{ z.label }}</button>
      </span>
    </div>
    <div class="qp-row">
      <span class="qp-row-k">Panel animation</span>
      <input class="qp-range" type="range" min="0" max="400" step="20" :value="s.panelMotionMs" aria-label="Panel animation in milliseconds" @change="set({ panelMotionMs: Number(($event.target as HTMLInputElement).value) })" />
      <span class="qp-num">{{ s.panelMotionMs }} ms</span>
    </div>
    <p v-if="reduced" class="qp-settings-desc">Windows asks for reduced motion: panels open without animation.</p>
    <label class="qp-check">
      <input type="checkbox" :checked="s.renderMath" @change="set({ renderMath: ($event.target as HTMLInputElement).checked })" />
      <span>Render math <em>— <code>$…$</code> and <code>$$…$$</code> in replies become formulas</em></span>
    </label>
  </div>
</template>

<style scoped>
.qp-settings { display: flex; flex-direction: column; gap: 12px; font-size: 12.5px; color: var(--qss-text, #d4d4d8); }
.qp-settings-desc { margin: 0; color: var(--qss-text-secondary, #9a9aa5); font-size: 12px; }
.qp-row { display: flex; align-items: center; gap: 12px; }
.qp-row-k { width: 150px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-seg { display: inline-flex; border: 1px solid var(--qss-border, #47474f); border-radius: 7px; overflow: hidden; }
.qp-seg-btn { padding: 4px 11px; border: none; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; font-size: 12px; cursor: pointer; }
.qp-seg-btn + .qp-seg-btn { border-left: 1px solid var(--qss-border-subtle, #35353d); }
.qp-seg-btn.on { background: var(--qss-bg-card, #292930); color: var(--qss-text, #d4d4d8); }
.qp-range { width: 220px; accent-color: var(--qss-accent, #7aa2f7); }
.qp-num { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-check { display: flex; align-items: flex-start; gap: 8px; line-height: 1.4; }
.qp-check em { font-style: normal; color: var(--qss-text-muted, #6e6e7a); }
.qp-check code { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; }
</style>
