<script setup lang="ts">
/**
 * QuantMemory's section in the unified settings modal: where the vault
 * lives, the index, and the trash. The vault path is a core.db setting —
 * pointing it at an existing Obsidian vault is a supported move.
 */
import { useVaultStore } from '#memory/stores/vault'
import Retrieval from '#memory/components/Settings/Retrieval.vue'

const vault = useVaultStore()

const pathDraft = ref('')
const busy = ref(false)
const notice = ref('')

onMounted(async () => {
  if (!vault.vaultInfo) await vault.loadAll()
  pathDraft.value = vault.vaultInfo?.path ?? ''
  void vault.loadTrash()
})

async function browse() {
  try {
    const dialog = await import('@tauri-apps/plugin-dialog')
    const picked = await dialog.open({ directory: true, multiple: false })
    if (typeof picked === 'string') pathDraft.value = picked
  } catch {
    // Outside Tauri there is no dialog — the text field still works.
  }
}

async function applyPath() {
  const target = pathDraft.value.trim()
  if (!target || target === vault.vaultInfo?.path) return
  busy.value = true
  notice.value = ''
  try {
    await vault.setVaultPath(target)
    notice.value = `Vault moved — ${vault.stats?.memories ?? 0} memories indexed.`
  } catch (err) {
    notice.value = String(err)
  } finally {
    busy.value = false
    pathDraft.value = vault.vaultInfo?.path ?? pathDraft.value
  }
}

async function resetPath() {
  busy.value = true
  notice.value = ''
  try {
    await vault.setVaultPath(null)
    pathDraft.value = vault.vaultInfo?.path ?? ''
    notice.value = 'Back to the default vault.'
  } finally {
    busy.value = false
  }
}

async function reindex() {
  busy.value = true
  notice.value = ''
  try {
    const report = await vault.reindex()
    notice.value = report
      ? `Re-indexed ${report.total} files — ${report.added} added, ${report.updated} updated, ${report.removed} removed.`
      : 'Re-index done.'
  } finally {
    busy.value = false
  }
}

async function purge() {
  if (!confirm('Permanently delete everything in the vault trash?')) return
  await vault.purgeTrash()
  notice.value = 'Trash emptied.'
}

async function restore(fileName: string) {
  await vault.restore(fileName)
}
</script>

<template>
  <div class="qm-settings">
    <Retrieval />
    <section class="qm-set-block">
      <h3 class="qm-set-title">General vault location</h3>
      <p class="qm-set-hint">
        Choose a markdown folder or an existing Obsidian vault. Changing it rebuilds the index.
      </p>
      <div class="qm-set-row">
        <input v-model="pathDraft" class="qm-set-input mono" spellcheck="false" />
        <button class="qm-set-btn" @click="browse">Browse…</button>
      </div>
      <div class="qm-set-row">
        <button class="qm-set-btn is-primary" :disabled="busy" @click="applyPath">Apply</button>
        <button
          v-if="vault.vaultInfo && !vault.vaultInfo.isDefault"
          class="qm-set-btn"
          :disabled="busy"
          @click="resetPath"
        >
          Reset to default
        </button>
        <span v-if="vault.vaultInfo" class="qm-set-status">
          {{ vault.vaultInfo.watching ? 'Watching for external edits' : 'Watcher inactive' }}
        </span>
      </div>
    </section>

    <section class="qm-set-block">
      <h3 class="qm-set-title">Index</h3>
      <div class="qm-set-row">
        <button class="qm-set-btn" :disabled="busy" @click="reindex">Re-index vault</button>
        <span v-if="vault.stats" class="qm-set-status">
          {{ vault.stats.memories }} memories · {{ vault.stats.resolvedLinks }} links
        </span>
      </div>
    </section>

    <section class="qm-set-block">
      <h3 class="qm-set-title">Trash — {{ vault.scopeName(vault.scope) }} ({{ vault.trash.length }})</h3>
      <div v-for="t in vault.trash" :key="t.fileName" class="qm-set-row">
        <span class="qm-set-file mono">{{ t.fileName }}</span>
        <button class="qm-set-btn" @click="restore(t.fileName)">Restore</button>
      </div>
      <div class="qm-set-row">
        <button class="qm-set-btn is-danger" :disabled="!vault.trash.length" @click="purge">
          Empty trash
        </button>
      </div>
    </section>

    <p v-if="notice" class="qm-set-notice">{{ notice }}</p>
  </div>
</template>

<style scoped>
.qm-settings {
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.qm-set-block {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.qm-set-title {
  font-size: 12px;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  color: var(--qss-text-muted);
}

.qm-set-hint {
  font-size: 12px;
  color: var(--qss-text-secondary);
}

.qm-set-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.qm-set-input {
  flex: 1;
  min-width: 0;
  padding: 6px 10px;
  border: 1px solid var(--qss-border);
  border-radius: 6px;
  background: var(--qss-bg);
  color: var(--qss-text);
  outline: none;
}

.qm-set-btn {
  padding: 5px 12px;
  border: 1px solid var(--qss-border);
  border-radius: 6px;
  background: var(--qss-bg-card);
  color: var(--qss-text);
  cursor: pointer;
  white-space: nowrap;
}
.qm-set-btn:disabled {
  opacity: 0.5;
  cursor: default;
}
.qm-set-btn.is-primary {
  border-color: #60a5fa;
}
.qm-set-btn.is-danger {
  border-color: #b4494c;
}

.qm-set-status {
  font-size: 12px;
  color: var(--qss-text-muted);
}

.qm-set-file {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  color: var(--qss-text-secondary);
}

.qm-set-notice {
  font-size: 12px;
  color: var(--qss-text-secondary);
}
</style>
