/** All amounts are integer cents in the module's display currency. */
export interface FundInput {
  name: string
  openedOn: string
  openingCents: number
  openingValueCents: number
  targetCents: number | null
  notes: string
}
export type FundEntryKind = 'deposit' | 'withdrawal' | 'valuation'
export interface FundEntryInput {
  fundId: string
  kind: FundEntryKind
  amountCents: number
  occurredOn: string
  notes: string
}
export interface FundEntry extends Omit<FundEntryInput, 'fundId'> {
  id: string
  planId: string | null
}
export interface FundPlanInput {
  fundId: string
  name: string
  amountCents: number
  everyMonths: number
  nextOn: string
  isActive: boolean
}
export interface FundPlan extends FundPlanInput { id: string }
export interface Fund extends FundInput {
  id: string
  depositedCents: number
  withdrawnCents: number
  netInputCents: number
  currentValueCents: number
  gainCents: number
  valueAsOf: string
  entries: FundEntry[]
  plans: FundPlan[]
}
