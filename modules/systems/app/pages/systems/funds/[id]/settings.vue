<script setup lang="ts">
definePageMeta({ layout: 'systems', path: '/algo/manual/funds/:id/settings' })

import { useFundsStore } from '#systems/stores/funds'
import { useActiveView } from '#systems/composables/useActiveView'
import SystemsFundForm from '#systems/components/Fund/Form.vue'

const funds = useFundsStore()
const router = useRouter()
const { fundId } = useActiveView()
const fund = computed(() => funds.byId(fundId.value))
const confirmDelete = ref(false)
const deleting = ref(false)
const error = ref<string | null>(null)

async function remove() {
  if (!fund.value || deleting.value) return
  deleting.value = true
  error.value = null
  try {
    await funds.remove(fund.value.id)
    await router.push('/algo/manual')
  } catch (e) {
    error.value = String(e)
  } finally {
    deleting.value = false
  }
}
// Leaving always shows the first step again when coming back.
onDeactivated(() => { confirmDelete.value = false })
watch(fundId, () => { confirmDelete.value = false })
onMounted(() => { void funds.load() })
</script>

<template>
  <div v-if="fund" class="qs-fund-settings">
    <h1>Settings</h1>
    <SystemsFundForm :fund="fund" @saved="f => router.push(`/algo/manual/funds/${f.id}/backtest`)" @cancel="router.push(`/algo/manual/funds/${fund!.id}/backtest`)">
      <template #actions>
        <p v-if="error" class="qs-fund-settings__error" role="alert">{{ error }}</p>
        <template v-if="confirmDelete">
          <span class="qs-fund-settings__confirm">Delete this fund? Its systems stay.</span>
          <button type="button" class="btn" :disabled="deleting" @click="confirmDelete = false">Keep</button>
          <button type="button" class="btn" :disabled="deleting" @click="remove">{{ deleting ? 'Deleting…' : 'Delete fund' }}</button>
        </template>
        <button v-else type="button" class="btn" @click="confirmDelete = true">Delete…</button>
      </template>
    </SystemsFundForm>
  </div>
</template>

<style scoped>
.qs-fund-settings { display: flex; flex-direction: column; gap: var(--qss-gap); height: 100%; min-height: 0; }
.qs-fund-settings h1 { flex-shrink: 0; margin: 0; font-size: var(--qss-text-title); font-weight: 600; letter-spacing: -.01em; }
.qs-fund-settings__confirm { align-self: center; font-size: var(--qss-text-label); color: var(--qs-text-secondary); }
.qs-fund-settings__error { align-self: center; margin: 0; color: var(--qs-error); font-size: var(--qss-text-label); }
</style>
