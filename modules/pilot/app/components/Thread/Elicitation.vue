<script setup lang="ts">
/**
 * An MCP server waiting on the user (parity card 05): a form built from the
 * server's schema, or a step on a web page (URL mode, e.g. signing in).
 *
 * - Form: every field with its type, choices, limits and whether it is
 *   required; checked here as typed and again by the backend before it goes
 *   out. A refused submit keeps everything typed (drafts live per request).
 * - Web page: who asks, why and where to. Nothing opens by itself — Open (or
 *   Copy link) first, then "I've finished" reports completion; Decline and
 *   Cancel say no.
 */
import { computed, reactive, ref, watchEffect } from 'vue'
import { Copy, ExternalLink, Plug } from 'lucide-vue-next'
import { useThreadStore } from '#pilot/stores/thread'
import type { ThreadItem } from '#pilot/types'
import { openExternal } from '#pilot/utils/external'
import { fieldError, formContent, initialValue, requestDraft, type FormField } from '#pilot/utils/requestDrafts'

const props = defineProps<{ threadId: string; item: ThreadItem; position?: string }>()
const emit = defineEmits<{ done: [] }>()
const threads = useThreadStore()

const req = computed(() => props.item.data.elicitation as {
  mode: 'form' | 'url'
  server: string
  message: string
  fields?: FormField[]
  url?: string
  host?: string
})
const requestId = computed(() => String(props.item.data.requestId))
const draft = computed(() => requestDraft(props.threadId, requestId.value))
const fields = computed<FormField[]>(() => req.value.fields ?? [])

// Fill each field's starting value once; later edits stay in the draft.
watchEffect(() => {
  const d = draft.value
  for (const f of fields.value) if (!(f.key in d.values)) d.values[f.key] = initialValue(f)
})

const touched = reactive<Record<string, boolean>>({})
const tried = ref(false)
const busy = ref(false)
const errors = computed(() => Object.fromEntries(fields.value.map((f) => [f.key, fieldError(f, draft.value.values[f.key])])))
const valid = computed(() => fields.value.every((f) => !errors.value[f.key]))
const shown = (f: FormField) => (tried.value || touched[f.key] ? errors.value[f.key] : null)

async function send(action: 'accept' | 'decline' | 'cancel') {
  if (busy.value) return
  if (action === 'accept' && req.value.mode === 'form') {
    tried.value = true
    if (!valid.value) return
  }
  busy.value = true
  const content = action === 'accept' && req.value.mode === 'form' ? formContent(fields.value, draft.value.values) : null
  const err = await threads.answer(props.threadId, requestId.value, { decision: 'elicitation', action, content }, true)
  busy.value = false
  draft.value.error = err
  if (!err) emit('done')
}

function toggleMulti(f: FormField, value: string) {
  const cur = (draft.value.values[f.key] as string[] | undefined) ?? []
  draft.value.values[f.key] = cur.includes(value) ? cur.filter((v) => v !== value) : [...cur, value]
  touched[f.key] = true
}
function setBool(f: FormField, v: boolean) {
  // An optional yes/no can be cleared again by picking the same answer.
  draft.value.values[f.key] = !f.required && draft.value.values[f.key] === v ? null : v
  touched[f.key] = true
}
function inputType(f: FormField): string {
  if (f.kind === 'number' || f.kind === 'integer') return 'number'
  switch (f.format) {
    case 'email':
      return 'email'
    case 'uri':
      return 'url'
    case 'date':
      return 'date'
    case 'date-time':
      return 'datetime-local'
    default:
      return 'text'
  }
}
function hint(f: FormField): string {
  const parts: string[] = []
  if (f.minLength != null && f.maxLength != null) parts.push(`${f.minLength}–${f.maxLength} characters`)
  else if (f.minLength != null) parts.push(`at least ${f.minLength} characters`)
  else if (f.maxLength != null) parts.push(`up to ${f.maxLength} characters`)
  if (f.minimum != null && f.maximum != null) parts.push(`${f.minimum}–${f.maximum}`)
  else if (f.minimum != null) parts.push(`at least ${f.minimum}`)
  else if (f.maximum != null) parts.push(`at most ${f.maximum}`)
  if (f.kind === 'multi' && (f.minItems != null || f.maxItems != null)) {
    parts.push(f.minItems != null && f.maxItems != null ? `pick ${f.minItems}–${f.maxItems}` : f.minItems != null ? `pick at least ${f.minItems}` : `pick up to ${f.maxItems}`)
  }
  return parts.join(' · ')
}

