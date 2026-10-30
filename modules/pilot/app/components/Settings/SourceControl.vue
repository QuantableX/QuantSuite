<script setup lang="ts">
/**
 * Settings → Source control (card 19): whether the GitHub CLI can open pull
 * requests (installed and signed in — with the exact command when not), and
 * how generated commit messages read.
 */
import { onMounted, ref, watch } from 'vue'
import { usePilotStore } from '#pilot/stores/pilot'
import { pilot } from '#pilot/utils/invoke'

const store = usePilotStore()
const gh = ref<{ ok: boolean; text: string } | null>(null)
const checking = ref(false)
const commitStyle = ref<'repository' | 'standard'>('repository')
const notice = ref('')

async function check() {
  checking.value = true
  try {
    await pilot('plugin:pilot|pilot_gh_status')
    gh.value = { ok: true, text: 'GitHub CLI signed in to github.com — pull requests open from a thread.' }
  } catch (e) {
    gh.value = { ok: false, text: e instanceof Error ? e.message : String(e) }
  } finally {
    checking.value = false
  }
}
onMounted(async () => {
  if (!store.settings) await store.loadSettings()
  commitStyle.value = store.settings?.commitStyle ?? 'repository'
  void check()
})
watch(() => store.settings?.commitStyle, (v) => v && (commitStyle.value = v))

async function saveStyle() {
  await store.saveSettings({ commitStyle: commitStyle.value }, { quiet: true })
  notice.value = 'Saved.'
}
</script>

<template>
  <div class="qp-settings" data-module="pilot">
    <h3 class="qp-settings-title">GitHub</h3>
    <div class="qp-gh" :class="{ ok: gh?.ok, bad: gh && !gh.ok }">
      <span class="qp-dot" />
      <span>{{ checking && !gh ? 'Checking the GitHub CLI…' : gh?.text }}</span>
      <button class="qp-btn qp-btn-sm" :disabled="checking" @click="check">{{ checking ? 'Checking…' : 'Check again' }}</button>
    </div>
    <p v-if="gh && !gh.ok" class="qp-settings-desc">Run <code>gh auth login</code> in a terminal (the drawer's works), then check again.</p>

    <h3 class="qp-settings-title">Commit messages</h3>
    <label class="qp-settings-field">
      <span>Generated messages <em>(never with AI co-author or "generated with" lines)</em></span>
      <select v-model="commitStyle" class="qp-input" @change="saveStyle">
        <option value="repository">Repository conventions — recent subjects and commit instructions</option>
        <option value="standard">Standard — imperative subject, short bullets</option>
      </select>
    </label>
    <span v-if="notice" class="qp-settings-notice">{{ notice }}</span>
  </div>
</template>

<style scoped>
.qp-settings { display: flex; flex-direction: column; gap: 10px; font-size: 12.5px; color: var(--qss-text, #d4d4d8); }
.qp-settings-title { margin: 6px 0 0; font-size: 13px; font-weight: 600; }
.qp-settings-desc { margin: 0; color: var(--qss-text-secondary, #9a9aa5); line-height: 1.5; font-size: 12px; }
.qp-settings-desc code { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; background: var(--qss-bg-card, #292930); padding: 1px 5px; border-radius: 4px; }
.qp-gh { display: flex; align-items: center; gap: 8px; padding: 8px 10px; border: 1px solid var(--qss-border-subtle, #35353d); border-radius: 8px; background: var(--qss-bg-raised, #1f1f25); }
.qp-gh > span:nth-child(2) { flex: 1; }
.qp-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--qss-border, #47474f); flex-shrink: 0; }
.qp-gh.ok .qp-dot { background: var(--qss-success, #34d399); }
.qp-gh.bad .qp-dot { background: var(--qss-error, #f87171); }
.qp-settings-field { display: flex; flex-direction: column; gap: 4px; }
.qp-settings-field > span { font-size: 11.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-settings-field em { font-style: normal; color: var(--qss-text-muted, #6e6e7a); }
.qp-input { background: var(--qss-bg, #18181e); border: 1px solid var(--qss-border, #47474f); border-radius: 6px; color: var(--qss-text, #d4d4d8); font: inherit; font-size: 12.5px; padding: 6px 9px; outline: none; }
.qp-input:focus { border-color: var(--qss-accent, #a0a0a8); }
.qp-input::placeholder { color: var(--qss-text-muted, #6e6e7a); }
.qp-settings-notice { color: var(--qss-text-secondary, #9a9aa5); font-size: 12px; }
</style>
