<script setup lang="ts">
/**
 * The model picker (Ctrl+Shift+M; T3 Code's ProviderModelPicker): agents on
 * the left, the chosen agent's catalog on the right with a search. Models
 * come from the installed CLI itself (its initialize handshake), keep their
 * invocation id apart from their name, and an id the catalog does not list
 * can still be typed. Keys: Up/Down and Enter in the list, Left in an empty
 * search or Shift+Tab to the agents, Right back, Ctrl+Shift+Up/Down switch
 * agents, Ctrl+1..9 pick, Escape closes.
 */
import { computed, nextTick, onMounted, ref, watch } from 'vue'
import { Check, Loader2, RefreshCw, Search } from 'lucide-vue-next'
import type { ProviderCatalog } from '#pilot/types'
import { modelRows, type ProviderOption } from '#pilot/utils/models'

const props = defineProps<{
  providers: ProviderOption[]
  provider: string
  model: string
  catalog: ProviderCatalog | null
  loading: boolean
  error: string | null
}>()
const emit = defineEmits<{ pick: [provider: string, model: string]; close: []; provider: [id: string]; refresh: [] }>()

const query = ref('')
const search = ref<HTMLInputElement | null>(null)
const providerList = ref<HTMLElement | null>(null)
const shownProvider = ref(props.provider)
const active = ref(0)
const providerActive = ref(Math.max(0, props.providers.findIndex((p) => p.id === props.provider)))

const rows = computed(() => modelRows(props.catalog?.models ?? [], query.value))
watch(rows, () => {
  const cur = rows.value.findIndex((r) => r.id === props.model)
  active.value = query.value ? 0 : Math.max(0, cur)
})
watch(shownProvider, (id) => emit('provider', id))

function pick(i: number) {
  const r = rows.value[i]
  if (!r) return
  emit('pick', shownProvider.value, r.id)
}

function switchProvider(dir: 1 | -1) {
  const n = props.providers.length
  if (!n) return
  let i = providerActive.value
  for (let k = 0; k < n; k++) {
    i = (i + dir + n) % n
    if (props.providers[i].available) break
  }
  providerActive.value = i
  shownProvider.value = props.providers[i].id
  query.value = ''
}

function onSearchKey(e: KeyboardEvent) {
  const mod = e.ctrlKey || e.metaKey
  if (mod && e.shiftKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
    e.preventDefault()
    switchProvider(e.key === 'ArrowUp' ? -1 : 1)
  } else if (e.key === 'ArrowDown') {
    e.preventDefault()
    active.value = Math.min(rows.value.length - 1, active.value + 1)
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    active.value = Math.max(0, active.value - 1)
  } else if (e.key === 'Enter') {
    e.preventDefault()
    pick(active.value)
  } else if ((e.key === 'ArrowLeft' && !query.value) || (e.key === 'Tab' && e.shiftKey)) {
    e.preventDefault()
    providerList.value?.focus()
  } else if (mod && /^[1-9]$/.test(e.key)) {
    e.preventDefault()
    pick(Number(e.key) - 1)
  } else if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    emit('close')
  }
}

function onProviderKey(e: KeyboardEvent) {
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    switchProvider(e.key === 'ArrowUp' ? -1 : 1)
  } else if (e.key === 'Enter' || e.key === 'ArrowRight' || e.key === 'Tab') {
    e.preventDefault()
    search.value?.focus()
  } else if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    emit('close')
  }
}

function chooseProvider(i: number) {
  const p = props.providers[i]
  if (!p?.available) return
  providerActive.value = i
  shownProvider.value = p.id
  void nextTick(() => search.value?.focus())
}

onMounted(() => void nextTick(() => search.value?.focus()))
</script>

<template>
  <div class="qp-mp" role="dialog" aria-label="Choose a model">
    <div ref="providerList" class="qp-mp-providers" tabindex="0" role="listbox" aria-label="Agents" @keydown="onProviderKey">
      <button
        v-for="(p, i) in providers"
        :key="p.id"
        class="qp-mp-provider"
        role="option"
        tabindex="-1"
        :aria-selected="p.id === shownProvider"
        :class="{ on: p.id === shownProvider, disabled: !p.available }"
        :title="p.available ? p.label : p.reason"
        @click="chooseProvider(i)"
      >
        <span class="qp-mp-dot" :data-provider="p.id" />
        <span class="qp-mp-provider-name">{{ p.label }}</span>
      </button>
    </div>
    <div class="qp-mp-main">
      <div class="qp-mp-search">
        <Search :size="13" />
        <input ref="search" v-model="query" placeholder="Search models or type an id" spellcheck="false" @keydown="onSearchKey" />
        <button class="qp-mp-refresh" title="Ask the CLI again" @click="emit('refresh')">
          <Loader2 v-if="loading" class="qp-spin" :size="13" />
          <RefreshCw v-else :size="13" />
        </button>
      </div>
      <div class="qp-mp-list" role="listbox">
        <div v-if="error && !rows.length" class="qp-mp-empty">{{ error }}</div>
        <div v-else-if="loading && !catalog" class="qp-mp-empty">Asking {{ providers.find((p) => p.id === shownProvider)?.label }} for its models…</div>
        <button
          v-for="(r, i) in rows"
          :key="r.id + (r.custom ? ':custom' : '')"
          class="qp-mp-row"
          role="option"
          :aria-selected="r.id === model && !r.custom"
          :class="{ active: i === active }"
          @mouseenter="active = i"
          @click="pick(i)"
        >
          <span class="qp-mp-check"><Check v-if="r.id === model && !r.custom && shownProvider === provider" :size="13" /></span>
          <span class="qp-mp-text">
            <span class="qp-mp-label">{{ r.label }}</span>
            <span v-if="r.detail" class="qp-mp-detail">{{ r.detail }}</span>
          </span>
          <span v-if="i < 9" class="qp-mp-key">Ctrl+{{ i + 1 }}</span>
        </button>
      </div>
      <div v-if="catalog?.version" class="qp-mp-foot">{{ providers.find((p) => p.id === shownProvider)?.label }} {{ catalog.version }}</div>
    </div>
  </div>
