<script setup lang="ts">
/**
 * One rendered Markdown body (an agent reply, a user message, a plan).
 *
 * Re-rendered at most once per frame while text streams in, so a long reply
 * never parses per token. Code blocks are coloured once the text stops
 * streaming; KaTeX loads the first time a formula appears. Clicks are
 * delegated: Copy on a code block, external links to the system browser,
 * anything else that looks like a path is copied (the Files tab that opens
 * it arrives with card 13).
 */
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { ensureKatex, hasMath, highlightCode, renderMarkdown } from '#pilot/utils/markdown'

const props = withDefaults(defineProps<{ source: string; streaming?: boolean }>(), { streaming: false })
const emit = defineEmits<{ notice: [text: string] }>()

const root = ref<HTMLElement | null>(null)
const html = ref('')
let frame = 0

function paint() {
  frame = 0
  html.value = renderMarkdown(props.source)
  if (!props.streaming) void nextTick(() => root.value && highlightCode(root.value))
}

function schedule() {
  if (frame) return
  frame = requestAnimationFrame(paint)
}

watch(() => props.source, schedule)
watch(
  () => props.streaming,
  (s) => {
    if (!s) paint()
  }
)

onMounted(() => {
  paint()
  if (hasMath(props.source)) void ensureKatex().then(paint)
})
watch(
  () => hasMath(props.source),
  (m) => {
    if (m) void ensureKatex().then(schedule)
  }
)
onBeforeUnmount(() => {
  if (frame) cancelAnimationFrame(frame)
})

async function copyText(text: string) {
  try {
    await navigator.clipboard.writeText(text)
    return true
  } catch {
    return false
  }
}

async function openExternal(url: string) {
  try {
    const { open } = await import('@tauri-apps/plugin-shell')
    await open(url)
  } catch {
    window.open(url, '_blank', 'noopener')
  }
}

async function onClick(e: MouseEvent) {
  const target = e.target as HTMLElement
  const copy = target.closest<HTMLElement>('[data-copy]')
  if (copy) {
    e.preventDefault()
    const code = copy.closest('.qp-md-code')?.querySelector('code')?.textContent ?? ''
    if (await copyText(code)) {
      copy.textContent = 'Copied'
      setTimeout(() => (copy.textContent = 'Copy'), 1200)
    }
    return
  }
  const link = target.closest<HTMLAnchorElement>('a[href]')
  if (!link) return
  e.preventDefault()
  const href = link.getAttribute('href') ?? ''
  if (/^(https?|mailto):/i.test(href)) {
    void openExternal(href)
  } else if (href && !href.startsWith('#')) {
    if (await copyText(decodeURI(href))) emit('notice', `Copied ${decodeURI(href)}`)
  }
}
</script>

<template>
  <!-- eslint-disable-next-line vue/no-v-html — sanitised in renderMarkdown -->
  <div ref="root" class="qp-md" :class="{ streaming }" @click="onClick" v-html="html" />
</template>

