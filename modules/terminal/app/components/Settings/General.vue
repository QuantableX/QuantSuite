<script setup lang="ts">
/**
 * QuantTerminal → General — TradingView chart settings, extracted from the
 * old sidebar-footer modal into the unified settings modal (V3). The
 * light/dark theme segment did not survive the move: the suite is
 * dark-monochrome by decision (2026-08-14).
 */
import { useSettingsStore } from '#terminal/stores/settings'
import {
  INTERVAL_OPTIONS,
  STYLE_OPTIONS,
  TIMEZONE_OPTIONS,
  LOCALE_OPTIONS,
} from '#terminal/stores/settings'

const settings = useSettingsStore()
const tv = computed(() => settings.tvSettings)

function rgbaToHex(rgba: string): string {
  const match = rgba.match(/[\d.]+/g)
  if (!match || match.length < 3) return '#000000'
  const r = Math.round(Number(match[0]))
  const g = Math.round(Number(match[1]))
  const b = Math.round(Number(match[2]))
  return '#' + [r, g, b].map(v => v.toString(16).padStart(2, '0')).join('')
}

function hexToRgba(hex: string, existingRgba: string): string {
  const match = existingRgba.match(/[\d.]+/g)
  const a = match && match.length >= 4 ? match[3] : '1'
  const r = parseInt(hex.slice(1, 3), 16)
  const g = parseInt(hex.slice(3, 5), 16)
  const b = parseInt(hex.slice(5, 7), 16)
  return `rgba(${r}, ${g}, ${b}, ${a})`
}
</script>

<template>
  <div>
    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Chart interval</p>
      </div>
      <select
        class="qsu-select"
        :value="tv.interval"
        @change="settings.updateTvSetting('interval', ($event.target as HTMLSelectElement).value)"
      >
        <option v-for="opt in INTERVAL_OPTIONS" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
      </select>
    </div>

    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Chart style</p>
      </div>
      <select
        class="qsu-select"
        :value="tv.style"
        @change="settings.updateTvSetting('style', ($event.target as HTMLSelectElement).value)"
      >
        <option v-for="opt in STYLE_OPTIONS" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
      </select>
    </div>

    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Timezone</p>
      </div>
      <select
        class="qsu-select"
        :value="tv.timezone"
        @change="settings.updateTvSetting('timezone', ($event.target as HTMLSelectElement).value)"
      >
        <option v-for="opt in TIMEZONE_OPTIONS" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
      </select>
    </div>

    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Language</p>
      </div>
      <select
        class="qsu-select"
        :value="tv.locale"
        @change="settings.updateTvSetting('locale', ($event.target as HTMLSelectElement).value)"
      >
        <option v-for="opt in LOCALE_OPTIONS" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
      </select>
    </div>

    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Chart background</p>
        <p class="qsu-hint">Dark / light chart variants.</p>
      </div>
      <div class="ts-colors">
        <label class="ts-color">
          <input
            type="color"
            :value="rgbaToHex(tv.backgroundColor.dark)"
            @input="settings.updateTvSetting('backgroundColor', { ...tv.backgroundColor, dark: hexToRgba(($event.target as HTMLInputElement).value, tv.backgroundColor.dark) })"
          />
          <span>{{ rgbaToHex(tv.backgroundColor.dark) }}</span>
        </label>
        <label class="ts-color">
          <input
            type="color"
            :value="rgbaToHex(tv.backgroundColor.light)"
            @input="settings.updateTvSetting('backgroundColor', { ...tv.backgroundColor, light: hexToRgba(($event.target as HTMLInputElement).value, tv.backgroundColor.light) })"
          />
          <span>{{ rgbaToHex(tv.backgroundColor.light) }}</span>
        </label>
      </div>
    </div>

    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Chart grid</p>
        <p class="qsu-hint">Dark / light chart variants.</p>
      </div>
      <div class="ts-colors">
        <label class="ts-color">
          <input
            type="color"
            :value="rgbaToHex(tv.gridColor.dark)"
            @input="settings.updateTvSetting('gridColor', { ...tv.gridColor, dark: hexToRgba(($event.target as HTMLInputElement).value, tv.gridColor.dark) })"
          />
          <span>{{ rgbaToHex(tv.gridColor.dark) }}</span>
        </label>
        <label class="ts-color">
          <input
            type="color"
            :value="rgbaToHex(tv.gridColor.light)"
            @input="settings.updateTvSetting('gridColor', { ...tv.gridColor, light: hexToRgba(($event.target as HTMLInputElement).value, tv.gridColor.light) })"
          />
          <span>{{ rgbaToHex(tv.gridColor.light) }}</span>
        </label>
      </div>
    </div>

    <div class="qsu-row">
      <div class="qsu-row-text">
        <p class="qsu-label">Reset TradingView settings</p>
        <p class="qsu-hint">Restore interval, style, timezone, language and colors.</p>
      </div>
      <button class="qsu-btn" @click="settings.resetTvSettings()">Reset</button>
    </div>
  </div>
</template>

<style scoped>
.ts-colors {
  display: flex;
  gap: 12px;
}
.ts-color {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  font-family: var(--qss-font-mono);
  color: var(--qss-text-secondary);
}
.ts-color input[type='color'] {
  width: 26px;
  height: 26px;
  padding: 0;
  border: 1px solid var(--qss-border);
  border-radius: 6px;
  background: transparent;
  cursor: pointer;
}
</style>
