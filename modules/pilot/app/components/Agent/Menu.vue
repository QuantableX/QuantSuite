<script setup lang="ts">
/**
 * The agent menu (round 2, card 02) — one menu wherever an agent is chosen:
 * the composer's agent control, New thread, Start on a board card, the
 * QuantCanvas chat window, fan-out. Every agent is listed with its logo:
 * usable ones with the setup they will really use, the others greyed with
 * why (Not installed, Not signed in, No chat on this host, Off …) and a
 * one-click fix. A row's submenu lists the agent's models (recently used
 * first) with their reasoning effort — and fast mode where the catalog
 * offers it — in the same row. Keys: type to filter, Up/Down, Right/Enter
 * into the submenu or pick, Left/Right across a model's effort, Escape back
 * then closed; Shift adds models for a fan-out. Teleported and placed in
 * the window so it fits short windows too.
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Check, ChevronRight, Loader2, RefreshCw, Search, Settings2 } from 'lucide-vue-next'
import { usePilotStore } from '#pilot/stores/pilot'
import { useThreadStore } from '#pilot/stores/thread'
import { effortsFor, modelRows, type ModelRow } from '#pilot/utils/models'
import { agentRowState, filterAgents, orderModels, placeMenu, placeSubmenu, pushRecent, setupLabel, type AgentSetup, type RecentModels } from '#pilot/utils/agentMenu'
import type { ModelInfo } from '#pilot/types'

const props = withDefaults(
  defineProps<{
    anchor: HTMLElement | null
    prefer?: 'below' | 'above'
    /** The agent in use (thread) — checked, and the one other rows are compared with. */
    provider?: string | null
    model?: string
    effort?: string
    fast?: boolean
    /** The project a thread starts or runs in (its pins and offered agents). */
    project?: string | null
    /** The thread's agent is working: other agents wait. */
    locked?: string | null
    title?: string
    allowMulti?: boolean
    multi?: { provider: string; model: string }[]
  }>(),
  { prefer: 'below', provider: null, model: undefined, effort: undefined, fast: false, project: null, locked: null, title: 'Agent', allowMulti: false, multi: () => [] },
)
const emit = defineEmits<{
  pick: [setup: AgentSetup]
  multi: [list: { provider: string; model: string }[]]
  close: []
}>()

const store = usePilotStore()
const threads = useThreadStore()

// ── Rows ──
const query = ref('')
const projectPrefs = computed(() => (props.project ? store.settings?.projects?.[props.project] : undefined))
const agents = computed(() =>
  filterAgents(store.adapters, query.value).map((a) => {
    const state = agentRowState(a, {
      disabled: Boolean(store.settings?.providers?.[a.id]?.disabled),
      projectEnabled: projectPrefs.value?.enabled ?? null,
      locked: props.locked,
      current: props.provider,
    })
    return { a, state, setup: setupOf(a.id) }
  }),
)

/** What picking the agent's row starts with: the thread's own setup, else a new thread's. */
function setupOf(id: string): AgentSetup {
  if (id === props.provider && props.model !== undefined) return { provider: id, model: props.model, effort: props.effort ?? '', fast: props.fast }
  const s = store.newThreadSetup(props.project, id)
  return { provider: id, model: s.model, effort: s.effort, fast: false }
}
function names(id: string): Record<string, string> {
  const out: Record<string, string> = {}
  for (const m of threads.catalogs[id]?.models ?? []) out[m.id === 'default' ? '' : m.id] = m.displayName || m.id
  return out
}
function setupText(s: AgentSetup): string {
  return setupLabel(s.model, s.effort, names(s.provider)) + (s.fast ? ' · fast' : '')
}

// ── Keyboard state ──
const active = ref(Math.max(0, store.adapters.findIndex((a) => a.id === props.provider)))
watch(query, () => (active.value = Math.max(0, agents.value.findIndex((r) => r.state.usable))))
const sub = ref<string | null>(null)
const subQuery = ref('')
const subActive = ref(0)
/** -1 = the model itself, 0.. = its effort chips, then the fast chip. */
const subCol = ref(-1)
const search = ref<HTMLInputElement | null>(null)
const subSearch = ref<HTMLInputElement | null>(null)

// ── Catalogs, on demand ──
const loading = ref<Record<string, boolean>>({})
const failed = ref<Record<string, boolean>>({})
async function loadCatalog(id: string, refresh = false) {
  if (!refresh && threads.catalogs[id]) return
  loading.value = { ...loading.value, [id]: true }
  const c = await threads.catalog(id, refresh)
  loading.value = { ...loading.value, [id]: false }
  failed.value = { ...failed.value, [id]: !c }
}

