/**
 * Registers QuantScript's section in the unified settings modal (V3).
 */
import { markRaw } from 'vue'
import { registerSettingsSections } from '@quantsuite/core'
import General from '../components/Settings/General.vue'
import Store from '../components/Settings/Store.vue'

export default defineNuxtPlugin(() => {
  registerSettingsSections('script', [
    { id: 'general', label: 'General', component: markRaw(General), order: 0 },
    { id: 'store', label: 'Store', component: markRaw(Store), order: 1 },
  ])
})
