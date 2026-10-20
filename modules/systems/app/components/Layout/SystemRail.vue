<script setup lang="ts">
import { ChevronRight, Folder, FolderOpen, FolderPlus, Plus, MoreHorizontal } from 'lucide-vue-next'
import { bus } from '@quantsuite/core'
import { useAppStore } from '#systems/stores/app'
import { useSystemsStore } from '#systems/stores/systems'
import { useConfigStore } from '#systems/stores/config'
import { useStrategyGroupsStore } from '#systems/stores/groups'
import { useActiveView } from '#systems/composables/useActiveView'
import { groupSections, orderSystems, GROUPS_SCOPE, GROUPS_KEY, GROUPS_DEV_KEY, type StrategyGroup } from '#systems/utils/strategyGroups'
import type { SystemMeta } from '#systems/types'
import StrategyRailItem from './StrategyRailItem.vue'
import StrategyGroupEditor from './StrategyGroupEditor.vue'

const app = useAppStore()
const systems = useSystemsStore()
const config = useConfigStore()
const groups = useStrategyGroupsStore()
const router = useRouter()
const { systemId } = useActiveView()
const sections = computed(() => groupSections(groups.groups, systems.systems, groups.order))
const ordered = computed(() => orderSystems(systems.systems, groups.order))
const editing = ref(false)
const editedGroup = ref<StrategyGroup | null>(null)
const rail = ref<HTMLElement>()
const draggedId = ref<string | null>(null)
/** Where the dragged strategy lands: a section (null = ungrouped), before a strategy (null = at the section's end). */
const drop = ref<{ groupId: string | null; before: string | null } | null>(null)
let press: { id: string; x: number; y: number; pointerId: number; el: HTMLElement; cancelled: boolean } | null = null
let pointer = { x: 0, y: 0 }
let scrollBox: HTMLElement | undefined
let frame = 0
let suppressClick = false
let unsubscribe: (() => void) | undefined
const shortcut = (id: string) => ordered.value.findIndex(s => s.id === id)

