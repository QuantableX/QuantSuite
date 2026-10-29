/**
 * Registers QuantPilot's section in the unified settings modal (V3).
 */
import { markRaw } from 'vue'
import { registerSettingsSections } from '@quantsuite/core'
import General from '../components/Settings/General.vue'
import Actions from '../components/Settings/Actions.vue'

export default defineNuxtPlugin(() => {
  registerSettingsSections('pilot', [
    { id: 'general', label: 'General', component: markRaw(General), order: 0 },
    { id: 'actions', label: 'Project actions', component: markRaw(Actions), order: 1 },
  ])
})
