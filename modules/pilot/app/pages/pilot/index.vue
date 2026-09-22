<script setup lang="ts">
/**
 * The one page: the open session's terminal. Every launched session keeps
 * its pane mounted (the console's own tab pattern — switching is instant
 * and the scrollback stays); only the active one is shown. Without a
 * session, the pilot waits in the middle.
 */
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { usePilotStore } from '#pilot/stores/pilot'

definePageMeta({ layout: 'pilot' })

const store = usePilotStore()
const session = computed(() => store.active)
const live = computed(() => store.activeLive)
const stage = ref<HTMLElement | null>(null)

const anyReady = computed(() => (store.status ? store.adapters.some((a) => a.found && a.loggedIn !== false) : true))

/**
 * The grid the stage would hold at the pane's type size, kept in the store
 * so a launch from the sidebar draws the TUI at the right size first time;
 * the pane refits to the exact cell size the moment it attaches.
 */
function measure() {
  const el = stage.value
  if (!el || el.clientWidth < 40 || el.clientHeight < 40) return
  store.stage = {
    cols: Math.max(40, Math.floor((el.clientWidth - 8) / 8.4)),
    rows: Math.max(10, Math.floor((el.clientHeight - 6) / 18.6)),
  }
}

let observer: ResizeObserver | null = null
onMounted(() => {
  observer = new ResizeObserver(measure)
  if (stage.value) observer.observe(stage.value)
  measure()
})
watch(stage, (el) => {
  observer?.disconnect()
  if (el && observer) observer.observe(el)
  measure()
})
onBeforeUnmount(() => observer?.disconnect())

function restart() {
  measure()
  if (session.value) void store.launch(session.value.id)
}
</script>

<template>
  <div class="qp-page">
    <template v-if="session">
      <div v-if="live.error" class="qp-banner is-error">
        <span class="qp-banner-text">{{ live.error }}</span>
        <button class="qp-btn qp-btn-sm" @click="restart">Try again</button>
      </div>
      <div v-else-if="live.ptyId && !live.alive" class="qp-banner">
        <span class="qp-banner-text">Session ended.</span>
        <button class="qp-btn qp-btn-sm" @click="restart">Reopen</button>
      </div>
      <div v-if="live.notice" class="qp-banner is-notice">{{ live.notice }}</div>

      <div ref="stage" class="qp-stage">
        <div
          v-for="s in store.launched"
          v-show="s.id === store.activeId"
          :key="s.id"
          class="qp-pane-host"
        >
          <ConsolePane
            :key="store.live[s.id]?.ptyId ?? s.id"
            :session-id="store.live[s.id]?.ptyId"
            :cwd="s.cwd"
            :label="s.title || store.adapterLabel(s.provider)"
            :active="s.id === store.activeId"
            :visible="s.id === store.activeId"
            @exited="store.markExited(s.id)"
            @failed="(m: string) => store.paneFailed(s.id, m)"
          />
        </div>

        <div v-if="!live.ptyId" class="qp-launch">
          <PilotFace :state="live.launching ? 'thinking' : live.error ? 'error' : 'idle'" />
          <div class="qp-launch-text">
            {{ live.launching ? 'Opening the terminal…' : 'No terminal open.' }}
          </div>
          <button v-if="!live.launching" class="qp-btn qp-btn-primary" @click="restart">Open terminal</button>
        </div>
      </div>
    </template>

    <div v-else class="qp-empty">
      <div class="qp-empty-face">
        <PilotFace :state="anyReady ? 'idle' : 'offline'" />
      </div>
      <h2 class="qp-empty-title">Agent sessions</h2>

      <p class="qp-empty-text">
        Create a session from the sidebar to open an agent terminal.
      </p>
      <p v-if="store.status && !anyReady" class="qp-empty-warn">
        No agents available. Check installation and login in Settings.
      </p>
    </div>
  </div>
</template>

<style scoped>
.qp-page { display: flex; flex-direction: column; flex: 1; min-height: 0; }
.qp-banner {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
  padding: 6px 12px;
  color: var(--qss-warning, #d29a3f);
  border-bottom: 1px solid color-mix(in srgb, var(--qss-warning, #d29a3f) 40%, var(--qss-border, #47474f));
  background: color-mix(in srgb, var(--qss-warning, #d29a3f) 6%, transparent);
}
.qp-banner-text { flex: 1; min-width: 0; }
.qp-banner.is-error {
  color: var(--qss-error, #f87171);
  border-bottom-color: color-mix(in srgb, var(--qss-error, #f87171) 40%, var(--qss-border, #47474f));
  background: color-mix(in srgb, var(--qss-error, #f87171) 6%, transparent);
}
.qp-banner.is-notice {
  color: var(--qss-text-secondary, #9a9aa5);
  border-bottom-color: var(--qss-border-subtle, #35353d);
  background: transparent;
}
.qp-stage {
  position: relative;
  flex: 1;
  min-height: 0;
  /* The terminal type size; the console pane reads it. */
  --cpane-font-size: 13px;
}
.qp-pane-host { position: absolute; inset: 0; }
.qp-launch {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 24px;
  text-align: center;
}
.qp-launch > :first-child { width: 120px; }
.qp-launch-text { font-size: 12.5px; color: var(--qss-text-secondary, #9a9aa5); }
.qp-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 24px;
  text-align: center;
}
.qp-empty-face { width: 180px; margin-bottom: 6px; }
.qp-empty-title { margin: 0; font-size: 16px; font-weight: 600; color: var(--qss-text, #d4d4d8); }
.qp-empty-text { margin: 0; max-width: 460px; font-size: 12.5px; line-height: 1.55; color: var(--qss-text-secondary, #9a9aa5); }
.qp-empty-text code { font-family: var(--qss-font-mono, ui-monospace, monospace); font-size: 11.5px; color: var(--qss-text, #d4d4d8); background: var(--qss-bg-card, #292930); padding: 1px 5px; border-radius: 4px; }
.qp-empty-warn { margin: 6px 0 0; max-width: 460px; font-size: 12px; line-height: 1.5; color: var(--qss-warning, #d29a3f); }
</style>