// ── Recently used models (per machine) ──
const RECENT_KEY = 'qp-recent-models'
function readRecent(): RecentModels {
  try {
    return JSON.parse(localStorage.getItem(RECENT_KEY) ?? '{}') as RecentModels
  } catch {
    return {}
  }
}
const recent = ref<RecentModels>(readRecent())
function remember(s: AgentSetup) {
  recent.value = pushRecent(recent.value, s.provider, s.model)
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(recent.value))
  } catch {
    /* private mode */
  }
}

// ── The submenu's models ──
const subAgent = computed(() => agents.value.find((r) => r.a.id === sub.value) ?? null)
const subModels = computed<ModelInfo[]>(() => (sub.value ? threads.catalogs[sub.value]?.models ?? [] : []))
const subRows = computed(() => {
  const rows = modelRows(subModels.value, subQuery.value)
  if (subQuery.value.trim()) return { recent: [] as ModelRow[], rest: rows }
  return orderModels(rows, recent.value[sub.value ?? ''] ?? [])
})
const flatRows = computed(() => [...subRows.value.recent, ...subRows.value.rest])
function infoOf(id: string): ModelInfo | undefined {
  return subModels.value.find((m) => (m.id === 'default' ? '' : m.id) === id)
}
function effortsOf(id: string): string[] {
  return effortsFor(subModels.value, id)
}
const isCurrent = (id: string) => sub.value === props.provider && props.model !== undefined && id === props.model
const inMulti = (id: string) => props.multi.some((m) => m.provider === sub.value && m.model === id)
/** The effort a model takes when its row (not a chip) is picked: the current one if it has it. */
function carriedEffort(id: string): string {
  const want = sub.value === props.provider ? props.effort ?? '' : setupOf(sub.value ?? '').effort
  return want && effortsOf(id).includes(want) ? want : ''
}
function chipsOf(id: string): { kind: 'effort' | 'fast'; value: string }[] {
  const out: { kind: 'effort' | 'fast'; value: string }[] = effortsOf(id).map((e) => ({ kind: 'effort', value: e }))
  if (infoOf(id)?.fast) out.push({ kind: 'fast', value: 'fast' })
  return out
}
/** Effort chips stay on one line: short labels, the full word in the tooltip. */
const SHORT: Record<string, string> = { minimal: 'min', medium: 'med', xhigh: 'xhigh' }
function chipOn(id: string, chip: { kind: 'effort' | 'fast'; value: string }): boolean {
  if (!isCurrent(id)) return false
  return chip.kind === 'fast' ? props.fast : (props.effort ?? '') === chip.value
}

function openSub(id: string, focus = false) {
  const row = agents.value.find((r) => r.a.id === id)
  if (!row?.state.usable) {
    sub.value = null
    return
  }
  if (sub.value !== id) {
    sub.value = id
    subQuery.value = ''
    subCol.value = -1
    void loadCatalog(id)
  }
  void nextTick(async () => {
    const cur = flatRows.value.findIndex((r) => isCurrent(r.id))
    subActive.value = Math.max(0, cur)
    place()
    // Focus once it is placed: a hidden element cannot take focus.
    await nextTick()
    if (focus) subSearch.value?.focus()
  })
}
function closeSub() {
  sub.value = null
  void nextTick(() => search.value?.focus())
}

// ── Picking ──
function pick(s: AgentSetup) {
  remember(s)
  emit('pick', s)
}
function pickAgent(i: number) {
  const r = agents.value[i]
  if (!r) return
  if (!r.state.usable) {
    if (r.state.fix) fix(r.a.id, r.state.reason)
    return
  }
  pick(r.setup)
}
function pickModel(id: string, chip?: { kind: 'effort' | 'fast'; value: string }) {
  if (!sub.value) return
  const current = isCurrent(id)
  let effort = carriedEffort(id)
  let fast = Boolean(infoOf(id)?.fast) && (sub.value === props.provider ? props.fast : false)
  if (chip?.kind === 'effort') effort = current && (props.effort ?? '') === chip.value ? '' : chip.value
  if (chip?.kind === 'fast') fast = !(current && props.fast)
  pick({ provider: sub.value, model: id, effort, fast })
}
/** Shift: several models for a new thread's first message (card 10). */
function toggleMulti(id: string) {
  if (!props.allowMulti || !sub.value) return false
  const list = [...props.multi]
  if (!list.length && props.provider) list.push({ provider: props.provider, model: props.model ?? '' })
  const at = list.findIndex((m) => m.provider === sub.value && m.model === id)
  if (at >= 0) list.splice(at, 1)
  else list.push({ provider: sub.value, model: id })
  emit('multi', list)
  return true
}
function onModelClick(e: MouseEvent, id: string) {
  if (e.shiftKey && toggleMulti(id)) return
  pickModel(id)
}