function select(id: string, status: string) {
  if (suppressClick || status !== 'ready') return
  app.setActiveSystem(id)
  void config.load(id)
  void router.push(`/algo/manual/${id}/${app.viewFor(id)}`)
}
function edit(group: StrategyGroup | null) {
  editedGroup.value = group
  editing.value = true
}
function storageChanged(event: StorageEvent) {
  if (event.key === GROUPS_DEV_KEY) void groups.load(true)
}
// Pointer events, not HTML5 drag and drop: that API broke differently at every step inside WebView2 (see TabStrip.vue).
function startDrag(event: PointerEvent) {
  if (event.button !== 0 || !event.isPrimary) return
  const el = (event.target as HTMLElement).closest<HTMLElement>('[data-strategy-id]')
  const id = el?.dataset.strategyId
  if (!el || !id || groups.busy || !groups.loaded || systems.byId(id)?.status !== 'ready') return
  endDrag()
  press = { id, x: event.clientX, y: event.clientY, pointerId: event.pointerId, el, cancelled: false }
  window.addEventListener('pointermove', moveDrag, { passive: false })
  window.addEventListener('pointerup', finishDrag)
  window.addEventListener('pointercancel', endDrag)
  window.addEventListener('keydown', escapeDrag)
  window.addEventListener('blur', endDrag)
}
function moveDrag(event: PointerEvent) {
  if (!press || press.cancelled || event.pointerId !== press.pointerId) return
  pointer = { x: event.clientX, y: event.clientY }
  if (!draggedId.value) {
    if (Math.hypot(pointer.x - press.x, pointer.y - press.y) < 5) return
    if (groups.busy) return endDrag()
    draggedId.value = press.id
    try { press.el.setPointerCapture(press.pointerId) } catch { /* synthetic events carry no pointer to capture */ }
    document.body.style.userSelect = 'none'
    scrollBox = scroller()
    frame = requestAnimationFrame(autoScroll)
  }
  event.preventDefault()
  updateDrop()
}
function finishDrag(event: PointerEvent) {
  if (!press || event.pointerId !== press.pointerId) return
  const id = draggedId.value
  const dragged = !!id || press.cancelled
  if (id) { pointer = { x: event.clientX, y: event.clientY }; updateDrop() }
  const target = drop.value
  endDrag()
  if (!dragged) return
  // The click that ends a drag must not open the strategy; cleared after that click has fired.
  suppressClick = true
  setTimeout(() => { suppressClick = false })
  if (id && target) void groups.moveSystem(id, target.groupId, target.before, systems.systems.map(s => s.id))
}
function escapeDrag(event: KeyboardEvent) {
  if (event.key !== 'Escape' || !press || !draggedId.value) return
  event.preventDefault()
  press.cancelled = true
  draggedId.value = null
  drop.value = null
  cancelAnimationFrame(frame)
}
function endDrag() {
  const previous = press
  press = null
  draggedId.value = null
  drop.value = null
  cancelAnimationFrame(frame)
  document.body.style.userSelect = ''
  window.removeEventListener('pointermove', moveDrag)
  window.removeEventListener('pointerup', finishDrag)
  window.removeEventListener('pointercancel', endDrag)
  window.removeEventListener('keydown', escapeDrag)
  window.removeEventListener('blur', endDrag)
  if (previous?.el.hasPointerCapture(previous.pointerId)) previous.el.releasePointerCapture(previous.pointerId)
}
function sectionSystems(groupId: string | null) {
  return groupId === null ? sections.value.ungrouped : sections.value.groups.find(s => s.group.id === groupId)?.systems ?? []
}
/** The section under the pointer and the first visible strategy in it whose middle lies below the pointer. */
function updateDrop() {
  drop.value = null
  const section = document.elementFromPoint(pointer.x, pointer.y)?.closest<HTMLElement>('[data-drop-group]')
  if (!section || !rail.value?.contains(section) || !draggedId.value) return
  const groupId = section.dataset.dropGroup || null
  const next = [...section.querySelectorAll<HTMLElement>('[data-strategy-id]')].find(el => {
    const box = el.getBoundingClientRect()
    return box.height > 0 && pointer.y < box.top + box.height / 2
  })
  const before = next?.dataset.strategyId ?? null
  const ids = sectionSystems(groupId).map(s => s.id)
  const at = ids.indexOf(draggedId.value)
  // Its own slot: nothing would change, so no line.
  if (at >= 0 && (before === draggedId.value || before === (ids[at + 1] ?? null))) return
  drop.value = { groupId, before }
}
function scroller() {
  for (let el = rail.value?.parentElement; el; el = el.parentElement) {
    if (/auto|scroll/.test(getComputedStyle(el).overflowY)) return el
  }
}
/** Scrolls the sidebar while the pointer is near its top or bottom edge, so far-away groups stay reachable. */
function autoScroll() {
  const rect = scrollBox?.getBoundingClientRect()
  if (scrollBox && rect && pointer.x >= rect.left && pointer.x <= rect.right) {
    const step = pointer.y < rect.top + 28 ? -7 : pointer.y > rect.bottom - 28 ? 7 : 0
    if (step) { scrollBox.scrollTop += step; updateDrop() }
  }
  frame = requestAnimationFrame(autoScroll)
}
function itemClass(groupId: string | null, id: string, list: SystemMeta[]) {
  const here = drop.value?.groupId === groupId ? drop.value : null
  return {
    'qs-rail__item--dragged': draggedId.value === id,
    'qs-rail__item--before': here?.before === id,
    'qs-rail__item--after': !!here && here.before === null && list.at(-1)?.id === id,
  }
}
onMounted(() => {
  void groups.load()
  unsubscribe = bus.on('core.setting.changed', event => {
    const payload = event.payload as { scope?: string; key?: string }
    if (payload.scope === GROUPS_SCOPE && payload.key === GROUPS_KEY) void groups.load(true)
  })
  window.addEventListener('storage', storageChanged)
})
onBeforeUnmount(() => {
  endDrag()
  unsubscribe?.()
  window.removeEventListener('storage', storageChanged)
})
onDeactivated(() => { editing.value = false; endDrag() })
</script>

