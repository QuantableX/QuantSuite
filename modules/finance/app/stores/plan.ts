import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import type {
  GoalProgress,
  Item,
  ItemInput,
  ItemKind,
  ItemPatch,
  SankeyData,
  Summary,
  TargetPatch,
} from '#finance/types'

const EMPTY_SUMMARY: Summary = {
  incomeCents: 0,
  savingCents: 0,
  expenseCents: 0,
  leftoverCents: 0,
  savingsRate: null,
}

/**
 * The whole module in one store: the plan, its totals, its flow and its
 * targets. They are four views of the same handful of rows, so splitting them
 * would only create four places to remember to refresh.
 */
export const usePlanStore = defineStore('finance/plan', () => {
  const items = ref<Item[]>([])
  const summary = ref<Summary>({ ...EMPTY_SUMMARY })
  const sankey = ref<SankeyData | null>(null)
  const goals = ref<GoalProgress[]>([])
  const loading = ref(false)
  const error = ref('')
  let request = 0

  const income = computed(() => items.value.filter((i) => i.kind === 'income'))
  const saving = computed(() => items.value.filter((i) => i.kind === 'saving'))
  const expense = computed(() => items.value.filter((i) => i.kind === 'expense'))

  function of(kind: ItemKind): Item[] {
    if (kind === 'income') return income.value
    if (kind === 'saving') return saving.value
    return expense.value
  }

  /** True when the plan spends more than it takes in — worth shouting about. */
  const overspent = computed(() => summary.value.leftoverCents < 0)

  async function load() {
    const revision = ++request
    loading.value = true
    try {
      const [rows, totals, chart, targets] = await Promise.all([
        invoke<Item[]>('plugin:finance|list_items'),
        invoke<Summary>('plugin:finance|summary'),
        invoke<SankeyData>('plugin:finance|sankey'),
        invoke<GoalProgress[]>('plugin:finance|goals'),
      ])
      if (revision !== request) return
      items.value = rows
      summary.value = totals
      sankey.value = chart
      goals.value = targets
      error.value = ''
    } catch (e) {
      if (revision === request) error.value = `Could not load the plan: ${String(e)}`
    } finally {
      if (revision === request) loading.value = false
    }
  }

  async function add(kind: ItemKind, name = '') {
    const created = await invoke<Item>('plugin:finance|create_item', {
      item: { kind, name, amountCents: 0, everyMonths: 1 } satisfies ItemInput,
    })
    await load()
    return created
  }

  /**
   * Patch one line.
   *
   * Optimistic on the row itself so a typed figure sticks while the write
   * lands, then a full reload — every total, the chart and the targets all
   * move with a single amount, and recomputing them here would be a second
   * implementation of the crate's arithmetic.
   */
  async function update(id: string, patch: ItemPatch) {
    error.value = ''
    const at = items.value.findIndex((i) => i.id === id)
    if (at !== -1) items.value[at] = { ...items.value[at]!, ...patch } as Item
    try {
      await invoke<Item>('plugin:finance|update_item', { id, patch })
      await load()
    } catch (e) {
      await load()
      error.value = String(e)
    }
  }

  async function remove(id: string) {
    items.value = items.value.filter((i) => i.id !== id)
    await invoke<boolean>('plugin:finance|delete_item', { id })
    await load()
  }

  // ── Standalone targets ────────────────────────────────────────────────
  //
  // Not funded by the plan and not part of any total — for money coming out of
  // the leftover or a bonus, or a pot that is only being watched.

  async function addTarget(name: string) {
    await invoke<boolean>('plugin:finance|create_target', { name })
    await load()
  }

  async function updateTarget(id: string, patch: TargetPatch) {
    // Optimistic on the row so a typed figure sticks while the write lands.
    const at = goals.value.findIndex((g) => g.id === id && g.source === 'manual')
    if (at !== -1) goals.value[at] = { ...goals.value[at]!, ...patch }
    await invoke<boolean>('plugin:finance|update_target', { id, patch })
    await load()
  }

  async function removeTarget(id: string) {
    goals.value = goals.value.filter((g) => !(g.id === id && g.source === 'manual'))
    await invoke<boolean>('plugin:finance|delete_target', { id })
    await load()
  }

  return {
    items,
    summary,
    sankey,
    goals,
    loading,
    error,
    income,
    saving,
    expense,
    overspent,
    of,
    load,
    add,
    update,
    remove,
    addTarget,
    updateTarget,
    removeTarget,
  }
})