/** Settings → Providers on the agent (or the project's settings for "Off in this project"). */
function fix(id: string, reason: string | null) {
  store.openAgentSettings(id, reason === 'Off in this project' ? props.project : null)
  emit('close')
}
function openSettings() {
  const r = agents.value[active.value]
  store.openAgentSettings(r?.a.id ?? props.provider ?? undefined)
  emit('close')
}

// ── Keys ──
function onKey(e: KeyboardEvent) {
  const n = agents.value.length
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    if (!n) return
    active.value = (active.value + (e.key === 'ArrowDown' ? 1 : -1) + n) % n
    if (sub.value && agents.value[active.value]?.a.id !== sub.value) sub.value = null
  } else if (e.key === 'ArrowRight' && (search.value?.selectionStart ?? 0) >= query.value.length) {
    const r = agents.value[active.value]
    if (r?.state.usable) {
      e.preventDefault()
      openSub(r.a.id, true)
    }
  } else if (e.key === 'Enter') {
    e.preventDefault()
    pickAgent(active.value)
  } else if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    if (sub.value) sub.value = null
    else emit('close')
  }
}
function onSubKey(e: KeyboardEvent) {
  const rows = flatRows.value
  const row = rows[subActive.value]
  const chips = row ? chipsOf(row.id) : []
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault()
    if (!rows.length) return
    subActive.value = Math.max(0, Math.min(rows.length - 1, subActive.value + (e.key === 'ArrowDown' ? 1 : -1)))
    subCol.value = Math.min(subCol.value, chipsOf(rows[subActive.value]!.id).length - 1)
  } else if (e.key === 'ArrowRight' && (subSearch.value?.selectionStart ?? 0) >= subQuery.value.length) {
    if (subCol.value < chips.length - 1) {
      e.preventDefault()
      subCol.value++
    }
  } else if (e.key === 'ArrowLeft' && (subSearch.value?.selectionStart ?? 0) === 0) {
    e.preventDefault()
    if (subCol.value >= 0) subCol.value--
    else closeSub()
  } else if (e.key === 'Enter') {
    e.preventDefault()
    if (!row) return
    if (e.shiftKey && toggleMulti(row.id)) return
    pickModel(row.id, subCol.value >= 0 ? chips[subCol.value] : undefined)
  } else if ((e.ctrlKey || e.metaKey) && /^[1-9]$/.test(e.key)) {
    e.preventDefault()
    const r = rows[Number(e.key) - 1]
    if (r) pickModel(r.id)
  } else if (e.key === 'Escape') {
    e.preventDefault()
    e.stopPropagation()
    closeSub()
  }
}

// ── Hover opens the submenu after a moment, like a menu bar ──
let hoverTimer: ReturnType<typeof setTimeout> | null = null
function onRowEnter(i: number) {
  active.value = i
  if (hoverTimer) clearTimeout(hoverTimer)
  const r = agents.value[i]
  hoverTimer = setTimeout(() => {
    if (r?.state.usable) openSub(r.a.id)
    else sub.value = null
  }, 140)
}

