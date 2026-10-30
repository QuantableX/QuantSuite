<script setup lang="ts">
/**
 * A page the agent rendered with `quantsuite.pilot.render_page` (card 22):
 * its HTML in a sandboxed frame — scripts run, but without the app's origin
 * nothing of the app is reachable, and its CSP lets nothing load from the
 * network. The suite's colours come in as CSS variables; the frame grows
 * with its content up to a limit.
 */
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref } from 'vue'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { PanelTop } from 'lucide-vue-next'
import type { ThreadItem } from '#pilot/types'

const props = defineProps<{ item: ThreadItem }>()
const frame = ref<HTMLIFrameElement | null>(null)
const height = ref(160)
const MAX_HEIGHT = 1200

const TOKENS = [
  '--qss-bg', '--qss-bg-raised', '--qss-bg-card', '--qss-bg-hover', '--qss-text', '--qss-text-secondary', '--qss-text-muted',
  '--qss-border', '--qss-border-subtle', '--qss-accent', '--qss-success', '--qss-warning', '--qss-error', '--qss-font-sans', '--qss-font-mono',
]
function theme(): string {
  const cs = getComputedStyle(document.documentElement)
  return TOKENS.map((t) => {
    const v = cs.getPropertyValue(t).trim()
    return v ? `${t}:${v};` : ''
  }).join('')
}

const CSP = "default-src 'none'; img-src data: blob:; media-src data: blob:; font-src data:; style-src 'unsafe-inline'; script-src 'unsafe-inline'"
const srcdoc = computed(() => {
  const html = String(props.item.data.html ?? '').replace(/^\s*<!doctype[^>]*>/i, '')
  const id = JSON.stringify(props.item.id)
  return (
    `<!doctype html><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${CSP}">` +
    `<style>:root{color-scheme:dark;${theme()}}html,body{margin:0}body{padding:12px;background:transparent;color:var(--qss-text,#d4d4d8);` +
    `font-family:var(--qss-font-sans,system-ui,sans-serif);font-size:13px;line-height:1.5}</style>` +
    `<script>(function(){var id=${id};function post(){parent.postMessage({qpPage:id,height:Math.ceil(document.documentElement.scrollHeight)},'*')}` +
    `addEventListener('load',post);try{new ResizeObserver(post).observe(document.documentElement)}catch(e){}setTimeout(post,60)})()<\/script>` +
    html
  )
})
const title = computed(() => String(props.item.data.title ?? '').trim())

function onMessage(e: MessageEvent) {
  if (!frame.value || e.source !== frame.value.contentWindow) return
  const h = Number(e.data?.height)
  if (e.data?.qpPage === props.item.id && Number.isFinite(h) && h > 0) height.value = Math.min(Math.max(h, 48), MAX_HEIGHT)
}
const listen = () => window.addEventListener('message', onMessage)
const unlisten = () => window.removeEventListener('message', onMessage)
onMounted(() => {
  if (inActiveKeepAliveTree()) listen()
})
onActivated(listen)
onDeactivated(unlisten)
onBeforeUnmount(unlisten)
</script>

<template>
  <figure class="qp-render" :data-item-id="item.id">
    <figcaption class="qp-render-cap">
      <PanelTop :size="12" />
      <span>{{ title || 'Page' }}</span>
    </figcaption>
    <iframe
      ref="frame"
      class="qp-render-frame"
      sandbox="allow-scripts"
      referrerpolicy="no-referrer"
      :title="title || 'A page the agent rendered'"
      :srcdoc="srcdoc"
      :style="{ height: `${height}px` }"
    />
  </figure>
</template>

<style scoped>
.qp-render {
  margin: 4px 0;
  border: 1px solid var(--qss-border-subtle, #35353d);
  border-radius: 10px;
  background: var(--qss-bg-raised, #1f1f25);
  overflow: hidden;
}
.qp-render-cap {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 10px;
  border-bottom: 1px solid var(--qss-border-subtle, #35353d);
  color: var(--qss-text-secondary, #9a9aa5);
  font-size: 11.5px;
}
.qp-render-cap span { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-render-frame { display: block; width: 100%; border: 0; background: transparent; }
</style>
