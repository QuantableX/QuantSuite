/**
 * The forge page's reading of a job: the child's JSON lines folded into one
 * row per indicator (gauntlet / walk-forward) or per series (shelf refresh).
 * Pure functions — the store keeps the jobs, the components call these on
 * render. Moved from QuantAlgo with the forge (docs/PLAN-QUANTSCRIPT.md §2).
 */
import type { ForgeEvent, ForgeJob } from '#script/types'

export const AXES = ['asset', 'exchange', 'parameter', 'temporal', 'monte_carlo'] as const
export type AxisKey = (typeof AXES)[number]

export const AXIS_LABELS: Record<AxisKey, string> = {
  asset: 'Asset',
  exchange: 'Exchange',
  parameter: 'Parameter',
  temporal: 'Temporal',
  monte_carlo: 'Monte Carlo',
}

export type AxisStatus = 'pending' | 'running' | 'done' | 'skipped'

export interface AxisProgress {
  status: AxisStatus
  score: number | null
  elapsed: number | null
}

export interface VerdictEvent {
  score: number
  grade: string
  perm_p: number | null
  certified: boolean
  report: string
  report_name: string
  elapsed_s: number
  scores: Record<string, number | null>
  reasons: string[]
}

export interface FoldEvent {
  fold: number
  train_bars: number
  test_bars: number
  chosen: Record<string, number>
  is_best_sharpe: number
  is_selected_sharpe: number
}

export interface WalkforwardEvent {
  series: string
  oos_sharpe: number
  is_sharpe_mean: number
  wfe: number | null
  oos_total_log_ret: number
  final_choice: Record<string, number>
  defaults: Record<string, unknown>
  elapsed_s: number
}

export interface IndicatorProgress {
  /** The row key — the indicator, or `indicator@track` in an all-tracks job. */
  key: string
  indicator: string
  /** The certification track this row runs on (null when the job runs one track). */
  track: string | null
  name: string | null
  params: Record<string, unknown> | null
  contract: 'pending' | 'running' | 'passed' | 'failed'
  contractMessage: string | null
  primary: string | null
  axes: Record<AxisKey, AxisProgress>
  verdict: VerdictEvent | null
  error: string | null
  folds: FoldEvent[]
  walkforward: WalkforwardEvent | null
  started: boolean
}

export interface SeriesProgress {
  key: string
  ok: boolean
  status: string
}

function emptyAxes(): Record<AxisKey, AxisProgress> {
  return {
    asset: { status: 'pending', score: null, elapsed: null },
    exchange: { status: 'pending', score: null, elapsed: null },
    parameter: { status: 'pending', score: null, elapsed: null },
    temporal: { status: 'pending', score: null, elapsed: null },
    monte_carlo: { status: 'pending', score: null, elapsed: null },
  }
}

function num(v: unknown): number | null {
  return typeof v === 'number' && Number.isFinite(v) ? v : null
}

function str(v: unknown): string | null {
  return typeof v === 'string' ? v : null
}

/**
 * One row per indicator of a gauntlet or walk-forward job, in request order —
 * one row per indicator AND track when the job runs every track (`all`).
 */