// ── Placement ──
const menuEl = ref<HTMLElement | null>(null)
const subEl = ref<HTMLElement | null>(null)
const menuStyle = ref<Record<string, string>>({ visibility: 'hidden' })
const subStyle = ref<Record<string, string>>({ visibility: 'hidden' })
/** The height without scrolling: the box minus its list's visible part plus the list's full height. */
function naturalHeight(el: HTMLElement): number {
  const list = el.querySelector<HTMLElement>('.qp-am-list')
  return list ? el.offsetHeight - list.clientHeight + list.scrollHeight : el.scrollHeight
}
function place() {
  const view = { width: window.innerWidth, height: window.innerHeight }
  if (menuEl.value && props.anchor) {
    const a = props.anchor.getBoundingClientRect()
    const size = { width: menuEl.value.offsetWidth, height: naturalHeight(menuEl.value) }
    const p = placeMenu({ left: a.left, top: a.top, width: a.width, height: a.height }, size, view, props.prefer)
    menuStyle.value = { left: `${p.left}px`, top: `${p.top}px`, maxHeight: `${p.maxHeight}px` }
  }
  if (subEl.value && menuEl.value && sub.value) {
    const rowEl = menuEl.value.querySelector<HTMLElement>(`[data-agent-row="${CSS.escape(sub.value)}"]`)
    const m = menuEl.value.getBoundingClientRect()
    const r = rowEl?.getBoundingClientRect() ?? m
    const p = placeSubmenu({ left: r.left, top: r.top, width: r.width, height: r.height }, { left: m.left, top: m.top, width: m.width, height: m.height }, { width: subEl.value.offsetWidth, height: naturalHeight(subEl.value) }, view)
    subStyle.value = { left: `${p.left}px`, top: `${p.top}px`, maxHeight: `${p.maxHeight}px` }
  }
}
watch([() => agents.value.length, flatRows, sub], () => void nextTick(place))

function onDocDown(e: PointerEvent) {
  const t = e.target as Node
  if (menuEl.value?.contains(t) || subEl.value?.contains(t) || props.anchor?.contains(t)) return
  emit('close')
}
function onResize() {
  place()
}
onMounted(() => {
  document.addEventListener('pointerdown', onDocDown, true)
  window.addEventListener('resize', onResize)
  if (!store.status) void store.refreshStatus()
  // The rows' setups name models: the catalogs of the usable agents load in the background (cached).
  for (const r of agents.value) if (r.state.usable && (r.setup.model || r.a.id === props.provider)) void loadCatalog(r.a.id)
  void nextTick(async () => {
    place()
    await nextTick()
    search.value?.focus()
  })
})
onBeforeUnmount(() => {
  document.removeEventListener('pointerdown', onDocDown, true)
  window.removeEventListener('resize', onResize)
  if (hoverTimer) clearTimeout(hoverTimer)
})
</script>

