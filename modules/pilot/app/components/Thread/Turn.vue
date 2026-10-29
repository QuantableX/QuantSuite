<script setup lang="ts">
/**
 * One turn of the conversation: the user's message, the "Worked for …"
 * fold, the answer, then whatever needs the user (requests) and what must
 * stay visible (plans, errors, notices).
 */
import { computed } from 'vue'
import { AlertTriangle, CornerDownRight, Info } from 'lucide-vue-next'
import type { TurnBlock } from '#pilot/utils/timeline'

const props = defineProps<{
  block: TurnBlock
  running: boolean
  elapsed: number
  cost: number | null
}>()
const emit = defineEmits<{ notice: [text: string] }>()

const userText = computed(() => String(props.block.user?.data.text ?? ''))
const images = computed<{ kind: string }[]>(() => (props.block.user?.data.attachments ?? []).filter((a: { kind: string }) => a.kind === 'image'))
const showActivity = computed(() => props.block.activity.length > 0 || (props.running && !props.block.answer))

async function copyUser() {
  try {
    await navigator.clipboard.writeText(userText.value)
    emit('notice', 'Message copied')
  } catch {
    /* clipboard denied — nothing to do */
  }
}
</script>

<template>
  <section class="qp-turn" :data-turn="block.turnId">
    <div v-if="block.user" class="qp-user" :data-item-id="block.user.id">
      <div class="qp-user-bubble">
        <div v-if="images.length" class="qp-user-att">{{ images.length }} image{{ images.length === 1 ? '' : 's' }}</div>
        <PilotThreadMarkdown :source="userText" @notice="(t: string) => emit('notice', t)" />
      </div>
      <button class="qp-user-copy" title="Copy message" @click="copyUser">Copy</button>
    </div>

    <PilotThreadActivity v-if="showActivity" :block="block" :running="running" :elapsed="elapsed" @notice="(t: string) => emit('notice', t)" />

    <template v-for="o in [...block.before, null, ...block.outside]" :key="o?.id ?? 'answer'">
      <template v-if="o === null">
        <div v-if="block.answer" class="qp-answer" :data-item-id="block.answer.id">
          <PilotThreadMarkdown :source="String(block.answer.data.text ?? '')" :streaming="Boolean(block.answer.data.streaming)" @notice="(t: string) => emit('notice', t)" />
        </div>
        <!-- Pending approvals and questions wait in the bar above the box (Approvals.vue). -->
        <PilotThreadPlanCard v-for="r in block.requests.filter((x) => x.kind === 'plan')" :key="r.id" :item="r" @notice="(t: string) => emit('notice', t)" />
      </template>
      <div v-else-if="o.kind === 'user'" class="qp-user is-steer" :data-item-id="o.id">
        <div class="qp-user-bubble">
          <div class="qp-user-att"><CornerDownRight :size="11" /> Sent while the agent worked</div>
          <PilotThreadMarkdown :source="String(o.data.text ?? '')" @notice="(t: string) => emit('notice', t)" />
        </div>
      </div>
      <div v-else-if="o.kind === 'error'" class="qp-error" :data-item-id="o.id">
        <AlertTriangle :size="14" />
        <span class="qp-error-text">{{ o.data.text }}</span>
      </div>
      <div v-else-if="o.kind === 'notice'" class="qp-notice"><Info :size="12" />{{ o.data.text }}</div>
      <div v-else-if="o.kind === 'plan'" class="qp-plan" :data-item-id="o.id">
        <div class="qp-plan-head">Plan · {{ o.data.status === 'approved' ? 'implementing' : o.data.status === 'rejected' ? 'kept planning' : o.data.status }}</div>
        <PilotThreadMarkdown :source="String(o.data.plan ?? '')" />
      </div>
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
.qp-user:hover .qp-user-copy,
.qp-user-copy:focus-visible { opacity: 1; }
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
.qp-plan {
  margin: 6px 0;
  padding: 10px 12px;
  border-radius: 10px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
}
.qp-plan-head { font-size: 12px; font-weight: 600; color: var(--qss-text-secondary, #9a9aa5); margin-bottom: 6px; }
</style>
