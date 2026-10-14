<script setup lang="ts">
import { ChevronRight, Folder, FolderOpen, FolderPlus, Plus, MoreHorizontal } from 'lucide-vue-next'
import { bus } from '@quantsuite/core'
import { useAppStore } from '#systems/stores/app'
import { useSystemsStore } from '#systems/stores/systems'
import { useConfigStore } from '#systems/stores/config'
import { useStrategyGroupsStore } from '#systems/stores/groups'
import { useActiveView } from '#systems/composables/useActiveView'
import { groupSections, GROUPS_SCOPE, GROUPS_KEY, GROUPS_DEV_KEY, type StrategyGroup } from '#systems/utils/strategyGroups'
import StrategyRailItem from './StrategyRailItem.vue'
import StrategyGroupEditor from './StrategyGroupEditor.vue'

const app = useAppStore()
const systems = useSystemsStore()
const config = useConfigStore()
const groups = useStrategyGroupsStore()
const router = useRouter()
const { systemId } = useActiveView()
const sections = computed(() => groupSections(groups.groups, systems.systems))
const editing = ref(false)
const editedGroup = ref<StrategyGroup | null>(null)
const draggedId = ref<string | null>(null)
const dropTarget = ref<string | null | undefined>(undefined)
const dragType = 'application/x-quantsuite-strategy'
let unsubscribe: (() => void) | undefined
const shortcut = (id: string) => systems.systems.findIndex(s => s.id === id)

function select(id: string, status: string) {
  if (status !== 'ready') return
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
function endDrag() { draggedId.value = null; dropTarget.value = undefined }
function startDrag(event: DragEvent) {
  const id = (event.target as HTMLElement).closest<HTMLElement>('[data-strategy-id]')?.dataset.strategyId
  if (groups.busy || !groups.loaded || !id || !systems.systems.some(s => s.id === id) || !event.dataTransfer) {
    event.preventDefault()
    return
  }
  draggedId.value = id
  event.dataTransfer.effectAllowed = 'move'
  event.dataTransfer.setData(dragType, id)
}
function dragOver(event: DragEvent, target: string | null) {
  if (!draggedId.value || groups.busy || !event.dataTransfer?.types.includes(dragType)) return
  event.preventDefault()
  event.dataTransfer.dropEffect = 'move'
  dropTarget.value = target
}
function dragLeave(event: DragEvent) {
  if (!(event.currentTarget as HTMLElement).contains(event.relatedTarget as Node | null)) dropTarget.value = undefined
}
function drop(event: DragEvent, target: string | null) {
  if (!draggedId.value || groups.busy) return
  event.preventDefault()
  const id = event.dataTransfer?.getData(dragType)
  if (id === draggedId.value && systems.systems.some(s => s.id === id)) void groups.moveSystem(id, target)
  endDrag()
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
  unsubscribe?.()
  window.removeEventListener('storage', storageChanged)
})
onDeactivated(() => { editing.value = false; endDrag() })
</script>

<template>
  <QSidebar>
    <nav class="qs-rail" :class="{ 'qs-rail--dragging': draggedId }" aria-label="Strategies" @dragstart="startDrag" @dragend="endDrag">
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
      <div v-if="sections.ungrouped.length || groups.groups.length" class="qs-rail__list" :class="{ 'qs-rail__drop-target': dropTarget === null }" aria-label="Ungrouped strategies" @dragover="dragOver($event, null)" @dragleave="dragLeave" @drop="drop($event, null)">
        <span v-if="groups.groups.length" class="qs-rail__label">Ungrouped</span>
        <StrategyRailItem v-for="s in sections.ungrouped" :key="s.id" :strategy="s" :active="systemId === s.id" :shortcut="shortcut(s.id)" @select="select(s.id, s.status)" />
        <span v-if="!sections.ungrouped.length" class="qs-rail__drop-hint">Drop here to ungroup</span>
      </div>
      <section v-for="section in sections.groups" :key="section.group.id" class="qs-rail__section" :class="{ 'qs-rail__drop-target': dropTarget === section.group.id }" :aria-label="`Group ${section.group.name}`" @dragover="dragOver($event, section.group.id)" @dragleave="dragLeave" @drop="drop($event, section.group.id)">
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
          <StrategyRailItem v-for="s in section.systems" :key="s.id" :strategy="s" :active="systemId === s.id" :shortcut="shortcut(s.id)" @select="select(s.id, s.status)" />
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
