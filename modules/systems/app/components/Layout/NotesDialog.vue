<script setup lang="ts">
/**
 * A strategy's notes: what the engine noted on its last run (read only) and the
 * user's own text, saved per strategy (stores/notes). Same dialog style as the
 * group editor. Unsaved text is never dropped by Esc or the X: the dialog asks.
 */
import { X } from 'lucide-vue-next'
import { useStrategyNotesStore } from '#systems/stores/notes'

const props = defineProps<{ systemId: string; name: string; engineNotes: string[]; source: string }>()
const emit = defineEmits<{ close: [] }>()
const store = useStrategyNotesStore()
const dialog = ref<HTMLDialogElement>()
const saved = ref(store.text(props.systemId))
const draft = ref(saved.value)
const leaving = ref(false)
const dirty = computed(() => draft.value !== saved.value)

async function save() {
  if (store.saving || !dirty.value) return
  if (await store.save(props.systemId, draft.value)) {
    saved.value = draft.value
    emit('close')
  }
}
function close() {
  if (dirty.value) { leaving.value = true; return }
  emit('close')
}
function onCancel(event: Event) {
  event.preventDefault()
  close()
}
function onKey(event: KeyboardEvent) {
  if ((event.ctrlKey || event.metaKey) && (event.key === 's' || event.key === 'Enter')) {
    event.preventDefault()
    save()
  }
}
onMounted(async () => {
  store.error = null
  dialog.value?.showModal()
  if (!store.loaded) {
    await store.load()
    // Text typed while the notes were loading wins.
    if (!dirty.value) draft.value = saved.value = store.text(props.systemId)
  }
})
onBeforeUnmount(() => dialog.value?.close())
</script>

<template>
  <dialog ref="dialog" class="qs-notes-dialog" aria-labelledby="qs-notes-title" @cancel="onCancel" @keydown="onKey">
    <form @submit.prevent="save">
      <header class="qs-notes-dialog__header">
        <div class="qs-notes-dialog__title">
          <h2 id="qs-notes-title">Notes</h2>
          <span>{{ name }}</span>
        </div>
        <button type="button" class="qs-notes-dialog__close" aria-label="Close notes" @click="close"><X :size="18" /></button>
      </header>
      <section class="qs-notes-dialog__section" aria-label="Engine notes">
        <span class="qs-notes-dialog__label">{{ source }}</span>
        <ul v-if="engineNotes.length" class="qs-notes-dialog__engine">
          <li v-for="(note, i) in engineNotes" :key="i">{{ note }}</li>
        </ul>
        <p v-else class="qs-notes-dialog__hint">Nothing noted. Notes appear here after a backtest or a live evaluation.</p>
      </section>
      <label class="qs-notes-dialog__section">
        <span class="qs-notes-dialog__label">Your notes</span>
        <textarea v-model="draft" class="textarea qs-notes-dialog__text" placeholder="Anything worth remembering about this strategy…" autofocus :disabled="store.saving" />
      </label>
      <p v-if="store.error" class="qs-notes-dialog__error" role="alert">{{ store.error }}</p>
      <div v-if="leaving" class="qs-notes-dialog__leave" role="group" aria-label="Unsaved notes">
        <p>Your notes have unsaved changes.</p>
        <button type="button" class="btn" @click="leaving = false">Keep editing</button>
        <button type="button" class="btn" @click="emit('close')">Discard</button>
      </div>
      <footer class="qs-notes-dialog__footer">
        <span class="qs-notes-dialog__hint">Ctrl+S saves</span>
        <button type="button" class="btn" :disabled="store.saving" @click="close">Cancel</button>
        <button class="btn btn-primary" :disabled="store.saving || !dirty">{{ store.saving ? 'Saving…' : 'Save notes' }}</button>
      </footer>
    </form>
  </dialog>
</template>

<style scoped>
.qs-notes-dialog { position: fixed; inset: 0; margin: auto; width: min(760px, calc(100vw - 32px)); height: fit-content; max-height: calc(100vh - 48px); overflow-y: auto; padding: 22px; border: 1px solid var(--qs-border); border-radius: 12px; background: var(--qs-bg-card); color: var(--qs-text); box-shadow: 0 24px 80px #0006; }
.qs-notes-dialog::backdrop { background: #0008; }
.qs-notes-dialog form { display: flex; flex-direction: column; gap: 16px; }
.qs-notes-dialog__header, .qs-notes-dialog__footer, .qs-notes-dialog__leave { display: flex; align-items: center; gap: 12px; }
.qs-notes-dialog__title { display: flex; flex: 1; min-width: 0; align-items: baseline; gap: 10px; }
.qs-notes-dialog__title h2 { margin: 0; font-size: 18px; font-weight: 600; }
.qs-notes-dialog__title span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 12px; color: var(--qs-text-muted); }
.qs-notes-dialog__close { display: grid; place-items: center; padding: 5px; border: 0; background: transparent; color: var(--qs-text-secondary); cursor: pointer; }
.qs-notes-dialog__close:hover { color: var(--qs-text); }
.qs-notes-dialog__section { display: flex; flex-direction: column; gap: 8px; }
.qs-notes-dialog__label { font-size: 12px; color: var(--qs-text-secondary); }
.qs-notes-dialog__engine { display: flex; flex-direction: column; gap: 6px; max-height: min(220px, 28vh); overflow-y: auto; margin: 0; padding: 10px 12px 10px 28px; list-style: disc; border: 1px solid var(--qs-border); border-radius: 8px; font-size: 12px; line-height: 1.5; color: var(--qs-text-secondary); }
.qs-notes-dialog__text { min-height: clamp(220px, 36vh, 460px); resize: vertical; font: inherit; font-size: 13px; line-height: 1.6; }
.qs-notes-dialog__hint { margin: 0; font-size: 11px; line-height: 1.5; color: var(--qs-text-muted); }
.qs-notes-dialog__error { margin: 0; font-size: 12px; color: var(--qs-error); }
.qs-notes-dialog__leave { padding: 10px 12px; border: 1px solid var(--qs-border); border-radius: 8px; font-size: 12px; }
.qs-notes-dialog__leave p { flex: 1; margin: 0; }
.qs-notes-dialog__footer { padding-top: 4px; }
.qs-notes-dialog__footer > span { flex: 1; }
.qs-notes-dialog button:disabled { cursor: default; opacity: .5; }
</style>
