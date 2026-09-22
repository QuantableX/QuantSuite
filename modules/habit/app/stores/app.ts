import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import type { AppSettings, Grouping } from '#habit/types'

const defaultSettings: AppSettings = {
  grouping: 'week',
  sidebarLeftOpen: true,
  sidebarRightOpen: true,
}

/** UI state and persisted preferences. */
export const useAppStore = defineStore('habit/app', () => {
  const settings = ref<AppSettings>({ ...defaultSettings })

  const sidebarLeftOpen = computed(() => settings.value.sidebarLeftOpen)
  const sidebarRightOpen = computed(() => settings.value.sidebarRightOpen)
  const grouping = computed(() => settings.value.grouping)

  async function loadSettings() {
    try {
      settings.value = {
        ...defaultSettings,
        ...(await invoke<AppSettings>('plugin:habit|get_app_settings')),
      }
    } catch {
      settings.value = { ...defaultSettings }
    }
  }

  async function saveSettings() {
    try {
      settings.value = await invoke<AppSettings>('plugin:habit|update_app_settings', {
        settings: settings.value,
      })
    } catch {
      /* browser development */
    }
  }

  function toggleSidebar(side: 'left' | 'right') {
    if (side === 'left') settings.value.sidebarLeftOpen = !settings.value.sidebarLeftOpen
    else settings.value.sidebarRightOpen = !settings.value.sidebarRightOpen
    void saveSettings()
  }

  function setGrouping(value: Grouping) {
    settings.value.grouping = value
    void saveSettings()
  }

  return {
    settings,
    sidebarLeftOpen,
    sidebarRightOpen,
    grouping,
    loadSettings,
    saveSettings,
    toggleSidebar,
    setGrouping,
  }
})
