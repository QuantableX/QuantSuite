<script setup lang="ts">
definePageMeta({ layout: 'algo' })

import { useBotsStore } from '#algo/stores/bots'

const route = useRoute()
const router = useRouter()
const botsStore = useBotsStore()

const botTerminalRef = ref<{ clear: () => void; getContent?: () => string } | null>(null)
const autoScroll = ref(true)

// The bot whose stream is shown; '' means every bot. `?bot=<id>` deep-links
// from the Bots page; the choice writes back to the query so it survives
// the warm cache.
const selectedBotId = ref<string>(typeof route.query.bot === 'string' ? route.query.bot : '')

watch(selectedBotId, (id) => {
  router.replace({ path: route.path, query: { ...route.query, bot: id || undefined } })
})
watch(() => route.query.bot, (bot) => {
  const next = typeof bot === 'string' ? bot : ''
  if (next !== selectedBotId.value) selectedBotId.value = next
})

const selectedBot = computed(() => botsStore.byId(selectedBotId.value))

function handleClear() {
  botTerminalRef.value?.clear()
}

function handleExport() {
  if (!botTerminalRef.value) return
  const content = botTerminalRef.value.getContent?.() ?? ''
  if (!content) return

  const blob = new Blob([content], { type: 'text/plain' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  const suffix = selectedBot.value ? `-${selectedBot.value.name.replace(/[^a-z0-9]+/gi, '_')}` : ''
  a.download = `quantalgo-log${suffix}-${new Date().toISOString().slice(0, 19).replace(/:/g, '-')}.txt`
  document.body.appendChild(a)
  a.click()
  document.body.removeChild(a)
  URL.revokeObjectURL(url)
}

function toggleAutoScroll() {
  autoScroll.value = !autoScroll.value
}
</script>

<template>
  <div class="terminal-page">
    <div class="terminal-toolbar">
      <div class="terminal-toolbar__left">
        <h2 class="terminal-toolbar__title">Bot Terminal</h2>
        <select v-model="selectedBotId" class="input terminal-toolbar__select">
          <option value="">All bots</option>
          <option v-for="bot in botsStore.list" :key="bot.id" :value="bot.id">
            {{ bot.name }}{{ bot.status === 'running' ? ' · running' : '' }}
          </option>
        </select>
      </div>
      <div class="terminal-toolbar__right">
        <button
          class="btn btn-sm"
          :class="{ 'btn--active': autoScroll }"
          @click="toggleAutoScroll"
        >
          Auto-scroll {{ autoScroll ? 'On' : 'Off' }}
        </button>
        <button class="btn btn-sm" @click="handleClear">Clear</button>
        <button class="btn btn-sm" @click="handleExport">Export Log</button>
      </div>
    </div>

    <div class="terminal-container">
      <AlgoTerminalBotTerminal
        ref="botTerminalRef"
        :auto-scroll="autoScroll"
        :bot-id="selectedBotId || null"
      />
    </div>
  </div>
</template>

<style scoped>
.terminal-page {
  display: flex;
  flex-direction: column;
  gap: var(--qss-gap);
  height: 100%;
  overflow: hidden;
  padding: var(--qss-page-edge);
}

.terminal-toolbar {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: space-between;
  gap: var(--qss-gap-tight) var(--qss-gap);
  flex-shrink: 0;
}

.terminal-toolbar__left {
  display: flex;
  align-items: center;
  gap: var(--qss-gap);
  min-width: 0;
}

.terminal-toolbar__title {
  font-size: var(--qss-text-title);
  font-weight: 600;
  letter-spacing: -.01em;
  color: var(--qa-text);
  white-space: nowrap;
}

.terminal-toolbar__select {
  width: auto;
  min-width: 200px;
}

.terminal-toolbar__right {
  display: flex;
  align-items: center;
  gap: var(--qss-gap-tight);
}

.btn--active {
  background: var(--qa-accent);
  color: var(--qss-accent-ink);
  border-color: var(--qa-accent);
}

.btn--active:hover {
  background: var(--qa-accent-hover);
  border-color: var(--qa-accent-hover);
}

.terminal-container {
  flex: 1 1 0;
  min-height: 0;
  overflow: hidden;
  border: 1px solid var(--qa-border);
  border-radius: var(--qss-panel-radius);
}
</style>
