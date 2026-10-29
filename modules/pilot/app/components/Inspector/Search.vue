<script setup lang="ts">
/**
 * Project search (Ctrl+Shift+F, card 13): text or regex, case on or off,
 * across the checkout's files — `.gitignore` honoured, binary and large
 * files skipped, at most 1,000 matches. A match opens its file at the line.
 */
import { nextTick, onMounted, ref, watch } from 'vue'
import { CaseSensitive, Regex } from 'lucide-vue-next'
import { pilot } from '#pilot/utils/invoke'
import { useInspectorStore } from '#pilot/stores/inspector'
import type { TextSearchResult } from '#pilot/types'

const props = defineProps<{ threadId: string; current: string | null }>()
const emit = defineEmits<{ open: [rel: string, line: number] }>()

const query = ref('')
const regex = ref(false)
const cased = ref(false)
const result = ref<TextSearchResult | null>(null)
const error = ref('')
const busy = ref(false)
const input = ref<HTMLInputElement | null>(null)
let timer: ReturnType<typeof setTimeout> | null = null
let generation = 0

async function run() {
  const q = query.value
  const gen = ++generation
  if (!q.trim()) {
    result.value = null
    error.value = ''
    return
  }
  busy.value = true
  try {
    const r = await pilot<TextSearchResult>('plugin:pilot|pilot_text_search', { id: props.threadId, query: q, regex: regex.value, case: cased.value })
    if (gen !== generation) return
    result.value = r
    error.value = ''
  } catch (e) {
    if (gen !== generation) return
    result.value = null
    error.value = String(e)
  } finally {
    if (gen === generation) busy.value = false
  }
}
watch([query, regex, cased], () => {
  if (timer) clearTimeout(timer)
  timer = setTimeout(run, 220)
})
watch(() => props.threadId, () => void run())

/** Where a match is inside the preview line, for the highlight. */
function parts(text: string): [string, string, string] {
  try {
    const re = regex.value ? new RegExp(query.value, cased.value ? '' : 'i') : null
    const m = re ? re.exec(text) : null
    if (re && m) return [text.slice(0, m.index), m[0], text.slice(m.index + m[0].length)]
    if (!re) {
      const hay = cased.value ? text : text.toLowerCase()
      const needle = cased.value ? query.value : query.value.toLowerCase()
      const i = hay.indexOf(needle)
      if (i >= 0) return [text.slice(0, i), text.slice(i, i + needle.length), text.slice(i + needle.length)]
    }
  } catch {
    /* an invalid pattern shows plain text */
  }
  return [text, '', '']
}

onMounted(() => void nextTick(() => input.value?.focus()))
watch(
  () => useInspectorStore().searchNonce,
  () => void nextTick(() => input.value?.select())
)
defineExpose({ focus: () => input.value?.select() })
</script>

<template>
  <div class="qp-ps">
    <div class="qp-ps-field">
      <input ref="input" v-model="query" class="qp-ps-input" placeholder="Search the project" spellcheck="false" aria-label="Search the project" />
      <button class="qp-ps-opt" :class="{ on: cased }" title="Match case" :aria-pressed="cased" @click="cased = !cased"><CaseSensitive :size="14" /></button>
      <button class="qp-ps-opt" :class="{ on: regex }" title="Regular expression" :aria-pressed="regex" @click="regex = !regex"><Regex :size="14" /></button>
    </div>
    <div class="qp-ps-sum">
      <template v-if="busy">Searching…</template>
      <template v-else-if="error">{{ error }}</template>
      <template v-else-if="result">
        {{ result.total }} match{{ result.total === 1 ? '' : 'es' }} in {{ result.files.length }} file{{ result.files.length === 1 ? '' : 's' }}{{ result.truncated ? ' — stopped early' : '' }}
      </template>
    </div>
    <div class="qp-ps-list">
      <div v-for="f in result?.files ?? []" :key="f.rel" class="qp-ps-file">
        <div class="qp-ps-name" :title="f.rel">
          <span class="qp-ps-base">{{ f.rel.split('/').pop() }}</span>
          <span class="qp-ps-dir">{{ f.rel.split('/').slice(0, -1).join('/') }}</span>
          <span class="qp-ps-count">{{ f.matches.length }}</span>
        </div>
        <button
          v-for="m in f.matches"
          :key="m.line"
          class="qp-ps-hit"
          :class="{ on: current === f.rel }"
          :title="`${f.rel}:${m.line}`"
          @click="emit('open', f.rel, m.line)"
        >
          <span class="qp-ps-line">{{ m.line }}</span>
          <span class="qp-ps-text">{{ parts(m.text)[0] }}<mark>{{ parts(m.text)[1] }}</mark>{{ parts(m.text)[2] }}</span>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qp-ps { display: flex; flex-direction: column; min-height: 0; height: 100%; font-size: 12px; }
.qp-ps-field { display: flex; align-items: center; gap: 2px; margin: 6px 6px 4px; padding: 2px 4px 2px 8px; border: 1px solid var(--qss-border, #47474f); border-radius: 6px; background: var(--qss-bg, #18181e); }
.qp-ps-field:focus-within { border-color: var(--qss-accent, #7aa2f7); }
.qp-ps-input { flex: 1; min-width: 0; padding: 3px 0; border: none; background: transparent; color: var(--qss-text, #d4d4d8); font: inherit; outline: none; }
.qp-ps-opt { display: inline-flex; padding: 2px; border: 1px solid transparent; border-radius: 4px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-ps-opt.on { border-color: var(--qss-accent, #7aa2f7); color: var(--qss-text, #d4d4d8); }
.qp-ps-sum { padding: 0 8px 4px; color: var(--qss-text-muted, #6e6e7a); overflow-wrap: anywhere; }
.qp-ps-list { flex: 1; min-height: 0; overflow: auto; padding: 0 4px 8px; }
.qp-ps-name { display: flex; align-items: baseline; gap: 6px; padding: 4px 4px 2px; min-width: 0; }
.qp-ps-base { color: var(--qss-text, #d4d4d8); font-weight: 600; white-space: nowrap; }
.qp-ps-dir { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--qss-text-muted, #6e6e7a); }
.qp-ps-count { margin-left: auto; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-ps-hit { display: flex; gap: 6px; width: 100%; padding: 2px 4px 2px 10px; border: none; border-radius: 4px; background: transparent; color: var(--qss-text-secondary, #9a9aa5); font: inherit; text-align: left; cursor: pointer; }
.qp-ps-hit:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-ps-line { width: 34px; flex-shrink: 0; text-align: right; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-ps-text { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: pre; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; }
.qp-ps-text mark { background: color-mix(in srgb, var(--qss-accent, #7aa2f7) 30%, transparent); color: var(--qss-text, #d4d4d8); border-radius: 2px; }
</style>
