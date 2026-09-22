import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import { ref, computed } from 'vue'
import type { AppSettings, BotDefaults } from '#algo/types'

const BOT_TIMEFRAMES = ['1m', '5m', '15m', '1h', '4h', '1d']

export const useAppStore = defineStore('algo/app', () => {
  // ── State ──

  const settings = ref<AppSettings>({
    theme: 'dark',
    font_size: 14,
    default_exchange_id: null,
    default_pair: 'BTC/USDT',
    default_timeframe: '1m',
    python_path: '',
    strategy_dir: '',
    backtest_dir: '',
    risk_per_trade: 1,
    max_concurrent_positions: 2,
    slippage_tolerance: 0,
    paper_fee_pct: 0.1,
    default_budget: 10000,
    default_warmup_candles: 200,
    defaults_version: 2,
    notify_on_trade: true,
    notify_on_error: true,
    notify_on_daily_summary: false,
  })

  const isLoading = ref(false)

  /**
   * The bot defaults with their fallbacks — the ONE source New Bot, the
   * backtest form and the Settings page read (user, 2026-09-07). An empty
   * or unlisted stored value never leaks a different default anywhere.
   */
  const botDefaults = computed<BotDefaults>(() => {
    const s = settings.value
    return {
      exchange_id: s.default_exchange_id,
      pair: s.default_pair || 'BTC/USDT',
      timeframe: BOT_TIMEFRAMES.includes(s.default_timeframe) ? s.default_timeframe : '1m',
      budget: Number.isFinite(s.default_budget) && s.default_budget >= 100 ? s.default_budget : 10000,
      risk_per_trade: s.risk_per_trade,
      max_positions: s.max_concurrent_positions,
      slippage: s.slippage_tolerance,
      fee: s.paper_fee_pct,
      warmup_candles: s.default_warmup_candles || 200,
    }
  })

  // ── Actions ──

  async function loadSettings() {
    isLoading.value = true
    try {
      const loaded = await invoke<AppSettings>('plugin:algo|get_settings')
      settings.value = loaded
      applyTheme(loaded.theme)
      applyFontSize(loaded.font_size)
    } catch (err) {
      console.error('[app store] Failed to load settings:', err)
    } finally {
      isLoading.value = false
    }
  }

  async function saveSettings(partial: Partial<AppSettings>) {
    const merged = { ...settings.value, ...partial }
    try {
      await invoke('plugin:algo|update_settings', { settings: merged })
      settings.value = merged

      if (partial.theme !== undefined) {
        applyTheme(partial.theme)
      }
      if (partial.font_size !== undefined) {
        applyFontSize(partial.font_size)
      }
    } catch (err) {
      console.error('[app store] Failed to save settings:', err)
      throw err
    }
  }

  function applyTheme(theme: 'dark' | 'light') {
    if (import.meta.client) {
      document.documentElement.setAttribute('data-theme', theme)
      document.documentElement.classList.toggle('dark', theme === 'dark')
      localStorage.setItem('quantalgo-theme', theme)
    }
    settings.value.theme = theme
  }

  function applyFontSize(size: number) {
    if (import.meta.client) {
      document.documentElement.style.setProperty('--app-font-size', `${size}px`)
    }
    settings.value.font_size = size
  }

  async function detectPython(): Promise<string> {
    try {
      const path = await invoke<string | null>('plugin:algo|detect_python')
      const resolved = path ?? settings.value.python_path ?? ''
      settings.value.python_path = resolved
      return resolved
    } catch (err) {
      console.error('[app store] Failed to detect Python:', err)
      throw err
    }
  }

  return {
    settings,
    botDefaults,
    isLoading,
    loadSettings,
    saveSettings,
    applyTheme,
    applyFontSize,
    detectPython,
  }
})
