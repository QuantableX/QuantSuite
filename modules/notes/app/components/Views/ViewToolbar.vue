<script setup lang="ts">
/**
 * The active view's controls: what it groups by, what it sorts by, which
 * properties it shows, and a way to add a property to the collection.
 *
 * Everything here writes to the *view's* config, not to the notes — the same
 * collection looks different per view, which is what makes views cheap.
 */
import { useSchemaStore } from '#notes/stores/schema'
import { useNotesStore } from '#notes/stores/notes'
import type { PropertyKind, ViewSort } from '#notes/types'

const schema = useSchemaStore()
const notes = useNotesStore()
const router = useRouter()

const openMenu = ref<'group' | 'sort' | 'props' | 'new' | null>(null)
const root = ref<HTMLElement | null>(null)

const view = computed(() => schema.activeView)
const selectProperties = computed(() => schema.properties.filter((p) => p.kind === 'select'))
const dateProperties = computed(() => schema.properties.filter((p) => p.kind === 'date'))

const groupLabel = computed(() => {
  const id = view.value?.config.groupBy
  return id ? (schema.propertyById.get(id)?.name ?? 'None') : 'None'
})
const dateLabel = computed(() => {
  const id = view.value?.config.dateBy
  return id ? (schema.propertyById.get(id)?.name ?? 'None') : 'None'
})
const sorts = computed<ViewSort[]>(() => view.value?.config.sorts ?? [])

function isVisible(id: string): boolean {
  const visible = view.value?.config.visible
  return !visible || visible.length === 0 || visible.includes(id)
}

async function toggleVisible(id: string) {
  if (!view.value) return
  // An absent `visible` means "all"; materialise it before removing one, or
  // the first click would hide everything except the one just clicked.
  const current = view.value.config.visible?.length
    ? [...view.value.config.visible]
    : schema.properties.map((p) => p.id)
  const at = current.indexOf(id)
  if (at === -1) current.push(id)
  else current.splice(at, 1)
  await schema.patchActiveConfig({ visible: current })
}

async function setSort(propertyId: string | null) {
  openMenu.value = null
  if (!propertyId) {
    await schema.patchActiveConfig({ sorts: [] })
    return
  }
  const existing = sorts.value[0]
  const direction = existing?.propertyId === propertyId && existing.direction === 'asc' ? 'desc' : 'asc'
  await schema.patchActiveConfig({ sorts: [{ propertyId, direction }] })
}

const newName = ref('')
const newKind = ref<PropertyKind>('text')
const KINDS: Array<{ value: PropertyKind; label: string }> = [
  { value: 'text', label: 'Text' },
  { value: 'number', label: 'Number' },
  { value: 'select', label: 'Select' },
  { value: 'multi_select', label: 'Multi-select' },
  { value: 'date', label: 'Date' },
  { value: 'checkbox', label: 'Checkbox' },
  { value: 'url', label: 'URL' },
]

async function createProperty() {
  const name = newName.value.trim()
  if (!name) return
  const property = await schema.createProperty(name, newKind.value)
  newName.value = ''
  openMenu.value = null
  // A new property nobody can see is a bug report waiting to happen.
  if (view.value?.config.visible?.length) {
    await schema.patchActiveConfig({ visible: [...view.value.config.visible, property.id] })
  }
}

async function addNote() {
  const note = await notes.create({})
  router.push(`/notes/n/${note.id}`)
}

function onPointerDown(event: PointerEvent) {
  if (!root.value?.contains(event.target as Node)) openMenu.value = null
}

watch(openMenu, (open) => {
  if (open) document.addEventListener('pointerdown', onPointerDown)
  else document.removeEventListener('pointerdown', onPointerDown)
})

onBeforeUnmount(() => document.removeEventListener('pointerdown', onPointerDown))
</script>

