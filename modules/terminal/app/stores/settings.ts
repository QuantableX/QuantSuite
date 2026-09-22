import { defineStore } from 'pinia'

export interface TradingViewSettings {
  interval: string
  style: string
  timezone: string
  locale: string
  backgroundColor: { dark: string; light: string }
  gridColor: { dark: string; light: string }
  hideTopToolbar: boolean
  hideSideToolbar: boolean
  allowSymbolChange: boolean
  calendar: boolean
  hideVolume: boolean
  withdateranges: boolean
  details: boolean
  hotlist: boolean
  showPopupButton: boolean
  studies: string[]
  range: string
}

const INTERVAL_OPTIONS = [
  { value: '1', label: '1 minute' },
  { value: '3', label: '3 minutes' },
  { value: '5', label: '5 minutes' },
  { value: '15', label: '15 minutes' },
  { value: '30', label: '30 minutes' },
  { value: '60', label: '1 hour' },
  { value: '120', label: '2 hours' },
  { value: '180', label: '3 hours' },
  { value: '240', label: '4 hours' },
  { value: 'D', label: '1 day' },
  { value: 'W', label: '1 week' },
  { value: 'M', label: '1 month' },
]

const STYLE_OPTIONS = [
  { value: '1', label: 'Candles' },
  { value: '0', label: 'Bars' },
  { value: '2', label: 'Line' },
  { value: '3', label: 'Area' },
  { value: '4', label: 'Renko' },
  { value: '5', label: 'Kagi' },
  { value: '6', label: 'Point & Figure' },
  { value: '7', label: 'Line Break' },
  { value: '8', label: 'Heikin Ashi' },
  { value: '9', label: 'Hollow Candles' },
  { value: '10', label: 'Baseline' },
]

const TIMEZONE_OPTIONS = [
  { value: 'Etc/UTC', label: 'UTC' },
  { value: 'America/New_York', label: 'New York (EST/EDT)' },
  { value: 'America/Chicago', label: 'Chicago (CST/CDT)' },
  { value: 'America/Los_Angeles', label: 'Los Angeles (PST/PDT)' },
  { value: 'America/Toronto', label: 'Toronto (EST/EDT)' },
  { value: 'America/Sao_Paulo', label: 'Sao Paulo (BRT)' },
  { value: 'Europe/London', label: 'London (GMT/BST)' },
  { value: 'Europe/Berlin', label: 'Berlin (CET/CEST)' },
  { value: 'Europe/Paris', label: 'Paris (CET/CEST)' },
  { value: 'Europe/Moscow', label: 'Moscow (MSK)' },
  { value: 'Asia/Dubai', label: 'Dubai (GST)' },
  { value: 'Asia/Kolkata', label: 'Kolkata (IST)' },
  { value: 'Asia/Shanghai', label: 'Shanghai (CST)' },
  { value: 'Asia/Hong_Kong', label: 'Hong Kong (HKT)' },
  { value: 'Asia/Tokyo', label: 'Tokyo (JST)' },
  { value: 'Asia/Singapore', label: 'Singapore (SGT)' },
  { value: 'Australia/Sydney', label: 'Sydney (AEST/AEDT)' },
  { value: 'Pacific/Auckland', label: 'Auckland (NZST/NZDT)' },
  { value: 'exchange', label: 'Exchange timezone' },
]

const LOCALE_OPTIONS = [
  { value: 'en', label: 'English' },
  { value: 'de_DE', label: 'Deutsch' },
  { value: 'es', label: 'Espanol' },
  { value: 'fr', label: 'Francais' },
  { value: 'it', label: 'Italiano' },
  { value: 'ja', label: 'Japanese' },
  { value: 'ko', label: 'Korean' },
  { value: 'pt', label: 'Portugues' },
  { value: 'ru', label: 'Russian' },
  { value: 'zh_CN', label: 'Chinese (Simplified)' },
  { value: 'zh_TW', label: 'Chinese (Traditional)' },
  { value: 'ar_AE', label: 'Arabic' },
  { value: 'tr', label: 'Turkish' },
  { value: 'vi_VN', label: 'Vietnamese' },
  { value: 'th_TH', label: 'Thai' },
]

