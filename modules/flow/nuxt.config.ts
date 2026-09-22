import { fileURLToPath } from 'node:url'

/** Calendar and habit surfaces keep their existing stores and local databases. */
export default defineNuxtConfig({
  alias: { '#flow': fileURLToPath(new URL('./app', import.meta.url)) },
  components: [{ path: fileURLToPath(new URL('./app/components', import.meta.url)), prefix: 'Flow' }],
})
