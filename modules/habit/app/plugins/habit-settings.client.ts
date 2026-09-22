/**
 * Registers QuantFlow's habit settings in the unified settings modal.
 */
import { markRaw } from 'vue'
import { registerSettingsSections } from '@quantsuite/core'
import General from '../components/Settings/General.vue'

export default defineNuxtPlugin(() => {
  registerSettingsSections('flow', [
    { id: 'habit', label: 'Habits', component: markRaw(General), order: 0 },
  ])
})
