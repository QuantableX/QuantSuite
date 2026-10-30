<script setup lang="ts">
/**
 * One turn of the conversation: the user's message, the "Worked for …"
 * fold, the task list (card 16), the answer, the files it changed, then
 * whatever needs the user (requests) and what must stay visible (plans,
 * errors, notices). "Edit from here" rewinds the thread to before this
 * message (card 11).
 */
import { computed, ref } from 'vue'
import { AlertTriangle, CornerDownRight, Info } from 'lucide-vue-next'
import type { TurnBlock } from '#pilot/utils/timeline'
import { plainText } from '#pilot/utils/mentions'
import { pilot } from '#pilot/utils/invoke'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'

const props = defineProps<{
  block: TurnBlock
  threadId: string
  running: boolean
  /** No turn runs in the thread: the message can be edited from. */
  idle: boolean
  elapsed: number
  cost: number | null
}>()
const emit = defineEmits<{ notice: [text: string] }>()
const store = usePilotStore()
const threads = useThreadStore()
const editing = ref(false)
const reverting = ref(false)
const worktree = computed(() => Boolean(store.sessions.find((s) => s.id === props.threadId)?.worktreePath))
const thread = computed(() => store.sessions.find((s) => s.id === props.threadId) ?? null)
/** After a provider switch, earlier turns say which agent answered them (card 15). */
const otherAgent = computed(() => {
  const p = String(props.block.user?.data.provider ?? '')
  return p && p !== (thread.value?.provider || 'claude') ? store.adapterLabel(p) : ''
})
const resumed = computed(() => Boolean(props.block.user?.data.resume))
const forking = ref(false)
async function fork(worktree: boolean) {
  forking.value = false
  const item = props.block.answer ?? props.block.user
  if (item) await threads.fork(props.threadId, item.id, worktree)
}

const userText = computed(() => plainText(String(props.block.user?.data.text ?? '')))
const showActivity = computed(() => props.block.activity.length > 0 || (props.running && !props.block.answer))

async function copyUser() {
  try {
    await navigator.clipboard.writeText(userText.value)
    emit('notice', 'Message copied')
  } catch {
    /* clipboard denied — nothing to do */
  }
}

/** The message and everything after leave the thread; its text goes back into the box. */
async function revert(files: boolean) {
  const item = props.block.user
  if (!item || reverting.value) return
  reverting.value = true
  try {
    const r = await pilot<{ prompt: string; attachments: Record<string, any>[]; files: string[] }>('plugin:pilot|pilot_thread_revert', { id: props.threadId, itemId: item.id, files })
    editing.value = false
    await threads.open(props.threadId, true)
    window.dispatchEvent(new CustomEvent('qp-composer-set', { detail: { threadId: props.threadId, text: r?.prompt ?? '', attachments: r?.attachments ?? [] } }))
    emit('notice', files && r?.files.length ? `Rewound — ${r.files.length} file${r.files.length === 1 ? '' : 's'} restored` : 'Rewound — edit the message and send it again')
  } catch (e) {
    emit('notice', String(e))
  } finally {
    reverting.value = false
  }
}
</script>

