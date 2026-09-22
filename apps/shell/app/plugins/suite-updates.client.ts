/**
 * Checks the suite's update channel once per start. An available update shows
 * as the download button in the home topbar; Settings → General has the rest.
 */
export default defineNuxtPlugin(() => {
  void useSuiteUpdates().checkOnStart()
})
