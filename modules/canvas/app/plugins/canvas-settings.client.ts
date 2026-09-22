/**
 * Registers QuantCanvas's sections in the unified settings modal (V3).
 */
import { markRaw } from 'vue'
import { registerSettingsSections } from '@quantsuite/core'
import General from '../components/Settings/General.vue'
import About from '../components/Settings/About.vue'

export default defineNuxtPlugin(() => {
  registerSettingsSections('canvas', [
    { id: 'general', label: 'General', component: markRaw(General), order: 0 },
    { id: 'about', label: 'About', component: markRaw(About), order: 1 },
  ])
})