<template>
  <Teleport to="body">
    <div ref="menuEl" class="qp-am" role="menu" :aria-label="title" :style="menuStyle" data-agent-menu>
      <slot name="header" />
      <div class="qp-am-search">
        <Search :size="13" />
        <input ref="search" v-model="query" placeholder="Search agents" spellcheck="false" aria-label="Search agents" @keydown="onKey" />
      </div>
      <div class="qp-am-list">
        <div
          v-for="(r, i) in agents"
          :key="r.a.id"
          class="qp-am-row"
          role="menuitem"
          :aria-disabled="!r.state.usable"
          :aria-haspopup="r.state.usable ? 'menu' : undefined"
          :aria-expanded="r.state.usable ? sub === r.a.id : undefined"
          :data-agent-row="r.a.id"
          :class="{ active: i === active, off: !r.state.usable, open: sub === r.a.id }"
          @mouseenter="onRowEnter(i)"
          @click="pickAgent(i)"
        >
          <PilotAgentIcon :agent="r.a.id" :size="16" decorative />
          <span class="qp-am-name">{{ r.a.label }}</span>
          <Check v-if="r.a.id === provider && r.state.usable" class="qp-am-check" :size="13" />
          <template v-if="r.state.usable">
            <span class="qp-am-setup" :title="setupText(r.setup)">{{ setupText(r.setup) }}</span>
            <button class="qp-am-more" tabindex="-1" :aria-label="`${r.a.label}: models and effort`" @click.stop="openSub(r.a.id, true)">
              <ChevronRight :size="13" />
            </button>
          </template>
          <template v-else>
            <span class="qp-am-reason" :title="r.state.reason ?? undefined">{{ r.state.reason }}</span>
            <button v-if="r.state.fix" class="qp-am-fix" tabindex="-1" @click.stop="fix(r.a.id, r.state.reason)">{{ r.state.fix }}</button>
          </template>
        </div>
        <div v-if="!agents.length" class="qp-am-empty">No agent matches “{{ query }}”.</div>
      </div>
      <slot name="footer" />
      <button class="qp-am-foot" role="menuitem" @click="openSettings">
        <Settings2 :size="13" />
        <span>Agent settings…</span>
      </button>
    </div>

    <div v-if="sub && subAgent" ref="subEl" class="qp-am qp-am-sub" role="menu" :aria-label="`${subAgent.a.label} models`" :style="subStyle" data-agent-submenu>
      <div class="qp-am-search">
        <Search :size="13" />
        <input ref="subSearch" v-model="subQuery" placeholder="Search models or type an id" spellcheck="false" aria-label="Search models" @keydown="onSubKey" />
        <button class="qp-am-refresh" title="Ask the CLI again" @click="loadCatalog(sub!, true)">
          <Loader2 v-if="loading[sub]" class="qp-am-spin" :size="13" />
          <RefreshCw v-else :size="13" />
        </button>
      </div>
      <div class="qp-am-list">
        <div v-if="loading[sub] && !subModels.length" class="qp-am-empty">Asking {{ subAgent.a.label }} for its models…</div>
        <div v-else-if="failed[sub] && !flatRows.length" class="qp-am-empty">The CLI did not report its models. You can still type a model id.</div>
        <template v-for="(group, g) in [subRows.recent, subRows.rest]" :key="g">
          <div v-if="g === 0 && group.length" class="qp-am-group">Recent</div>
          <div v-else-if="g === 1 && subRows.recent.length && group.length" class="qp-am-group">All models</div>
          <div
            v-for="(m, j) in group"
            :key="m.id + (m.custom ? ':custom' : '')"
            class="qp-am-model"
            role="menuitemradio"
            :aria-checked="isCurrent(m.id)"
            :class="{ active: (g === 0 ? j : subRows.recent.length + j) === subActive }"
            @mouseenter="(subActive = g === 0 ? j : subRows.recent.length + j), (subCol = -1)"
            @click="(e: MouseEvent) => onModelClick(e, m.id)"
          >
            <span class="qp-am-mcheck">
              <Check v-if="multi.length > 1 ? inMulti(m.id) : isCurrent(m.id)" :size="13" />
            </span>
            <span class="qp-am-mtext">
              <span class="qp-am-mlabel">{{ m.label }}</span>
              <span v-if="m.detail" class="qp-am-mdetail">{{ m.detail }}</span>
            </span>
            <span v-if="chipsOf(m.id).length" class="qp-am-chips">
              <button
                v-for="(c, k) in chipsOf(m.id)"
                :key="c.value"
                class="qp-am-chip"
                tabindex="-1"
                :class="{ on: chipOn(m.id, c), focus: (g === 0 ? j : subRows.recent.length + j) === subActive && k === subCol, fast: c.kind === 'fast' }"
                :title="c.kind === 'fast' ? 'Fast mode: faster output at a higher price (Claude Code)' : `${m.label} with ${c.value} effort`"
                @click.stop="pickModel(m.id, c)"
              >
                {{ c.kind === 'fast' ? 'Fast' : SHORT[c.value] ?? c.value }}
              </button>
            </span>
          </div>
        </template>
      </div>
      <div v-if="multi.length > 1" class="qp-am-note">{{ multi.length }} models — sending starts one thread and worktree each</div>
      <div v-else-if="allowMulti" class="qp-am-note">Shift-click to send to several models at once</div>
      <div v-else-if="threads.catalogs[sub]?.version" class="qp-am-note">{{ subAgent.a.label }} {{ threads.catalogs[sub]?.version }}</div>
    </div>
  </Teleport>
</template>

