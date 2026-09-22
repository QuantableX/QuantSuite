/**
 * Registers QuantSystems' section in the unified settings modal (V3).
 */
import { markRaw } from 'vue'
import { registerSettingsSections } from '@quantsuite/core'
import General from '../components/Settings/General.vue'

export default defineNuxtPlugin(() => {
  registerSettingsSections('algo', [
    { id: 'manual', label: 'Manual', component: markRaw(General), order: 0 },
  ])
})