async function open() {
  if (!req.value.url) return
  draft.value.opened = true
  await openExternal(req.value.url)
}
async function copy() {
  if (!req.value.url) return
  try {
    await navigator.clipboard.writeText(req.value.url)
    draft.value.opened = true
  } catch {
    draft.value.error = 'Could not copy the link.'
  }
}
</script>

<template>
  <div class="qp-el" role="group" :aria-label="`${req.server} asks for input`">
    <div class="qp-el-head">
      <Plug :size="13" class="qp-el-icon" />
      <span class="qp-el-tag">{{ req.server }}</span>
      <span class="qp-el-kind">{{ req.mode === 'url' ? 'wants you to continue on a web page' : 'asks for details' }}</span>
      <span v-if="position" class="qp-el-pos">{{ position }}</span>
    </div>
    <div v-if="req.message" class="qp-el-msg">{{ req.message }}</div>

    <template v-if="req.mode === 'url'">
      <div class="qp-el-dest">
        <span class="qp-el-host">{{ req.host }}</span>
        <code class="qp-el-url" :title="req.url">{{ req.url }}</code>
      </div>
      <div class="qp-el-note">Nothing opens on its own. Check the address, finish there, then come back and confirm.</div>
      <div class="qp-ap-actions">
        <button class="qp-btn qp-btn-sm" :class="{ 'qp-btn-primary': !draft.opened }" :disabled="busy" @click="open"><ExternalLink :size="12" /> Open in browser</button>
        <button class="qp-btn qp-btn-sm" :disabled="busy" @click="copy"><Copy :size="12" /> Copy link</button>
        <button
          class="qp-btn qp-btn-sm"
          :class="{ 'qp-btn-primary': draft.opened }"
          :disabled="busy || !draft.opened"
          :title="draft.opened ? '' : 'Open or copy the link first'"
          @click="send('accept')"
        >
          I've finished
        </button>
        <button class="qp-btn qp-btn-sm" :disabled="busy" @click="send('decline')">Decline</button>
        <button class="qp-btn qp-btn-sm" :disabled="busy" @click="send('cancel')">Cancel</button>
      </div>
    </template>

    <form v-else class="qp-el-form" novalidate @submit.prevent="send('accept')">
      <div v-for="f in fields" :key="f.key" class="qp-el-field">
        <label class="qp-el-label" :for="`qp-el-${item.id}-${f.key}`">
          {{ f.title }}<span v-if="f.required" class="qp-el-req" aria-label="required">*</span>
          <span v-else class="qp-el-opt">optional</span>
        </label>
        <div v-if="f.description" class="qp-el-desc">{{ f.description }}</div>

        <div v-if="f.kind === 'boolean'" class="qp-el-seg" role="radiogroup" :aria-label="f.title">
          <button type="button" class="qp-ap-opt" :class="{ on: draft.values[f.key] === true }" :aria-checked="draft.values[f.key] === true" role="radio" @click="setBool(f, true)">Yes</button>
          <button type="button" class="qp-ap-opt" :class="{ on: draft.values[f.key] === false }" :aria-checked="draft.values[f.key] === false" role="radio" @click="setBool(f, false)">No</button>
        </div>
        <select
          v-else-if="f.kind === 'choice'"
          :id="`qp-el-${item.id}-${f.key}`"
          v-model="draft.values[f.key]"
          class="qp-el-input"
          @blur="touched[f.key] = true"
        >
          <option value="">{{ f.required ? 'Choose…' : '— none —' }}</option>
          <option v-for="o in f.options" :key="o.value" :value="o.value">{{ o.label }}</option>
        </select>
        <div v-else-if="f.kind === 'multi'" class="qp-el-multi">
          <label v-for="o in f.options" :key="o.value" class="qp-el-check">
            <input type="checkbox" :checked="((draft.values[f.key] as string[]) ?? []).includes(o.value)" @change="toggleMulti(f, o.value)" />
            {{ o.label }}
          </label>
        </div>
        <input
          v-else
          :id="`qp-el-${item.id}-${f.key}`"
          v-model="draft.values[f.key]"
          class="qp-el-input"
          :type="inputType(f)"
          :min="f.minimum ?? undefined"
          :max="f.maximum ?? undefined"
          :step="f.kind === 'integer' ? 1 : f.kind === 'number' ? 'any' : undefined"
          :maxlength="f.maxLength ?? undefined"
          autocomplete="off"
          spellcheck="false"
          @blur="touched[f.key] = true"
        />
        <div v-if="shown(f)" class="qp-el-err">{{ shown(f) }}</div>
        <div v-else-if="hint(f)" class="qp-el-hint">{{ hint(f) }}</div>
      </div>
      <div class="qp-ap-actions">
        <button type="submit" class="qp-btn qp-btn-sm qp-btn-primary" :disabled="busy">Send</button>
        <button type="button" class="qp-btn qp-btn-sm" :disabled="busy" @click="send('decline')">Decline</button>
        <button type="button" class="qp-btn qp-btn-sm" :disabled="busy" @click="send('cancel')">Cancel</button>
      </div>
    </form>
    <div v-if="draft.error" class="qp-el-err" role="alert">{{ draft.error }}</div>
  </div>
