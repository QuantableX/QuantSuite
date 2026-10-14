<script setup lang="ts">
import { Search, X } from 'lucide-vue-next'
import { useSystemsStore } from '#systems/stores/systems'
import { useStrategyGroupsStore } from '#systems/stores/groups'
import type { StrategyGroup } from '#systems/utils/strategyGroups'

const props = defineProps<{ group: StrategyGroup | null }>()
const emit = defineEmits<{ close: [] }>()
const systems = useSystemsStore()
const groups = useStrategyGroupsStore()
const original = props.group ? { ...props.group, systemIds: [...props.group.systemIds] } : undefined
const name = ref(props.group?.name ?? '')
const selected = ref([...(props.group?.systemIds ?? [])])
const search = ref('')
const dialog = ref<HTMLDialogElement>()
const deleting = ref(false)
const filtered = computed(() => systems.systems.filter(s => `${s.name} ${s.short}`.toLowerCase().includes(search.value.toLowerCase().trim())))
const count = computed(() => systems.systems.filter(s => selected.value.includes(s.id)).length)
const currentGroup = (id: string) => groups.groups.find(g => g.id !== props.group?.id && g.systemIds.includes(id))?.name
function selectVisible(add: boolean) {
  const visible = new Set(filtered.value.map(s => s.id))
  selected.value = add ? [...new Set([...selected.value, ...visible])] : selected.value.filter(id => !visible.has(id))
}
async function save() {
  if (groups.busy || deleting.value) return
  if (await groups.saveGroup(props.group?.id ?? null, name.value, selected.value, original)) emit('close')
}
async function remove() { if (props.group && await groups.remove(props.group.id)) emit('close') }
onMounted(() => { groups.error = null; dialog.value?.showModal() })
onBeforeUnmount(() => dialog.value?.close())
</script>

<template>
  <dialog ref="dialog" class="qs-group-editor" aria-labelledby="qs-group-title" @cancel="groups.busy ? $event.preventDefault() : emit('close')">
    <form @submit.prevent="save">
      <header class="qs-group-editor__header"><h2 id="qs-group-title">{{ group ? 'Edit group' : 'New group' }}</h2><button type="button" class="qs-group-editor__close" aria-label="Close group editor" :disabled="groups.busy" @click="emit('close')"><X :size="18" /></button></header>
      <label class="qs-group-editor__field">Group name<input v-model="name" class="input" required maxlength="80" autofocus placeholder="e.g. Top 10 · Aggregates" :disabled="groups.busy" /></label>
      <div class="qs-group-editor__search"><Search :size="15" aria-hidden="true" /><input v-model="search" class="input" aria-label="Find strategies" placeholder="Find strategies…" /></div>
      <div class="qs-group-editor__selection"><span>{{ count }} selected</span><button type="button" :disabled="groups.busy || !filtered.length" @click="selectVisible(true)">Select visible</button><button type="button" :disabled="groups.busy || !filtered.length" @click="selectVisible(false)">Clear visible</button></div>
      <div class="qs-group-editor__members" role="group" aria-label="Group strategies">
        <label v-for="s in filtered" :key="s.id" class="qs-group-editor__member"><input v-model="selected" type="checkbox" :value="s.id" :disabled="groups.busy" /><span><strong>{{ s.name }}</strong><small v-if="currentGroup(s.id)">Currently in {{ currentGroup(s.id) }}</small></span></label>
        <p v-if="!filtered.length" class="qs-group-editor__hint">{{ systems.systems.length ? 'No strategies match your search.' : 'Create a strategy whenever you are ready to add members.' }}</p>
      </div>
      <p class="qs-group-editor__hint">Selected strategies move into this group. Unchecked members return to the ungrouped list.</p>
      <p v-if="groups.error" class="qs-group-editor__error" role="alert">{{ groups.error }}</p>
      <div v-if="deleting" class="qs-group-editor__delete" role="group" aria-label="Confirm group deletion"><p>Delete “{{ group?.name }}”? Its strategies will stay in the ungrouped list.</p><button type="button" class="btn" :disabled="groups.busy" @click="deleting = false">Keep group</button><button type="button" class="btn" :disabled="groups.busy" @click="remove">Delete group only</button></div>
      <footer class="qs-group-editor__footer"><button v-if="group && !deleting" type="button" class="qs-group-editor__remove" :disabled="groups.busy" @click="deleting = true">Delete group</button><span /><button type="button" class="btn" :disabled="groups.busy" @click="emit('close')">Cancel</button><button class="btn btn-primary" :disabled="groups.busy || !name.trim() || deleting">{{ groups.busy ? 'Saving…' : 'Save group' }}</button></footer>
    </form>
  </dialog>
</template>

<style scoped>
.qs-group-editor { position: fixed; inset: 0; margin: auto; width: min(520px, calc(100vw - 32px)); height: fit-content; max-height: calc(100vh - 48px); overflow-y: auto; padding: 22px; border: 1px solid var(--qs-border); border-radius: 12px; background: var(--qs-bg-card); color: var(--qs-text); box-shadow: 0 24px 80px #0006; }
.qs-group-editor::backdrop { background: #0008; }
.qs-group-editor form { display: flex; flex-direction: column; gap: 15px; }
.qs-group-editor__header, .qs-group-editor__selection, .qs-group-editor__footer { display: flex; align-items: center; gap: 12px; }
.qs-group-editor__header h2 { margin: 0; flex: 1; font-size: 18px; font-weight: 600; }
.qs-group-editor__close { display: grid; place-items: center; padding: 5px; border: 0; background: transparent; color: var(--qs-text-secondary); cursor: pointer; }
.qs-group-editor__field { display: flex; flex-direction: column; gap: 6px; font-size: 12px; }
.qs-group-editor__search { position: relative; }
.qs-group-editor__search svg { position: absolute; left: 10px; top: 10px; color: var(--qs-text-muted); }
.qs-group-editor__search input { width: 100%; padding-left: 32px; }
.qs-group-editor__selection { font-size: 11px; color: var(--qs-text-secondary); }
.qs-group-editor__selection span { flex: 1; }
.qs-group-editor__selection button, .qs-group-editor__remove { border: 0; padding: 0; background: transparent; color: var(--qs-accent); font-size: 11px; cursor: pointer; }
.qs-group-editor__members { max-height: min(340px, 40vh); overflow-y: auto; border: 1px solid var(--qs-border); border-radius: 8px; }
.qs-group-editor__member { display: flex; align-items: center; gap: 10px; padding: 10px 12px; font-size: 12px; cursor: pointer; }
.qs-group-editor__member:hover { background: var(--qs-bg-hover); }
.qs-group-editor__member input { accent-color: var(--qs-accent); }
.qs-group-editor__member span { min-width: 0; overflow-wrap: anywhere; }
.qs-group-editor__member strong { font-weight: 500; }
.qs-group-editor__member small { display: block; font-size: 10px; margin-top: 3px; color: var(--qs-text-muted); }
.qs-group-editor__hint { font-size: 11px; color: var(--qs-text-muted); margin: 0; line-height: 1.5; }
.qs-group-editor__members > p { padding: 16px; }
.qs-group-editor__footer { padding-top: 4px; }
.qs-group-editor__footer > span { flex: 1; }
.qs-group-editor__error { margin: 0; font-size: 12px; color: var(--qs-error); }
.qs-group-editor__delete { padding: 12px; border: 1px solid var(--qs-border); border-radius: 8px; font-size: 12px; }
.qs-group-editor__delete p { margin: 0 0 12px; }
.qs-group-editor button:disabled { cursor: default; opacity: .5; }
</style>
