'use strict'
// QuantPilot on a phone (round 4, card 1). Plain script, nothing from elsewhere:
// the DOM is built from text only, so agent output can never become markup.

const root = document.getElementById('app')

function h(tag, props, ...kids) {
  const el = document.createElement(tag)
  for (const [k, v] of Object.entries(props || {})) {
    if (v === undefined || v === null || v === false) continue
    if (k === 'class') el.className = v
    else if (k.startsWith('on')) el.addEventListener(k.slice(2), v)
    else if (k === 'value') el.value = v
    else el.setAttribute(k, v === true ? '' : v)
  }
  for (const kid of kids.flat()) {
    if (kid === undefined || kid === null || kid === false) continue
    el.append(kid.nodeType ? kid : document.createTextNode(String(kid)))
  }
  return el
}

function show(...nodes) {
  root.replaceChildren(...nodes)
}

let toastTimer = 0
function toast(text, bad) {
  document.querySelector('.toast')?.remove()
  document.body.append(h('div', { class: bad ? 'toast bad' : 'toast', role: 'status' }, text))
  clearTimeout(toastTimer)
  toastTimer = setTimeout(() => document.querySelector('.toast')?.remove(), 4000)
}

class ApiError extends Error {
  constructor(status, body) {
    super(body.error || 'Something went wrong.')
    this.status = status
    this.needPin = !!body.needPin
  }
}

async function call(path, body) {
  const options = { credentials: 'same-origin' }
  if (body !== undefined) {
    options.method = 'POST'
    options.headers = { 'Content-Type': 'application/json', 'X-QS-Phone': '1' }
    options.body = JSON.stringify(body)
  }
  let res
  try {
    res = await fetch(path, options)
  } catch {
    throw new ApiError(0, { error: 'The PC cannot be reached.' })
  }
  const data = await res.json().catch(() => ({}))
  if (!res.ok) throw new ApiError(res.status, data)
  return data
}

// ── The PIN ─────────────────────────────────────────────────────────────────

function askPin() {
  return new Promise((resolve) => {
    const input = h('input', { type: 'password', inputmode: 'numeric', autocomplete: 'off', placeholder: 'PIN', 'aria-label': 'PIN' })
    const note = h('div', { class: 'note hidden' })
    const close = (ok) => {
      sheet.remove()
      resolve(ok)
    }
    const form = h('form', {
      onsubmit: async (e) => {
        e.preventDefault()
        try {
          await call('/api/pin', { pin: input.value })
          close(true)
        } catch (err) {
          input.value = ''
          note.textContent = err.message
          note.classList.remove('hidden')
        }
      },
    },
    h('h2', {}, 'Enter your PIN'),
    h('p', {}, 'It stays valid for 10 minutes on this phone.'),
    input,
    note,
    h('div', { class: 'buttons' },
      h('button', { type: 'button', onclick: () => close(false) }, 'Cancel'),
      h('button', { type: 'submit', class: 'primary' }, 'Unlock')))
    const sheet = h('div', { class: 'sheet' }, form)
    document.body.append(sheet)
    input.focus()
  })
}

/** A write. If it needs the PIN, ask once and try again. */
async function act(path, body) {
  try {
    return await call(path, body)
  } catch (err) {
    if (err.needPin && (await askPin())) return call(path, body)
    if (err.needPin) return null
    throw err
  }
}

// ── Pairing ─────────────────────────────────────────────────────────────────

function pairView(code) {
  const input = h('input', { inputmode: 'numeric', autocomplete: 'one-time-code', placeholder: '8-digit code', value: code || '', 'aria-label': 'Pairing code' })
  const name = h('input', { placeholder: 'Name of this phone', value: 'Phone', autocomplete: 'off', 'aria-label': 'Name of this phone' })
  const note = h('div', { class: 'note hidden' })
  const form = h('form', {
    class: 'center',
    onsubmit: async (e) => {
      e.preventDefault()
      try {
        await call('/api/pair', { code: input.value, name: name.value })
        goLive()
        listView()
      } catch (err) {
        note.textContent = err.message
        note.classList.remove('hidden')
      }
    },
  },
  h('h1', {}, 'Pair this phone'),
  h('p', {}, 'In QuantSuite on your PC, open Settings, QuantPilot, Phone access, and choose Pair a phone.'),
  input,
  name,
  note,
  h('button', { type: 'submit', class: 'primary' }, 'Pair'))
  show(form)
}

