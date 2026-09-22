import { computed, ref, shallowRef } from 'vue'
import { defineStore } from 'pinia'
import { invoke } from '@tauri-apps/api/core'
import type { Fund, FundEntryInput, FundInput, FundPlanInput } from '../types/funds'

type EditorRequest =
  | { kind: 'fund'; id?: string }
  | { kind: 'plan'; fundId: string; id?: string }
  | { kind: 'delete'; recordKind: 'fund' | 'entry' | 'plan'; id: string; name: string }

export const useFundsStore = defineStore('finance/funds', () => {
  const funds = ref<Fund[]>([])
  const selectedId = ref<string | null>(null)
  const loading = ref(false)
  const busy = ref(false)
  const error = ref('')
  const loaded = ref(false)
  // The Funds page owns one dialog, opened from either sidebar or its content.
  const editorRequest = shallowRef<EditorRequest | null>(null)
  let request = 0
  const selected = computed(() => funds.value.find((f) => f.id === selectedId.value) ?? null)
  const totals = computed(() => funds.value.reduce((sum, fund) => ({
    value: sum.value + fund.currentValueCents,
    input: sum.input + fund.netInputCents,
    gain: sum.gain + fund.gainCents,
  }), { value: 0, input: 0, gain: 0 }))

  async function load() {
    const revision = ++request
    loading.value = true
    try {
      const rows = await invoke<Fund[]>('plugin:finance|funds_overview')
      if (revision !== request) return
      funds.value = rows
      if (!rows.some((f) => f.id === selectedId.value)) selectedId.value = rows[0]?.id ?? null
      loaded.value = true
      error.value = ''
    } catch (e) {
      if (revision === request) error.value = `Could not load funds: ${String(e)}`
    } finally {
      if (revision === request) loading.value = false
    }
  }

  async function write(command: string, args: Record<string, unknown>) {
    if (busy.value) return false
    busy.value = true
    error.value = ''
    try {
      const result = await invoke<unknown>(command, args)
      if (command === 'plugin:finance|save_fund_account' && typeof result === 'string') selectedId.value = result
      await load()
      // A completed write must not be resubmitted if only the refresh failed.
      return true
    } catch (e) {
      error.value = String(e)
      return false
    } finally { busy.value = false }
  }

  return {
    funds, selectedId, selected, totals, loading, busy, loaded, error, load, editorRequest,
    openEditor: (request: EditorRequest) => { editorRequest.value = request },
    saveFund: (input: FundInput, id: string | null = null) => write('plugin:finance|save_fund_account', { id, input }),
    addEntry: (input: FundEntryInput) => write('plugin:finance|add_fund_entry', { input }),
    savePlan: (input: FundPlanInput, id: string | null = null) => write('plugin:finance|save_fund_plan', { id, input }),
    bookPlan: (id: string, expectedOn: string) => write('plugin:finance|book_fund_plan', { id, expectedOn }),
    remove: (kind: 'fund' | 'entry' | 'plan', id: string) => write('plugin:finance|delete_fund_record', { kind, id }),
  }
})
