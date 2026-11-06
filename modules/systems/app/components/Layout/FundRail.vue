<script setup lang="ts">
import { Landmark, Plus } from 'lucide-vue-next'
import { useAppStore } from '#systems/stores/app'
import { useFundsStore } from '#systems/stores/funds'
import { useActiveView } from '#systems/composables/useActiveView'

const app = useAppStore()
const funds = useFundsStore()
const router = useRouter()
const { fundId } = useActiveView()

function open(id: string) {
  const view = app.lastView === 'execution' ? 'live' : app.lastView
  void router.push(`/algo/manual/funds/${id}/${view}`)
}
onMounted(() => { void funds.load() })
</script>

<template>
  <section class="qs-fund-rail" aria-label="Funds">
    <div class="qs-fund-rail__heading">
      <span>Funds</span><span class="qs-fund-rail__count">{{ funds.funds.length }}</span>
    </div>
    <button class="btn qs-fund-rail__new" @click="router.push('/algo/manual/funds/new')"><Plus :size="14" /> New fund</button>
    <p v-if="funds.error" class="qs-fund-rail__error" role="alert">{{ funds.error }}</p>
    <button v-for="f in funds.funds" :key="f.id" class="qs-fund-rail__item" :class="{ 'qs-fund-rail__item--active': fundId === f.id }"
      :aria-current="fundId === f.id ? 'page' : undefined" :title="f.name" @click="open(f.id)">
      <Landmark :size="16" :stroke-width="1.7" aria-hidden="true" />
      <span class="qs-fund-rail__name">{{ f.name }}</span>
    </button>
  </section>
</template>

<style scoped>
.qs-fund-rail { display: flex; flex-direction: column; gap: 6px; padding-bottom: 12px; border-bottom: 1px solid var(--qs-border); }
.qs-fund-rail__heading { display: flex; align-items: center; justify-content: space-between; padding: 0 8px 6px; font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: .07em; color: var(--qs-text-muted); }
.qs-fund-rail__count { font-size: 10px; font-variant-numeric: tabular-nums; }
.qs-fund-rail__new { display: flex; align-items: center; justify-content: center; gap: 5px; font-size: 11px; }
.qs-fund-rail__item { display: flex; align-items: center; gap: 8px; width: 100%; min-width: 0; padding: 9px 8px; border: 0; border-radius: 6px; background: transparent; color: var(--qs-text-secondary); cursor: pointer; text-align: left; }
.qs-fund-rail__item:hover, .qs-fund-rail__item--active { background: var(--qs-bg-hover); color: var(--qs-text); }
.qs-fund-rail__item--active { box-shadow: inset 2px 0 var(--qs-accent); }
.qs-fund-rail__item:focus-visible { outline: 2px solid var(--qs-accent); outline-offset: -2px; }
.qs-fund-rail__item svg { flex-shrink: 0; color: var(--qs-accent); }
.qs-fund-rail__name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; }
.qs-fund-rail__error { margin: 0; padding: 0 8px; color: var(--qs-error); font-size: 11px; overflow-wrap: anywhere; }
</style>
