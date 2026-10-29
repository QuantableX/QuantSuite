/**
 * The suite's one HTML sanitiser for rendered Markdown.
 *
 * Markdown from a workspace or an agent is not trusted input: a cloned
 * repo's README or a model's reply reaches `v-html`, and this runs inside
 * Tauri, where one injected `<script>` has the whole IPC surface. `marked`
 * passes raw HTML through by design, so its output is scrubbed before it is
 * mounted: the elements that can execute or phone out are dropped, and so
 * is every `on*` handler and `javascript:` URL. Parsed with DOMParser, into
 * an inert document — assigning to a live `innerHTML` to clean it would
 * already have run the loaders this is meant to strip.
 *
 * Extracted from QMarkdownPreview on 2026-10-29 when QuantPilot's chat
 * needed the same guarantee: one sanitiser, never two that drift.
 */
const FORBIDDEN = 'script,iframe,object,embed,link,meta,base,form,style'

/** Whitespace and C0 controls, which browsers ignore inside a URL scheme —
 *  `java\tscript:` is a live link, so they come out before the prefix test. */
function stripUrlNoise(value: string): string {
  let out = ''
  for (const ch of value) if (ch.codePointAt(0)! > 0x20) out += ch
  return out
}

export function sanitizeHtml(dirty: string): string {
  const doc = new DOMParser().parseFromString(dirty, 'text/html')
  doc.body.querySelectorAll(FORBIDDEN).forEach((el) => el.remove())
  doc.body.querySelectorAll('*').forEach((el) => {
    for (const attr of [...el.attributes]) {
      const name = attr.name.toLowerCase()
      if (name.startsWith('on')) {
        el.removeAttribute(attr.name)
        continue
      }
      if (name !== 'href' && name !== 'src' && name !== 'xlink:href') continue
      const value = stripUrlNoise(attr.value).toLowerCase()
      if (value.startsWith('javascript:') || value.startsWith('data:text/html')) {
        el.removeAttribute(attr.name)
      }
    }
  })
  return doc.body.innerHTML
}