<template>
  <QSidebar>
    <nav ref="rail" class="qs-rail" :class="{ 'qs-rail--dragging': draggedId }" aria-label="Strategies" @pointerdown="startDrag">
      <div class="qs-rail__heading">
        <span>Strategies</span><span class="qs-rail__count">{{ systems.systems.length }}</span>
      </div>
      <div class="qs-rail__actions">
        <button class="btn qs-rail__new" @click="router.push('/algo/manual/new')"><Plus :size="14" /> New strategy</button>
        <button class="qs-rail__icon" title="Create group" aria-label="Create group" :disabled="!groups.loaded || groups.busy" @click="edit(null)"><FolderPlus :size="17" /></button>
      </div>
      <div v-if="groups.error && !editing" class="qs-rail__error" role="alert">
        {{ groups.error }}
        <button class="btn" :disabled="groups.busy" @click="groups.load(true)">Reload groups</button>
      </div>
      <div v-if="sections.ungrouped.length || groups.groups.length" class="qs-rail__list" :class="{ 'qs-rail__drop-target': drop?.groupId === null }" data-drop-group="" aria-label="Ungrouped strategies">
        <span v-if="groups.groups.length" class="qs-rail__label">Ungrouped</span>
        <StrategyRailItem v-for="s in sections.ungrouped" :key="s.id" :class="itemClass(null, s.id, sections.ungrouped)" :strategy="s" :active="systemId === s.id" :shortcut="shortcut(s.id)" @select="select(s.id, s.status)" />
        <span v-if="!sections.ungrouped.length" class="qs-rail__drop-hint">Drop here to ungroup</span>
      </div>
      <section v-for="section in sections.groups" :key="section.group.id" class="qs-rail__section" :class="{ 'qs-rail__drop-target': drop?.groupId === section.group.id }" :data-drop-group="section.group.id" :aria-label="`Group ${section.group.name}`">
        <div class="qs-rail__group-header" :class="{ 'qs-rail__group-header--active': section.systems.some(s => s.id === systemId) }">
          <button class="qs-rail__toggle" :aria-expanded="!section.group.collapsed" :aria-controls="`strategy-group-${section.group.id}`" :title="section.group.name" :disabled="groups.busy" @click="groups.toggle(section.group.id)">
            <ChevronRight class="qs-rail__chevron" :class="{ 'qs-rail__chevron--open': !section.group.collapsed }" :size="13" />
            <component :is="section.group.collapsed ? Folder : FolderOpen" :size="15" />
            <span class="qs-rail__group-name">{{ section.group.name }}</span>
            <span class="qs-rail__count">{{ section.systems.length }}</span>
          </button>
          <button class="qs-rail__icon qs-rail__edit" :aria-label="`Edit group ${section.group.name}`" :title="`Edit group ${section.group.name}`" :disabled="groups.busy" @click="edit(section.group)"><MoreHorizontal :size="15" /></button>
        </div>
        <div v-show="!section.group.collapsed" :id="`strategy-group-${section.group.id}`" class="qs-rail__members">
          <StrategyRailItem v-for="s in section.systems" :key="s.id" :class="itemClass(section.group.id, s.id, section.systems)" :strategy="s" :active="systemId === s.id" :shortcut="shortcut(s.id)" @select="select(s.id, s.status)" />
          <button v-if="!section.systems.length" class="qs-rail__empty" @click="edit(section.group)">Add strategies</button>
        </div>
      </section>
    </nav>
    <StrategyGroupEditor v-if="editing" :group="editedGroup" @close="editing = false" />
  </QSidebar>
