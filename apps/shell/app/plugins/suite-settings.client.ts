/**
 * Registers the shell's suite-level sections in the unified settings modal
 * (V3). Modules register theirs the same way from their own layers.
 */
import { markRaw } from 'vue'
import { registerSettingsSections } from '@quantsuite/core'
import SettingsSuiteApps from '../components/SettingsSuiteApps.vue'
import SettingsSuiteGeneral from '../components/SettingsSuiteGeneral.vue'
import SettingsSuiteData from '../components/SettingsSuiteData.vue'

export default defineNuxtPlugin(() => {
  registerSettingsSections('suite', [
    { id: 'general', label: 'General', component: markRaw(SettingsSuiteGeneral), order: 0 },
    { id: 'apps', label: 'Apps', component: markRaw(SettingsSuiteApps), order: 1 },
    { id: 'data', label: 'Data', component: markRaw(SettingsSuiteData), order: 2 },
  ])
})