<style scoped>
.qp-am {
  position: fixed;
  z-index: 1200;
  display: flex;
  flex-direction: column;
  width: 340px;
  border-radius: 10px;
  border: 1px solid var(--qss-border, #47474f);
  background: var(--qss-bg-raised, #1f1f25);
  box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
  color: var(--qss-text, #d4d4d8);
  font-family: var(--qss-font-sans, system-ui, sans-serif);
  overflow: hidden;
  animation: qp-am-in 0.14s ease-out;
}
.qp-am-sub { width: 460px; }
@keyframes qp-am-in {
  from { opacity: 0; transform: translateY(3px); }
  to { opacity: 1; transform: none; }
}
@media (prefers-reduced-motion: reduce) {
  .qp-am { animation: none; }
}
.qp-am-search {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 7px 10px;
  border-bottom: 1px solid var(--qss-border-subtle, #35353d);
  color: var(--qss-text-muted, #6e6e7a);
  flex-shrink: 0;
}
.qp-am-search input { flex: 1; min-width: 0; border: none; outline: none; background: transparent; color: var(--qss-text, #d4d4d8); font: inherit; font-size: 12.5px; }
.qp-am-refresh { display: inline-flex; border: none; background: transparent; color: var(--qss-text-muted, #6e6e7a); padding: 3px; border-radius: 5px; cursor: pointer; }
.qp-am-refresh:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-am-list { flex: 1 1 auto; min-height: 0; overflow-y: auto; padding: 4px; }
.qp-am-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-height: 30px;
  padding: 4px 6px 4px 8px;
  border-radius: 6px;
  font-size: 12.5px;
  cursor: pointer;
}
.qp-am-row.active,
.qp-am-row.open { background: var(--qss-bg-hover, #313139); }
.qp-am-row.off { cursor: default; }
.qp-am-row.off > :deep(.qp-aicon) { opacity: 0.45; filter: grayscale(1); }
.qp-am-row.off .qp-am-name { color: var(--qss-text-muted, #6e6e7a); }
.qp-am-name { flex-shrink: 0; max-width: 140px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-am-check { flex-shrink: 0; color: var(--qss-accent, #7aa2f7); }
.qp-am-setup,
.qp-am-reason { flex: 1; min-width: 0; text-align: right; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-am-reason { color: color-mix(in srgb, var(--qss-warning, #d29a3f) 80%, var(--qss-text-muted, #6e6e7a)); }
.qp-am-more,
.qp-am-fix {
  flex-shrink: 0;
  display: inline-flex;
  align-items: center;
  border: none;
  border-radius: 5px;
  background: transparent;
  color: var(--qss-text-muted, #6e6e7a);
  font: inherit;
  font-size: 11px;
  cursor: pointer;
}
.qp-am-more { padding: 3px; }
.qp-am-fix { padding: 2px 6px; border: 1px solid var(--qss-border-subtle, #35353d); color: var(--qss-text-secondary, #9a9aa5); }
.qp-am-more:hover,
.qp-am-fix:hover { background: var(--qss-bg-card, #292930); color: var(--qss-text, #d4d4d8); }
.qp-am-empty { padding: 14px 10px; font-size: 12px; color: var(--qss-text-muted, #6e6e7a); }
.qp-am-group { padding: 6px 8px 2px; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); }
.qp-am-foot {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
  padding: 7px 12px;
  border: none;
  border-top: 1px solid var(--qss-border-subtle, #35353d);
  background: transparent;
  color: var(--qss-text-secondary, #9a9aa5);
  font: inherit;
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}
.qp-am-foot:hover { background: var(--qss-bg-hover, #313139); color: var(--qss-text, #d4d4d8); }
.qp-am-model { display: flex; align-items: center; gap: 6px; padding: 5px 6px 5px 4px; border-radius: 6px; cursor: pointer; }
.qp-am-model.active { background: var(--qss-bg-hover, #313139); }
.qp-am-mcheck { width: 16px; flex-shrink: 0; display: inline-flex; justify-content: center; color: var(--qss-accent, #7aa2f7); }
.qp-am-mtext { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
.qp-am-mlabel { font-size: 12.5px; }
.qp-am-mdetail { font-size: 11px; color: var(--qss-text-muted, #6e6e7a); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.qp-am-chips { display: inline-flex; justify-content: flex-end; gap: 2px; flex-shrink: 0; }
.qp-am-chip {
  padding: 0 5px;
  border: 1px solid var(--qss-border-subtle, #35353d);
  border-radius: 999px;
  background: transparent;
  color: var(--qss-text-secondary, #9a9aa5);
  font: inherit;
  font-size: 11px;
  line-height: 16px;
  cursor: pointer;
}
.qp-am-chip:hover,
.qp-am-chip.focus { border-color: var(--qss-border, #47474f); color: var(--qss-text, #d4d4d8); background: var(--qss-bg-card, #292930); }
.qp-am-chip.on { border-color: color-mix(in srgb, var(--qss-accent, #7aa2f7) 70%, transparent); color: var(--qss-text, #d4d4d8); background: color-mix(in srgb, var(--qss-accent, #7aa2f7) 18%, transparent); }
.qp-am-chip.fast.on { border-color: color-mix(in srgb, var(--qss-warning, #d29a3f) 70%, transparent); background: color-mix(in srgb, var(--qss-warning, #d29a3f) 16%, transparent); }
.qp-am-note { flex-shrink: 0; padding: 5px 10px; font-size: 11px; color: var(--qss-text-muted, #6e6e7a); border-top: 1px solid var(--qss-border-subtle, #35353d); }
.qp-am-spin { animation: qp-am-spin 0.9s linear infinite; }
@keyframes qp-am-spin { to { transform: rotate(360deg); } }
</style>
