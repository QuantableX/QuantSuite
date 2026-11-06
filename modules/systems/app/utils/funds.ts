import { formatMoney } from '@quantsuite/core'
import type { FundMethod, FundRebalance } from '#systems/types'

export const FUND_METHODS: { id: FundMethod; label: string; hint: string }[] = [
  { id: 'equal', label: 'Equal', hint: 'The same share for every system.' },
  { id: 'fixed', label: 'Fixed %', hint: 'Your own shares. What you leave out stays in cash.' },
  { id: 'profit', label: 'Profit', hint: 'More for the systems that made more over the lookback.' },
  { id: 'sharpe', label: 'Sharpe', hint: 'More for return per risk over the lookback.' },
  { id: 'sortino', label: 'Sortino', hint: 'More for return per downside risk over the lookback.' },
  { id: 'return_drawdown', label: 'Return / drawdown', hint: 'More for return per worst drop over the lookback.' },
  { id: 'inverse_volatility', label: 'Inverse volatility', hint: 'The calmer the system, the larger its share. Ignores profit.' },
  { id: 'kelly', label: 'Kelly', hint: 'Mean return per variance, times a fraction. Never leverage; under 100% the rest stays in cash.' },
  { id: 'combined', label: 'Combined', hint: 'Rank points for profit, Sharpe and drawdown, added up.' },
]
export const FUND_LOOKBACKS = [30, 90, 180, 365]
export const FUND_REBALANCES: { id: FundRebalance; label: string }[] = [
  { id: 'never', label: 'Never' },
  { id: 'weekly', label: 'Weekly' },
  { id: 'monthly', label: 'Monthly' },
  { id: 'quarterly', label: 'Quarterly' },
]
export const KELLY_FRACTIONS = [{ value: 0.25, label: '1/4' }, { value: 0.5, label: '1/2' }, { value: 1, label: 'Full' }]

export function methodLabel(method: string) {
  return FUND_METHODS.find(m => m.id === method)?.label ?? method
}

/** A method that reads the lookback and takes the cap: Equal and Fixed % have shares of their own. */
export function usesPerformance(method: FundMethod) {
  return method !== 'equal' && method !== 'fixed'
}

export function money(amount: number, digits = 2) {
  return formatMoney(amount, 'USD', { minimumFractionDigits: digits, maximumFractionDigits: digits })
}

export function percent(value: number | null | undefined, digits = 1) {
  return value == null || !Number.isFinite(value) ? '—' : `${(value * 100).toFixed(digits)}%`
}