</template>

<style scoped>
.qp-el {
  padding: 9px 12px;
  border-radius: 10px;
  border: 1px solid color-mix(in srgb, var(--qss-warning, #d29a3f) 50%, var(--qss-border, #47474f));
  background: color-mix(in srgb, var(--qss-warning, #d29a3f) 6%, var(--qss-bg-raised, #1f1f25));
  display: flex;
  flex-direction: column;
  gap: 7px;
}
.qp-el-head { display: flex; align-items: center; gap: 6px; font-size: 12.5px; min-width: 0; }
.qp-el-icon { flex-shrink: 0; color: var(--qss-warning, #d29a3f); }
.qp-el-tag { font-weight: 600; color: var(--qss-warning, #d29a3f); }
.qp-el-kind { color: var(--qss-text-secondary, #9a9aa5); }
.qp-el-pos { margin-left: auto; font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-el-msg { font-size: 13px; color: var(--qss-text, #d4d4d8); white-space: pre-wrap; word-break: break-word; }
.qp-el-dest { display: flex; flex-direction: column; gap: 2px; padding: 7px 9px; border-radius: 7px; background: var(--qss-bg, #18181e); }
.qp-el-host { font-size: 13px; font-weight: 600; color: var(--qss-text, #d4d4d8); }
.qp-el-url { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); word-break: break-all; user-select: text; }
.qp-el-note { font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-el-form { display: flex; flex-direction: column; gap: 9px; }
.qp-el-field { display: flex; flex-direction: column; gap: 3px; }
.qp-el-label { font-size: 12.5px; color: var(--qss-text, #d4d4d8); display: flex; align-items: baseline; gap: 4px; }
.qp-el-req { color: var(--qss-warning, #d29a3f); }
.qp-el-opt { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-el-desc,
.qp-el-hint { font-size: 11.5px; color: var(--qss-text-muted, #6e6e7a); }
.qp-el-err { font-size: 11.5px; color: var(--qss-error, #e06c75); }
.qp-el-input {
  height: 28px;
  padding: 0 9px;
  border-radius: 7px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font-size: 12.5px;
  outline: none;
  color-scheme: dark;
}
.qp-el-input:focus { border-color: var(--qss-accent, #7aa2f7); }
.qp-el-seg { display: flex; gap: 6px; }
.qp-el-seg .qp-ap-opt { min-width: 64px; justify-content: center; }
.qp-el-multi { display: flex; flex-wrap: wrap; gap: 4px 14px; }
.qp-el-check { display: inline-flex; align-items: center; gap: 5px; font-size: 12.5px; color: var(--qss-text, #d4d4d8); cursor: pointer; }
.qp-ap-actions { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
.qp-ap-actions .qp-btn { display: inline-flex; align-items: center; gap: 4px; }
.qp-ap-opt {
  display: flex;
  align-items: center;
  padding: 5px 9px;
  border-radius: 8px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg, #18181e);
  color: var(--qss-text, #d4d4d8);
  font-size: 12.5px;
  cursor: pointer;
}
.qp-ap-opt:hover { background: var(--qss-bg-hover, #313139); }
.qp-ap-opt.on { border-color: var(--qss-accent, #7aa2f7); background: color-mix(in srgb, var(--qss-accent, #7aa2f7) 14%, var(--qss-bg, #18181e)); }
</style>