<template>
  <section class="qp-turn" :data-turn="block.turnId">
    <div v-if="block.user && resumed" class="qp-resumed" :data-item-id="block.user.id">↻ Resumed<template v-if="otherAgent"> · {{ otherAgent }}</template></div>
    <div v-else-if="block.user" class="qp-user" :data-item-id="block.user.id">
      <div class="qp-user-bubble">
        <PilotThreadUserMessage :item="block.user" @notice="(t: string) => emit('notice', t)" />
      </div>
      <div class="qp-user-tools" :class="{ on: editing }">
        <button v-if="idle" class="qp-user-copy" title="Rewind the thread to before this message and edit it" @click="editing = !editing">Edit from here</button>
        <button class="qp-user-copy" title="Copy message" @click="copyUser">Copy</button>
        <button v-if="idle" class="qp-user-copy" title="A new thread from here, with this conversation up to this turn" @click="forking = !forking">Fork</button>
        <span v-if="block.user.data.fromTerminal" class="qp-user-tag">from the terminal</span>
        <span v-if="otherAgent" class="qp-user-tag">{{ otherAgent }}</span>
      </div>
      <div v-if="forking" class="qp-rewind" role="dialog" aria-label="Fork">
        <div class="qp-rewind-text">A new thread with this conversation up to here; this one stays as it is.</div>
        <div class="qp-rewind-actions">
          <button class="qp-rewind-btn" @click="forking = false">Cancel</button>
          <button class="qp-rewind-btn" title="A new worktree from this thread's branch (committed work only)" @click="fork(true)">Into a new worktree</button>
          <button class="qp-rewind-btn primary" @click="fork(false)">In this checkout</button>
        </div>
      </div>
      <div v-if="editing" class="qp-rewind" role="dialog" aria-label="Edit from here">
        <div class="qp-rewind-text">This message and everything after it leave the thread. The agent picks up from before it, and the message comes back into the box.</div>
        <div class="qp-rewind-actions">
          <button class="qp-rewind-btn" @click="editing = false">Cancel</button>
          <button v-if="worktree" class="qp-rewind-btn" :disabled="reverting" title="Also put the worktree's files back to how they were before this message" @click="revert(true)">Rewind with files</button>
          <button class="qp-rewind-btn primary" :disabled="reverting" @click="revert(false)">Rewind</button>
        </div>
      </div>
    </div>

    <PilotThreadActivity v-if="showActivity" :block="block" :running="running" :elapsed="elapsed" @notice="(t: string) => emit('notice', t)" />
    <PilotThreadTasks v-if="block.tasks" :item="block.tasks" :running="running" />

    <template v-for="o in [...block.before, null, ...block.outside]" :key="o?.id ?? 'answer'">
      <template v-if="o === null">
        <div v-if="block.answer" class="qp-answer" :data-item-id="block.answer.id">
          <PilotThreadMarkdown :source="String(block.answer.data.text ?? '')" :streaming="Boolean(block.answer.data.streaming)" @notice="(t: string) => emit('notice', t)" />
        </div>
        <PilotThreadChanges v-if="block.changes" :item="block.changes" />
        <!-- Pending approvals and questions wait in the bar above the box (Approvals.vue). -->
        <PilotThreadPlanCard v-for="r in block.requests.filter((x) => x.kind === 'plan')" :key="r.id" :item="r" @notice="(t: string) => emit('notice', t)" />
      </template>
      <div v-else-if="o.kind === 'user'" class="qp-user is-steer" :data-item-id="o.id">
        <div class="qp-user-bubble">
          <div class="qp-user-att"><CornerDownRight :size="11" /> Sent while the agent worked</div>
          <PilotThreadUserMessage :item="o" @notice="(t: string) => emit('notice', t)" />
        </div>
      </div>
      <div v-else-if="o.kind === 'error'" class="qp-error" :data-item-id="o.id">
        <AlertTriangle :size="14" />
        <span class="qp-error-text">{{ o.data.text }}</span>
      </div>
      <div v-else-if="o.kind === 'notice'" class="qp-notice"><Info :size="12" />{{ o.data.text }}</div>
      <PilotThreadPlanCard v-else-if="o.kind === 'plan'" :item="o" @notice="(t: string) => emit('notice', t)" />
      <PilotThreadPage v-else-if="o.kind === 'page'" :item="o" />
    </template>
  </section>
</template>

<style scoped>
.qp-turn { display: flex; flex-direction: column; padding: 6px 0 14px; }
.qp-user { display: flex; flex-direction: column; align-items: flex-end; gap: 2px; margin-bottom: 10px; }
.qp-user-bubble {
  max-width: min(86%, 640px);
  padding: 8px 12px;
  border-radius: 12px;
  background: var(--qss-bg-card, #292930);
  border: 1px solid var(--qss-border-subtle, #35353d);
}
.qp-user-bubble :deep(.qp-md) { font-size: 13.5px; }
.qp-user-att { display: flex; align-items: center; gap: 4px; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); margin-bottom: 4px; }
.qp-user.is-steer { margin: 4px 0 10px; }
.qp-user.is-steer .qp-user-bubble { border-style: dashed; }
.qp-user-copy {
  opacity: 0;
  border: none;
  background: transparent;
  color: var(--qss-text-muted, #6e6e7a);
  font-size: 11px;
  padding: 1px 6px;
  border-radius: 5px;
  cursor: pointer;
}
.qp-user-tools { display: flex; gap: 2px; }
.qp-user:hover .qp-user-copy,
.qp-user-tools.on .qp-user-copy,
.qp-user-copy:focus-visible { opacity: 1; }
.qp-resumed { align-self: flex-end; margin-bottom: 10px; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-user-tag { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); padding: 1px 4px; }
.qp-rewind {
  max-width: min(86%, 640px);
  margin-top: 4px;
  padding: 8px 10px;
  border-radius: 10px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  font-size: 12px;
}
.qp-rewind-text { color: var(--qss-text-secondary, #9a9aa5); line-height: 1.5; }
.qp-rewind-actions { display: flex; justify-content: flex-end; gap: 6px; margin-top: 8px; }
.qp-rewind-btn {
  padding: 3px 10px;
  border: 1px solid var(--qss-border, #47474f);
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  cursor: pointer;
}
.qp-rewind-btn:hover:not(:disabled) { background: var(--qss-bg-hover, #313139); }
.qp-rewind-btn.primary { background: var(--qss-accent, #7aa2f7); border-color: transparent; color: var(--qss-bg, #18181e); }
.qp-rewind-btn:disabled { opacity: 0.5; cursor: default; }
.qp-user-copy:hover { color: var(--qss-text, #d4d4d8); background: var(--qss-bg-hover, #313139); }
.qp-answer { padding: 0 2px; }
.qp-error {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  margin: 6px 0;
  padding: 8px 10px;
  border-radius: 8px;
  border: 1px solid color-mix(in srgb, var(--qss-error, #f87171) 45%, var(--qss-border, #47474f));
  background: color-mix(in srgb, var(--qss-error, #f87171) 7%, transparent);
  color: var(--qss-error, #f87171);
  font-size: 12.5px;
}
.qp-error-text { white-space: pre-wrap; overflow-wrap: anywhere; user-select: text; color: var(--qss-text, #d4d4d8); }
.qp-notice { display: flex; align-items: center; gap: 6px; margin: 4px 0; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); }
</style>
