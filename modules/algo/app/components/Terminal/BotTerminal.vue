<script setup lang="ts">
import { nextTick, onActivated, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { useBotsStore } from '#algo/stores/bots'
import type { BotLogEvent, LogEntry } from '#algo/types'

const props = withDefaults(
  defineProps<{
    autoScroll?: boolean
    /** Show one bot's lines; null shows every bot's (prefixed with its name). */
    botId?: string | null
  }>(),
  {
    autoScroll: true,
    botId: null,
  },
)

const botsStore = useBotsStore()
const containerRef = ref<HTMLDivElement | null>(null)

let term: import('@xterm/xterm').Terminal | null = null
let fitAddon: import('@xterm/addon-fit').FitAddon | null = null
let unlisten: UnlistenFn | null = null
let resizeObserver: ResizeObserver | null = null

function formatTimestamp(iso: string): string {
  const d = new Date(iso)
  const h = String(d.getHours()).padStart(2, '0')
  const m = String(d.getMinutes()).padStart(2, '0')
  const s = String(d.getSeconds()).padStart(2, '0')
  return `${h}:${m}:${s}`
}

function colorForLevel(level: string): string {
  switch (level.toLowerCase()) {
    case 'trade': return '\x1b[37m'  // white (monochrome)
    case 'warn':  return '\x1b[33m'  // yellow
    case 'error': return '\x1b[31m'  // red
    default:      return '\x1b[0m'   // default
  }
}

function accepts(botId: string | null | undefined): boolean {
  return props.botId === null || botId === props.botId
}

function writeLine(payload: BotLogEvent) {
  if (!term) return
  const ts = formatTimestamp(payload.timestamp)
  const color = colorForLevel(payload.level)
  const reset = '\x1b[0m'
  const dimTs = `\x1b[90m[${ts}]${reset} `
  // Every bot's stream at once: name each line, so two bots on two pairs
  // stay readable in one terminal.
  const tag = props.botId === null && payload.bot_id
    ? `\x1b[36m${botsStore.name(payload.bot_id)}${reset} `
    : ''
  term.writeln(`${dimTs}${tag}${color}${payload.message}${reset}`)

  if (props.autoScroll) {
    term.scrollToBottom()
  }
}

function clear() {
  term?.clear()
}

function getContent(): string {
  if (!term) return ''
  const buffer = term.buffer.active
  const lines: string[] = []
  for (let i = 0; i < buffer.length; i++) {
    const line = buffer.getLine(i)
    if (line) {
      lines.push(line.translateToString(true))
    }
  }
  while (lines.length > 0) {
    const lastLine = lines[lines.length - 1]
    if (!lastLine || lastLine.trim() !== '') {
      break
    }
    lines.pop()
  }
  return lines.join('\n')
}

defineExpose({ clear, getContent })

/** The persisted tail for the current selection, written as one block. */
async function loadHistory(): Promise<Set<string>> {
  const written = new Set<string>()
  if (!term) return written
  try {
    const historicalLogs = await invoke<LogEntry[]>('plugin:algo|get_bot_logs', {
      botId: props.botId,
      limit: 500,
      offset: 0,
    })
    if (historicalLogs.length > 0) {
      term.writeln('\x1b[90m-- Historical logs --\x1b[0m')
      for (const log of historicalLogs) {
        written.add(`${log.timestamp}|${log.message}`)
        writeLine({ timestamp: log.timestamp, level: log.level, message: log.message, bot_id: log.bot_id })
      }
      term.writeln('\x1b[90m-- Current session --\x1b[0m')
    } else {
      term.writeln('\x1b[90m-- No log lines yet --\x1b[0m')
    }
  } catch {
    // Historical logs unavailable, continue with new events only
  }
  return written
}

onMounted(async () => {
  if (!containerRef.value) return

  // Listen before anything else is awaited — lines emitted while the addons
  // load and the history query runs are buffered and replayed after the
  // historical block.
  let pending: BotLogEvent[] | null = []
  unlisten = await listen<BotLogEvent[]>('bot:log', (event) => {
    if (pending) {
      pending.push(...event.payload)
      return
    }
    for (const entry of event.payload) {
      if (accepts(entry.bot_id)) writeLine(entry)
    }
  })

  const { Terminal } = await import('@xterm/xterm')
  const { FitAddon } = await import('@xterm/addon-fit')
  const { Unicode11Addon } = await import('@xterm/addon-unicode11')
  await import('@xterm/xterm/css/xterm.css')
  await document.fonts.ready

  term = new Terminal({
    theme: {
      background: '#18181e',
      foreground: '#d4d4d8',
      cursor: '#d4d4d8',
      cursorAccent: '#18181e',
      selectionBackground: '#313139',
      black: '#18181e',
      red: '#ff4757',
      green: '#a0a0a8',
      yellow: '#ffa502',
      blue: '#a0a0a8',
      magenta: '#a0a0a8',
      cyan: '#9a9aa5',
      white: '#d4d4d8',
    },
    fontFamily: "'CaskaydiaCove Nerd Font', ui-monospace, 'Cascadia Code', 'JetBrains Mono', Menlo, Consolas, monospace",
    fontSize: 13,
    lineHeight: 1.4,
    cursorBlink: false,
    disableStdin: true,
    convertEol: true,
  })

  fitAddon = new FitAddon()
  term.loadAddon(fitAddon)
  const unicode11 = new Unicode11Addon()
  term.loadAddon(unicode11)
  term.unicode.activeVersion = '11'
  term.open(containerRef.value)

  try {
    const { WebglAddon } = await import('@xterm/addon-webgl')
    const webgl = new WebglAddon()
    webgl.onContextLoss(() => {
      webgl.dispose()
    })
    term.loadAddon(webgl)
  } catch {
    // WebGL not available, fall back to canvas renderer
  }

  fitAddon.fit()
  resizeObserver = new ResizeObserver(() => {
    fitAddon?.fit()
  })
  resizeObserver.observe(containerRef.value)

  const written = await loadHistory()

  const buffered = pending ?? []
  pending = null
  for (const entry of buffered) {
    if (!accepts(entry.bot_id)) continue
    if (written.has(`${entry.timestamp}|${entry.message}`)) continue
    writeLine(entry)
  }
})

// A different bot: start over with that bot's tail.
watch(() => props.botId, async () => {
  if (!term) return
  term.clear()
  term.reset()
  await loadHistory()
})

onActivated(() => {
  nextTick(() => {
    fitAddon?.fit()
    if (term) term.refresh(0, term.rows - 1)
  })
})

watch(
  () => props.autoScroll,
  (enabled) => {
    if (enabled && term) {
      term.scrollToBottom()
    }
  },
)

onBeforeUnmount(() => {
  if (resizeObserver) {
    resizeObserver.disconnect()
    resizeObserver = null
  }
  if (unlisten) {
    unlisten()
    unlisten = null
  }
  if (term) {
    term.dispose()
    term = null
  }
  fitAddon = null
})
</script>

<template>
  <div ref="containerRef" class="bot-terminal" />
</template>

<style scoped>
.bot-terminal {
  width: 100%;
  height: 100%;
  min-height: 200px;
}

.bot-terminal :deep(.xterm) {
  padding: 8px;
  height: 100%;
}

.bot-terminal :deep(.xterm-viewport) {
  overflow-y: auto !important;
}
</style>
