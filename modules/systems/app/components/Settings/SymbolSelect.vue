<script setup lang="ts">
import { onClickOutside, useEventListener } from '@vueuse/core'
import { Check, ChevronDown, Search, X } from 'lucide-vue-next'
import { STRATEGY_ICONS, STRATEGY_ICON_GROUPS, strategyIcon } from '#systems/utils/strategyIcons'

const props = defineProps<{ modelValue: string; disabled?: boolean }>()
const emit = defineEmits<{ 'update:modelValue': [value: string] }>()
const root = ref<HTMLElement | null>(null)
const trigger = ref<HTMLButtonElement | null>(null)
const panel = ref<HTMLElement | null>(null)
const search = ref<HTMLInputElement | null>(null)
const open = ref(false)
const query = ref('')
const focusedId = ref('')
const previewId = ref('')
const panelId = useId()
const panelStyle = ref<Record<string, string>>({})
const selected = computed(() => strategyIcon({ id: '', icon: props.modelValue }))
const preview = computed(() => strategyIcon({ id: '', icon: previewId.value || props.modelValue }))
const filtered = computed(() => {
  const words = query.value.trim().toLowerCase().split(/\s+/)
  return STRATEGY_ICONS.filter(icon => words.every(word =>
    `${icon.label} ${icon.id} ${icon.category}`.toLowerCase().includes(word)))
})
const groups = computed(() => STRATEGY_ICON_GROUPS.map(label => ({
  label, icons: filtered.value.filter(icon => icon.category === label),
})).filter(group => group.icons.length))

onClickOutside(panel, () => { open.value = false }, { ignore: [root] })
watch(() => props.disabled, disabled => { if (disabled) open.value = false })
watch(filtered, icons => { focusedId.value = icons[0]?.id ?? ''; previewId.value = '' })
onDeactivated(() => { open.value = false })

function positionPanel() {
  if (!open.value || !trigger.value) return
  const rect = trigger.value.getBoundingClientRect()
  const width = Math.min(432, window.innerWidth - 24)
  const below = window.innerHeight - rect.bottom - 20
  const above = rect.top - 20
  const upwards = below < 360 && above > below
  // The shell scopes its theme bridge to the module; preserve it across Teleport.
  const theme = getComputedStyle(trigger.value)
  const colors = ['bg-card', 'bg-input', 'bg-hover', 'border', 'accent', 'selection', 'text', 'text-secondary', 'text-muted', 'scrollbar-thumb']
  panelStyle.value = {
    ...Object.fromEntries(colors.map(color => [`--qs-${color}`, theme.getPropertyValue(`--qs-${color}`)])),
    fontFamily: theme.fontFamily,
    width: `${width}px`,
    left: `${Math.max(12, Math.min(rect.right - width, window.innerWidth - width - 12))}px`,
    maxHeight: `${Math.max(160, upwards ? above : below)}px`,
    ...(upwards ? { bottom: `${window.innerHeight - rect.top + 8}px` } : { top: `${rect.bottom + 8}px` }),
  }
}

useEventListener('resize', positionPanel)
useEventListener('scroll', positionPanel, { capture: true })

