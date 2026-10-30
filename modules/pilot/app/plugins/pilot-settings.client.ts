/**
 * Registers QuantPilot's section in the unified settings modal (V3).
 */
import { markRaw } from 'vue'
import { registerSettingsSections } from '@quantsuite/core'
import General from '../components/Settings/General.vue'
import Providers from '../components/Settings/Providers.vue'
import Actions from '../components/Settings/Actions.vue'
import Keybindings from '../components/Settings/Keybindings.vue'
import SourceControl from '../components/Settings/SourceControl.vue'
import Appearance from '../components/Settings/Appearance.vue'
import Advanced from '../components/Settings/Advanced.vue'

export default defineNuxtPlugin(() => {
  registerSettingsSections('pilot', [
    { id: 'general', label: 'General', component: markRaw(General), order: 0 },
    { id: 'providers', label: 'Providers', component: markRaw(Providers), order: 1 },
    { id: 'keybindings', label: 'Keybindings', component: markRaw(Keybindings), order: 2 },
    { id: 'actions', label: 'Project actions', component: markRaw(Actions), order: 3 },
    { id: 'source-control', label: 'Source control', component: markRaw(SourceControl), order: 4 },
    { id: 'appearance', label: 'Appearance', component: markRaw(Appearance), order: 5 },
    { id: 'advanced', label: 'Advanced', component: markRaw(Advanced), order: 6 },
  ])
})
