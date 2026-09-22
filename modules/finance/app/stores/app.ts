import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import type { AppSettings } from '#finance/types'

const defaultSettings: AppSettings = {
  currency: 'EUR',
  locale: 'de-DE',
  sidebarLeftOpen: true,
  sidebarRightOpen: true,
}

/**
 * UI state and persisted preferences.
 *
 * There is no period here any more: the plan is a standing monthly picture,
 * not a month you page through. Paging implied a ledger with history, which
 * this module deliberately does not keep.
 */
export const useAppStore = defineStore('finance/app', () => {
  const settings = ref<AppSettings>({ ...defaultSettings })

  const sidebarLeftOpen = computed(() => settings.value.sidebarLeftOpen)
  const sidebarRightOpen = computed(() => settings.value.sidebarRightOpen)

  async function loadSettings() {
    try {
      settings.value = {
        ...defaultSettings,
        ...(await invoke<AppSettings>('plugin:finance|get_app_settings')),
      }
    } catch {
      settings.value = { ...defaultSettings }
    }
  }

  async function saveSettings() {
    settings.value = await invoke<AppSettings>('plugin:finance|update_app_settings', {
      settings: settings.value,
    })
  }

  function toggleSidebar(side: 'left' | 'right') {
    if (side === 'left') settings.value.sidebarLeftOpen = !settings.value.sidebarLeftOpen
    else settings.value.sidebarRightOpen = !settings.value.sidebarRightOpen
  }

  return { settings, sidebarLeftOpen, sidebarRightOpen, loadSettings, saveSettings, toggleSidebar }
})
