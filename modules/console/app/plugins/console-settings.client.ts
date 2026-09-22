/**
 * Registers QuantConsole's sections in the unified settings modal (V3).
 *
 * Four entries, one component: `SettingsPanel` renders exactly the group its
 * `section` prop names. Each entry needs its own component identity:
 * `SettingsSection.component` is rendered as `<component :is>` with no props,
 * so the section id travels as a closed-over prop in a one-line wrapper.
 */
import { defineComponent, h, markRaw } from 'vue'
import { registerSettingsSections } from '@quantsuite/core'
import SettingsPanel from '../components/SettingsPanel.vue'

function panel(section: 'shell' | 'palette' | 'keymap' | 'agent', name: string) {
  return markRaw(
    defineComponent({
      name,
      setup: () => () => h(SettingsPanel, { section }),
    })
  )
}

export default defineNuxtPlugin(() => {
  registerSettingsSections('console', [
    { id: 'shell', label: 'Shell', component: panel('shell', 'ConsoleSettingsShell'), order: 0 },
    { id: 'palette', label: 'ANSI colours', component: panel('palette', 'ConsoleSettingsPalette'), order: 20 },
    { id: 'keymap', label: 'Shortcuts', component: panel('keymap', 'ConsoleSettingsKeymap'), order: 30 },
    { id: 'agent', label: 'Agent access', component: panel('agent', 'ConsoleSettingsAgent'), order: 40 },
  ])
})
