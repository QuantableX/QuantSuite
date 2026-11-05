<script setup lang="ts">
definePageMeta({ layout: 'systems', path: '/algo/manual/new' })

import { useAppStore } from '#systems/stores/app'
import { useSystemsStore } from '#systems/stores/systems'
import { defaultRunConfig, useConfigStore } from '#systems/stores/config'
import { strategyIcon } from '#systems/utils/strategyIcons'
import SystemsSettingsSingleAssetForm from '#systems/components/Settings/SingleAssetForm.vue'
import type { SystemMode } from '#systems/types'
import { defaultSingleAsset } from '#systems/utils/systemMode'

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
const mode = ref<SystemMode>('rotation')
const singleAsset = ref(defaultSingleAsset())

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
    const settings = sourceId.value ? JSON.parse(JSON.stringify(config.get(sourceId.value)))
      : { ...defaultRunConfig(), mode: mode.value, singleAsset: { ...singleAsset.value } }
    const strategy = await systems.create(name.value.trim(), short.value.trim(), description.value.trim(), settings, strategyIcon(source.value).id)
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
  <form class="qs-new" @submit.prevent="create">
    <div class="qs-new__head">
      <h1>New strategy</h1>
      <p>Start fresh or copy a strategy’s settings. Every name and setting can be changed later.</p>
    </div>
    <div class="card qs-new__panel">
      <label class="qs-new__field">
        <span class="label">Name</span>
        <input v-model="name" class="input" required autofocus placeholder="My strategy" :disabled="busy" />
      </label>
      <label class="qs-new__field">
        <span class="label">Short label <span class="qs-new__optional">(optional)</span></span>
        <input v-model="short" class="input" placeholder="Shown alongside the name" :disabled="busy" />
      </label>
      <label class="qs-new__field">
        <span class="label">Starting settings</span>
        <select v-model="sourceId" class="select" :disabled="busy">
          <option value="">Default settings</option>
          <option v-for="s in systems.systems" :key="s.id" :value="s.id">Copy {{ s.name }}</option>
        </select>
      </label>
      <label v-if="!sourceId" class="qs-new__field">
        <span class="label">System mode</span>
        <select v-model="mode" class="select" :disabled="busy">
          <option value="rotation">Evaluation Rotation System</option><option value="single_asset">Single Asset</option>
        </select>
      </label>
      <SystemsSettingsSingleAssetForm v-if="!sourceId && mode === 'single_asset'" class="qs-new__full" v-model="singleAsset" :disabled="busy" />
      <label class="qs-new__field qs-new__full">
        <span class="label">Description <span class="qs-new__optional">(optional)</span></span>
        <textarea v-model="description" class="input" rows="3" :disabled="busy" />
      </label>
      <p v-if="error" class="qs-new__error qs-new__full" role="alert">{{ error }}</p>
      <div class="qs-new__actions qs-new__full">
        <button type="button" class="btn" :disabled="busy" @click="router.push('/algo/manual')">Cancel</button>
        <button class="btn btn-primary" :disabled="busy || !name.trim()">{{ busy ? 'Creating…' : 'Create strategy' }}</button>
      </div>
    </div>
  </form>
</template>

<style scoped>
.qs-new { container: manual-new / inline-size; display: flex; flex-direction: column; gap: var(--qss-gap); }
.qs-new__head { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px var(--qss-gap); }
.qs-new h1 { margin: 0; font-size: var(--qss-text-title); font-weight: 600; letter-spacing: -.01em; }
.qs-new p { margin: 0; color: var(--qs-text-secondary); font-size: var(--qss-text-label); }
.qs-new__panel { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: var(--qss-gap); padding: var(--qss-panel-pad); }
.qs-new__field { display: flex; flex-direction: column; min-width: 0; }
.qs-new__full { grid-column: 1 / -1; }
.qs-new__optional { color: var(--qs-text-muted); font-weight: 400; }
.qs-new__actions { display: flex; justify-content: flex-end; gap: var(--qss-gap-tight); }
.qs-new .qs-new__error { color: var(--qs-error); }
.qs-new__panel :deep(.qs-single-form__grid) { grid-template-columns: repeat(4, minmax(0, 1fr)); }
.qs-new__panel :deep(.qs-single-form__wide) { grid-column: auto; }

@container manual-new (max-width: 900px) {
  .qs-new__panel,
  .qs-new__panel :deep(.qs-single-form__grid) { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}

@container manual-new (max-width: 460px) {
  .qs-new__panel { grid-template-columns: minmax(0, 1fr); }
}
</style>
