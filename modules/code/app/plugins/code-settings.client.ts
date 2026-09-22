/**
 * Registers QuantCode's sections in the unified settings modal (V3).
 */
import { markRaw } from 'vue'
import { registerSettingsSections } from '@quantsuite/core'
import General from '../components/Settings/General.vue'

export default defineNuxtPlugin(() => {
  registerSettingsSections('code', [
    { id: 'editor', label: 'Editor', component: markRaw(General), order: 0 },
  ])
})
