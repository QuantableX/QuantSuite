/**
 * The Usage page (card 18): estimated API cost and tokens from the agents'
 * own logs, and the subscription windows. Reports load when the page opens
 * or refreshes — never on a timer — and stay for the session; the period
 * and tab are a per-device preference.
 */
import { defineStore } from 'pinia'
import { pilot } from '#pilot/utils/invoke'
import type { ProviderLimits, UsagePeriod, UsageReport } from '#pilot/types'

export type UsageTab = 'cost' | 'tokens' | 'limits'

function load<T>(key: string, fallback: T): T {
  try {
    const raw = localStorage.getItem(key)
    return raw === null ? fallback : (JSON.parse(raw) as T)
  } catch {
    return fallback
  }
}
function save(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value))
  } catch {
    /* storage unavailable: it lives for this session */
  }
}

export const useUsageStore = defineStore('pilot/usage', {
  state: () => ({
    tab: load<UsageTab>('pilot.usage.tab', 'cost'),
    period: load<UsagePeriod>('pilot.usage.period', '7d'),
    reports: {} as Partial<Record<UsagePeriod, UsageReport>>,
    loading: false,
    error: '',
    limits: null as ProviderLimits[] | null,
    limitsLoading: false,
    /** The model whose trend the breakdown shows. */
    model: null as string | null,
    /** `/usage` in a thread: its agent's limits above the message box. */
    inline: null as string | null,
  }),
  getters: {
    report: (s) => s.reports[s.period] ?? null,
  },
  actions: {
    setTab(tab: UsageTab) {
      this.tab = tab
      save('pilot.usage.tab', tab)
      if (tab === 'limits') void this.loadLimits()
      else void this.load()
    },
    setPeriod(period: UsagePeriod) {
      this.period = period
      save('pilot.usage.period', period)
      void this.load()
    },
    async load(refresh = false) {
      if (this.loading || (!refresh && this.reports[this.period])) return
      this.loading = true
      this.error = ''
      const period = this.period
      try {
        const r = await pilot<UsageReport>('plugin:pilot|pilot_usage_report', { period, refresh })
        if (r) this.reports = { ...(refresh ? {} : this.reports), [period]: r }
      } catch (e) {
        this.error = e instanceof Error ? e.message : String(e)
      } finally {
        this.loading = false
      }
    },
    async loadLimits(refresh = false) {
      if (this.limitsLoading || (!refresh && this.limits)) return
      this.limitsLoading = true
      try {
        this.limits = (await pilot<ProviderLimits[]>('plugin:pilot|pilot_usage_limits', { refresh })) ?? []
      } catch (e) {
        this.error = e instanceof Error ? e.message : String(e)
      } finally {
        this.limitsLoading = false
      }
    },
    refresh() {
      return this.tab === 'limits' ? this.loadLimits(true) : this.load(true)
    },
  },
})
