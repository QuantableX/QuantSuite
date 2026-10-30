<script setup lang="ts">
/**
 * A linked pull request — `PR #45 · open` (compact: `#45`) — or issue —
 * `#12 · open` (card 23) — in its state's colour; a click opens it on
 * GitHub (card 12).
 */
import { computed } from 'vue'
import { CircleCheck, CircleDot, GitPullRequest, GitPullRequestClosed, GitPullRequestDraft, GitMerge } from 'lucide-vue-next'
import { openExternal } from '#pilot/utils/external'
import type { ThreadLink } from '#pilot/types'

const props = defineProps<{ link: ThreadLink; compact?: boolean }>()
const ICONS: Record<string, typeof GitPullRequest> = { open: GitPullRequest, draft: GitPullRequestDraft, merged: GitMerge, closed: GitPullRequestClosed }
const issue = computed(() => props.link.kind === 'issue')
const icon = computed(() => (issue.value ? (props.link.state === 'closed' ? CircleCheck : CircleDot) : (ICONS[props.link.state] ?? GitPullRequest)))
const tip = computed(() => `${props.link.title ? `${props.link.title} — ` : ''}${issue.value ? 'issue' : 'pull request'} #${props.link.number} (${props.link.state}). Click to open on GitHub.`)
</script>

<template>
  <button class="qp-pr" :class="[link.state, { compact, issue }]" :title="tip" @click.stop="openExternal(link.url)" @pointerdown.stop>
    <component :is="icon" :size="compact ? 11 : 12" />
    <template v-if="compact">#{{ link.number }}</template>
    <template v-else>{{ issue ? '' : 'PR ' }}#{{ link.number }} · {{ link.state }}</template>
  </button>
</template>

<style scoped>
.qp-pr {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
  padding: 1px 8px;
  border: 1px solid var(--qss-border-subtle, #35353d);
  border-radius: 999px;
  background: transparent;
  color: var(--qss-success, #34d399);
  font: inherit;
  font-size: 11px;
  cursor: pointer;
}
.qp-pr:hover { background: var(--qss-bg-hover, #313139); }
.qp-pr.draft,
.qp-pr.closed { color: var(--qss-text-muted, #6e6e7a); }
.qp-pr.closed { color: var(--qss-error, #f87171); }
.qp-pr.merged { color: var(--qss-accent, #7aa2f7); }
.qp-pr.issue.closed { color: #a78bfa; }
.qp-pr.compact { padding: 0 5px; border: none; font-size: 10.5px; }
</style>
