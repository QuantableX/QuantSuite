// @ts-check
// One framework-free board for the drawer and QuantMCP's /progress page.
// Hosts call updateBoard after polling; it preserves disclosure state and focus.

/** @type {Record<string, string>} */
const ESC = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }
/** @param {unknown} value */
function esc(value) {
  return String(value ?? '').replace(/[&<>"']/g, (c) => ESC[c] ?? c)
}
/** @param {unknown} value */
function fraction(value) {
  return typeof value === 'number' && Number.isFinite(value) ? Math.max(0, Math.min(1, value)) : null
}
/** @param {number} ms */
export function formatDuration(ms) {
  const seconds = Math.max(0, Math.round(ms / 1000))
  if (seconds < 60) return `${seconds}s`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m`
  const hours = Math.floor(minutes / 60)
  const rest = minutes % 60
  return hours < 10 && rest ? `${hours}h ${rest}m` : `${hours}h`
}
/** @param {number} value */
export function percentText(value) {
  return `${Math.floor((fraction(value) ?? 0) * 100)}%`
}
/** @param {any} task @param {number} now */
export function timeLine(task, now) {
  const elapsed = formatDuration((task.finishedAt ?? now) - task.startedAt)
  if (task.state === 'done') return `Done in ${elapsed}`
  if (task.state === 'failed') return `Failed after ${elapsed}`
  return `${elapsed} elapsed`
}
/** @param {any} task @param {number} now */
function isStale(task, now) {
  return task.state === 'running' && (task.stale || now - Math.max(task.updatedAt ?? 0, task.followChangedAt ?? 0) > 15 * 60_000)
}

/** Display work units, never turn equally weighted steps into a time estimate.
 * @param {any} task
 */
function progress(task) {
  const steps = Number.isInteger(task.steps) && task.steps > 0 ? task.steps : null
  const step = steps ? Math.max(1, Math.min(steps, task.step ?? 1)) : 1
  const fromLog = fraction(task.followFraction)
  const reported = steps ? fraction(task.stepFraction) : fraction(task.percent == null ? task.stepFraction : task.percent / 100)
  const value = fromLog ?? reported
  if (task.state === 'done') return { value: 1, label: '100%', caption: 'Completed', steps: null }
  if (steps && value == null) {
    const completed = step - 1
    return { value: completed / steps, label: `${completed}/${steps}`, caption: 'Steps completed · current step unmeasured', steps }
  }
  const scope = steps ? 'Current step' : 'Progress'
  const source = fromLog != null ? 'from log' : 'agent estimate'
  // A log may reach 100% before its parent step/task has actually finished.
  const waiting = value === 1 && task.state === 'running'
  return {
    value,
    label: value == null ? '—' : percentText(value),
    caption: value == null ? 'Progress not reported' : `${scope} · ${source}${waiting ? ' · awaiting completion' : ''}`,
    steps: null,
  }
}

/** @param {any} task @param {number} now */
function renderTask(task, now) {
  const stale = isStale(task, now)
  const state = stale ? 'stale' : task.state
  const status = stale ? 'No recent update' : task.state === 'done' ? 'Done' : task.state === 'failed' ? 'Failed' : 'Running'
  const p = progress(task)
  const known = p.value != null
  const width = (p.value ?? 0) * 100
  const step = task.steps ? `Step ${task.step ?? 1} of ${task.steps}` : task.step ? `Step ${task.step}` : ''
  const description = [step, task.stepText].filter(Boolean).join(' · ')
  const quietSince = Math.max(task.updatedAt ?? 0, task.followChangedAt ?? 0)
  const freshness = task.state === 'running' ? `${stale ? 'No update for' : 'Updated'} ${formatDuration(now - quietSince)}${stale ? '' : ' ago'}` : ''
  const estimate = typeof task.estimateSecs === 'number' && task.estimateSecs > 0 && task.state === 'running'
    ? `Agent estimate: ${formatDuration(task.estimateSecs * 1000)} total${now - task.startedAt > task.estimateSecs * 1000 ? ' · exceeded' : ''}` : ''
  const note = task.state !== 'running' && task.note ? `<p class="qpb-note">${esc(task.note)}</p>` : ''
  const aria = known ? ` aria-valuenow="${width.toFixed(1)}" aria-valuemin="0" aria-valuemax="100"` : ''
  const segments = p.steps && p.steps <= 20 ? ' is-steps' : ''
  const style = segments ? ` style="--qpb-steps:${p.steps}"` : ''
  return `<article class="qpb-task is-${esc(state)}">` +
    `<div class="qpb-task-head"><h3 class="qpb-title">${esc(task.title)}</h3><span class="qpb-status">${status}</span></div>` +
    (description ? `<p class="qpb-step">${esc(description)}</p>` : '') +
    `<div class="qpb-progress-label"><span>${esc(p.caption)}</span><strong class="qpb-pct">${esc(p.label)}</strong></div>` +
    `<div class="qpb-bar${!known && task.state === 'running' ? ' is-indeterminate' : ''}${segments}"${style} role="progressbar" aria-label="${esc(task.title)}" aria-valuetext="${esc(`${p.caption}: ${p.label}`)}"${aria}><span style="width:${width.toFixed(1)}%"></span></div>` +
    `<div class="qpb-meta"><span class="qpb-time">${esc(timeLine(task, now))}</span><span class="qpb-freshness">${esc(freshness)}</span></div>` +
    (estimate ? `<p class="qpb-estimate">${esc(estimate)}</p>` : '') + note + '</article>'
}

/** @param {any} shell @param {number} now */
function renderShell(shell, now) {
  const running = shell.endedAt == null
  const kind = running ? (shell.background ? 'background' : 'running') : shell.failed ? 'failed' : 'ended'
  const time = running
    ? `${shell.background ? 'Background' : 'Running'} · ${formatDuration(now - shell.startedAt)}`
    : `${shell.failed ? 'Failed' : 'Done'} · ${formatDuration(shell.endedAt - shell.startedAt)}`
  return `<li class="qpb-shell is-${kind}" title="${esc(shell.cwd ?? '')}"><code class="qpb-cmd">${esc(shell.command)}</code><span class="qpb-shell-time">${esc(time)}</span></li>`
}

/** @param {any} lane @param {number} now */
function renderLane(lane, now) {
  const priority = (/** @type {any} */ t) => t.state === 'running' ? (isStale(t, now) ? 1 : 0) : 2
  const tasks = [...(lane.tasks ?? [])].sort((a, b) => priority(a) - priority(b))
  const shells = lane.shells ?? []
  const logs = tasks.filter((/** @type {any} */ t) => t.detail).map((/** @type {any} */ t) =>
    `<div class="qpb-log"><span>${esc(t.title)}</span><code>${esc(t.detail)}</code></div>`).join('')
  const details = shells.length || logs
    ? `<details class="qpb-technical" data-qpb-key="${esc(`lane:${lane.key}`)}"><summary>Technical details<span>${shells.length ? `${shells.length} command${shells.length === 1 ? '' : 's'}` : 'Logs'}</span></summary><div class="qpb-technical-body">${logs}${shells.length ? `<ul class="qpb-shells">${shells.map((/** @type {any} */ s) => renderShell(s, now)).join('')}</ul>` : ''}</div></details>` : ''
  return `<section class="qpb-lane"><header class="qpb-lane-head"><span class="qpb-agent">${esc(lane.agent)}</span><span class="qpb-place" title="${esc(lane.folder)}">${esc(lane.place)}</span></header>` +
    tasks.map((/** @type {any} */ t) => renderTask(t, now)).join('') + details + '</section>'
}

/** @param {any} view @param {{ empty?: string }} [options] */
export function renderBoard(view, options = {}) {
  const now = typeof view?.now === 'number' ? view.now : Date.now()
  const lanes = Array.isArray(view?.lanes) ? view.lanes : []
  const reporting = lanes.filter((/** @type {any} */ l) => l.tasks?.length)
  const background = lanes.filter((/** @type {any} */ l) => !l.tasks?.length && l.shells?.length)
  const tasks = reporting.flatMap((/** @type {any} */ l) => l.tasks)
  const running = tasks.filter((/** @type {any} */ t) => t.state === 'running' && !isStale(t, now)).length
  const stale = tasks.filter((/** @type {any} */ t) => isStale(t, now)).length
  const failed = tasks.filter((/** @type {any} */ t) => t.state === 'failed').length
  const done = tasks.filter((/** @type {any} */ t) => t.state === 'done').length
  const summary = [running ? `${running} running` : '', stale ? `${stale} awaiting update` : '', failed ? `${failed} failed` : '', done ? `${done} completed` : ''].filter(Boolean).join(' · ')
  const empty = options.empty ?? 'No agent is reporting work right now.'
  return (summary ? `<p class="qpb-summary">${summary}</p>` : '') +
    `<div class="qpb-board">${reporting.length ? reporting.map((/** @type {any} */ l) => renderLane(l, now)).join('') : `<p class="qpb-empty">${esc(empty)}</p>`}</div>` +
    (background.length ? `<details class="qpb-background" data-qpb-key="background"><summary>Other activity<span>${background.length} session${background.length === 1 ? '' : 's'} without a progress report</span></summary><div class="qpb-board">${background.map((/** @type {any} */ l) => renderLane(l, now)).join('')}</div></details>` : '')
}

/** Refresh without closing details, moving their scroll position or losing keyboard focus.
 * @param {HTMLElement} root @param {any} view @param {{ empty?: string }} [options]
 */
export function updateBoard(root, view, options = {}) {
  const details = Array.from(root.querySelectorAll('details[data-qpb-key]'))
  const active = root.ownerDocument.activeElement
  const focused = details.find((el) => el.querySelector('summary') === active)?.getAttribute('data-qpb-key')
  const opened = new Set(details.filter((el) => el.hasAttribute('open')).map((el) => el.getAttribute('data-qpb-key')))
  const scroll = new Map(details.map((el) => [el.getAttribute('data-qpb-key'), el.querySelector('.qpb-technical-body')?.scrollTop ?? 0]))
  root.innerHTML = renderBoard(view, options)
  for (const el of root.querySelectorAll('details[data-qpb-key]')) {
    const key = el.getAttribute('data-qpb-key')
    if (opened.has(key)) el.setAttribute('open', '')
    const body = el.querySelector('.qpb-technical-body')
    if (body) body.scrollTop = scroll.get(key) ?? 0
    if (key === focused) el.querySelector('summary')?.focus({ preventScroll: true })
  }
}
