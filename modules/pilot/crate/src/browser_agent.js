// QuantPilot parity 06: what an agent may do in the Browser tab's page.
// Injected per call with a fixed command and JSON arguments — never code
// from the agent. Elements are addressed by refs handed out by `read`
// (`e1`, `e2`, …); a ref is valid for the page document that issued it.
// Returns a plain object; errors come back as { error, stale? }.
(function (cmd, args) {
  const STATE = '__qpAgent'
  const TEXT_MAX = 8000
  const ELEMENTS_MAX = 150
  const NAME_MAX = 120
  const g = globalThis
  const clip = (s, n) => {
    s = String(s ?? '').replace(/\s+/g, ' ').trim()
    return s.length > n ? s.slice(0, n) + '…' : s
  }
  const visible = (el) => {
    if (!el.isConnected) return false
    const r = el.getBoundingClientRect()
    if (r.width === 0 && r.height === 0) return false
    const cs = getComputedStyle(el)
    return cs.visibility !== 'hidden' && cs.display !== 'none' && cs.opacity !== '0'
  }
  const nameOf = (el) => {
    const aria = el.getAttribute('aria-label')
    if (aria) return aria
    const by = el.getAttribute('aria-labelledby')
    if (by) {
      const t = by.split(/\s+/).map((id) => document.getElementById(id)?.innerText || '').join(' ')
      if (t.trim()) return t
    }
    if (el.id) {
      const l = document.querySelector(`label[for="${CSS.escape(el.id)}"]`)
      if (l?.innerText.trim()) return l.innerText
    }
    const wrap = el.closest('label')
    if (wrap?.innerText.trim()) return wrap.innerText
    if (el.tagName === 'INPUT' && ['button', 'submit', 'reset'].includes(el.type) && el.value) return el.value
    return el.getAttribute('placeholder') || el.getAttribute('title') || el.getAttribute('alt') || el.innerText || el.value || ''
  }
  const roleOf = (el) => {
    const r = el.getAttribute('role')
    if (r) return r
    const t = el.tagName.toLowerCase()
    if (t === 'a') return 'link'
    if (t === 'button') return 'button'
    if (t === 'select') return 'combobox'
    if (t === 'textarea') return 'textbox'
    if (t === 'input') {
      const ty = (el.type || 'text').toLowerCase()
      if (['button', 'submit', 'reset', 'image'].includes(ty)) return 'button'
      if (ty === 'checkbox' || ty === 'radio') return ty
      if (ty === 'range') return 'slider'
      return 'textbox'
    }
    if (el.isContentEditable) return 'textbox'
    if (/^h[1-6]$/.test(t)) return 'heading'
    return t
  }
  const state = () => g[STATE]
  const byRef = (ref) => {
    const s = state()
    if (!s) return { error: 'The page changed since your last read (it navigated or reloaded). Call read again for fresh refs.', stale: true }
    const m = /^e(\d+)$/.exec(String(ref || ''))
    if (!m) return { error: `"${ref}" is not a ref from read (refs look like e12).` }
    const el = document.querySelector(`[data-qp-ref="${s.nonce}-${m[1]}"]`)
    if (!el || !el.isConnected) return { error: `${ref} is gone from the page (it re-rendered). Call read again for fresh refs.`, stale: true }
    return { el }
  }
  const describe = (el, ref) => {
    const out = { ref, role: roleOf(el), name: clip(nameOf(el), NAME_MAX) }
    const t = el.tagName.toLowerCase()
    if (t === 'input' || t === 'textarea' || t === 'select') {
      if (el.type === 'password') out.value = el.value ? '••••' : ''
      else if (el.type !== 'checkbox' && el.type !== 'radio') out.value = clip(el.value, NAME_MAX)
    }
    if (el.type === 'checkbox' || el.type === 'radio') out.checked = !!el.checked
    if (el.disabled || el.getAttribute('aria-disabled') === 'true') out.disabled = true
    if (t === 'a' && el.getAttribute('href')) out.href = clip(el.href, 200)
    if (/^h[1-6]$/.test(t)) out.level = Number(t[1])
    return out
  }
  const SELECTOR = 'a[href],button,input:not([type=hidden]),select,textarea,summary,[role=button],[role=link],[role=checkbox],[role=radio],[role=tab],[role=menuitem],[role=switch],[role=option],[role=textbox],[role=combobox],[contenteditable=""],[contenteditable=true],h1,h2,h3,label'

  switch (cmd) {
    case 'read': {
      const nonce = Math.random().toString(36).slice(2, 10)
      document.querySelectorAll('[data-qp-ref]').forEach((el) => el.removeAttribute('data-qp-ref'))
      g[STATE] = { nonce }
      const elements = []
      let n = 0
      let more = 0
      for (const el of document.querySelectorAll(SELECTOR)) {
        if (!visible(el)) continue
        if (el.tagName === 'LABEL' && (el.control || el.querySelector('input,select,textarea'))) continue
        if (elements.length >= ELEMENTS_MAX) {
          more++
          continue
        }
        n++
        el.setAttribute('data-qp-ref', `${nonce}-${n}`)
        elements.push(describe(el, `e${n}`))
      }
      const text = document.body ? document.body.innerText : ''
      return {
        url: location.href,
        title: document.title,
        text: text.length > TEXT_MAX ? text.slice(0, TEXT_MAX) : text,
        textCut: text.length > TEXT_MAX,
        elements,
        moreElements: more,
        focused: document.activeElement && document.activeElement !== document.body ? clip(nameOf(document.activeElement), NAME_MAX) : null,
      }
    }
    case 'click': {
      const r = byRef(args.ref)
      if (r.error) return r
      const el = r.el
      if (el.disabled || el.getAttribute('aria-disabled') === 'true') return { error: `${args.ref} is disabled.` }
      el.scrollIntoView({ block: 'center', inline: 'nearest' })
      const b = el.getBoundingClientRect()
      const opts = { bubbles: true, cancelable: true, composed: true, clientX: b.left + b.width / 2, clientY: b.top + b.height / 2, button: 0 }
      el.dispatchEvent(new PointerEvent('pointerdown', opts))
      el.dispatchEvent(new MouseEvent('mousedown', opts))
      if (typeof el.focus === 'function') el.focus({ preventScroll: true })
      el.dispatchEvent(new PointerEvent('pointerup', opts))
      el.dispatchEvent(new MouseEvent('mouseup', opts))
      el.click()
      return { ok: true, clicked: describe(el, args.ref), url: location.href }
    }
    case 'fill': {
      const r = byRef(args.ref)
      if (r.error) return r
      const el = r.el
      const text = String(args.text ?? '')
      if (el.disabled || el.readOnly) return { error: `${args.ref} is disabled or read-only.` }
      el.scrollIntoView({ block: 'center', inline: 'nearest' })
      el.focus({ preventScroll: true })
      const t = el.tagName
      if (t === 'SELECT') {
        const o = [...el.options].find((o) => o.value === text) || [...el.options].find((o) => o.text.trim() === text.trim())
        if (!o) return { error: `${args.ref} has no option "${clip(text, 60)}". Options: ${[...el.options].map((o) => clip(o.text, 40)).join(', ')}` }
        el.value = o.value
      } else if (t === 'INPUT' || t === 'TEXTAREA') {
        if (el.type === 'checkbox' || el.type === 'radio') return { error: `${args.ref} is a ${el.type} — use click.` }
        const proto = t === 'INPUT' ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype
        Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, text)
      } else if (el.isContentEditable) {
        el.textContent = text
      } else {
        return { error: `${args.ref} is not a field (it is a ${roleOf(el)}).` }
      }
      el.dispatchEvent(new Event('input', { bubbles: true, composed: true }))
      el.dispatchEvent(new Event('change', { bubbles: true }))
      return { ok: true, field: describe(el, args.ref) }
    }
    case 'press': {
      const key = String(args.key || '')
      let el = document.activeElement || document.body
      if (args.ref) {
        const r = byRef(args.ref)
        if (r.error) return r
        el = r.el
        el.focus({ preventScroll: false })
      }
      const named = { Enter: 13, Tab: 9, Escape: 27, Backspace: 8, Delete: 46, Space: 32, ArrowUp: 38, ArrowDown: 40, ArrowLeft: 37, ArrowRight: 39, Home: 36, End: 35, PageUp: 33, PageDown: 34 }
      if (!(key in named) && [...key].length !== 1) return { error: `Unsupported key "${clip(key, 20)}". Use one character or ${Object.keys(named).join(', ')}.` }
      const k = key === 'Space' ? ' ' : key
      const init = { key: k, code: key.length === 1 ? `Key${key.toUpperCase()}` : key, keyCode: named[key] || k.charCodeAt(0), bubbles: true, cancelable: true, composed: true }
      const go = el.dispatchEvent(new KeyboardEvent('keydown', init))
      el.dispatchEvent(new KeyboardEvent('keyup', init))
      if (go && key === 'Enter' && el.form && (el.tagName === 'INPUT')) el.form.requestSubmit()
      return { ok: true, key, target: describe(el, args.ref || null) }
    }
    case 'scroll': {
      if (args.ref) {
        const r = byRef(args.ref)
        if (r.error) return r
        r.el.scrollIntoView({ block: 'center' })
      } else {
        window.scrollBy(0, Math.max(-20000, Math.min(20000, Number(args.dy) || window.innerHeight * 0.8)))
      }
      return { ok: true, scrollY: Math.round(window.scrollY), scrollHeight: document.documentElement.scrollHeight, viewport: window.innerHeight }
    }
    case 'check': {
      // wait: is the text there / the ref visible?
      const has = args.text ? (document.body?.innerText || '').includes(String(args.text)) : true
      let ref = true
      if (args.ref) {
        const r = byRef(args.ref)
        ref = !r.error && visible(r.el)
      }
      return { ok: has && ref, ready: document.readyState }
    }
    default:
      return { error: `unknown command ${cmd}` }
  }
})(__QP_CMD__, __QP_ARGS__)
