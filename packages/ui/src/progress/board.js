// @ts-check
/**
 * Agent status — the ONE template of the progress board
 * (docs/PLAN-AGENT-STATUS.md). Two hosts render it and must look the same:
 *
 *   - the drawer's Agent › Status view (QDrawerAgent.vue), and
 *   - the page QuantMCP serves at /progress (modules/mcp/crate/src/progress.rs
 *     embeds this file and board.css with include_str! and serves them as is).
 *
 * So: plain JavaScript, no imports, no framework. A host fetches the board
 * (`plugin:mcp|progress_board` or GET /progress/state) once a second and puts
 * `renderBoard(view)` into its container. Every value is escaped here.
 */

/** @type {Record<string, string>} */
const ESC = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }

/** @param {unknown} value */
function esc(value) {
  return String(value ?? '').replace(/[&<>"']/g, (c) => ESC[c] ?? c)
}

/**
 * 45s · 12m · 1h 10m · 14h
 * @param {number} ms
 */
export function formatDuration(ms) {
  const seconds = Math.max(0, Math.round(ms / 1000))
  if (seconds < 60) return `${seconds}s`
  const minutes = Math.floor(seconds / 60)
  if (minutes < 60) return `${minutes}m`
  const hours = Math.floor(minutes / 60)
  const rest = minutes % 60
  if (hours < 10 && rest) return `${hours}h ${rest}m`
  return `${hours}h`
}

/**
 * Floored, so a running bar never claims 100 %.
 * @param {number} fraction
 */
export function percentText(fraction) {
  return `${Math.floor(Math.max(0, Math.min(1, fraction)) * 100)}%`
}

/** @param {any} task */
function stepLine(task) {
  const parts = []
  if (task.step != null && task.steps != null) parts.push(`Step ${task.step}/${task.steps}`)
  else if (task.step != null) parts.push(`Step ${task.step}`)
  if (task.stepText) parts.push(task.stepText)
  return parts.join(' · ')
}

/**
 * elapsed / ~total while running, what it took when finished.
 * @param {any} task
 * @param {number} now
 */
export function timeLine(task, now) {
  const elapsed = (task.finishedAt ?? now) - task.startedAt
  if (task.state === 'done') return `done in ${formatDuration(elapsed)}`
  if (task.state === 'failed') return `failed after ${formatDuration(elapsed)}`
  const total = task.totalSecs != null ? ` / ~${formatDuration(task.totalSecs * 1000)}` : ''
  return `${formatDuration(elapsed)}${total}`
}

/**
 * @param {any} task
 * @param {number} now
 */
function renderTask(task, now) {
  const known = typeof task.overall === 'number'
  const state = task.state === 'running' && task.stale ? 'stale' : task.state
  const label = task.state === 'done' ? 'Done' : task.state === 'failed' ? 'Failed' : known ? percentText(task.overall) : ''
  const width = task.state === 'failed' && !known ? 100 : known ? Math.max(0, Math.min(100, task.overall * 100)) : 0
  const bar =
    task.state === 'running' && !known
      ? '<div class="qpb-bar is-indeterminate"><span></span></div>'
      : `<div class="qpb-bar"><span style="width:${width.toFixed(1)}%"></span></div>`
  const quietSince = Math.max(task.updatedAt ?? 0, task.followChangedAt ?? 0)
  const quiet = task.stale ? ` · no update for ${formatDuration(now - quietSince)}` : ''
  const step = stepLine(task)
  const detail = task.state === 'running' ? task.detail : task.note ?? task.detail
  return (
    `<div class="qpb-task is-${state}">` +
    `<div class="qpb-task-head"><span class="qpb-title">${esc(task.title)}</span><span class="qpb-pct">${esc(label)}</span></div>` +
    bar +
    `<div class="qpb-meta"><span class="qpb-step">${esc(step)}</span><span class="qpb-time">${esc(timeLine(task, now) + quiet)}</span></div>` +
    (detail ? `<div class="qpb-detail" title="${esc(detail)}">${esc(detail)}</div>` : '') +
    '</div>'
  )
}

/**
 * @param {any} shell
 * @param {number} now
 */
function renderShell(shell, now) {
  const running = shell.endedAt == null
  const kind = running ? (shell.background ? 'is-background' : 'is-running') : shell.failed ? 'is-failed' : 'is-ended'
  const time = running
    ? `${shell.background ? 'background' : 'running'} ${formatDuration(now - shell.startedAt)}`
    : `${shell.failed ? 'failed' : 'done'} · ${formatDuration(shell.endedAt - shell.startedAt)}`
  return (
    `<li class="qpb-shell ${kind}" title="${esc(shell.cwd ?? '')}">` +
    '<span class="qpb-dot"></span>' +
    `<span class="qpb-cmd">${esc(shell.command)}</span>` +
    `<span class="qpb-shell-time">${esc(time)}</span>` +
    '</li>'
  )
}

/**
 * @param {any} lane
 * @param {number} now
 */
function renderLane(lane, now) {
  const place = lane.place ? `<span class="qpb-place" title="${esc(lane.folder ?? '')}">${esc(lane.place)}</span>` : ''
  const tasks = (lane.tasks ?? []).map((/** @type {any} */ t) => renderTask(t, now)).join('')
  const shells = (lane.shells ?? []).length
    ? `<ul class="qpb-shells">${lane.shells.map((/** @type {any} */ s) => renderShell(s, now)).join('')}</ul>`
    : ''
  return `<section class="qpb-lane"><header class="qpb-lane-head"><span class="qpb-agent">${esc(lane.agent)}</span>${place}</header>${tasks}${shells}</section>`
}

/**
 * The whole board as HTML.
 * @param {any} view   what `progress_board` / GET /progress/state return
 * @param {{ empty?: string }} [options]
 */
export function renderBoard(view, options = {}) {
  const now = typeof view?.now === 'number' ? view.now : Date.now()
  const lanes = Array.isArray(view?.lanes) ? view.lanes : []
  if (!lanes.length) {
    const empty = options.empty ?? 'No agent is reporting work right now.'
    return `<div class="qpb-board"><p class="qpb-empty">${esc(empty)}</p></div>`
  }
  return `<div class="qpb-board">${lanes.map((/** @type {any} */ lane) => renderLane(lane, now)).join('')}</div>`
}
