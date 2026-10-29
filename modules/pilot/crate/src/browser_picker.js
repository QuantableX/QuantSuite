// QuantPilot's element picker (card 14), evaluated in the browser preview's
// page. Hover highlights, the next click selects, Escape cancels. The page
// gets no app access: the result waits in a page global the app reads by
// evaluation. Form values, editable text, scripts and hidden nodes stay out.
(() => {
  const KEY = __QP_PICK_KEY__;
  if (globalThis[KEY]) return;
  const SAFE_ATTRS = ['id', 'class', 'role', 'aria-label', 'aria-labelledby', 'href', 'src', 'alt', 'title', 'type', 'name', 'for', 'placeholder', 'data-testid'];
  const DROP = new Set(['SCRIPT', 'STYLE', 'NOSCRIPT', 'TEMPLATE', 'IFRAME', 'OBJECT', 'EMBED', 'svg']);
  const box = document.createElement('div');
  box.style.cssText = 'position:fixed;z-index:2147483647;pointer-events:none;border:2px solid #7aa2f7;background:rgba(122,162,247,.15);border-radius:3px;transition:all .05s;display:none';
  const tag = document.createElement('div');
  tag.style.cssText = 'position:fixed;z-index:2147483647;pointer-events:none;font:12px/1.4 system-ui,sans-serif;background:#18181e;color:#d4d4d8;padding:2px 6px;border-radius:4px;display:none';
  document.documentElement.append(box, tag);
  let current = null;

  const hidden = (el) => {
    if (el.hidden || el.getAttribute('aria-hidden') === 'true') return true;
    const s = getComputedStyle(el);
    return s.display === 'none' || s.visibility === 'hidden';
  };
  const selectorOf = (el) => {
    if (el.id && /^[A-Za-z][\w-]*$/.test(el.id)) return '#' + el.id;
    const parts = [];
    for (let n = el; n && n.nodeType === 1 && parts.length < 5; n = n.parentElement) {
      if (n.id && /^[A-Za-z][\w-]*$/.test(n.id)) {
        parts.unshift('#' + n.id);
        break;
      }
      let part = n.tagName.toLowerCase();
      const cls = [...n.classList].filter((c) => /^[A-Za-z][\w-]*$/.test(c)).slice(0, 2);
      if (cls.length) part += '.' + cls.join('.');
      const sibs = n.parentElement ? [...n.parentElement.children].filter((c) => c.tagName === n.tagName) : [];
      if (sibs.length > 1) part += ':nth-of-type(' + (sibs.indexOf(n) + 1) + ')';
      parts.unshift(part);
    }
    return parts.join(' > ');
  };
  // A copy that keeps structure and safe attributes only.
  const summary = (el, depth) => {
    if (DROP.has(el.tagName) || hidden(el)) return '';
    const name = el.tagName.toLowerCase();
    const attrs = SAFE_ATTRS.filter((a) => el.hasAttribute(a))
      .map((a) => ' ' + a + '="' + String(el.getAttribute(a)).slice(0, 120).replace(/[<>"]/g, '') + '"')
      .join('');
    if (el.matches('input, textarea, select, [contenteditable="true"]')) return '<' + name + attrs + '>';
    if (depth > 3) return '<' + name + attrs + '>…</' + name + '>';
    let inner = '';
    for (const c of el.childNodes) {
      if (inner.length > 2400) break;
      if (c.nodeType === 3) inner += c.textContent.replace(/\s+/g, ' ').slice(0, 200);
      else if (c.nodeType === 1) inner += summary(c, depth + 1);
    }
    return '<' + name + attrs + '>' + inner.trim() + '</' + name + '>';
  };
  const visibleText = (el) => {
    if (el.matches('input, textarea, select, [contenteditable="true"]')) return '';
    const parts = [];
    const walk = (n) => {
      for (const c of n.childNodes) {
        if (c.nodeType === 3) parts.push(c.textContent);
        else if (c.nodeType === 1 && !DROP.has(c.tagName) && !hidden(c) && !c.matches('input, textarea, select, [contenteditable="true"]')) walk(c);
      }
    };
    walk(el);
    return parts.join(' ').replace(/\s+/g, ' ').trim().slice(0, 1200);
  };
  const place = (el) => {
    const r = el.getBoundingClientRect();
    Object.assign(box.style, { display: 'block', left: r.left + 'px', top: r.top + 'px', width: r.width + 'px', height: r.height + 'px' });
    tag.textContent = el.tagName.toLowerCase() + (el.id ? '#' + el.id : '') + '  ' + Math.round(r.width) + '×' + Math.round(r.height);
    Object.assign(tag.style, { display: 'block', left: Math.max(0, r.left) + 'px', top: Math.max(0, r.top - 22) + 'px' });
  };
  const cleanup = () => {
    removeEventListener('mousemove', onMove, true);
    removeEventListener('click', onClick, true);
    removeEventListener('keydown', onKey, true);
    box.remove();
    tag.remove();
  };
  const onMove = (e) => {
    const el = document.elementFromPoint(e.clientX, e.clientY);
    if (el && el !== box && el !== tag && el !== current) {
      current = el;
      place(el);
    }
  };
  const onClick = (e) => {
    e.preventDefault();
    e.stopPropagation();
    const el = current || document.elementFromPoint(e.clientX, e.clientY);
    cleanup();
    if (!el) {
      state.result = { cancelled: true };
      return;
    }
    const r = el.getBoundingClientRect();
    state.result = {
      url: location.href,
      title: document.title,
      selector: selectorOf(el),
      tag: el.tagName.toLowerCase(),
      text: visibleText(el),
      html: summary(el, 0).slice(0, 3000),
      bounds: { x: r.left, y: r.top, width: r.width, height: r.height },
    };
  };
  const onKey = (e) => {
    if (e.key !== 'Escape') return;
    e.preventDefault();
    e.stopPropagation();
    state.cancel();
  };
  const state = {
    result: null,
    cancel() {
      cleanup();
      state.result = { cancelled: true };
    },
  };
  globalThis[KEY] = state;
  addEventListener('mousemove', onMove, true);
  addEventListener('click', onClick, true);
  addEventListener('keydown', onKey, true);
})();
