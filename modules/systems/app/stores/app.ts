import { defineStore } from 'pinia'
import { useEngine } from '#systems/composables/useEngine'
import type { AppSettings, EngineStatus, EngineView, Theme } from '#systems/types'

const DEFAULT_VIEW: EngineView = 'live'

const defaultSettings: AppSettings = {
  theme: 'dark',
  fontSize: 14,
  activeSystemId: 'lces',
}

export const useAppStore = defineStore('systems/app', () => {
  const engine = useEngine()
  const settings = ref<AppSettings>({ ...defaultSettings })
  const engineStatus = ref<EngineStatus>({ status: 'stopped', pid: null })

  // Each system keeps its own active view, so switching systems restores the
  // tab you last had open there instead of carrying the current one over.
  const viewBySystem = ref<Record<string, EngineView>>({})

  function rememberView(systemId: string, view: EngineView) {
    if (!systemId) return
    viewBySystem.value[systemId] = view
  }

  function viewFor(systemId: string): EngineView {
    return viewBySystem.value[systemId] ?? DEFAULT_VIEW
  }

  const theme = computed(() => settings.value.theme)
  const fontSize = computed(() => settings.value.fontSize)
  const activeSystemId = computed(() => settings.value.activeSystemId)

  function resolveTheme(value: Theme): Exclude<Theme, 'system'> {
    if (value === 'system' && typeof window !== 'undefined') {
      return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark'
    }
    return value === 'system' ? 'dark' : value
  }

  // The module fills a box inside the shell, it does not own the document: a
  // font size written to documentElement scaled every other module's rem units
  // too, and data-theme belongs to the shell (packages/core fixes the suite to
  // dark). The layout binds this on its own root instead.
  const rootStyle = computed(() => ({ fontSize: `${settings.value.fontSize}px` }))

  async function loadSettings() {
    try {
      const loaded = await engine.getAppSettings()
      settings.value = { ...defaultSettings, ...loaded }
    } catch {
      settings.value = { ...defaultSettings }
    }
  }

  async function saveSettings() {
    try {
      settings.value = await engine.updateAppSettings(settings.value)
    } catch {
      /* not in Tauri — keep local state */
    }
  }

  function setActiveSystem(id: string) {
    settings.value.activeSystemId = id
    void saveSettings()
  }

  function setTheme(value: Theme) {
    settings.value.theme = value
    void saveSettings()
  }

  function toggleTheme() {
    const resolved = resolveTheme(settings.value.theme)
    setTheme(resolved === 'dark' ? 'light' : 'dark')
  }

  async function refreshEngineStatus() {
    try {
      engineStatus.value = await engine.engineStatus()
    } catch {
      engineStatus.value = { status: 'stopped', pid: null }
    }
  }

  return {
    settings,
    engineStatus,
    viewBySystem,
    rememberView,
    viewFor,
    theme,
    fontSize,
    activeSystemId,
    rootStyle,
    loadSettings,
    saveSettings,
    setActiveSystem,
    setTheme,
    toggleTheme,
    refreshEngineStatus,
  }
})
