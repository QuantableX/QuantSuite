/**
 * One theme for the whole suite.
 *
 * The migration dropped `@nuxtjs/color-mode` from every layer on the grounds
 * that "theming is qs-core's job" — and then nothing took the job. `useColorMode`
 * resolved to VueUse's instead, which defaults to `auto`, so on a light system
 * nothing ever put `.dark` on `<html>` and QuantView sat on its `:root` light
 * palette permanently.
 *
 * VueUse writes the mode as a class on `<html>`, which is exactly what
 * QuantView's palette keys off (`.dark { --surface-0: … }`).
 *
 * **The storage key is left at VueUse's default on purpose.** Modules call
 * `useColorMode()` with no arguments, so they bind to that default key; giving
 * the shell a custom one would create a second, independent instance and the
 * two would fight over the class on `<html>`.
 *
 * `auto` is deliberately not offered. This is a desktop app with a deliberate
 * look, not a website that should follow the OS.
 */
export default defineNuxtPlugin(() => {
  const mode = useColorMode({ initialValue: 'dark' })

  if (mode.value !== 'light' && mode.value !== 'dark') mode.value = 'dark'
})
