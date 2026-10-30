<script setup lang="ts">
/**
 * The project's actions in the thread header (card 13; T3 Code's project
 * scripts): the first one as a button, all of them in the menu beside it,
 * each by its own key too. Run opens the action's terminal in the drawer
 * (a live one is reused). Without actions, the menu offers to add one.
 */
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref } from 'vue'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { Bug, ChevronDown, Hammer, Play, Settings2, SlidersHorizontal, TestTube2, WandSparkles } from 'lucide-vue-next'
import { usePilotStore } from '#pilot/stores/pilot'
import { useDrawerStore } from '#pilot/stores/drawer'
import { chordOf } from '#pilot/utils/keybindings'
import type { ProjectAction } from '#pilot/types'

const props = defineProps<{ threadId: string }>()
const store = usePilotStore()
const drawer = useDrawerStore()
const root = ref<HTMLElement | null>(null)
const open = ref(false)

const ICON: Record<string, unknown> = { play: Play, test: TestTube2, lint: WandSparkles, build: Hammer, configure: SlidersHorizontal, debug: Bug }
const thread = computed(() => store.sessions.find((s) => s.id === props.threadId) ?? null)
const actions = computed<ProjectAction[]>(() => (thread.value ? (store.settings?.actions?.[thread.value.contextId] ?? []) : []))
const first = computed(() => actions.value[0] ?? null)
const options = computed(() => [
  ...actions.value.map((a) => ({ id: a.id, label: a.name, description: a.command + (a.key ? ` · ${a.key}` : '') })),
  { id: '__edit', label: actions.value.length ? 'Edit actions…' : 'Add an action…', description: 'Settings → Project actions' },
])

function run(a: ProjectAction) {
  open.value = false
  void drawer.runAction(props.threadId, a)
}
function pick(id: string) {
  open.value = false
  if (id === '__edit') {
    window.dispatchEvent(new CustomEvent('qss:settings', { detail: { module: 'pilot', section: 'actions' } }))
    return
  }
  const a = actions.value.find((x) => x.id === id)
  if (a) run(a)
}

/** An action's own key runs it. */
function onKey(e: KeyboardEvent) {
  if (e.defaultPrevented || !actions.value.length) return
  const chord = chordOf(e)
  const a = chord ? actions.value.find((x) => x.key && x.key === chord) : null
  if (!a) return
  e.preventDefault()
  run(a)
}
function onDocDown(e: MouseEvent) {
  if (open.value && root.value && !root.value.contains(e.target as Node)) open.value = false
}
// The warm cache keeps this mounted behind other modules: keys only while QuantPilot is in front.
onMounted(() => {
  if (inActiveKeepAliveTree()) window.addEventListener('keydown', onKey)
  document.addEventListener('mousedown', onDocDown)
})
onActivated(() => window.addEventListener('keydown', onKey))
onDeactivated(() => window.removeEventListener('keydown', onKey))
onBeforeUnmount(() => {
  window.removeEventListener('keydown', onKey)
  document.removeEventListener('mousedown', onDocDown)
})
</script>

<template>
  <div ref="root" class="qp-pa">
    <button v-if="first" class="qp-pa-main" :title="`${first.command}${first.key ? ` (${first.key})` : ''}`" @click="run(first)">
      <component :is="ICON[first.icon] ?? Play" :size="12" />
      <span>{{ first.name }}</span>
    </button>
    <button class="qp-pa-more" :class="{ alone: !first, on: open }" :title="first ? 'Project actions' : 'Project actions — add one'" aria-label="Project actions" @click="open = !open">
      <Settings2 v-if="!first" :size="13" />
      <ChevronDown v-else :size="13" />
    </button>
    <div v-if="open" class="qp-pa-pop">
      <PilotThreadMenuList :options="options" selected="" title="Project actions" @pick="pick" @close="open = false" />
    </div>
  </div>
</template>

<style scoped>
.qp-pa { position: relative; display: inline-flex; align-items: stretch; flex-shrink: 0; }
.qp-pa-main,
.qp-pa-more {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  border: 1px solid var(--qss-border, #47474f);
  background: transparent;
  color: var(--qss-text, #d4d4d8);
  font: inherit;
  font-size: 12px;
  cursor: pointer;
}
.qp-pa-main { max-width: 160px; padding: 2px 9px; border-radius: 6px 0 0 6px; border-right: none; }
.qp-pa-main span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-pa-more { padding: 2px 5px; border-radius: 0 6px 6px 0; }
.qp-pa-more.alone { border-radius: 6px; color: var(--qss-text-muted, #6e6e7a); }
.qp-pa-main:hover,
.qp-pa-more:hover,
.qp-pa-more.on { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-pa-main:active,
.qp-pa-more:active { background: var(--qss-bg-card, #292930); }
.qp-pa-pop { position: absolute; top: calc(100% + 6px); right: 0; z-index: 40; }
</style>
