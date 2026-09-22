<script setup lang="ts">
import { useAppStore } from '#memory/stores/app'
import { useVaultStore } from '#memory/stores/vault'

const app = useAppStore()
const vault = useVaultStore()
const router = useRouter()
const dialog = ref<HTMLDialogElement | null>(null)
const kind = ref('reference')
const tags = ref('')
const body = ref('')
const busy = ref(false)
const error = ref('')

watch(() => app.createOpen, async open => {
  if (!open) { dialog.value?.close(); return }
  kind.value = 'reference'
  tags.value = ''
  body.value = ''
  error.value = ''
  await nextTick()
  dialog.value?.showModal()
})

function close() {
  if (!busy.value) app.createOpen = false
}

async function create() {
  if (busy.value || !app.createTitle.trim()) return
  busy.value = true
  error.value = ''
  try {
    const meta = await vault.create({
      title: app.createTitle.trim(), scope: app.createScope, kind: kind.value,
      tags: [...new Set(tags.value.split(',').map(t => t.trim().replace(/^#/, '')).filter(Boolean))],
      body: body.value,
    })
    if (!meta) throw new Error('Memory was not created. Try again in the desktop app.')
    app.createOpen = false
    await router.push(`/memory/m/${encodeURIComponent(meta.id)}`)
  } catch (err) { error.value = String(err) }
  finally { busy.value = false }
}
</script>

<template>
  <dialog ref="dialog" class="qm-create" aria-labelledby="qm-create-title" @cancel.prevent="close" @click="e => { if (e.target === dialog) close() }">
    <form @submit.prevent="create">
      <header><h2 id="qm-create-title">New memory</h2><button type="button" :disabled="busy" aria-label="Close new memory" @click="close">×</button></header>
      <label>Title<input v-model="app.createTitle" autofocus required maxlength="240" :disabled="busy" /></label>
      <div class="qm-create-fields">
        <label>Save to<select v-model="app.createScope" :disabled="busy">
          <option value="general">General</option>
          <option v-for="scope in vault.scopes.filter(s => s.scope !== 'general')" :key="scope.scope" :value="scope.scope">{{ scope.name }}</option>
        </select></label>
        <label>Type<select v-model="kind" :disabled="busy"><option v-for="value in ['reference', 'decision', 'project', 'insight', 'user']" :key="value">{{ value }}</option></select></label>
      </div>
      <label>Tags<input v-model="tags" placeholder="Comma-separated" :disabled="busy" /></label>
      <label>Content<textarea v-model="body" rows="7" placeholder="What should be remembered?" :disabled="busy" /></label>
      <p v-if="error" role="alert" class="qm-create-error">{{ error }}</p>
      <footer><button type="button" :disabled="busy" @click="close">Cancel</button><button type="submit" :disabled="busy || !app.createTitle.trim()">{{ busy ? 'Creating…' : 'Create memory' }}</button></footer>
    </form>
  </dialog>
</template>

<style scoped>
.qm-create { position: fixed; width: min(560px, calc(100vw - 48px)); max-height: calc(100vh - 64px); padding: 24px; border: 1px solid var(--qm-border); border-radius: 10px; background: var(--qm-bg-raised); color: var(--qm-text); margin: auto; overflow: auto; }
.qm-create::backdrop { background: rgb(0 0 0 / .55); }
form { display: flex; flex-direction: column; gap: 16px; }
header, footer { display: flex; align-items: center; gap: 10px; }
header { justify-content: space-between; }
h2 { margin: 0; font-size: 20px; font-weight: 500; }
label { display: flex; flex-direction: column; gap: 7px; font-size: 12px; color: var(--qm-text-secondary); }
input, select, textarea { width: 100%; min-width: 0; border: 1px solid var(--qm-border); border-radius: 5px; padding: 9px 10px; background: var(--qm-bg); color: var(--qm-text); font: inherit; }
textarea { resize: vertical; line-height: 1.6; }
.qm-create-fields { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
button { border: 1px solid var(--qm-border); border-radius: 5px; padding: 8px 12px; background: var(--qm-bg-card); color: var(--qm-text); cursor: pointer; }
button:disabled { opacity: .5; cursor: default; }
footer { justify-content: flex-end; }
button[type='submit'] { background: var(--qss-accent); color: var(--qss-accent-ink); }
.qm-create-error { color: var(--qss-error); font-size: 12px; }
</style>
