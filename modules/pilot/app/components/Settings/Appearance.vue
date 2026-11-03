<script setup lang="ts">
/**
 * Settings → Appearance (card 19). The colours follow the suite (dark, by
 * the suite's decision); here: the conversation's text size, how long
 * panels and menus take to open (none while Windows asks for reduced
 * motion), and whether `$…$` renders as formulas. Changes apply at once.
 */
import { computed, onMounted, ref } from 'vue'
import { usePilotStore } from '#pilot/stores/pilot'
import { useMascotStore } from '#pilot/stores/mascot'
import { ACCESSORIES, EYES, MASCOT_FULL_NAME, MASCOT_NAME, MOOD_LABEL, MOODS, MOTIONS, QUIRK_LABEL, SIZES as MASCOT_SIZES, SKINS, type Quirk } from '#pilot/utils/mascot'
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
// Bob The Blob (round 2, card 05; named 2026-11-03): kept on this machine;
// the gallery shows every mood, the tricks play any quirk on a preview Bob.
const mascot = useMascotStore()
const m = computed(() => mascot.prefs)
const TRICKS = (Object.keys(QUIRK_LABEL) as Quirk[]).map((id) => ({ id, label: QUIRK_LABEL[id] }))
const preview = ref<{ play: (q: Quirk) => void } | null>(null)

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

    <h3 class="qp-sub">{{ MASCOT_FULL_NAME }}</h3>
    <label class="qp-check">
      <input type="checkbox" :checked="m.on" @change="mascot.set({ on: ($event.target as HTMLInputElement).checked })" />
      <span>Show {{ m.name || MASCOT_NAME }} <em>— in the panel, empty threads and the crew view</em></span>
    </label>
    <template v-if="m.on">
      <div class="qp-row">
        <span class="qp-row-k">Name</span>
        <input class="qp-text" :value="m.name" maxlength="24" :placeholder="MASCOT_NAME" aria-label="His name" @change="mascot.set({ name: ($event.target as HTMLInputElement).value })" />
      </div>
      <div class="qp-row">
        <span class="qp-row-k">Skin</span>
        <span class="qp-swatches" role="radiogroup" aria-label="Skin">
          <button v-for="k in SKINS" :key="k.id" role="radio" class="qp-swatch" :class="{ on: m.skin === k.id }" :aria-checked="m.skin === k.id" :title="k.label" :style="{ background: `radial-gradient(circle at 35% 30%, ${k.light}, ${k.base})` }" @click="mascot.set({ skin: k.id })" />
        </span>
      </div>
      <div class="qp-row">
        <span class="qp-row-k">Eyes</span>
        <span class="qp-seg" role="radiogroup" aria-label="Eyes">
          <button v-for="e in EYES" :key="e.id" role="radio" class="qp-seg-btn" :class="{ on: m.eyes === e.id }" :aria-checked="m.eyes === e.id" @click="mascot.set({ eyes: e.id })">{{ e.label }}</button>
        </span>
      </div>
      <div class="qp-row">
        <span class="qp-row-k">Accessory</span>
        <span class="qp-seg" role="radiogroup" aria-label="Accessory">
          <button v-for="a in ACCESSORIES" :key="a.id" role="radio" class="qp-seg-btn" :class="{ on: m.accessory === a.id }" :aria-checked="m.accessory === a.id" @click="mascot.set({ accessory: a.id })">{{ a.label }}</button>
        </span>
      </div>
      <div class="qp-row">
        <span class="qp-row-k">Size in empty threads</span>
        <span class="qp-seg" role="radiogroup" aria-label="Size">
          <button v-for="z in MASCOT_SIZES" :key="z.id" role="radio" class="qp-seg-btn" :class="{ on: m.size === z.id }" :aria-checked="m.size === z.id" @click="mascot.set({ size: z.id })">{{ z.label }}</button>
        </span>
      </div>
      <div class="qp-row">
        <span class="qp-row-k">Movement</span>
        <span class="qp-seg" role="radiogroup" aria-label="Movement">
          <button v-for="mo in MOTIONS" :key="mo.id" role="radio" class="qp-seg-btn" :class="{ on: m.motion === mo.id }" :aria-checked="m.motion === mo.id" @click="mascot.set({ motion: mo.id })">{{ mo.label }}</button>
        </span>
      </div>
      <label class="qp-check">
        <input type="checkbox" :checked="m.agentTint" @change="mascot.set({ agentTint: ($event.target as HTMLInputElement).checked })" />
        <span>Tint him with the thread's agent <em>— Claude warm, Gemini blue, …</em></span>
      </label>
      <label class="qp-check">
        <input type="checkbox" :checked="m.companion" @change="mascot.set({ companion: ($event.target as HTMLInputElement).checked })" />
        <span>Companion <em>— a small {{ m.name || MASCOT_NAME }} you can carry anywhere in QuantPilot; he stays where you put him</em></span>
      </label>
      <div v-if="m.motion !== 'still'" class="qp-tricks">
        <div class="qp-tricks-face"><PilotFace ref="preview" state="idle" /></div>
        <div class="qp-tricks-list" aria-label="Tricks">
          <button v-for="t in TRICKS" :key="t.id" class="qp-trick" @click="preview?.play(t.id)">{{ t.label }}</button>
        </div>
      </div>
      <div class="qp-gallery" aria-label="Every mood">
        <figure v-for="mood in MOODS" :key="mood" class="qp-gallery-item">
          <div class="qp-gallery-face"><PilotFace state="idle" :mood="mood" :interactive="false" /></div>
          <figcaption>{{ MOOD_LABEL[mood] }}</figcaption>
        </figure>
      </div>
    </template>
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
.qp-sub { margin: 8px 0 0; font-size: 13px; font-weight: 600; color: var(--qss-text, #d4d4d8); }
.qp-text { width: 220px; height: 26px; padding: 0 8px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: var(--qss-bg, #18181e); color: var(--qss-text, #d4d4d8); font: inherit; font-size: 12.5px; }
.qp-swatches { display: inline-flex; gap: 8px; }
.qp-swatch { width: 22px; height: 22px; padding: 0; border: 2px solid transparent; border-radius: 50%; cursor: pointer; }
.qp-swatch.on { border-color: var(--qss-text, #d4d4d8); }
.qp-tricks { display: flex; align-items: center; gap: 14px; }
.qp-tricks-face { flex: 0 0 auto; width: 96px; }
.qp-tricks-list { display: flex; flex-wrap: wrap; gap: 6px; }
.qp-trick { padding: 3px 9px; border: 1px solid var(--qss-border, #47474f); border-radius: 999px; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; font-size: 11.5px; cursor: pointer; }
.qp-trick:hover { color: var(--qss-text, #d4d4d8); background: var(--qss-bg-card, #292930); }
.qp-gallery { display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); gap: 10px 8px; margin-top: 4px; }
.qp-gallery-item { display: flex; flex-direction: column; align-items: center; gap: 2px; margin: 0; }
.qp-gallery-face { width: 72px; }
.qp-gallery-item figcaption { font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); }
</style>