export function progressFor(job: ForgeJob): IndicatorProgress[] {
  const rows = new Map<string, IndicatorProgress>()
  const jobEvent = job.events.find((e) => e.event === 'job') ?? job.summary.find((e) => e.event === 'job')
  const multiTrack = (job.request.timeframe ?? str(jobEvent?.timeframe) ?? '1d') === 'all'
  const tracks = Array.isArray(jobEvent?.tracks) ? (jobEvent.tracks as string[]) : ['1d', '4h', '1h', '1m']
  const row = (indicator: string, track: string | null): IndicatorProgress => {
    const key = multiTrack && track ? `${indicator}@${track}` : indicator
    let r = rows.get(key)
    if (!r) {
      r = {
        key,
        indicator,
        track: multiTrack ? track : null,
        name: null,
        params: null,
        contract: 'pending',
        contractMessage: null,
        primary: null,
        axes: emptyAxes(),
        verdict: null,
        error: null,
        folds: [],
        walkforward: null,
        started: false,
      }
      rows.set(key, r)
    }
    return r
  }
  // `all` / `certified` expand inside the forge; the `job` event names the
  // real list, the request is the fallback until it arrives.
  const listed = Array.isArray(jobEvent?.indicators) ? (jobEvent.indicators as string[]) : job.indicators
  for (const key of listed) {
    if (key === 'all' || key === 'certified') continue
    if (multiTrack) for (const t of tracks) row(key, t)
    else row(key, null)
  }

  // Axis events carry no track; the last `begin` of an indicator says which
  // track it is on.
  const currentTrack = new Map<string, string>()
  const events: ForgeEvent[] = job.events.length ? job.events : job.summary
  for (const e of events) {
    const key = str(e.indicator)
    if (!key) continue
    const tf = str(e.timeframe)
    if (tf) currentTrack.set(key, tf)
    const r = row(key, tf ?? currentTrack.get(key) ?? tracks[0] ?? null)
    switch (e.event) {
      case 'begin':
        r.started = true
        r.name = str(e.name)
        r.params = (e.params as Record<string, unknown>) ?? null
        break
      case 'contract': {
        const status = str(e.status)
        r.primary = str(e.primary)
        r.contract = status === 'passed' ? 'passed' : status === 'failed' ? 'failed' : 'running'
        r.contractMessage = str(e.message)
        break
      }
      case 'axis': {
        const axis = str(e.axis) as AxisKey | null
        if (!axis || !(axis in r.axes)) break
        const status = str(e.status)
        r.axes[axis] = {
          status: status === 'done' ? 'done' : status === 'skipped' ? 'skipped' : 'running',
          score: num(e.score),
          elapsed: num(e.elapsed_s),
        }
        break
      }
      case 'verdict':
        r.verdict = {
          score: num(e.score) ?? 0,
          grade: str(e.grade) ?? '',
          perm_p: num(e.perm_p),
          certified: e.certified === true,
          report: str(e.report) ?? '',
          report_name: str(e.report_name) ?? '',
          elapsed_s: num(e.elapsed_s) ?? 0,
          scores: (e.scores as Record<string, number | null>) ?? {},
          reasons: Array.isArray(e.reasons) ? e.reasons.filter((s): s is string => typeof s === 'string') : [],
        }
        break
      case 'fold':
        r.started = true
        r.folds.push({
          fold: num(e.fold) ?? r.folds.length + 1,
          train_bars: num(e.train_bars) ?? 0,
          test_bars: num(e.test_bars) ?? 0,
          chosen: (e.chosen as Record<string, number>) ?? {},
          is_best_sharpe: num(e.is_best_sharpe) ?? 0,
          is_selected_sharpe: num(e.is_selected_sharpe) ?? num(e.is_best_sharpe) ?? 0,
        })
        break
      case 'walkforward':
        r.walkforward = {
          series: str(e.series) ?? '',
          oos_sharpe: num(e.oos_sharpe) ?? 0,
          is_sharpe_mean: num(e.is_sharpe_mean) ?? 0,
          wfe: num(e.wfe),
          oos_total_log_ret: num(e.oos_total_log_ret) ?? 0,
          final_choice: (e.final_choice as Record<string, number>) ?? {},
          defaults: (e.defaults as Record<string, unknown>) ?? {},
          elapsed_s: num(e.elapsed_s) ?? 0,
        }
        break
      case 'error':
        r.error = str(e.message) ?? 'failed'
        break
      default:
        break
    }
  }
  // A queued track's row has not seen its `begin` yet — borrow the name from
  // a sibling row of the same indicator so the table never reads "dc dc".
  const names = new Map<string, string>()
  for (const r of rows.values()) if (r.name) names.set(r.indicator, r.name)
  for (const r of rows.values()) if (!r.name) r.name = names.get(r.indicator) ?? null
  return [...rows.values()]
}

/** The series a shelf refresh reported, in the order they came back. */
export function seriesFor(job: ForgeJob): SeriesProgress[] {
  const events: ForgeEvent[] = job.events.length ? job.events : job.summary
  return events
    .filter((e) => e.event === 'series')
    .map((e) => ({ key: str(e.key) ?? '', ok: e.ok === true, status: str(e.status) ?? '' }))
}

/** The plain lines a job printed (`log` events and the raw stdout/stderr). */
export function logLinesFor(job: ForgeJob): string[] {
  const fromEvents = job.events.filter((e) => e.event === 'log').map((e) => str(e.message) ?? '')
  return [...fromEvents, ...job.log]
}

export function jobError(job: ForgeJob): string | null {
  if (job.error) return job.error
  const e = [...job.summary].reverse().find((ev) => ev.event === 'error' && !ev.indicator)
  return e ? (str(e.message) ?? 'failed') : null
}

export function formatP(p: number | null | undefined): string {
  return p == null ? '—' : p.toFixed(3)
}

export function formatElapsed(startedAt: string, finishedAt: string | null): string {
  const start = Date.parse(startedAt)
  const end = finishedAt ? Date.parse(finishedAt) : Date.now()
  if (!Number.isFinite(start) || !Number.isFinite(end)) return ''
  const s = Math.max(0, Math.round((end - start) / 1000))
  return s < 60 ? `${s}s` : `${Math.floor(s / 60)}m ${s % 60}s`
}

export const KIND_LABELS: Record<string, string> = {
  gauntlet: 'Gauntlet',
  walkforward: 'Walk-forward',
  compare: 'Comparison',
  refresh: 'Shelf refresh',
}
