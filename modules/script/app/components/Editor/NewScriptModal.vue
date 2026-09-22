<script setup lang="ts">
/**
 * New script: a registry key, a class name, a display name — and the
 * template lands as `<key>.py`, registering the key through REGISTER /
 * WARMUP (indicators/_discover.py). The key is checked against the registry
 * in the sandbox before anything is written.
 */
import { useWorkbenchStore } from '#script/stores/workbench'
import { ArrowRight, FileCode2 } from 'lucide-vue-next'

const wb = useWorkbenchStore()
const router = useRouter()

const key = ref('')
const className = ref('')
const name = ref('')
const busy = ref(false)
const error = ref('')
const keyTouched = ref(false)
const dialog = ref<HTMLElement | null>(null)
const previousFocus = typeof document !== 'undefined' ? document.activeElement as HTMLElement | null : null
watch(name, (value) => {
  if (!keyTouched.value) key.value = value.normalize('NFKD').replace(/[\u0300-\u036f]/g, '').toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^[^a-z]+/, '').replace(/_+$/, '').slice(0, 40)
})

const KEY = /^[a-z][a-z0-9_]{0,39}$/
const CLASS = /^[A-Z][A-Za-z0-9_]{0,63}$/

const keyTaken = computed(() => wb.scripts.some((s) => s.classes.some((c) => c.key === key.value) || s.file === `${key.value}.py`))
const keyOk = computed(() => KEY.test(key.value) && !keyTaken.value)
const classOk = computed(() => CLASS.test(className.value))
const canCreate = computed(() => keyOk.value && classOk.value && !busy.value && !!wb.python?.ok)

/** `my_trend` → `MyTrend`, offered while the class field is untouched. */
const classTouched = ref(false)
watch(key, (k) => {
  if (classTouched.value) return
  className.value = k
    .split('_')
    .filter(Boolean)
    .map((p) => p.charAt(0).toUpperCase() + p.slice(1))
    .join('')
})

async function create() {
  if (!canCreate.value) return
  busy.value = true
  error.value = ''
  try {
    const file = await wb.create({ key: key.value.trim(), class_name: className.value.trim(), name: name.value.trim() || undefined })
    if (file) void router.push('/script')
    else error.value = wb.notice?.text ?? 'Could not create the script. Try another key or check Python in Settings.'
  } finally {
    busy.value = false
  }
}

function close() {
  if (!busy.value) wb.newScriptOpen = false
}

const keyInput = ref<HTMLInputElement | null>(null)
onMounted(() => keyInput.value?.focus())
onUnmounted(() => previousFocus?.focus())
function trapFocus(e: KeyboardEvent) {
  if (e.key !== 'Tab') return
  const elements = Array.from(dialog.value?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), summary') ?? []).filter((el) => el.getClientRects().length)
  const first = elements[0], last = elements.at(-1)
  if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last?.focus() }
  else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first?.focus() }
}
</script>