async function toggle() {
  open.value = !open.value
  if (open.value) {
    query.value = ''
    focusedId.value = selected.value.id
    previewId.value = ''
    positionPanel()
    await nextTick()
    search.value?.focus()
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
  const target = event.relatedTarget as Node | null
  if (target && !root.value?.contains(target) && !panel.value?.contains(target)) open.value = false
}

function focusIcon(index: number) {
  const buttons = panel.value?.querySelectorAll<HTMLButtonElement>('[data-symbol]')
  const button = buttons?.[Math.max(0, Math.min(index, buttons.length - 1))]
  button?.focus()
  button?.scrollIntoView({ block: 'nearest' })
}

function navigate(event: KeyboardEvent) {
  const buttons = Array.from(panel.value?.querySelectorAll<HTMLButtonElement>('[data-symbol]') ?? [])
  const index = buttons.indexOf(event.target as HTMLButtonElement)
  const grid = (event.target as HTMLElement).parentElement
  const columns = grid ? getComputedStyle(grid).gridTemplateColumns.split(' ').length : 8
  const offsets: Record<string, number> = { ArrowRight: 1, ArrowLeft: -1, ArrowDown: columns, ArrowUp: -columns }
  const offset = offsets[event.key]
  if (offset == null && event.key !== 'Home' && event.key !== 'End') return
  event.preventDefault()
  focusIcon(event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : index + offset!)
}
</script>

<template>
  <div ref="root" class="qs-symbol" @keydown.esc.stop.prevent="close" @focusout="leave">
    <span class="label">Symbol</span>
    <button ref="trigger" type="button" class="input qs-symbol__trigger" :disabled="disabled"
      :aria-label="`Symbol: ${selected.label}`" :aria-expanded="open" :aria-controls="panelId" aria-haspopup="dialog"
      @click="toggle" @keydown.down.prevent="!open && toggle()">
      <component :is="selected.component" :size="18" :stroke-width="1.7" aria-hidden="true" />
      <span class="qs-symbol__value">{{ selected.label }}</span>
      <ChevronDown class="qs-symbol__chevron" :size="12" aria-hidden="true" />
    </button>
    <Teleport to="body">
      <div v-if="open" :id="panelId" ref="panel" class="qs-symbol__panel" :style="panelStyle"
        role="dialog" aria-label="Strategy symbols" @keydown.esc.stop.prevent="close" @focusout="leave">
        <div class="qs-symbol__head">
          <span class="qs-symbol__heading">Choose symbol</span>
          <span class="qs-symbol__count">{{ STRATEGY_ICONS.length }} symbols</span>
          <button type="button" class="qs-symbol__close" aria-label="Close symbol picker" @click="close"><X :size="16" aria-hidden="true" /></button>
        </div>
        <div class="qs-symbol__search">
          <Search :size="16" aria-hidden="true" />
          <input ref="search" v-model="query" type="search" placeholder="Search symbols…" aria-label="Search symbols"
            @keydown.down.prevent="focusIcon(0)" />
        </div>
        <div class="qs-symbol__results" @pointerleave="previewId = ''">
          <section v-for="group in groups" :key="group.label" class="qs-symbol__group" :aria-label="group.label">
            <h3>{{ group.label }}</h3>
            <div class="qs-symbol__options">
              <button v-for="icon in group.icons" :key="icon.id" type="button" class="qs-symbol__option"
                :data-symbol="icon.id" :title="icon.label" :aria-label="icon.label" :aria-pressed="selected.id === icon.id"
                :tabindex="focusedId === icon.id ? 0 : -1" @click="choose(icon.id)" @keydown="navigate"
                @pointerenter="previewId = icon.id" @focus="focusedId = icon.id; previewId = icon.id">
                <component :is="icon.component" :size="22" :stroke-width="1.6" aria-hidden="true" />
              </button>
            </div>
          </section>
          <p v-if="!filtered.length" class="qs-symbol__empty">No symbols found. Try a shape or object name.</p>
        </div>
        <div class="qs-symbol__preview">
          <component :is="preview.component" :size="18" :stroke-width="1.7" aria-hidden="true" />
          <span>{{ preview.label }}</span>
          <span v-if="preview.id === selected.id" class="qs-symbol__selected"><Check :size="12" aria-hidden="true" /> Selected</span>
          <span v-else class="qs-symbol__category">{{ preview.category }}</span>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.qs-symbol { position: relative; min-width: 0; }
.qs-symbol__trigger { display: flex; align-items: center; gap: 8px; width: 100%; text-align: left; cursor: pointer; }
.qs-symbol__trigger svg { flex-shrink: 0; }
.qs-symbol__value { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qs-symbol__trigger:disabled { cursor: default; }
.qs-symbol__chevron { margin-left: auto; color: var(--qs-text-muted); }
.qs-symbol__panel { position: fixed; z-index: 2000; display: flex; flex-direction: column; overflow: hidden; box-sizing: border-box; border: 1px solid var(--qs-border); border-radius: 12px; background: var(--qs-bg-card); color: var(--qs-text); box-shadow: 0 12px 40px #0005; font-size: 13px; }
.qs-symbol__head { display: flex; align-items: center; gap: 10px; padding: 14px 14px 12px 16px; }
.qs-symbol__heading { font-weight: 600; }
.qs-symbol__count { margin-left: auto; color: var(--qs-text-muted); font-size: 11px; }
.qs-symbol__close { display: grid; place-items: center; width: 24px; height: 24px; padding: 0; border: 0; border-radius: 5px; background: transparent; color: var(--qs-text-muted); cursor: pointer; }
.qs-symbol__close:hover { background: var(--qs-bg-hover); color: var(--qs-text); }
.qs-symbol__search { display: flex; flex-shrink: 0; align-items: center; gap: 8px; margin: 0 16px 14px; padding: 0 10px; border: 1px solid var(--qs-border); border-radius: 7px; background: var(--qs-bg-input); color: var(--qs-text-muted); }
.qs-symbol__search:focus-within { border-color: var(--qs-accent); }
.qs-symbol__search input { width: 100%; min-width: 0; height: 36px; padding: 0; border: 0; outline: 0; background: transparent; color: var(--qs-text); font: inherit; }
.qs-symbol__search input::placeholder { color: var(--qs-text-muted); }
.qs-symbol__results { min-height: 0; max-height: 310px; overflow-y: auto; overscroll-behavior: contain; padding: 0 16px 16px; scrollbar-width: thin; scrollbar-color: var(--qs-scrollbar-thumb) transparent; }
.qs-symbol__group + .qs-symbol__group { margin-top: 18px; }
.qs-symbol__group h3 { margin: 0 0 8px; color: var(--qs-text-muted); font-size: 10px; font-weight: 600; letter-spacing: 0.06em; text-transform: uppercase; }
.qs-symbol__options { display: grid; grid-template-columns: repeat(8, minmax(0, 1fr)); gap: 6px; }
.qs-symbol__option { display: flex; align-items: center; justify-content: center; box-sizing: border-box; height: 44px; padding: 0; border: 1px solid transparent; border-radius: 8px; background: var(--qs-bg-input); color: var(--qs-text-secondary); cursor: pointer; transition: background 100ms ease, color 100ms ease, border-color 100ms ease; }
.qs-symbol__option:hover { border-color: var(--qs-border); background: var(--qs-bg-hover); color: var(--qs-text); }
.qs-symbol__option[aria-pressed='true'] { border-color: var(--qs-accent); background: var(--qs-selection); color: var(--qs-text); }
.qs-symbol__option:focus-visible, .qs-symbol__trigger:focus-visible, .qs-symbol__close:focus-visible { outline: 2px solid var(--qs-accent); outline-offset: 2px; }
.qs-symbol__empty { padding: 24px 8px; text-align: center; color: var(--qs-text-secondary); line-height: 1.6; }
.qs-symbol__preview { display: flex; flex-shrink: 0; align-items: center; gap: 8px; min-height: 44px; padding: 0 16px; border-top: 1px solid var(--qs-border); font-size: 12px; }
.qs-symbol__selected, .qs-symbol__category { display: flex; align-items: center; gap: 4px; margin-left: auto; color: var(--qs-text-muted); font-size: 11px; }
@media (max-width: 460px) {
  .qs-symbol__options { grid-template-columns: repeat(6, minmax(0, 1fr)); }
}
</style>