// ── Threads ─────────────────────────────────────────────────────────────────

let current = null // the thread on screen
let seenWaiting = 0

function age(ms) {
  const m = Math.round((Date.now() - ms) / 60000)
  if (m < 1) return 'now'
  if (m < 60) return `${m} min ago`
  const hours = Math.round(m / 60)
  if (hours < 24) return `${hours} h ago`
  return `${Math.round(hours / 24)} d ago`
}

function waitingNow(rows) {
  const waiting = rows.filter((r) => r.needs).length
  document.title = waiting ? `(${waiting}) QuantPilot` : 'QuantPilot'
  if (waiting > seenWaiting) navigator.vibrate?.(200)
  seenWaiting = waiting
}

function rowOf(r) {
  const detail = r.needs
    ? h('small', { class: 'ask' }, r.needs)
    : r.running
      ? h('small', { class: 'busy' }, `Working · ${r.project}`)
      : h('small', {}, `${r.project} · ${age(r.updatedAt)}`)
  return h('button', { class: r.needs ? 'row needs' : 'row', onclick: () => go(r.id) }, h('b', {}, r.title), detail)
}

async function listView() {
  current = null
  let rows
  try {
    rows = await call('/api/threads')
  } catch (err) {
    if (err.status === 401) return pairView()
    toast(err.message, true)
    rows = []
  }
  if (current) return
  waitingNow(rows)
  const needs = rows.filter((r) => r.needs)
  const running = rows.filter((r) => !r.needs && r.running)
  const rest = rows.filter((r) => !r.needs && !r.running)
  const scroll = window.scrollY
  show(
    h('div', { class: 'top' },
      h('div', { class: 'name' }, h('b', {}, 'QuantPilot')),
      h('button', { onclick: newThreadView }, 'New')),
    h('div', { class: 'list' },
      !rows.length && h('div', { class: 'empty' }, 'No threads yet.'),
      needs.length > 0 && [h('div', { class: 'section' }, 'Needs you'), needs.map(rowOf)],
      running.length > 0 && [h('div', { class: 'section' }, 'Working'), running.map(rowOf)],
      rest.length > 0 && [h('div', { class: 'section' }, 'Recent'), rest.map(rowOf)]),
  )
  window.scrollTo(0, scroll)
}

async function newThreadView() {
  current = 'new'
  let projects = []
  try {
    projects = await call('/api/projects')
  } catch (err) {
    toast(err.message, true)
  }
  const project = h('select', { 'aria-label': 'Project' }, projects.map((p) => h('option', { value: p.id }, p.name)))
  const text = h('textarea', { rows: 5, placeholder: 'What should the agent do?', 'aria-label': 'Message' })
  const send = h('button', { type: 'submit', class: 'primary' }, 'Start')
  show(
    h('div', { class: 'top' },
      h('button', { class: 'plain', onclick: () => go(''), 'aria-label': 'Back' }, 'Back'),
      h('div', { class: 'name' }, h('b', {}, 'New thread'))),
    h('form', {
      class: 'center',
      onsubmit: async (e) => {
        e.preventDefault()
        send.disabled = true
        try {
          const made = await act('/api/threads', { project: project.value, text: text.value })
          if (made) go(made.id)
        } catch (err) {
          toast(err.message, true)
        }
        send.disabled = false
      },
    }, project, text, send),
  )
  text.focus()
}

// ── One thread ──────────────────────────────────────────────────────────────

const TOOL_MARK = { running: '…', failed: '✕', interrupted: '✕' }

function itemNode(item) {
  switch (item.kind) {
    case 'user':
    case 'assistant':
    case 'notice':
    case 'error':
      return h('div', { class: `msg ${item.kind}` }, item.text)
    case 'tool': {
      const mark = TOOL_MARK[item.status] || '✓'
      const cls = item.status === 'running' ? 'run' : item.status === 'failed' ? 'fail' : ''
      return h('div', { class: 'tool' }, h('i', { class: cls }, mark), h('span', {}, item.title))
    }
    case 'plan':
      return h('div', { class: 'msg plan' }, item.text)
    case 'changes':
    case 'page':
      return h('div', { class: 'msg notice' }, item.text)
    case 'approval':
      return h('div', { class: 'done' }, `Answered: ${item.title}`)
    case 'question':
      return h('div', { class: 'done' }, item.onPc ? item.title : 'A question was answered.')
    default:
      return null
  }
}

