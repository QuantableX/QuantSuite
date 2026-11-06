<script setup lang="ts">
definePageMeta({ layout: 'systems', path: '/algo/manual/funds/new' })

import SystemsFundForm from '#systems/components/Fund/Form.vue'

const router = useRouter()
// The page is kept alive: a fresh form each time a fund was saved or the page was left.
const formKey = ref(0)
function saved(id: string) {
  formKey.value++
  void router.push(`/algo/manual/funds/${id}/backtest`)
}
function cancel() {
  formKey.value++
  void router.push('/algo/manual')
}
</script>

<template>
  <div class="qs-fund-new">
    <header class="qs-fund-new__head">
      <h1>New fund</h1>
      <p>One amount, split across several systems. A fund plans and evaluates; it places no orders.</p>
    </header>
    <SystemsFundForm :key="formKey" @saved="fund => saved(fund.id)" @cancel="cancel" />
  </div>
</template>

<style scoped>
.qs-fund-new { display: flex; flex-direction: column; gap: var(--qss-gap); height: 100%; min-height: 0; }
.qs-fund-new__head { flex-shrink: 0; display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px var(--qss-gap); }
.qs-fund-new h1 { margin: 0; font-size: var(--qss-text-title); font-weight: 600; letter-spacing: -.01em; }
.qs-fund-new p { margin: 0; color: var(--qs-text-secondary); font-size: var(--qss-text-label); }
</style>
