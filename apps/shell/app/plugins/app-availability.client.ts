import { watch } from 'vue'
import { appAvailability, isRouteEnabled, loadAppAvailability } from '@quantsuite/core'

export default defineNuxtPlugin(async (nuxtApp) => {
  await loadAppAvailability()
  const router = nuxtApp.$router
  router.beforeEach((to) => isRouteEnabled(to.path) || { path: '/', replace: true })
  watch(() => [appAvailability.ready.value, isRouteEnabled(router.currentRoute.value.path)], () => {
    if (!isRouteEnabled(router.currentRoute.value.path)) void router.replace('/')
  }, { immediate: true })
})
