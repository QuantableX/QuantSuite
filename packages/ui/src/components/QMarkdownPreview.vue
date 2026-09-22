<script setup lang="ts">
/**
 * Rendered Markdown, as a surface any module can drop next to an editor.
 *
 * Extracted on 2026-08-27 when QuantCode needed the preview toggle QuantCanvas'
 * `FileWindow` already had. Same reasoning as `QCodeEditor`: one parser, one
 * sanitiser, one stylesheet — two copies would have drifted the moment one of
 * them got a fix, and a sanitiser is exactly the kind of thing that must not
 * exist in only one of two copies.
 *
 * Colours come from the host's CSS custom properties (`tokens`), so the same
 * component sits inside a `--qss-*` (QuantCode) and a `--qc-*` (QuantCanvas)
 * surface without either host restyling it.
 */
import { onBeforeUnmount, ref, watch } from 'vue'

const props = withDefaults(
  defineProps<{
    /** The Markdown source. Re-rendered, debounced, whenever it changes. */
    source: string
    /** Which CSS custom properties to read colours from. */
    tokens?: 'qss' | 'qc'
  }>(),
  { tokens: 'qss' }
)

const html = ref('')

/**
 * Markdown in a workspace is not trusted input. A cloned repo's README reaches
 * `v-html` verbatim, and this runs inside Tauri, where one injected `<script>`
 * has the whole IPC surface — file writes included. `marked` passes raw HTML
 * through by design, so its output is scrubbed before it is mounted: the
 * elements that can execute or phone out are dropped, and so is every `on*`
 * handler and `javascript:` URL. Parsed with DOMParser, into an inert
 * document — assigning to a live `innerHTML` to clean it would already have
 * run the loaders this is meant to strip.
 */
const FORBIDDEN = 'script,iframe,object,embed,link,meta,base,form,style'
/** Whitespace and C0 controls, which browsers ignore inside a URL scheme —
 *  `java\tscript:` is a live link, so they come out before the prefix test. */
function stripUrlNoise(value: string): string {
  let out = ''
  for (const ch of value) if (ch.codePointAt(0)! > 0x20) out += ch
  return out
}

function sanitize(dirty: string): string {
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

/**
 * Rendering is debounced: the host feeds this the live buffer, and parsing plus
 * rebuilding the preview DOM at typing rate is the one thing that makes a long
 * document feel slow.
 */
let timer: ReturnType<typeof setTimeout> | null = null
let generation = 0

async function render(source: string) {
  const gen = ++generation
  const { marked } = await import('marked')
  // `marked.parse` returns string | Promise<string> depending on its options.
  const parsed = await Promise.resolve(marked.parse(source))
  // A newer source arrived while the parser was working — that result is the
  // one the user is waiting for; this one would paint the previous document.
  if (gen !== generation) return
  html.value = sanitize(parsed)
}

watch(
  () => props.source,
  (source) => {
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => void render(source), 150)
  },
  { immediate: true }
)

onBeforeUnmount(() => {
  if (timer) clearTimeout(timer)
  // Nothing in flight may paint into a component that is going away.
  generation++
})
</script>

<template>
  <div class="q-md" :class="`is-${tokens}`" v-html="html" />
</template>

<style scoped>
/* The host's palette, mapped once onto local names so the rules below are
   written a single time rather than once per token set. */
.q-md {
  --md-text: var(--qss-text);
  --md-muted: var(--qss-text-muted);
  --md-border: var(--qss-border);
  --md-raised: var(--qss-bg-raised);
  --md-mono: var(--qss-font-mono);
  --md-sans: var(--qss-font-sans);
}
.q-md.is-qc {
  --md-text: var(--qc-text);
  --md-muted: var(--qc-text-muted);
  --md-border: var(--qc-border);
  --md-raised: var(--qc-bg-surface);
  --md-mono: 'JetBrains Mono', 'Fira Code', monospace;
  --md-sans: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif;
}

.q-md {
  height: 100%;
  overflow: auto;
  padding: 16px 20px;
  font-family: var(--md-sans);
  font-size: 13px;
  line-height: 1.6;
  color: var(--md-text);
  /* The preview is prose, not chrome — selectable and copyable even where the
     surrounding module turns selection off. */
  user-select: text;
}

.q-md :deep(h1) {
  font-size: 1.6em;
  font-weight: 700;
  margin: 0.6em 0 0.4em;
  padding-bottom: 0.3em;
  border-bottom: 1px solid var(--md-border);
}
.q-md :deep(h2) {
  font-size: 1.3em;
  font-weight: 600;
  margin: 0.6em 0 0.3em;
  padding-bottom: 0.2em;
  border-bottom: 1px solid var(--md-border);
}
.q-md :deep(h3) {
  font-size: 1.1em;
  font-weight: 600;
  margin: 0.5em 0 0.3em;
}
.q-md :deep(h4),
.q-md :deep(h5),
.q-md :deep(h6) {
  font-size: 1em;
  font-weight: 600;
  margin: 0.4em 0 0.2em;
}
.q-md :deep(p) {
  margin: 0.4em 0;
}
.q-md :deep(a) {
  color: #60a5fa;
  text-decoration: none;
}
.q-md :deep(a:hover) {
  text-decoration: underline;
}
.q-md :deep(strong) {
  font-weight: 700;
  color: var(--md-text);
}
.q-md :deep(em) {
  font-style: italic;
}
.q-md :deep(code) {
  font-family: var(--md-mono);
  font-size: 0.88em;
  padding: 0.15em 0.35em;
  border-radius: 4px;
  background: var(--md-raised);
  color: #f59e0b;
}
.q-md :deep(pre) {
  margin: 0.5em 0;
  padding: 10px 14px;
  border-radius: 6px;
  background: var(--md-raised);
  overflow-x: auto;
}
.q-md :deep(pre code) {
  padding: 0;
  background: none;
  color: var(--md-text);
  font-size: 12px;
  line-height: 1.5;
}
.q-md :deep(blockquote) {
  margin: 0.5em 0;
  padding: 0.3em 1em;
  border-left: 3px solid var(--md-border);
  color: var(--md-muted);
}
.q-md :deep(ul),
.q-md :deep(ol) {
  margin: 0.4em 0;
  padding-left: 1.5em;
}
.q-md :deep(li) {
  margin: 0.15em 0;
}
.q-md :deep(hr) {
  border: none;
  border-top: 1px solid var(--md-border);
  margin: 0.8em 0;
}
.q-md :deep(table) {
  border-collapse: collapse;
  width: 100%;
  margin: 0.5em 0;
}
.q-md :deep(th),
.q-md :deep(td) {
  padding: 6px 10px;
  border: 1px solid var(--md-border);
  font-size: 12px;
}
.q-md :deep(th) {
  font-weight: 600;
  background: var(--md-raised);
}
.q-md :deep(img) {
  max-width: 100%;
  border-radius: 4px;
}
.q-md :deep(input[type='checkbox']) {
  margin-right: 0.4em;
}
</style>