<style scoped>
.qp-md {
  font-size: 13.5px;
  line-height: 1.65;
  color: var(--qss-text, #d4d4d8);
  user-select: text;
  overflow-wrap: anywhere;
  min-width: 0;
}
.qp-md :deep(> :first-child) { margin-top: 0; }
.qp-md :deep(> :last-child) { margin-bottom: 0; }
.qp-md :deep(p) { margin: 0 0 10px; }
.qp-md :deep(h1),
.qp-md :deep(h2),
.qp-md :deep(h3),
.qp-md :deep(h4) {
  margin: 16px 0 8px;
  font-weight: 600;
  line-height: 1.35;
  color: var(--qss-text, #d4d4d8);
}
.qp-md :deep(h1) { font-size: 18px; }
.qp-md :deep(h2) { font-size: 16px; }
.qp-md :deep(h3) { font-size: 14.5px; }
.qp-md :deep(h4) { font-size: 13.5px; }
.qp-md :deep(ul),
.qp-md :deep(ol) { margin: 0 0 10px; padding-left: 22px; }
.qp-md :deep(ul) { list-style: disc; }
.qp-md :deep(ul ul) { list-style: circle; }
.qp-md :deep(ol) { list-style: decimal; }
.qp-md :deep(li:has(> input[type='checkbox'])) { list-style: none; }
.qp-md :deep(li) { margin: 2px 0; }
.qp-md :deep(li::marker) { color: var(--qss-text-muted, #6e6e7a); }
.qp-md :deep(li > p) { margin: 0; }
.qp-md :deep(li input[type='checkbox']) { margin: 0 6px 0 -18px; vertical-align: -1px; }
.qp-md :deep(a) { color: var(--qss-accent, #7aa2f7); text-decoration: none; cursor: pointer; }
.qp-md :deep(a:hover) { text-decoration: underline; }
.qp-md :deep(strong) { font-weight: 600; color: var(--qss-text, #e4e4e7); }
.qp-md :deep(hr) { border: none; border-top: 1px solid var(--qss-border-subtle, #35353d); margin: 14px 0; }
.qp-md :deep(blockquote) {
  margin: 0 0 10px;
  padding: 2px 12px;
  border-left: 3px solid var(--qss-border, #47474f);
  color: var(--qss-text-secondary, #9a9aa5);
}
.qp-md :deep(:not(pre) > code) {
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 12px;
  padding: 1px 5px;
  border-radius: 4px;
  background: var(--qss-bg-card, #292930);
  color: var(--qss-text, #e4e4e7);
}
.qp-md :deep(table) {
  border-collapse: collapse;
  margin: 0 0 10px;
  font-size: 12.5px;
  display: block;
  max-width: 100%;
  overflow-x: auto;
}
.qp-md :deep(th),
.qp-md :deep(td) {
  border: 1px solid var(--qss-border-subtle, #35353d);
  padding: 4px 10px;
  text-align: left;
}
.qp-md :deep(th) { background: var(--qss-bg-raised, #1f1f25); font-weight: 600; }
.qp-md :deep(.qp-md-code) {
  margin: 0 0 12px;
  border: 1px solid var(--qss-border-subtle, #35353d);
  border-radius: 8px;
  background: var(--qss-bg-raised, #1f1f25);
  overflow: hidden;
}
.qp-md :deep(.qp-md-code-head) {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 6px 4px 12px;
  font-size: 11.5px;
  color: var(--qss-text-muted, #6e6e7a);
  border-bottom: 1px solid var(--qss-border-subtle, #35353d);
}
.qp-md :deep(.qp-md-copy) {
  border: none;
  background: transparent;
  color: var(--qss-text-muted, #6e6e7a);
  font: inherit;
  font-size: 11.5px;
  padding: 2px 8px;
  border-radius: 5px;
  cursor: pointer;
}
.qp-md :deep(.qp-md-copy:hover) { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-md :deep(pre) {
  margin: 0;
  padding: 10px 12px;
  overflow-x: auto;
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 12px;
  line-height: 1.55;
  tab-size: 2;
}
.qp-md :deep(pre code) { font: inherit; background: none; padding: 0; white-space: pre; }
.qp-md :deep(.qp-md-math) { margin: 0 0 10px; overflow-x: auto; }
.qp-md :deep(.qp-md-alert) { border-left-color: var(--qss-accent, #7aa2f7); }
.qp-md :deep(.qp-md-alert.is-tip) { border-left-color: var(--qss-success, #34d399); }
.qp-md :deep(.qp-md-alert.is-important) { border-left-color: #a78bfa; }
.qp-md :deep(.qp-md-alert.is-warning) { border-left-color: var(--qss-warning, #d29a3f); }
.qp-md :deep(.qp-md-alert.is-caution) { border-left-color: var(--qss-error, #f87171); }
.qp-md :deep(.qp-md-alert-title) { margin: 0 0 2px; font-weight: 600; color: var(--qss-text, #d4d4d8); }
</style>