<template>
  <div class="qsc-modal-overlay" @mousedown.self="close">
    <div ref="dialog" class="qsc-modal" role="dialog" aria-modal="true" aria-label="New script" @keydown.esc.stop="close" @keydown="trapFocus">
      <header class="qsc-modal-head">
        <h2 class="qsc-modal-title">Create an indicator</h2>
        <button class="qsc-icon-btn" aria-label="Close dialog" title="Close" :disabled="busy" @click="close">
          <svg width="12" height="12" viewBox="0 0 14 14" fill="none" aria-hidden="true">
            <path d="M3 3l8 8M11 3l-8 8" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" />
          </svg>
        </button>
      </header>

      <div class="qsc-template-summary"><FileCode2 :size="22" /><div><strong>EMA trend</strong><p>A working Python template with volatility normalization and hysteresis. Ready to customize.</p></div></div>

      <label class="qsc-field">
        <span class="qsc-field-label">Indicator name</span>
        <input ref="keyInput" v-model="name" :disabled="busy" class="qsc-input" placeholder="My trend indicator" @keydown.enter="create" />
        <span class="qsc-field-hint">The name shown in your library and reports.</span>
      </label>

      <label class="qsc-field">
        <span class="qsc-field-label">Script key</span>
        <input v-model.trim="key" :disabled="busy" class="qsc-input mono" placeholder="my_trend" spellcheck="false" @input="keyTouched = true" @keydown.enter="create" />
        <span class="qsc-field-hint" :class="{ 'is-bad': key && !keyOk }">
          <template v-if="key && keyTaken">taken — pick another</template>
          <template v-else-if="key && !KEY.test(key)">a letter, then lowercase letters, digits, underscores</template>
          <template v-else>what a strategy passes to <span class="mono">self.regime("{{ key || 'key' }}")</span></template>
        </span>
      </label>

      <details class="qsc-advanced"><summary>Advanced · Python class</summary><label class="qsc-field">
        <span class="qsc-field-label">Class name</span>
        <input v-model.trim="className" :disabled="busy" class="qsc-input mono" placeholder="MyTrend" spellcheck="false" @input="classTouched = true" @keydown.enter="create" />
        <span class="qsc-field-hint" :class="{ 'is-bad': className && !classOk }">CapitalizedWords</span>
      </label></details>
      <div class="qsc-create-preview"><span class="mono">{{ key || 'my_trend' }}.py</span><ArrowRight :size="13" /><span>Check & register</span><ArrowRight :size="13" /><span>Open editor</span></div>
      <p v-if="error" class="qsc-note is-error" role="alert">{{ error }}</p>
      <p v-else-if="!wb.python?.ok" class="qsc-note is-warn">{{ wb.listingLoading ? 'Connecting to Python…' : 'Python must be available to create and register a script. Check the interpreter in Settings.' }}</p>

      <footer class="qsc-modal-foot">
        <button class="qsc-btn" :disabled="busy" @click="close">Cancel</button>
        <button class="qsc-btn is-primary" :disabled="!canCreate" @click="create">{{ busy ? 'Checking & creating…' : 'Create script' }}</button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.qsc-modal-overlay {
  position: absolute;
  inset: 0;
  z-index: 40;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.45);
}
.qsc-modal {
  width: min(490px, calc(100% - 40px));
  max-height: calc(100% - 40px);
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 20px;
  padding: 24px;
  border: 1px solid var(--qss-border);
  border-radius: var(--qss-radius-lg);
  background: var(--qss-bg-raised);
  box-shadow: var(--qss-shadow-lg);
}
.qsc-template-summary { display: flex; gap: 14px; padding: 14px; background: var(--qss-bg); border: 1px solid var(--qss-border); border-radius: 8px; }
.qsc-template-summary svg { flex-shrink: 0; margin-top: 3px; }
.qsc-template-summary strong { font-size: 12px; font-weight: 600; }
.qsc-template-summary p { color: var(--qss-text-muted); font-size: 12px; line-height: 1.65; margin-top: 4px; }
.qsc-advanced summary { font-size: 11px; color: var(--qss-text-muted); cursor: pointer; }
.qsc-advanced .qsc-field { margin-top: 12px; }
.qsc-create-preview { display: flex; align-items: center; justify-content: space-between; gap: 7px; font-size: 10px; color: var(--qss-text-muted); border-top: 1px solid var(--qss-border); padding-top: 15px; }
.qsc-create-preview .mono { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qsc-field .qsc-input { height: 36px; }
.qsc-modal-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.qsc-modal-title {
  font-size: 14px;
  font-weight: 600;
}
.qsc-modal-hint {
  font-size: 12px;
  line-height: 1.45;
  color: var(--qss-text-secondary);
}
.qsc-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.qsc-field-label {
  font-size: 11px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--qss-text-muted);
}
.qsc-field-hint {
  font-size: 11px;
  color: var(--qss-text-muted);
}
.qsc-field-hint.is-bad {
  color: var(--qss-error);
}
.qsc-modal-foot {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 4px;
}
</style>