</template>

<style scoped>
.qp-mp {
  display: flex;
  width: 520px;
  max-width: calc(100vw - 48px);
  height: 340px;
  border-radius: 12px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
  overflow: hidden;
  animation: qp-mp-in 0.17s ease-out;
}
@keyframes qp-mp-in {
  from { opacity: 0; transform: translateY(4px) scale(0.98); }
  to { opacity: 1; transform: none; }
}
@media (prefers-reduced-motion: reduce) {
  .qp-mp { animation: none; }
}
.qp-mp-providers {
  width: 150px;
  flex-shrink: 0;
  padding: 6px;
  border-right: 1px solid var(--qss-border-subtle, #35353d);
  display: flex;
  flex-direction: column;
  gap: 2px;
  outline: none;
}
.qp-mp-providers:focus-visible { box-shadow: inset 0 0 0 1px var(--qss-accent, #7aa2f7); }
.qp-mp-provider {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border: none;
  border-radius: 6px;
  background: transparent;
  color: var(--qss-text-secondary, #9a9aa5);
  font-size: 12.5px;
  text-align: left;
  cursor: pointer;
}
.qp-mp-provider:hover:not(.disabled) { background: var(--qss-bg-hover, #313139); }
.qp-mp-provider.on { background: var(--qss-bg-card, #292930); color: var(--qss-text, #d4d4d8); }
.qp-mp-provider.disabled { opacity: 0.4; cursor: default; }
.qp-mp-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--qss-text-muted, #6e6e7a); flex-shrink: 0; }
.qp-mp-dot[data-provider='claude'] { background: #d97757; }
.qp-mp-dot[data-provider='codex'] { background: #e5e7eb; }
.qp-mp-main { flex: 1; min-width: 0; display: flex; flex-direction: column; }
.qp-mp-search { display: flex; align-items: center; gap: 6px; padding: 8px 10px; border-bottom: 1px solid var(--qss-border-subtle, #35353d); color: var(--qss-text-muted, #6e6e7a); }
.qp-mp-search input { flex: 1; min-width: 0; border: none; outline: none; background: transparent; color: var(--qss-text, #d4d4d8); font-size: 12.5px; }
.qp-mp-refresh { display: inline-flex; border: none; background: transparent; color: var(--qss-text-muted, #6e6e7a); padding: 4px; border-radius: 5px; cursor: pointer; }
.qp-mp-refresh:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-mp-list { flex: 1; min-height: 0; overflow-y: auto; padding: 4px; }
.qp-mp-empty { padding: 16px 10px; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); }
.qp-mp-row { display: flex; align-items: flex-start; gap: 6px; width: 100%; padding: 6px 8px 6px 4px; border: none; border-radius: 6px; background: transparent; color: var(--qss-text, #d4d4d8); text-align: left; cursor: pointer; }
.qp-mp-row.active { background: var(--qss-bg-hover, #313139); }
.qp-mp-check { width: 16px; flex-shrink: 0; display: inline-flex; justify-content: center; padding-top: 1px; color: var(--qss-accent, #7aa2f7); }
.qp-mp-text { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
.qp-mp-label { font-size: 12.5px; }
.qp-mp-detail { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-mp-key { flex-shrink: 0; font-size: 10.5px; color: var(--qss-text-muted, #6e6e7a); padding-top: 2px; opacity: 0; }
.qp-mp-row.active .qp-mp-key { opacity: 1; }
.qp-mp-foot { padding: 5px 10px; font-size: 10.5px; color: var(--qss-text-muted, #6e6e7a); border-top: 1px solid var(--qss-border-subtle, #35353d); }
.qp-spin { animation: qp-spin 0.9s linear infinite; }
@keyframes qp-spin { to { transform: rotate(360deg); } }
</style>
