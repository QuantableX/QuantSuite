<script setup lang="ts">
definePageMeta({ layout: 'systems', path: '/algo/manual/new' })

import { useAppStore } from '#systems/stores/app'
import { useSystemsStore } from '#systems/stores/systems'
import { useConfigStore } from '#systems/stores/config'

const app = useAppStore()
const systems = useSystemsStore()
const config = useConfigStore()
const route = useRoute()
const router = useRouter()
const sourceId = ref(typeof route.query.copy === 'string' ? route.query.copy : '')
const source = computed(() => systems.byId(sourceId.value))
const name = ref(source.value ? `${source.value.name} copy` : '')
const short = ref(source.value?.short ?? '')
const description = ref(source.value?.description ?? '')
const busy = ref(false)
const error = ref<string | null>(null)

async function create() {
  if (busy.value || !name.value.trim()) return
  busy.value = true
  error.value = null
  try {
    if (sourceId.value) {
      if (!source.value) throw new Error('Choose an existing strategy to copy.')
      await config.load(sourceId.value)
      if (config.errors[sourceId.value]) throw new Error(config.errors[sourceId.value]!)
    }
    const settings = sourceId.value ? JSON.parse(JSON.stringify(config.get(sourceId.value))) : undefined
    const strategy = await systems.create(name.value.trim(), short.value.trim(), description.value.trim(), settings)
    if (settings) config.remember(strategy.id, settings)
    app.setActiveSystem(strategy.id)
    await router.push(`/algo/manual/${strategy.id}/settings`)
  } catch (e) {
    error.value = String(e)
  } finally {
    busy.value = false
  }
}
</script>

<template>
  <form class="card qs-new" @submit.prevent="create">
    <div>
      <h1>New strategy</h1>
      <p>Start fresh or copy a strategy’s settings. Every name and setting can be changed later.</p>
    </div>
    <label class="qs-new__field">
      <span class="label">Name</span>
      <input v-model="name" class="input" required autofocus placeholder="My rotation strategy" :disabled="busy" />
    </label>
    <label class="qs-new__field">
      <span class="label">Short label <span class="qs-new__optional">(optional)</span></span>
      <input v-model="short" class="input" placeholder="Shown alongside the name" :disabled="busy" />
    </label>
    <label class="qs-new__field">
      <span class="label">Starting settings</span>
      <select v-model="sourceId" class="select" :disabled="busy">
        <option value="">Default rotation settings</option>
        <option v-for="s in systems.systems" :key="s.id" :value="s.id">Copy {{ s.name }}</option>
      </select>
    </label>
    <label class="qs-new__field">
      <span class="label">Description <span class="qs-new__optional">(optional)</span></span>
      <textarea v-model="description" class="input" rows="3" :disabled="busy" />
    </label>
    <p v-if="error" class="qs-new__error" role="alert">{{ error }}</p>
    <div class="qs-new__actions">
      <button type="button" class="btn" :disabled="busy" @click="router.push('/algo/manual')">Cancel</button>
      <button class="btn btn-primary" :disabled="busy || !name.trim()">{{ busy ? 'Creating…' : 'Create strategy' }}</button>
    </div>
  </form>
</template>

<style scoped>
.qs-new { display: flex; flex-direction: column; gap: 18px; max-width: 640px; margin: 12px auto; padding: 24px; }
.qs-new h1 { margin: 0 0 8px; font-size: 20px; font-weight: 600; }
.qs-new p { margin: 0; color: var(--qs-text-secondary); font-size: 13px; }
.qs-new__field { display: flex; flex-direction: column; gap: 4px; }
.qs-new__optional { color: var(--qs-text-muted); font-weight: 400; }
.qs-new__actions { display: flex; justify-content: flex-end; gap: 8px; }
.qs-new .qs-new__error { color: var(--qs-error); }
</style>
