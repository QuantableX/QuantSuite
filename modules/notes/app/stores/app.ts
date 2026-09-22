import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import type { AppSettings, Theme } from '#notes/types'

const defaultSettings: AppSettings = {
  theme: 'dark',
  fontSize: 14,
  defaultNoteFont: 'sans',
  smartQuotes: true,
  autoformat: true,
  spellcheckLanguage: 'en-US',
  defaultViewId: '',
  sidebarLeftOpen: true,
  sidebarRightOpen: true,
  focusMode: false,
}

/** UI state and persisted preferences. Nothing about notes lives here. */
export const useAppStore = defineStore('notes/app', () => {
  const settings = ref<AppSettings>({ ...defaultSettings })
  const commandPaletteOpen = ref(false)
  const searchQuery = ref('')

  const theme = computed(() => settings.value.theme)
  const fontSize = computed(() => settings.value.fontSize)
  const sidebarLeftOpen = computed(() => settings.value.sidebarLeftOpen)
  const sidebarRightOpen = computed(() => settings.value.sidebarRightOpen)
  const focusMode = computed(() => settings.value.focusMode)

  function resolveTheme(value: Theme): Exclude<Theme, 'system'> {
    if (value === 'system' && typeof window !== 'undefined') {
      return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark'
    }
    return value === 'system' ? 'dark' : value
  }

  function applyTheme(value = settings.value.theme) {
    if (typeof document === 'undefined') return
    document.documentElement.setAttribute('data-theme', resolveTheme(value))
    document.documentElement.style.fontSize = `${settings.value.fontSize}px`
  }

  async function loadSettings() {
    try {
      const loaded = await invoke<AppSettings>('plugin:notes|get_app_settings')
      settings.value = {
        ...defaultSettings,
        ...loaded,
        // Panel state and focus mode are per-session: a module that reopens
        // with both sidebars hidden reads as broken.
        sidebarLeftOpen: true,
        sidebarRightOpen: true,
        focusMode: false,
      }
    } catch {
      settings.value = { ...defaultSettings }
    }
    applyTheme()
  }

  async function saveSettings() {
    settings.value = await invoke<AppSettings>('plugin:notes|update_app_settings', {
      settings: settings.value,
    })
    applyTheme()
  }

  function setTheme(value: Theme) {
    settings.value.theme = value
    applyTheme()
    void saveSettings()
  }

  function toggleSidebar(side: 'left' | 'right') {
    if (side === 'left') settings.value.sidebarLeftOpen = !settings.value.sidebarLeftOpen
    else settings.value.sidebarRightOpen = !settings.value.sidebarRightOpen
  }

  function enterFocusMode(force?: boolean) {
    settings.value.focusMode = force ?? !settings.value.focusMode
    if (settings.value.focusMode) {
      settings.value.sidebarLeftOpen = false
      settings.value.sidebarRightOpen = false
    } else {
      settings.value.sidebarLeftOpen = true
      settings.value.sidebarRightOpen = true
    }
  }

  return {
    settings,
    commandPaletteOpen,
    searchQuery,
    theme,
    fontSize,
    sidebarLeftOpen,
    sidebarRightOpen,
    focusMode,
    applyTheme,
    loadSettings,
    saveSettings,
    setTheme,
    toggleSidebar,
    enterFocusMode,
  }
})