async function answer(threadId, requestId, body) {
  try {
    const done = await act(`/api/threads/${encodeURIComponent(threadId)}/answer`, { requestId, answer: body })
    if (done) refresh()
  } catch (err) {
    toast(err.message, true)
  }
}

function approvalCard(threadId, item) {
  return h('section', { class: 'ask' },
    h('h2', {}, 'The agent asks'),
    h('div', { class: 'what' }, item.title),
    item.text && item.text !== item.title && h('pre', {}, item.text),
    h('div', { class: 'buttons' },
      h('button', { class: 'danger', onclick: () => answer(threadId, item.requestId, { decision: 'deny' }) }, 'Deny'),
      item.allowSession && h('button', { onclick: () => answer(threadId, item.requestId, { decision: 'allowSession' }) }, 'Allow for this session'),
      h('button', { class: 'primary', onclick: () => answer(threadId, item.requestId, { decision: 'allow' }) }, 'Allow')))
}

function planCard(threadId, item) {
  return h('section', { class: 'ask' },
    h('h2', {}, 'The agent proposes a plan'),
    h('div', { class: 'what' }, 'It is shown above.'),
    h('div', { class: 'buttons' },
      h('button', { class: 'danger', onclick: () => answer(threadId, item.requestId, { decision: 'deny' }) }, 'Keep planning'),
      h('button', { class: 'primary', onclick: () => answer(threadId, item.requestId, { decision: 'allow' }) }, 'Go ahead')))
}

function questionCard(threadId, item) {
  const questions = Array.isArray(item.questions) ? item.questions : []
  const picked = new Map()
  const own = new Map()
  const blocks = questions.map((q) => {
    picked.set(q.question, new Set())
    const options = (q.options || []).map((o) => {
      const label = typeof o === 'string' ? o : o.label
      const button = h('button', {
        type: 'button',
        class: 'option',
        onclick: () => {
          const set = picked.get(q.question)
          if (!q.multiSelect) {
            set.clear()
            button.parentElement.querySelectorAll('.option').forEach((b) => b.classList.remove('on'))
          }
          if (set.has(label)) set.delete(label)
          else set.add(label)
          button.classList.toggle('on', set.has(label))
        },
      }, label, o.description ? h('div', { class: 'desc' }, o.description) : null)
      return button
    })
    const input = h('input', { type: q.isSecret ? 'password' : 'text', placeholder: options.length ? 'Or type your own' : 'Your answer', autocomplete: 'off', 'aria-label': q.question })
    own.set(q.question, input)
    return h('div', {}, h('div', { class: 'q' }, q.question), h('div', { class: 'buttons' }, options), input)
  })
  return h('section', { class: 'ask' },
    h('h2', {}, 'The agent asks'),
    blocks,
    h('div', { class: 'buttons' },
      h('button', { onclick: () => answer(threadId, item.requestId, { decision: 'dismiss' }) }, 'Skip'),
      h('button', {
        class: 'primary',
        onclick: () => {
          const answers = {}
          for (const q of questions) {
            const parts = [...picked.get(q.question)]
            const typed = own.get(q.question).value
            if (typed) parts.push(typed)
            if (!parts.length) return toast('Answer every question first.', true)
            answers[q.question] = parts.join(', ')
          }
          answer(threadId, item.requestId, { decision: 'answers', answers })
        },
      }, 'Send')))
}

function requestCard(threadId, item) {
  if (item.onPc) {
    return h('section', { class: 'ask' }, h('h2', {}, 'The agent asks'), h('div', { class: 'what' }, `${item.title}. This one has to be answered on the PC.`))
  }
  if (item.kind === 'approval') return approvalCard(threadId, item)
  if (item.kind === 'plan') return planCard(threadId, item)
  return questionCard(threadId, item)
}

let shell = null // the parts of the thread view that survive a refresh