</template>

<style scoped>
.qs-rail { display: flex; flex-direction: column; gap: 12px; padding: 16px 10px; }
.qs-rail__heading { display: flex; align-items: center; justify-content: space-between; padding: 0 8px; font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: .07em; color: var(--qs-text-muted); }
.qs-rail__count { flex-shrink: 0; font-size: 10px; font-variant-numeric: tabular-nums; color: var(--qs-text-muted); }
.qs-rail__actions { display: flex; gap: 6px; padding-bottom: 12px; border-bottom: 1px solid var(--qs-border); }
.qs-rail__new { flex: 1; display: flex; align-items: center; justify-content: center; gap: 5px; font-size: 11px; }
.qs-rail__icon { display: flex; flex-shrink: 0; align-items: center; justify-content: center; width: 30px; height: 30px; border: 1px solid var(--qs-border); border-radius: 6px; background: transparent; color: var(--qs-text-secondary); cursor: pointer; }
.qs-rail__icon:hover, .qs-rail__toggle:hover { color: var(--qs-text); background: var(--qs-bg-hover); }
.qs-rail__icon:disabled, .qs-rail__toggle:disabled { cursor: wait; opacity: .6; }
.qs-rail__list { display: flex; flex-direction: column; gap: 2px; }
.qs-rail__list, .qs-rail__section { border-radius: 6px; }
.qs-rail--dragging .qs-rail__list, .qs-rail--dragging .qs-rail__section { outline: 1px dashed var(--qs-border); outline-offset: 2px; }
.qs-rail--dragging .qs-rail__drop-target { outline: 2px solid var(--qs-accent); background: var(--qs-bg-hover); }
.qs-rail__item--dragged { opacity: .45; }
.qs-rail__item--before, .qs-rail__item--after { position: relative; }
.qs-rail__item--before::before, .qs-rail__item--after::after { content: ''; position: absolute; left: 4px; right: 4px; height: 2px; border-radius: 1px; background: var(--qs-accent); pointer-events: none; }
.qs-rail__item--before::before { top: -2px; }
.qs-rail__item--after::after { bottom: -2px; }
.qs-rail__drop-hint { padding: 6px 8px; font-size: 10px; color: var(--qs-text-muted); pointer-events: none; }
.qs-rail__label { padding: 0 8px 4px; font-size: 10px; color: var(--qs-text-muted); }
.qs-rail__group-header { display: flex; align-items: center; gap: 2px; border-radius: 6px; }
.qs-rail__group-header--active { background: var(--qs-bg-hover); }
.qs-rail__toggle { display: flex; flex: 1; min-width: 0; align-items: center; gap: 6px; padding: 8px 4px; border: 0; border-radius: 6px; background: transparent; color: var(--qs-text-secondary); cursor: pointer; text-align: left; }
.qs-rail__toggle > svg { flex-shrink: 0; }
.qs-rail__group-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; font-weight: 600; }
.qs-rail__chevron { transition: transform 120ms ease; }
.qs-rail__chevron--open { transform: rotate(90deg); }
.qs-rail__edit { width: 24px; height: 26px; border-color: transparent; }
.qs-rail__members { margin: 3px 0 0 10px; padding-left: 5px; border-left: 1px solid var(--qs-border); }
.qs-rail__empty { width: 100%; padding: 10px 7px; border: 0; background: transparent; color: var(--qs-text-muted); cursor: pointer; text-align: left; font-size: 11px; }
.qs-rail__empty:hover { color: var(--qs-accent); }
.qs-rail__error { display: grid; gap: 8px; padding: 8px; color: var(--qs-error); font-size: 11px; overflow-wrap: anywhere; }
.qs-rail button:focus-visible { outline: 2px solid var(--qs-accent); outline-offset: 2px; }
</style>
