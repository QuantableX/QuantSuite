<script setup lang="ts">
/**
 * The message box under the conversation. Enter sends, Shift+Enter is a
 * new line; while the agent works an empty box offers Interrupt. Queue,
 * steer, history, stash and attachments arrive with card 4, the pickers
 * with card 5.
 */
import { computed, nextTick, ref, watch } from 'vue'
import { ArrowUp, Square } from 'lucide-vue-next'
import { useThreadStore } from '#pilot/stores/thread'
import { usePilotStore } from '#pilot/stores/pilot'
import { modeLabel } from '#pilot/utils/format'

const props = defineProps<{ threadId: string }>()
const threads = useThreadStore()
const store = usePilotStore()

const view = computed(() => threads.view(props.threadId))
const thread = computed(() => store.sessions.find((s) => s.id === props.threadId) ?? null)
const running = computed(() => Boolean(view.value?.turnId))
const sending = ref(false)

/** Drafts per thread, kept while the module lives. */
const drafts = useState<Record<string, string>>('pilot-drafts', () => ({}))
const text = computed({
  get: () => drafts.value[props.threadId] ?? '',
  set: (v: string) => (drafts.value[props.threadId] = v),
})

const box = ref<HTMLTextAreaElement | null>(null)
const MAX_HEIGHT = 260

function grow() {
  const el = box.value
  if (!el) return
  el.style.height = 'auto'
  el.style.height = `${Math.min(el.scrollHeight, MAX_HEIGHT)}px`
}
watch(text, () => void nextTick(grow))
watch(
  () => props.threadId,
  () =>
    void nextTick(() => {
      grow()
      box.value?.focus()
    }),
  { immediate: true }
)

const canSend = computed(() => text.value.trim().length > 0 && !running.value && !sending.value)

async function send() {
  if (!canSend.value) return
  const message = text.value
  sending.value = true
  text.value = ''
  const ok = await threads.send(props.threadId, message)
  sending.value = false
  if (!ok && !text.value) text.value = message
  void nextTick(() => box.value?.focus())
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
    e.preventDefault()
    void send()
  }
}

function focus() {
  box.value?.focus()
}
defineExpose({ focus })
</script>

<template>
  <div class="qp-composer">
    <div class="qp-composer-box" @click="focus">
      <textarea
        ref="box"
        v-model="text"
        class="qp-composer-input"
        rows="1"
        :placeholder="running ? 'The agent is working… (queueing arrives with the next update)' : 'Ask anything, or describe a task'"
        @keydown="onKey"
      />
      <div class="qp-composer-row">
        <span class="qp-composer-meta">
          <span>{{ store.adapterLabel(thread?.provider || 'claude') }}</span>
          <span v-if="thread?.model">· {{ thread.model }}</span>
          <span>· {{ modeLabel(thread?.mode ?? 'full') }}</span>
        </span>
        <button v-if="running && !text.trim()" class="qp-composer-send is-stop" title="Interrupt the agent" @click.stop="threads.interrupt(threadId)">
          <Square :size="12" fill="currentColor" />
        </button>
        <button v-else class="qp-composer-send" :disabled="!canSend" title="Send (Enter)" @click.stop="send">
          <ArrowUp :size="15" />
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qp-composer { flex-shrink: 0; padding: 4px 24px 14px; }
.qp-composer-box {
  max-width: 780px;
  margin: 0 auto;
  padding: 10px 10px 8px 14px;
  border-radius: 14px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  cursor: text;
  transition: border-color 0.12s ease;
}
.qp-composer-box:focus-within { border-color: color-mix(in srgb, var(--qss-text, #d4d4d8) 45%, var(--qss-border, #47474f)); }
.qp-composer-input {
  display: block;
  width: 100%;
  min-height: 22px;
  max-height: 260px;
  resize: none;
  border: none;
  outline: none;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font-size: 13.5px;
  line-height: 1.55;
  padding: 0;
}
.qp-composer-input::placeholder { color: var(--qss-text-muted, #6e6e7a); }
/* The box shows focus (border); the field itself draws no second ring. */
.qp-composer .qp-composer-input:focus,
.qp-composer .qp-composer-input:focus-visible { outline: none; box-shadow: none; }
.qp-composer-row { display: flex; align-items: center; justify-content: space-between; margin-top: 6px; gap: 8px; }
.qp-composer-meta { display: flex; gap: 4px; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); min-width: 0; overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
.qp-composer-send {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 28px;
  height: 28px;
  border-radius: 50%;
  border: none;
  background: var(--qss-text, #d4d4d8);
  color: var(--qss-bg, #18181e);
  cursor: pointer;
}
.qp-composer-send:disabled { opacity: 0.3; cursor: default; }
.qp-composer-send:active:not(:disabled) { transform: scale(0.94); }
.qp-composer-send.is-stop { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); border: 1px solid var(--qss-border, #47474f); }
</style>