const RANGE_OPTIONS = [
  { value: '', label: 'Default' },
  { value: '1D', label: '1 Day' },
  { value: '5D', label: '5 Days' },
  { value: '1M', label: '1 Month' },
  { value: '3M', label: '3 Months' },
  { value: '6M', label: '6 Months' },
  { value: '12M', label: '12 Months' },
  { value: '60M', label: '5 Years' },
  { value: 'ALL', label: 'All' },
]

const STUDY_OPTIONS = [
  { value: 'MASimple@tv-basicstudies', label: 'SMA (Simple Moving Average)' },
  { value: 'MAExp@tv-basicstudies', label: 'EMA (Exponential Moving Average)' },
  { value: 'MACD@tv-basicstudies', label: 'MACD' },
  { value: 'RSI@tv-basicstudies', label: 'RSI' },
  { value: 'BB@tv-basicstudies', label: 'Bollinger Bands' },
  { value: 'StochasticRSI@tv-basicstudies', label: 'Stochastic RSI' },
  { value: 'Stochastic@tv-basicstudies', label: 'Stochastic' },
  { value: 'IchimokuCloud@tv-basicstudies', label: 'Ichimoku Cloud' },
  { value: 'VWAP@tv-basicstudies', label: 'VWAP' },
  { value: 'AwesomeOscillator@tv-basicstudies', label: 'Awesome Oscillator' },
  { value: 'MOM@tv-basicstudies', label: 'Momentum' },
  { value: 'CCI@tv-basicstudies', label: 'CCI' },
  { value: 'ADX@tv-basicstudies', label: 'ADX' },
  { value: 'PSAR@tv-basicstudies', label: 'Parabolic SAR' },
  { value: 'PivotPointsStandard@tv-basicstudies', label: 'Pivot Points' },
  { value: 'ATR@tv-basicstudies', label: 'ATR' },
  { value: 'OBV@tv-basicstudies', label: 'OBV (On Balance Volume)' },
  { value: 'VWMA@tv-basicstudies', label: 'VWMA' },
  { value: 'WilliamsR@tv-basicstudies', label: 'Williams %R' },
  { value: 'MF@tv-basicstudies', label: 'Money Flow Index' },
]

export {
  INTERVAL_OPTIONS,
  STYLE_OPTIONS,
  TIMEZONE_OPTIONS,
  LOCALE_OPTIONS,
  RANGE_OPTIONS,
  STUDY_OPTIONS,
}

const STORAGE_KEY = 'quantview-tv-settings'

function getDefaults(): TradingViewSettings {
  return {
    interval: 'D',
    style: '1',
    timezone: 'Etc/UTC',
    locale: 'en',
    backgroundColor: {
      dark: 'rgba(24, 24, 30, 1)',
      light: 'rgba(216, 216, 222, 1)',
    },
    gridColor: {
      dark: 'rgba(55, 55, 63, 0.06)',
      light: 'rgba(186, 186, 194, 0.06)',
    },
    hideTopToolbar: false,
    hideSideToolbar: false,
    allowSymbolChange: true,
    calendar: false,
    hideVolume: false,
    withdateranges: false,
    details: false,
    hotlist: false,
    showPopupButton: false,
    studies: [],
    range: '',
  }
}

export const useSettingsStore = defineStore('terminal/settings', () => {
  const tvSettings = ref<TradingViewSettings>(getDefaults())
  const showSettingsModal = ref(false)

  function loadFromStorage() {
    try {
      const raw = localStorage.getItem(STORAGE_KEY)
      if (raw) {
        const saved = JSON.parse(raw) as Partial<TradingViewSettings>
        tvSettings.value = { ...getDefaults(), ...saved }
      }
    } catch {
      // Ignore corrupt storage
    }
  }

  function saveToStorage() {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(tvSettings.value))
  }

  function updateTvSetting<K extends keyof TradingViewSettings>(key: K, value: TradingViewSettings[K]) {
    tvSettings.value[key] = value
    saveToStorage()
  }

  function resetTvSettings() {
    tvSettings.value = getDefaults()
    saveToStorage()
  }

  function toggleStudy(study: string) {
    const idx = tvSettings.value.studies.indexOf(study)
    if (idx >= 0) {
      tvSettings.value.studies.splice(idx, 1)
    } else {
      tvSettings.value.studies.push(study)
    }
    saveToStorage()
  }

  return {
    tvSettings,
    showSettingsModal,
    loadFromStorage,
    saveToStorage,
    updateTvSetting,
    resetTvSettings,
    toggleStudy,
  }
})
