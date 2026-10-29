<script setup lang="ts">
/**
 * A sent message: its attachments (thumbnails, files, folders), the text
 * with its chips, and an "Attached context" fold with what each chip
 * carried (card 9).
 */
import { computed } from 'vue'
import { ChevronRight, FileText, Folder } from 'lucide-vue-next'
import { CHIP_OPEN, segments } from '#pilot/utils/mentions'
import type { ThreadItem } from '#pilot/types'

const props = defineProps<{ item: ThreadItem }>()
const emit = defineEmits<{ notice: [text: string] }>()

interface Shown {
  kind: string
  name?: string
  path?: string
  thumb?: string | null
  chipKind?: string
  label?: string
  content?: string
}

const text = computed(() => String(props.item.data.text ?? ''))
const shown = computed<Shown[]>(() => props.item.data.attachments ?? [])
const images = computed(() => shown.value.filter((a) => a.kind === 'image'))
const files = computed(() => shown.value.filter((a) => a.kind === 'file' || a.kind === 'folder'))
const chips = computed(() => shown.value.filter((a) => a.kind === 'chip'))
/** What the chips carried — a skill carries only its name. */
const context = computed(() => chips.value.filter((c) => c.content && c.chipKind !== 'skill'))
const hasChips = computed(() => text.value.includes(CHIP_OPEN))
const parts = computed(() => segments(text.value))
const chipTitle = (label: string) => {
  const c = chips.value.find((x) => x.label === label)
  return c ? `${c.chipKind ?? 'context'} · ${(c.content ?? '').slice(0, 300)}` : label
}
</script>

<template>
  <div class="qp-um">
    <div v-if="images.length" class="qp-um-images">
      <img v-for="(im, i) in images" :key="i" class="qp-um-thumb" :src="im.thumb ?? undefined" :alt="im.name" :title="im.path" />
      <span v-if="images.some((im) => !im.thumb)" class="qp-um-count">{{ images.length }} image{{ images.length === 1 ? '' : 's' }}</span>
    </div>
    <div v-if="files.length" class="qp-um-files">
      <span v-for="(f, i) in files" :key="i" class="qp-um-file" :title="f.path">
        <Folder v-if="f.kind === 'folder'" :size="12" />
        <FileText v-else :size="12" />
        {{ f.name }}
      </span>
    </div>
    <div v-if="hasChips" class="qp-um-text">
      <template v-for="(p, i) in parts" :key="i">
        <span v-if="p.chip" class="qp-um-chip" :title="chipTitle(p.text)">{{ p.text }}</span>
        <template v-else>{{ p.text }}</template>
      </template>
    </div>
    <PilotThreadMarkdown v-else-if="text" :source="text" @notice="(t: string) => emit('notice', t)" />
    <details v-if="context.length" class="qp-um-context">
      <summary><ChevronRight class="qp-um-caret" :size="12" /> Attached context · {{ context.length }}</summary>
      <div v-for="(c, i) in context" :key="i" class="qp-um-block">
        <div class="qp-um-block-head">{{ c.label }}</div>
        <pre class="qp-um-pre">{{ c.content }}</pre>
      </div>
    </details>
  </div>
</template>

<style scoped>
.qp-um { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
.qp-um-images { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
.qp-um-thumb { max-width: 160px; max-height: 120px; border-radius: 6px; border: 1px solid var(--qss-border-subtle, #35353d); object-fit: cover; }
.qp-um-count { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-um-files { display: flex; flex-wrap: wrap; gap: 4px; }
.qp-um-file,
.qp-um-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 0 6px;
  border-radius: 5px;
  background: color-mix(in srgb, var(--qss-accent, #7aa2f7) 16%, transparent);
  color: var(--qss-text, #d4d4d8);
  font-size: 12px;
  line-height: 1.6;
}
.qp-um-text { font-size: 13.5px; line-height: 1.55; white-space: pre-wrap; overflow-wrap: anywhere; }
.qp-um-context { font-size: 12px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-um-context summary { display: inline-flex; align-items: center; gap: 4px; cursor: pointer; list-style: none; }
.qp-um-context summary::-webkit-details-marker { display: none; }
.qp-um-context[open] .qp-um-caret { transform: rotate(90deg); }
.qp-um-block { margin-top: 6px; }
.qp-um-block-head { font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); margin-bottom: 2px; }
.qp-um-pre {
  max-height: 200px;
  margin: 0;
  padding: 6px 8px;
  overflow: auto;
  border-radius: 6px;
  background: var(--qss-bg, #18181e);
  font-family: var(--qss-font-mono, ui-monospace, monospace);
  font-size: 11.5px;
  white-space: pre-wrap;
}
</style>
