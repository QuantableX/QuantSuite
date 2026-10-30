<script setup lang="ts">
/**
 * Open in… (card 24): a thread's file in VS Code or Cursor at its line
 * (when they are on PATH), or selected in Explorer. The last choice comes
 * first next time.
 */
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { SquareArrowOutUpRight } from 'lucide-vue-next'
import { pilot } from '#pilot/utils/invoke'
import type { MenuOption } from '#pilot/utils/models'

const props = defineProps<{ threadId: string; path: string; line?: number | null }>()
const open = ref(false)
const error = ref('')
const editors = useState<{ id: string; label: string }[] | null>('pilot-editors', () => null)

function lastUsed(): string {
  try {
    return localStorage.getItem('pilot.openIn') ?? ''
  } catch {
    return ''
  }
}
const options = computed<MenuOption[]>(() => {
  const list = [...(editors.value ?? [])]
  const last = lastUsed()
  list.sort((a, b) => Number(b.id === last) - Number(a.id === last))
  return list.map((e) => ({ id: e.id, label: e.id === 'explorer' ? 'Show in Explorer' : `Open in ${e.label}`, description: e.id === 'explorer' ? undefined : props.line ? `at line ${props.line}` : undefined }))
})

async function toggle() {
  error.value = ''
  if (!open.value && !editors.value) editors.value = (await pilot<{ id: string; label: string }[]>('plugin:pilot|pilot_editors').catch(() => null)) ?? [{ id: 'explorer', label: 'Explorer' }]
  open.value = !open.value
}

async function pick(id: string) {
  open.value = false
  try {
    localStorage.setItem('pilot.openIn', id)
  } catch {
    /* private mode */
  }
  try {
    await pilot('plugin:pilot|pilot_open_in', { id: props.threadId, path: props.path, line: props.line ?? null, editor: id })
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
    setTimeout(() => (error.value = ''), 4000)
  }
}

const root = ref<HTMLElement | null>(null)
function onPointer(e: PointerEvent) {
  if (!root.value?.contains(e.target as Node)) open.value = false
}
watch(open, (on) => (on ? window.addEventListener('pointerdown', onPointer, true) : window.removeEventListener('pointerdown', onPointer, true)))
onBeforeUnmount(() => window.removeEventListener('pointerdown', onPointer, true))
</script>

<template>
  <span ref="root" class="qp-oi">
    <button class="qp-oi-btn" :title="error || 'Open in…'" :class="{ bad: error }" aria-label="Open in…" @click.stop="toggle">
      <SquareArrowOutUpRight :size="12" />
    </button>
    <div v-if="open" class="qp-oi-pop" @click.stop>
      <PilotThreadMenuList :options="options" selected="" title="Open in…" @pick="pick" @close="open = false" />
    </div>
  </span>
</template>

<style scoped>
.qp-oi { position: relative; display: inline-flex; flex-shrink: 0; }
.qp-oi-btn { display: inline-flex; padding: 3px; border: none; border-radius: 5px; background: transparent; color: var(--qss-text-muted, #6e6e7a); cursor: pointer; }
.qp-oi-btn:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-oi-btn.bad { color: var(--qss-error, #f87171); }
.qp-oi-pop { position: absolute; top: calc(100% + 4px); right: 0; z-index: 40; animation: qp-oi-in var(--qp-motion, 160ms) cubic-bezier(0.2, 0.8, 0.2, 1); }
@keyframes qp-oi-in {
  from { opacity: 0; transform: translateY(-3px) scale(0.985); }
}
</style>
