<script setup lang="ts">
import { onClickOutside } from '@vueuse/core'
import { STRATEGY_ICONS, strategyIcon } from '#systems/utils/strategyIcons'

const props = defineProps<{ modelValue: string; disabled?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()
const root = ref<HTMLElement | null>(null)
const trigger = ref<HTMLButtonElement | null>(null)
const open = ref(false)
const panelId = useId()
const selected = computed(() => strategyIcon({ id: '', icon: props.modelValue }))

onClickOutside(root, () => { open.value = false })
watch(() => props.disabled, disabled => { if (disabled) open.value = false })

async function toggle() {
  open.value = !open.value
  if (open.value) {
    await nextTick()
    root.value?.querySelector<HTMLButtonElement>('[aria-pressed="true"]')?.focus()
  }
}

function close() {
  open.value = false
  trigger.value?.focus()
}

function choose(id: string) {
  emit('update:modelValue', id)
  close()
}

function leave(event: FocusEvent) {
  if (!root.value?.contains(event.relatedTarget as Node | null)) open.value = false
}
</script>

<template>
  <div ref="root" class="qs-symbol" @keydown.esc.stop.prevent="close" @focusout="leave">
    <span class="label">Symbol</span>
    <button ref="trigger" type="button" class="input qs-symbol__trigger" :disabled="disabled"
      :aria-label="`Symbol: ${selected.label}`" :aria-expanded="open" :aria-controls="panelId"
      @click="toggle" @keydown.down.prevent="!open && toggle()">
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
        <path :d="selected.path" />
      </svg>
      <span>{{ selected.label }}</span>
      <svg class="qs-symbol__chevron" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true"><path d="m6 9 6 6 6-6" /></svg>
    </button>
    <div v-if="open" :id="panelId" class="qs-symbol__panel" role="group" aria-label="Strategy symbols">
      <span class="qs-symbol__heading">Choose symbol</span>
      <div class="qs-symbol__options">
        <button v-for="icon in STRATEGY_ICONS" :key="icon.id" type="button" class="qs-symbol__option"
          :title="icon.label" :aria-label="icon.label" :aria-pressed="selected.id === icon.id" @click="choose(icon.id)">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
            <path :d="icon.path" />
          </svg>
        </button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.qs-symbol { position: relative; min-width: 0; }
.qs-symbol__trigger { display: flex; align-items: center; gap: 8px; width: 100%; text-align: left; cursor: pointer; }
.qs-symbol__trigger svg { flex-shrink: 0; }
.qs-symbol__trigger:disabled { cursor: default; }
.qs-symbol__chevron { margin-left: auto; color: var(--qs-text-muted); }
.qs-symbol__panel { position: absolute; z-index: 50; top: calc(100% + 6px); right: 0; width: 272px; padding: 12px; border: 1px solid var(--qs-border); border-radius: var(--qs-radius); background: var(--qs-bg-card); box-shadow: 0 8px 24px #0003; }
.qs-symbol__heading { display: block; margin-bottom: 8px; color: var(--qs-text-secondary); font-size: 12px; }
.qs-symbol__options { display: grid; grid-template-columns: repeat(6, 1fr); gap: 6px; }
.qs-symbol__option { display: flex; align-items: center; justify-content: center; height: 36px; border: 1px solid transparent; border-radius: 6px; background: transparent; color: var(--qs-text-secondary); cursor: pointer; }
.qs-symbol__option:hover { background: var(--qs-bg-hover); color: var(--qs-text); }
.qs-symbol__option[aria-pressed='true'] { border-color: var(--qs-accent); background: var(--qs-selection); color: var(--qs-text); }
.qs-symbol__option:focus-visible, .qs-symbol__trigger:focus-visible { outline: 2px solid var(--qs-accent); outline-offset: 2px; }
</style>