<template>
  <div v-if="view" ref="root" class="qn-vt">
    <!-- Board: the property whose options become the columns. -->
    <div v-if="view.kind === 'board'" class="qn-vt__slot">
      <button class="qn-vt__btn" @click="openMenu = openMenu === 'group' ? null : 'group'">
        Group: <strong>{{ groupLabel }}</strong>
      </button>
      <div v-if="openMenu === 'group'" class="qn-vt__menu">
        <button
          v-for="property in selectProperties"
          :key="property.id"
          class="qn-vt__menu-item"
          @click="schema.patchActiveConfig({ groupBy: property.id }); openMenu = null"
        >
          {{ property.name }}
        </button>
        <p v-if="!selectProperties.length" class="qn-vt__menu-empty">
          Add a select property to group by.
        </p>
      </div>
    </div>

    <!-- Calendar: the date property that places a note on a day. -->
    <div v-if="view.kind === 'calendar'" class="qn-vt__slot">
      <button class="qn-vt__btn" @click="openMenu = openMenu === 'group' ? null : 'group'">
        Date: <strong>{{ dateLabel }}</strong>
      </button>
      <div v-if="openMenu === 'group'" class="qn-vt__menu">
        <button
          v-for="property in dateProperties"
          :key="property.id"
          class="qn-vt__menu-item"
          @click="schema.patchActiveConfig({ dateBy: property.id }); openMenu = null"
        >
          {{ property.name }}
        </button>
        <p v-if="!dateProperties.length" class="qn-vt__menu-empty">
          Add a date property to place notes on.
        </p>
      </div>
    </div>

    <div class="qn-vt__slot">
      <button class="qn-vt__btn" @click="openMenu = openMenu === 'sort' ? null : 'sort'">
        Sort<template v-if="sorts.length">: <strong>{{ schema.propertyById.get(sorts[0]!.propertyId)?.name }} {{ sorts[0]!.direction === 'asc' ? '↑' : '↓' }}</strong></template>
      </button>
      <div v-if="openMenu === 'sort'" class="qn-vt__menu">
        <button
          v-for="property in schema.properties"
          :key="property.id"
          class="qn-vt__menu-item"
          @click="setSort(property.id)"
        >
          {{ property.name }}
        </button>
        <button v-if="sorts.length" class="qn-vt__menu-item qn-vt__menu-clear" @click="setSort(null)">
          Clear sort
        </button>
      </div>
    </div>

    <div class="qn-vt__slot">
      <button class="qn-vt__btn" @click="openMenu = openMenu === 'props' ? null : 'props'">
        Properties
      </button>
      <div v-if="openMenu === 'props'" class="qn-vt__menu">
        <button
          v-for="property in schema.properties"
          :key="property.id"
          class="qn-vt__menu-item qn-vt__menu-toggle"
          @click="toggleVisible(property.id)"
        >
          <span class="qn-vt__check" :class="{ 'is-on': isVisible(property.id) }" />
          {{ property.name }}
        </button>
      </div>
    </div>

    <div class="qn-vt__slot">
      <button class="qn-vt__btn" @click="openMenu = openMenu === 'new' ? null : 'new'">
        + Property
      </button>
      <div v-if="openMenu === 'new'" class="qn-vt__menu qn-vt__menu--form">
        <input v-model="newName" class="qn-vt__input" placeholder="Property name" @keydown.enter.prevent="createProperty" />
        <select v-model="newKind" class="qn-vt__select">
          <option v-for="kind in KINDS" :key="kind.value" :value="kind.value">{{ kind.label }}</option>
        </select>
        <button class="qn-vt__submit" @click="createProperty">Add property</button>
      </div>
    </div>

    <span class="qn-vt__spacer" />

    <button class="qn-vt__primary" @click="addNote">+ New note</button>
  </div>
</template>

<style scoped>
.qn-vt {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 0;
}

.qn-vt__slot {
  position: relative;
}

.qn-vt__btn {
  padding: 5px 9px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qn-text-muted);
  font-size: 12px;
  cursor: pointer;
}

.qn-vt__btn:hover {
  background: var(--qn-bg-hover);
  color: var(--qn-text);
}

.qn-vt__btn strong {
  color: var(--qn-text-secondary);
  font-weight: 600;
}

.qn-vt__spacer {
  flex: 1;
}

.qn-vt__primary {
  padding: 5px 11px;
  border: 1px solid var(--qn-border);
  border-radius: 6px;
  background: var(--qn-bg-card);
  color: var(--qn-text-secondary);
  font-size: 12px;
  cursor: pointer;
}

.qn-vt__primary:hover {
  border-color: var(--qn-accent);
  color: var(--qn-text);
}

.qn-vt__menu {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  z-index: 40;
  width: 200px;
  padding: 4px;
  border: 1px solid var(--qn-border);
  border-radius: 10px;
  background: var(--qn-bg-sidebar);
  box-shadow: 0 16px 36px rgba(0, 0, 0, 0.35);
}

.qn-vt__menu--form {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px;
}

.qn-vt__menu-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 8px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qn-text-secondary);
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}

.qn-vt__menu-item:hover {
  background: var(--qn-bg-hover);
  color: var(--qn-text);
}

.qn-vt__menu-clear {
  color: var(--qn-text-muted);
}

.qn-vt__menu-empty {
  margin: 0;
  padding: 8px;
  color: var(--qn-text-muted);
  font-size: 12px;
  line-height: 1.4;
}

.qn-vt__check {
  width: 12px;
  height: 12px;
  border: 1px solid var(--qn-border);
  border-radius: 3px;
}

.qn-vt__check.is-on {
  border-color: var(--qn-accent);
  background: var(--qn-accent);
}

.qn-vt__input,
.qn-vt__select {
  width: 100%;
  padding: 5px 8px;
  border: 1px solid var(--qn-border);
  border-radius: 6px;
  background: var(--qn-bg-input);
  color: var(--qn-text);
  font-size: 12px;
  font-family: inherit;
  outline: none;
}

.qn-vt__submit {
  padding: 6px;
  border: 1px solid var(--qn-border);
  border-radius: 6px;
  background: var(--qn-bg-card);
  color: var(--qn-text);
  font-size: 12px;
  cursor: pointer;
}

.qn-vt__submit:hover {
  border-color: var(--qn-accent);
}
</style>
