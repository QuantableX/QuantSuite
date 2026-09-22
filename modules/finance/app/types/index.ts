/**
 * QuantFinance's shared types — the mirror of the crate's serde structs.
 *
 * Monthly budget and savings-target types. Dated fund tracking is defined
 * separately in `funds.ts`.
 *
 * **Every amount is an integer number of cents.** `number` here always means
 * cents, never euros; formatting lives in `utils/money.ts` and nowhere else.
 */

/** What a line does to the money. */
export type ItemKind = 'income' | 'saving' | 'expense'

/** Keys into `--qf-flow-*`; never a hex value, which no theme change survives. */
export type FlowColor =
  | 'income'
  | 'savings'
  | 'invest'
  | 'housing'
  | 'food'
  | 'transport'
  | 'fun'
  | 'health'
  | 'other'
  | 'leftover'

/** 1 monthly · 3 quarterly · 6 half-yearly · 12 yearly. */
export type Cadence = 1 | 3 | 6 | 12

export interface Item {
  id: string
  fundId: string | null
  fundPlanCount: number
  kind: ItemKind
  name: string
  /** cents per occurrence — NOT per month. */
  amountCents: number
  everyMonths: number
  /** The same amount reduced to a month. Computed by the crate; never here. */
  monthlyCents: number
  /** Derived from `kind` by the crate — read-only here. */
  color: FlowColor
  notes: string | null
  isActive: boolean
  sortIndex: number
  /** Saving lines only: the number to reach, and what is already put aside. */
  targetCents: number | null
  savedCents: number
  createdAt: string
  updatedAt: string
}

export interface ItemInput {
  kind: ItemKind
  name: string
  amountCents: number
  everyMonths?: number
  notes?: string | null
  targetCents?: number | null
}

export interface ItemPatch {
  kind?: ItemKind
  name?: string
  amountCents?: number
  everyMonths?: number
  notes?: string | null
  isActive?: boolean
  sortIndex?: number
  targetCents?: number | null
  savedCents?: number
}

/** The four figures the module exists to produce. */
export interface Summary {
  incomeCents: number
  savingCents: number
  expenseCents: number
  /** income − saving − expense. Negative when the plan does not add up. */
  leftoverCents: number
  savingsRate: number | null
}

/**
 * A target, from either of the two places one can come from.
 *
 * `plan` — a saving line with a number to reach; its rate is that line's own
 * monthly amount, so it is edited on the Plan page.
 * `manual` — standalone, with a rate typed by hand. Touches no total.
 */
export interface GoalProgress {
  id: string
  source: 'plan' | 'manual'
  name: string
  color: FlowColor
  targetCents: number
  savedCents: number
  monthlyCents: number
  remainingCents: number
  monthsLeft: number | null
  /** `YYYY-MM` the target is reached, or null. */
  reachedOn: string | null
}

/** Standalone targets only — a plan-sourced one is patched through its item. */
export interface TargetPatch {
  name?: string
  targetCents?: number
  savedCents?: number
  monthlyCents?: number
  sortIndex?: number
}

// ─── Sankey ─────────────────────────────────────────────────────────────

export interface SankeyNode {
  id: string
  fundId: string | null
  label: string
  kind: 'income' | 'hub' | 'saving' | 'expense' | 'leftover'
  valueCents: number
  color: FlowColor
  /** 0 income lines · 1 the hub · 2 saving, fixed costs and the leftover. */
  layer: number
}

export interface SankeyLink {
  source: string
  target: string
  valueCents: number
  color: FlowColor
}

export interface SankeyData extends Summary {
  nodes: SankeyNode[]
  links: SankeyLink[]
}

export interface AppSettings {
  currency: string
  locale: string
  sidebarLeftOpen: boolean
  sidebarRightOpen: boolean
}

// ─── Laid-out Sankey geometry ───────────────────────────────────────────

export interface LaidOutNode extends SankeyNode {
  x: number
  y: number
  width: number
  height: number
}

export interface LaidOutLink extends SankeyLink {
  /** A closed path: top edge out, bottom edge back. */
  path: string
  midX: number
  midY: number
}

export interface SankeyLayout {
  nodes: LaidOutNode[]
  links: LaidOutLink[]
  width: number
  height: number
}