async function threadView(id) {
  current = id
  const items = h('div', { class: 'items', 'aria-live': 'polite' })
  const asks = h('div', {})
  const title = h('b', {}, '…')
  const sub = h('small', {}, '')
  const stop = h('button', { class: 'danger hidden', onclick: () => stopThread(id) }, 'Stop')
  const text = h('textarea', { rows: 1, placeholder: 'Message the agent', 'aria-label': 'Message' })
  text.addEventListener('input', () => {
    text.style.height = 'auto'
    text.style.height = `${text.scrollHeight + 2}px`
  })
  const send = h('button', { class: 'primary', onclick: () => sendMessage(id, text, send) }, 'Send')
  shell = { id, items, asks, title, sub, stop, signature: '' }
  show(
    h('div', { class: 'top' },
      h('button', { class: 'plain', onclick: () => go(''), 'aria-label': 'Back' }, 'Back'),
      h('div', { class: 'name' }, title, sub),
      stop),
    h('div', { class: 'thread' }, items, asks),
    h('div', { class: 'composer' }, text, send),
  )
  await drawThread(true)
  window.scrollTo(0, document.documentElement.scrollHeight)
}

async function drawThread(first) {
  if (!shell) return
  const id = shell.id
  let view
  try {
    view = await call(`/api/threads/${encodeURIComponent(id)}`)
  } catch (err) {
    if (err.status === 401) return pairView()
    if (first) toast(err.message, true)
    return
  }
  if (!shell || shell.id !== id) return
  shell.title.textContent = view.title
  shell.sub.textContent = view.running ? `Working · ${view.project}` : view.project
  shell.stop.classList.toggle('hidden', !view.running)
  const nearBottom = window.innerHeight + window.scrollY > document.documentElement.scrollHeight - 120
  const open = view.items.filter((i) => i.status === 'pending' && i.requestId)
  const rows = view.items.filter((i) => !(i.status === 'pending' && i.requestId && i.kind !== 'plan')).map(itemNode).filter(Boolean)
  shell.items.replaceChildren(...rows)
  // A form being filled in is left alone until its request changes.
  const signature = open.map((i) => i.requestId).join('|')
  if (signature !== shell.signature) {
    shell.signature = signature
    shell.asks.replaceChildren(...open.map((i) => requestCard(id, i)))
  }
  if (nearBottom) window.scrollTo(0, document.documentElement.scrollHeight)
}

async function sendMessage(id, text, button) {
  if (!text.value.trim()) return
  button.disabled = true
  try {
    const done = await act(`/api/threads/${encodeURIComponent(id)}/messages`, { text: text.value })
    if (done) {
      text.value = ''
      text.style.height = 'auto'
      refresh()
    }
  } catch (err) {
    toast(err.message, true)
  }
  button.disabled = false
}

async function stopThread(id) {
  try {
    const done = await act(`/api/threads/${encodeURIComponent(id)}/stop`, {})
    if (done) refresh()
  } catch (err) {
    toast(err.message, true)
  }
}

// ── Navigation and live updates ─────────────────────────────────────────────

function go(id) {
  if (id) location.hash = `t/${encodeURIComponent(id)}`
  else location.hash = ''
}

function route() {
  const match = location.hash.match(/^#t\/(.+)$/)
  shell = null
  if (match) threadView(decodeURIComponent(match[1]))
  else listView()
}

let refreshTimer = 0
function refresh() {
  clearTimeout(refreshTimer)
  refreshTimer = setTimeout(async () => {
    if (document.querySelector('.sheet')) return
    if (current === 'new') return
    if (current) {
      await drawThread(false)
      call('/api/threads').then(waitingNow).catch(() => {})
    } else {
      listView()
    }
  }, 350)
}

let live = false
function goLive() {
  if (live) return
  live = true
  window.addEventListener('hashchange', route)
  const events = new EventSource('/api/events')
  events.onmessage = refresh
  // A stream that ends may mean this phone was cut off: asking finds out.
  events.onerror = () => refresh()
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden) refresh()
  })
}

async function start() {
  const pairing = location.hash.match(/^#pair=(\d+)/)
  if (pairing) {
    history.replaceState(null, '', location.pathname)
    return pairView(pairing[1])
  }
  try {
    await call('/api/me')
  } catch (err) {
    if (err.status === 401) return pairView()
    return show(h('div', { class: 'center' }, h('h1', {}, 'QuantPilot'), h('p', {}, err.message)))
  }
  goLive()
  route()
}

start()
