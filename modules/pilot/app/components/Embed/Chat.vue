<script setup lang="ts">
/**
 * A QuantPilot chat embedded in another module (pre-1.1.0; first host:
 * QuantCanvas). The same conversation component as the /pilot page, bound
 * to one thread: a normal QuantPilot thread, listed in its sidebar and in
 * sync with it. Live events come in while the host is in front (warm cache:
 * on activation, off on deactivation, the thread reloaded on return when
 * QuantPilot was not listening meanwhile). Duplicate events from the /pilot
 * layout are harmless: the store applies each sequence number once.
 */
import { onActivated, onBeforeUnmount, onDeactivated, onMounted, provide, ref, watch } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'
import { PILOT_HOST, type PilotHost } from '#pilot/utils/host'
import type { ThreadEnvelope } from '#pilot/types'

const props = defineProps<{ threadId: string; host: Omit<PilotHost, 'threadId'> }>()
const store = usePilotStore()
const threads = useThreadStore()
const ready = ref(false)

provide(PILOT_HOST, {
  get kind() {
    return props.host.kind
  },
  get threadId() {
    return props.threadId
  },
  openFile: (path, line) => props.host.openFile?.(path, line) ?? false,
  get openLabel() {
    return props.host.openLabel
  },
})

let unlisten: UnlistenFn | null = null
let wasAway = false
async function arm() {
  if (unlisten) return
  const stop = await listen<ThreadEnvelope>('pilot-thread-event', (e) => {
    if (e.payload.threadId === props.threadId) threads.apply(e.payload)
  })
  if (unlisten) stop()
  else unlisten = stop
  // Back after a while away: catch up with what happened meanwhile.
  if (wasAway) void threads.open(props.threadId, true)
  wasAway = false
}
function disarm() {
  unlisten?.()
  unlisten = null
  wasAway = true
}

onMounted(async () => {
  if (!store.loaded) await store.load()
  ready.value = true
  if (inActiveKeepAliveTree()) void arm()
})
onActivated(() => void arm())
onDeactivated(disarm)
onBeforeUnmount(disarm)
watch(
  () => props.threadId,
  () => void threads.open(props.threadId)
)
</script>

<template>
  <div class="qp-embed" data-module="pilot">
    <PilotThreadConversation v-if="ready" :thread-id="threadId" />
    <div v-else class="qp-embed-loading">Loading QuantPilot…</div>
  </div>
</template>

<style scoped>
.qp-embed { position: relative; display: flex; flex-direction: column; width: 100%; height: 100%; min-height: 0; overflow: hidden; background: var(--qss-bg, #18181e); color: var(--qss-text, #d4d4d8); font-size: 13px; container-type: inline-size; }
.qp-embed > :deep(*) { flex: 1; min-height: 0; }
.qp-embed-loading { display: flex; align-items: center; justify-content: center; color: var(--qss-text-muted, #6e6e7a); }
/* A narrow window: less padding around the transcript and the box. */
@container (max-width: 560px) {
  .qp-embed :deep(.qp-tl-content) { padding: 10px 10px 6px; }
  .qp-embed :deep(.qp-ap) { width: calc(100% - 16px); }
}
</style>
