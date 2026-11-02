<script setup lang="ts">
/**
 * Follow-ups waiting for the running turn, at the end of the thread as
 * dashed messages (T3 Code). Each one: Send now (into the running turn when
 * the agent can take it, else as the next turn), Edit (opens it in the
 * composer), Remove. After a stop, a failure or a restart the queue is held
 * until Resume — or until the next message the user sends.
 */
import { computed } from 'vue'
import { CornerDownRight, Pencil, Play, X } from 'lucide-vue-next'
import { useThreadStore } from '#pilot/stores/thread'

const props = defineProps<{ threadId: string }>()
const threads = useThreadStore()
const view = computed(() => threads.view(props.threadId))
const queue = computed(() => view.value?.queue ?? [])
const running = computed(() => Boolean(view.value?.turnId))
</script>

<template>
  <div v-if="queue.length" class="qp-queue">
    <div class="qp-queue-head">
      <span>Queued · {{ queue.length }}</span>
      <template v-if="view?.queueHeld">
        <span class="qp-queue-held">on hold</span>
        <button v-if="!running" class="qp-queue-resume" @click="threads.queueResume(threadId)"><Play :size="11" fill="currentColor" /> Resume</button>
      </template>
    </div>
    <div v-for="q in queue" :key="q.id" class="qp-queue-item" :class="{ editing: view?.editingQueued === q.id }">
      <div class="qp-queue-content">
        <div v-if="['sending', 'running', 'awaiting'].includes(q.state)" class="qp-queue-held">Sending · retained until completion</div>
        <div class="qp-queue-text">{{ q.text }}</div>
        <div v-if="q.payload?.attachments.length" class="qp-queue-files">
          <span v-for="(a, index) in q.payload.attachments" :key="index" :title="a.path">
            <img v-if="a.kind === 'image' && a.thumb" :src="a.thumb" :alt="a.name" />{{ a.name }}
          </span>
        </div>
        <div v-if="q.error" class="qp-queue-error" role="status">{{ q.error }}</div>
      </div>
      <div class="qp-queue-actions">
        <button
          class="qp-queue-btn"
          :disabled="['sending', 'running', 'awaiting'].includes(q.state) || (running && !view?.canSteer)"
          :title="q.state === 'uncertain' ? 'Retry this delivery after checking the conversation' : running && view?.canSteer ? 'Send now, into the running turn' : 'Send now'"
          aria-label="Send queued message now"
          @click="threads.queueSendNow(threadId, q.id)"
        >
          <CornerDownRight :size="13" />
        </button>
        <button class="qp-queue-btn" :disabled="['sending', 'running', 'awaiting'].includes(q.state)" title="Edit (Alt+Up edits the newest)" @click="threads.editQueued(threadId, q.id)"><Pencil :size="12" /></button>
        <button class="qp-queue-btn" :disabled="['sending', 'running', 'awaiting'].includes(q.state)" title="Remove" @click="threads.queueRemove(threadId, q.id)"><X :size="13" /></button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qp-queue { display: flex; flex-direction: column; align-items: flex-end; gap: 6px; padding: 0 0 12px; }
.qp-queue-content { flex: 1; min-width: 0; }
.qp-queue-files { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 5px; font-size: 11px; }
.qp-queue-files span { display: inline-flex; align-items: center; gap: 4px; max-width: 180px; overflow: hidden; }
.qp-queue-files img { width: 28px; height: 28px; object-fit: cover; border-radius: 3px; }
.qp-queue-error { color: var(--qss-error, #f87171); font-size: 12px; margin-top: 5px; }
.qp-queue-btn:disabled { opacity: 0.35; cursor: default; }
.qp-queue-head { display: flex; align-items: center; gap: 8px; font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-queue-held { color: var(--qss-warning, #d29a3f); }
.qp-queue-resume {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 22px;
  padding: 0 8px;
  border-radius: 6px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  color: var(--qss-text, #d4d4d8);
  font-size: 11.5px;
  cursor: pointer;
}
.qp-queue-resume:hover { background: var(--qss-bg-hover, #313139); }
.qp-queue-item {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  max-width: min(86%, 640px);
  padding: 7px 8px 7px 12px;
  border-radius: 12px;
  border: 1px dashed var(--qss-border, #47474f);
  color: var(--qss-text-secondary, #9a9aa5);
}
.qp-queue-item.editing { border-color: var(--qss-accent, #7aa2f7); }
.qp-queue-text { flex: 1; min-width: 0; font-size: 13px; line-height: 1.55; white-space: pre-wrap; overflow-wrap: anywhere; max-height: 7.8em; overflow: hidden; }
.qp-queue-actions { display: flex; gap: 2px; flex-shrink: 0; opacity: 0.55; }
.qp-queue-item:hover .qp-queue-actions,
.qp-queue-actions:focus-within { opacity: 1; }
.qp-queue-btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text-secondary, #9a9aa5);
  cursor: pointer;
}
.qp-queue-btn:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
</style>
