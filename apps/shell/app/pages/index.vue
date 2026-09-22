<script setup lang="ts">
import {
  apps,
  isAppEnabled,
  isModuleEnabled,
  bus,
  inActiveKeepAliveTree,
  modulesForApp,
  qs,
  samePath,
} from '@quantsuite/core'
import { logoFor } from '@quantsuite/ui/logos'
import {
  ArrowUpRight,
  ArrowRight,
  Search,
  X,
  Pin,
  Folder,
  FolderOpen,
  Plus,
  Settings2,
  FilePenLine,
  ListTodo,
  PanelsTopLeft,
  Activity,
  RefreshCw,
  Command,
} from 'lucide-vue-next'

const router = useRouter()
const ws = useWorkspaces()
const query = ref('')
const opening = ref(false)
const now = ref(new Date())
const runningProcesses = useState<number>('qss-running-processes', () => 0)
const counts = ref<Record<string, number | null>>({})
const loading = ref(true)
const countsError = ref(false)
const appDetails = {
  quantzen: {
    label: 'Clarity, every day.',
    description: 'Capture ideas. Build routines. Plan ahead.',
  },
  quantview: {
    label: 'Find your edge.',
    description: 'Explore markets, research and strategies.',
  },
  quantagent: {
    label: 'Make room for more.',
    description: 'Connect tools, knowledge and your agents.',
  },
  quantspace: {
    label: 'Bring ideas to life.',
    description: 'Your space to code, create and experiment.',
  },
}
const groups = computed(() =>
  apps
    .filter((a) => a.id in appDetails && isAppEnabled(a.id))
    .map((a) => ({
      ...a,
      ...appDetails[a.id as keyof typeof appDetails],
      entries: modulesForApp(a.id).filter(
        (m) => m.status === 'migrated' && !m.ownWindow
      ),
    }))
)
const filteredGroups = computed(() => {
  const term = query.value.trim().toLowerCase()
  return groups.value
    .map((a) => ({
      ...a,
      entries: a.entries.filter((m) =>
        `${a.title} ${m.title} ${m.description}`.toLowerCase().includes(term)
      ),
    }))
    .filter((a) => a.entries.length)
})
const greeting = computed(() =>
  now.value.getHours() < 12
    ? 'Good morning.'
    : now.value.getHours() < 18
      ? 'Good afternoon.'
      : 'Good evening.'
)
const dateLabel = computed(() =>
  now.value.toLocaleDateString('en-GB', {
    weekday: 'long',
    day: 'numeric',
    month: 'long',
  })
)

function emitShell(event: string, detail?: unknown) {
  window.dispatchEvent(new CustomEvent(`qss:${event}`, { detail }))
}
async function openWorkspace(id?: string) {
  if (opening.value || !isAppEnabled('quantspace')) return
  opening.value = true
  try {
    const target = ws.list.value.find((w) => w.id === id)
    const opened = target ? await ws.open({ ...target }) : await ws.openFolder()
    if (opened) await router.push('/code')
  } finally {
    opening.value = false
  }
}
let countRequest: Promise<void> | null = null
function loadCounts() {
  if (countRequest) return countRequest
  loading.value = true
  countsError.value = false
  countRequest = (async () => {
    const results = await Promise.allSettled([
      qs.core.countEntities({ module: 'notes' }),
      qs.core.listEntities({ kind: 'kanban.card', limit: 10_000 }),
      qs.core.countEntities({ module: 'memory' }),
      qs.core.listEntities({ module: 'core', kind: 'workspace', limit: 200 }),
    ])
    counts.value.notes =
      results[0].status === 'fulfilled' ? results[0].value : null
    counts.value.memory =
      results[2].status === 'fulfilled' ? results[2].value : null

    if (results[1].status === 'fulfilled' && results[3].status === 'fulfilled') {
      const visibleBoards = new Set([
        'general',
        ...results[3].value.map((workspace) => workspace.id),
      ])
      counts.value.kanban = results[1].value.filter((card) => {
        const payload = card.payload
        if (!payload || typeof payload !== 'object') return false
        const boardId = (payload as Record<string, unknown>).workspace
        return typeof boardId === 'string' && visibleBoards.has(boardId)
      }).length
    } else {
      counts.value.kanban = null
    }

    if (results.some((result) => result.status === 'rejected')) {
      countsError.value = true
    }
  })().finally(() => {
    loading.value = false
    countRequest = null
  })
  return countRequest
}
function count(key: string) {
  return loading.value ? '…' : (counts.value[key]?.toLocaleString() ?? '—')
}
const offBus: Array<() => void> = []
let refreshTimer: ReturnType<typeof setTimeout> | undefined
let clockTimer: ReturnType<typeof setInterval> | undefined
function queueRefresh() {
  if (refreshTimer) clearTimeout(refreshTimer)
  refreshTimer = setTimeout(() => {
    void loadCounts()
  }, 400)
}
function startLive() {
  if (clockTimer) return
  now.value = new Date()
  void loadCounts()
  clockTimer = setInterval(() => {
    now.value = new Date()
  }, 60_000)
  offBus.push(
    bus.on('core.entity.upserted', queueRefresh),
    bus.on('core.entity.deleted', queueRefresh)
  )
}
function stopLive() {
  offBus.splice(0).forEach((off) => off())
  clearTimeout(refreshTimer)
  clearInterval(clockTimer)
  clockTimer = undefined
}
onMounted(() => {
  if (!ws.ready.value) void ws.load()
  if (inActiveKeepAliveTree()) startLive()
})
onActivated(startLive)
onDeactivated(stopLive)
onUnmounted(stopLive)
</script>

<template>
  <main class="home">
    <div class="home-inner">
      <header class="home-topbar">
        <div class="home-breadcrumb">
          <img src="/quantsuite-icon.png" alt="QuantableX" /><span
            class="home-divider"
            >/</span
          ><span>Home</span>
        </div>
        <div class="home-tools">
          <button class="home-search-trigger" aria-label="Search anything" @click="emitShell('palette')">
            <Search :size="14" /><span>Search anything</span><kbd>Ctrl K</kbd>
          </button>
          <QScreenshotButton />
          <button
            class="home-icon"
            aria-label="Suite settings"
            title="Suite settings"
            @click="emitShell('settings', { module: null })"
          >
            <Settings2 :size="17" />
          </button>
        </div>
      </header>

      <section class="home-hero" aria-labelledby="home-greeting">
        <div class="home-hero-copy">
          <p class="home-eyebrow">YOUR SPACE TO THINK. BUILD. GROW.</p>
          <img
            class="home-wordmark"
            :src="logoFor('QuantSuite')"
            alt="QuantSuite"
          />
          <h1 id="home-greeting">{{ greeting }} <span>Make it count.</span></h1>
          <p class="home-lead">
            Your ideas, markets and projects. All in one place.
          </p>
          <div class="home-hero-actions">
            <button
              v-if="isAppEnabled('quantspace') && ws.active.value"
              class="home-primary"
              @click="router.push('/code')"
            >
              <FolderOpen :size="16" /><span
                >Continue in {{ ws.active.value.name }}</span
              ><ArrowRight :size="16" />
            </button>
            <button
              v-else-if="isAppEnabled('quantspace')"
              class="home-primary"
              :disabled="opening"
              @click="openWorkspace()"
            >
              <Plus :size="16" />Open your first workspace<ArrowRight
                :size="16"
              />
            </button>
            <button
              v-if="isModuleEnabled('notes')"
              class="home-text-button"
              @click="emitShell('drawer', { tab: 'notes', capture: true })"
            >
              <FilePenLine :size="15" />Capture an idea
            </button>
          </div>
        </div>
        <div class="home-hero-art" aria-hidden="true">
          <div class="home-orbit home-orbit-one" />
          <div class="home-orbit home-orbit-two" />
          <div class="home-orbit home-orbit-three" />
          <img src="/quantsuite-icon.png" />
        </div>
        <span class="home-date">{{ dateLabel }}</span>
      </section>

      <div class="home-content">
        <section class="home-explore" aria-labelledby="explore-title">
          <div class="home-section-head">
            <div>
              <p class="home-eyebrow">THE SUITE</p>
              <h2 id="explore-title">What will you work on?</h2>
            </div>
            <label class="home-filter"
              ><Search :size="15" /><input
                v-model="query"
                aria-label="Find a module"
                placeholder="Find a module…" /><button
                v-if="query"
                aria-label="Clear module search"
                @click="query = ''"
              >
                <X :size="14" /></button
            ></label>
          </div>
          <div v-if="filteredGroups.length" class="home-app-grid">
            <article v-for="a in filteredGroups" :key="a.id" class="home-app">
              <button
                class="home-app-heading"
                @click="emitShell('open-app', { app: a.id })"
              >
                <span class="home-app-icon"
                  ><QAppIcon :app-id="a.id" :size="21" /></span
                ><span class="home-app-name"
                  >{{ a.title }}<small>{{ a.label }}</small></span
                ><ArrowUpRight class="home-app-arrow" :size="18" />
              </button>
              <p class="home-app-description">{{ a.description }}</p>
              <div class="home-module-list">
                <div v-for="m in a.entries" :key="m.id" class="home-module">
                  <button
                    class="home-module-link"
                    @click="router.push(m.route)"
                  >
                    <span>{{ m.title.replace(/^Quant/, '') }}</span
                    ><ArrowUpRight :size="13" />
                  </button>
                </div>
              </div>
            </article>
          </div>
          <div v-else class="home-empty">
            <Search :size="24" />
            <h3>No modules found</h3>
            <p>Try a module name, app or topic.</p>
            <button class="home-text-button" @click="query = ''">
              Clear search
            </button>
          </div>
          <div class="home-quick-actions" aria-label="Quick actions">
            <button v-if="isModuleEnabled('notes')"
              @click="emitShell('drawer', { tab: 'notes', capture: true })"
            >
              <FilePenLine :size="17" /><span
                >Quick note<small>Save a thought</small></span
              ><Plus :size="15" /></button
            ><button v-if="isModuleEnabled('flow')" @click="emitShell('drawer', { tab: 'todos' })">
              <ListTodo :size="17" /><span
                >To-dos<small>Find your next step</small></span
              ><ArrowUpRight :size="15" /></button
            ><button v-if="isModuleEnabled('mcp')" @click="emitShell('drawer', { tab: 'kanban' })">
              <PanelsTopLeft :size="17" /><span
                >Kanban<small>See the bigger picture</small></span
              ><ArrowUpRight :size="15" />
            </button>
          </div>
        </section>

        <aside class="home-sidebar">
          <section v-if="isAppEnabled('quantspace')" class="home-workspaces" aria-labelledby="workspaces-title">
            <div class="home-section-head">
              <h2 id="workspaces-title">
                Workspaces
                <span class="home-count">{{ ws.list.value.length }}</span>
              </h2>
              <button
                class="home-icon"
                aria-label="Open folder"
                title="Open folder"
                :disabled="opening"
                @click="openWorkspace()"
              >
                <Plus :size="17" />
              </button>
            </div>
            <p class="home-caption">Pick up where you left off.</p>
            <p v-if="ws.error.value" class="home-error" role="alert">
              {{ ws.error.value }}
            </p>
            <ul v-if="ws.sorted.value.length" class="home-workspace-list">
              <li
                v-for="w in ws.sorted.value"
                :key="w.id"
                :class="{
                  'is-active': samePath(w.path, ws.active.value?.path),
                }"
              >
                <button
                  class="home-workspace"
                  :title="w.path"
                  :disabled="opening"
                  @click="openWorkspace(w.id)"
                >
                  <span class="home-folder"><Folder :size="18" /></span
                  ><span class="home-workspace-info"
                    ><strong>{{ w.name }}</strong
                    ><small>{{ w.path }}</small></span
                  ><span
                    v-if="samePath(w.path, ws.active.value?.path)"
                    class="home-active-dot"
                    title="Current workspace"
                    aria-label="Current workspace"
                  /></button
                ><button
                  class="home-pin"
                  :class="{ 'is-pinned': w.pinned }"
                  :aria-label="`${w.pinned ? 'Unpin' : 'Pin'} workspace ${w.name}`"
                  :aria-pressed="w.pinned"
                  @click="ws.togglePin(w.id)"
                >
                  <Pin :size="13" /></button
                ><button
                  class="home-remove"
                  :aria-label="`Remove ${w.name} from recent workspaces`"
                  title="Remove from list; folder stays on disk"
                  @click="ws.remove(w.id)"
                >
                  <X :size="13" />
                </button>
              </li>
            </ul>
            <p v-else class="home-workspace-empty">
              {{
                ws.ready.value
                  ? 'Open a project folder to make yourself at home.'
                  : 'Loading workspaces…'
              }}
            </p>
            <button
              class="home-open-folder"
              :disabled="opening"
              @click="openWorkspace()"
            >
              <FolderOpen :size="15" />{{ opening ? 'Opening…' : 'Open folder'
              }}<span>+</span>
            </button>
          </section>
          <section class="home-overview" aria-labelledby="overview-title">
            <div class="home-section-head">
              <h2 id="overview-title">At a glance</h2>
              <button
                class="home-icon"
                :disabled="loading"
                aria-label="Refresh overview"
                title="Refresh overview"
                @click="loadCounts"
              >
                <RefreshCw :size="14" :class="{ 'home-spinning': loading }" />
              </button>
            </div>
            <div class="home-stat-grid" :aria-busy="loading">
              <button v-if="isModuleEnabled('notes')" @click="router.push('/notes')">
                <strong>{{ count('notes') }}</strong
                ><span>Note items<ArrowUpRight :size="12" /></span></button
              ><button v-if="isModuleEnabled('mcp')" @click="emitShell('drawer', { tab: 'kanban' })">
                <strong>{{ count('kanban') }}</strong
                ><span>Kanban cards<ArrowUpRight :size="12" /></span></button
              ><button v-if="isModuleEnabled('memory')" @click="router.push('/memory')">
                <strong>{{ count('memory') }}</strong
                ><span>Memories<ArrowUpRight :size="12" /></span></button
              ><button @click="router.push('/processes')">
                <strong>{{ runningProcesses }}</strong
                ><span>Processes<ArrowUpRight :size="12" /></span>
              </button>
            </div>
            <p v-if="countsError" class="home-data-note" role="status">
              Some data is unavailable. Refresh to try again.
            </p>
            <button
              class="home-process-link"
              @click="router.push('/processes')"
            >
              <Activity :size="14" /><span>{{
                runningProcesses
                  ? `${runningProcesses} processes running`
                  : 'No active processes'
              }}</span
              ><ArrowRight :size="14" />
            </button>
          </section>
          <SuiteUpdates />
        </aside>
      </div>
      <footer class="home-footer">
        <span
          ><img src="/quantsuite-icon.png" alt="" />Built for your flow.</span
        ><button @click="emitShell('palette')">
          <Command :size="13" />One shortcut to everything<kbd>Ctrl K</kbd>
        </button>
      </footer>
    </div>
  </main>
</template>

<style scoped>
.home {
  height: 100%;
  overflow-x: hidden;
  overflow-y: auto;
  background: var(--qss-bg);
  color: var(--qss-text);
  font-family: var(--qss-font-sans);
  container-type: size;
}
.home-inner {
  /* Scale against the actual stage, including when the drawer is open. */
  --home-body: clamp(13px, 0.8cqi, 16px);
  --home-small: clamp(12px, 0.7cqi, 14px);
  --home-gap: clamp(14px, 1.6cqh, 24px);
  --home-scale: 1;
  box-sizing: border-box;
  /* Keep cards at their content height; short stages scroll instead of
     compressing grid rows until their controls overlap. */
  min-height: 100%;
  zoom: var(--home-scale);
  display: flex;
  flex-direction: column;
  gap: var(--home-gap);
  padding: clamp(18px, 2.4cqh, 32px) clamp(24px, 3cqi, 64px);
  font-size: var(--home-body);
}
.home button {
  cursor: pointer;
  font: inherit;
  color: inherit;
}
.home button:disabled {
  cursor: wait;
  opacity: 0.55;
}
.home button:focus-visible,
.home input:focus-visible {
  outline: 2px solid var(--qss-text);
  outline-offset: 4px;
}
.home button {
  transition:
    background 140ms ease,
    border-color 140ms ease,
    color 140ms ease;
}
.home-topbar,
.home-breadcrumb,
.home-tools,
.home-search-trigger,
.home-hero-actions,
.home-primary,
.home-text-button,
.home-section-head,
.home-filter,
.home-app-heading,
.home-module,
.home-module-link,
.home-workspace,
.home-open-folder,
.home-process-link,
.home-footer,
.home-footer > span,
.home-footer button {
  display: flex;
  align-items: center;
}
.home-topbar {
  justify-content: space-between;
  gap: 16px;
  flex: none;
}
.home-breadcrumb {
  gap: 16px;
  font-size: var(--home-body);
  font-weight: 500;
}
.home-breadcrumb img {
  width: 24px;
  height: 24px;
  object-fit: contain;
  opacity: 0.8;
}
.home-divider {
  color: var(--qss-text-muted);
}
.home-tools {
  gap: 10px;
}
.home-search-trigger {
  gap: 9px;
  border: 1px solid var(--qss-border);
  background: transparent;
  padding: 7px 9px;
  border-radius: 7px;
  font-size: var(--home-body) !important;
  color: var(--qss-text-secondary) !important;
}
.home kbd {
  font: var(--home-small) var(--qss-font-mono);
  border: 1px solid var(--qss-border);
  border-radius: 4px;
  padding: 2px 5px;
  color: var(--qss-text-secondary);
}
.home-search-trigger kbd {
  margin-left: 32px;
}
.home-icon {
  display: grid;
  place-items: center;
  width: 36px;
  height: 36px;
  border: 0;
  background: transparent;
  border-radius: 6px;
  color: var(--qss-text-secondary) !important;
}
.home-icon:hover,
.home-search-trigger:hover {
  background: var(--qss-bg-hover);
}
.home-hero {
  position: relative;
  flex: none;
  min-height: clamp(240px, 26cqh, 360px);
  display: flex;
  align-items: center;
  overflow: hidden;
  padding: clamp(20px, 2.2cqh, 32px) clamp(28px, 2.5cqi, 52px);
  border: 1px solid var(--qss-border);
  border-radius: 18px;
  background:
    radial-gradient(
      ellipse at 90% 20%,
      color-mix(in srgb, var(--qss-text) 6%, transparent),
      transparent 65%
    ),
    var(--qss-bg-raised);
}
.home-hero-copy {
  min-width: 0;
  position: relative;
  z-index: 1;
}
.home-eyebrow {
  font-size: var(--home-small);
  line-height: 1.5;
  letter-spacing: 0.19em;
  font-weight: 600;
  color: var(--qss-text-secondary);
  margin: 0 0 12px;
}
.home-wordmark {
  display: block;
  height: clamp(36px, 2.8cqi, 56px);
  max-width: 100%;
  object-fit: contain;
  object-position: left;
  width: auto;
  margin-bottom: 12px;
}
.home h1 {
  font-size: clamp(24px, 1.8cqi, 34px);
  line-height: 1.3;
  font-weight: 500;
  letter-spacing: -0.035em;
  margin: 0 0 9px;
}
.home h1 span {
  color: var(--qss-text-secondary);
  font-weight: 400;
}
.home-lead {
  margin: 0;
  color: var(--qss-text-secondary);
  font-size: var(--home-body);
  line-height: 1.6;
}
.home-hero-actions {
  gap: 20px;
  margin-top: 18px;
  flex-wrap: wrap;
}
.home-primary {
  max-width: 100%;
  gap: 10px;
  border: 1px solid var(--qss-accent);
  border-radius: 7px;
  padding: 10px 14px;
  background: var(--qss-accent);
  color: var(--qss-accent-ink, #171717) !important;
  font-size: var(--home-body) !important;
  font-weight: 600 !important;
}
.home-primary span {
  min-width: 0;
  max-width: 290px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.home-primary:hover {
  filter: brightness(1.12);
}
.home-text-button {
  gap: 8px;
  padding: 5px 0;
  background: none;
  border: none;
  font-size: var(--home-body) !important;
  color: var(--qss-text-secondary) !important;
}
.home-text-button:hover {
  color: var(--qss-text) !important;
}
.home-hero-art {
  position: absolute;
  width: clamp(300px, 25cqi, 520px);
  aspect-ratio: 1;
  top: 50%;
  transform: translateY(-50%);
  right: -5px;
  display: grid;
  place-items: center;
  pointer-events: none;
}
.home-hero-art > img {
  width: 32%;
  height: 32%;
  opacity: 0.5;
}
.home-orbit {
  position: absolute;
  border: 1px solid color-mix(in srgb, var(--qss-text) 8%, transparent);
  border-radius: 50%;
}
.home-orbit-one {
  width: 54%;
  height: 54%;
}
.home-orbit-two {
  width: 79%;
  height: 79%;
}
.home-orbit-three {
  width: 103%;
  height: 103%;
}
.home-date {
  position: absolute;
  right: 24px;
  bottom: 18px;
  color: var(--qss-text-secondary);
  font-size: var(--home-small);
}
.home-content {
  flex: 1 0 auto;
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(320px, 25%);
  gap: clamp(24px, 2cqi, 40px);
}
.home-explore {
  min-width: 0;
  display: flex;
  flex-direction: column;
}
.home-section-head {
  justify-content: space-between;
  flex-wrap: wrap;
  gap: 15px;
  margin-bottom: 12px;
}
.home-section-head .home-eyebrow {
  margin-bottom: 6px;
}
.home h2 {
  margin: 0;
  font-size: clamp(20px, 1.3cqi, 26px);
  letter-spacing: -0.025em;
  font-weight: 500;
}
.home-filter {
  width: clamp(170px, 13cqi, 260px);
  max-width: 100%;
  gap: 7px;
  border-bottom: 1px solid var(--qss-border);
  padding: 8px 0;
  color: var(--qss-text-secondary);
}
.home-filter input {
  min-width: 0;
  width: 100%;
  border: none;
  background: none;
  color: var(--qss-text);
  font: var(--home-body) var(--qss-font-sans);
}
.home-filter input::placeholder {
  color: var(--qss-text-secondary);
}
.home-filter button {
  border: 0;
  background: none;
  display: grid;
  padding: 0;
}
.home-app-grid {
  flex: 1 0 auto;
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  grid-auto-rows: 1fr;
  gap: var(--home-gap);
}
.home-app {
  min-width: 0;
  border: 1px solid var(--qss-border);
  border-radius: 11px;
  padding: clamp(18px, 1.1cqi, 26px);
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  background: var(--qss-bg-raised);
  transition: border-color 140ms ease;
}
.home-app:hover {
  border-color: color-mix(in srgb, var(--qss-text) 28%, transparent);
}
.home-app-heading {
  width: 100%;
  gap: 11px;
  background: none;
  border: 0;
  padding: 0;
  text-align: left;
}
.home-app-icon {
  width: 46px;
  height: 46px;
  flex: none;
  display: grid;
  place-items: center;
  background: var(--qss-bg-card);
  border: 1px solid var(--qss-border);
  border-radius: 10px;
  color: var(--qss-text-secondary);
}
.home-app-name {
  min-width: 0;
  font-size: clamp(17px, 1.05cqi, 21px);
  font-weight: 600;
}
.home-app-name small {
  display: block;
  font-size: var(--home-small);
  font-weight: 400;
  color: var(--qss-text-secondary);
  margin-top: 4px;
}
.home-app-arrow {
  margin-left: auto;
  color: var(--qss-text-muted);
}
.home-app-heading:hover .home-app-arrow {
  color: var(--qss-text);
}
.home-app-description {
  color: var(--qss-text-secondary);
  font-size: var(--home-body);
  line-height: 1.6;
  margin: 12px 0;
}
.home-module-list {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}
.home-module {
  border: 1px solid var(--qss-border);
  border-radius: 5px;
  overflow: hidden;
}
.home-module-link {
  gap: 8px;
  border: 0;
  background: transparent;
  padding: 8px 11px;
  font-size: var(--home-body) !important;
}
.home-module-link svg {
  opacity: 0.45;
}
.home-module-link:hover,
.home-pin:hover {
  background: var(--qss-bg-hover);
}
.home-pin {
  display: grid;
  place-items: center;
  width: 25px;
  height: 27px;
  border: 0;
  background: transparent;
  color: var(--qss-text-muted) !important;
  flex: none;
}
.home-pin.is-pinned {
  color: var(--qss-text) !important;
  background: color-mix(in srgb, var(--qss-text) 5%, transparent);
}
.home-pin.is-pinned svg {
  fill: color-mix(in srgb, var(--qss-text) 20%, transparent);
}
.home-quick-actions {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 10px;
  margin-top: 17px;
}
.home-quick-actions button {
  display: flex;
  align-items: center;
  gap: 9px;
  background: transparent;
  border: 1px solid var(--qss-border);
  border-radius: 8px;
  padding: clamp(12px, 1.4cqh, 20px) 14px;
  text-align: left;
}
.home-quick-actions button:hover {
  background: var(--qss-bg-raised);
}
.home-quick-actions button > svg {
  color: var(--qss-text-secondary);
  flex: none;
}
.home-quick-actions button > svg:last-child {
  margin-left: auto;
  width: 12px;
}
.home-quick-actions span {
  font-size: var(--home-body);
}
.home-quick-actions small {
  display: block;
  margin-top: 4px;
  color: var(--qss-text-secondary);
  font-size: var(--home-small);
}
.home-sidebar {
  min-width: 0;
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  border-left: 1px solid var(--qss-border);
  padding-left: 26px;
}
.home-sidebar h2 {
  font-size: clamp(17px, 1.05cqi, 21px);
}
.home-sidebar .home-section-head {
  margin-bottom: 5px;
}
.home-count {
  margin-left: 6px;
  color: var(--qss-text-muted);
  font-size: var(--home-small);
}
.home-caption {
  font-size: var(--home-small);
  color: var(--qss-text-secondary);
  margin: 0 0 17px;
}
.home-workspace-list {
  --home-workspace-row-height: clamp(58px, 7.5cqh, 76px);
  margin: 0;
  padding: 0;
  list-style: none;
  max-height: calc(
    var(--home-workspace-row-height) + var(--home-workspace-row-height) +
      var(--home-workspace-row-height) + 18px
  );
  overflow-x: hidden;
  overflow-y: auto;
}
.home-workspace-list li {
  display: flex;
  align-items: center;
  height: var(--home-workspace-row-height);
  border-radius: 7px;
  margin: 3px 0;
  padding-right: 4px;
}
.home-workspace-list li.is-active {
  background: var(--qss-bg-raised);
}
.home-workspace-list li:hover {
  background: var(--qss-bg-hover);
}
.home-workspace {
  flex: 1;
  height: 100%;
  min-width: 0;
  gap: 9px;
  border: 0;
  background: none;
  padding: clamp(12px, 1.6cqh, 20px) 9px;
  text-align: left;
}
.home-folder {
  display: grid;
  place-items: center;
  color: var(--qss-text-secondary);
  flex: none;
}
.home-workspace-info {
  flex: 1;
  min-width: 0;
}
.home-workspace-info strong {
  display: block;
  font-size: var(--home-body);
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.home-workspace-info small {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--qss-text-secondary);
  font: var(--home-small) var(--qss-font-mono);
  margin-top: 5px;
}
.home-active-dot {
  width: 5px;
  height: 5px;
  background: var(--qss-success);
  border-radius: 50%;
  flex: none;
}
.home-remove {
  display: grid;
  place-items: center;
  background: none;
  border: 0;
  padding: 3px;
  color: var(--qss-text-secondary) !important;
}
.home-open-folder {
  width: 100%;
  margin-top: 12px;
  padding: 10px;
  gap: 9px;
  background: none;
  border: 1px dashed var(--qss-border);
  border-radius: 7px;
  color: var(--qss-text-secondary) !important;
  font-size: var(--home-body) !important;
}
.home-open-folder span {
  margin-left: auto;
}
.home-open-folder:hover {
  border-color: var(--qss-text-muted);
  background: var(--qss-bg-raised);
}
.home-overview {
  margin-top: 18px;
  padding-top: 12px;
  border-top: 1px solid var(--qss-border);
}
.home-stat-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(94px, 1fr));
  gap: 8px;
  margin-top: 9px;
}
.home-stat-grid button {
  text-align: left;
  padding: 10px 7px;
  border: 1px solid var(--qss-border);
  border-radius: 7px;
  background: var(--qss-bg-raised);
}
.home-stat-grid button:hover {
  background: var(--qss-bg-hover);
}
.home-stat-grid strong {
  display: block;
  font-size: clamp(26px, 1.8cqi, 36px);
  font-weight: 400;
  letter-spacing: -0.05em;
}
.home-stat-grid span svg {
  display: none;
}
.home-stat-grid span {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 4px;
  font-size: var(--home-small);
  white-space: nowrap;
  margin-top: 4px;
  color: var(--qss-text-secondary);
}
.home-process-link {
  width: 100%;
  gap: 8px;
  padding: 15px 0 0;
  background: none;
  border: none;
  font-size: var(--home-small) !important;
  color: var(--qss-text-secondary) !important;
}
.home-process-link > svg:last-child {
  margin-left: auto;
}
.home-data-note,
.home-workspace-empty {
  font-size: var(--home-small);
  color: var(--qss-text-secondary);
  line-height: 1.6;
}
.home-error {
  font-size: var(--home-body);
  color: var(--qss-error);
  overflow-wrap: anywhere;
}
.home-empty {
  flex: 1;
  min-height: 240px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  color: var(--qss-text-secondary);
  border: 1px dashed var(--qss-border);
  border-radius: 10px;
}
.home-empty h3 {
  font-size: 18px;
  margin: 12px 0 0;
}
.home-empty p {
  font-size: var(--home-body);
}
.home-footer {
  flex: none;
  justify-content: space-between;
  gap: 16px;
  padding-top: 12px;
  border-top: 1px solid var(--qss-border);
  color: var(--qss-text-muted);
  font-size: var(--home-small);
}
.home-footer > span {
  gap: 8px;
}
.home-footer img {
  width: 16px;
  height: 16px;
  opacity: 0.45;
}
.home-footer button {
  gap: 8px;
  background: none;
  border: none;
  font-size: var(--home-small);
}
.home-spinning {
  animation: home-spin 1s linear infinite;
}
@keyframes home-spin {
  to {
    transform: rotate(360deg);
  }
}
@container (max-height: 1000px) {
  .home-inner {
    --home-gap: 12px;
    padding-block: 8px;
  }
  .home-hero {
    min-height: 0;
    padding-block: 18px;
  }
  .home-hero .home-eyebrow {
    margin-bottom: 10px;
  }
  .home-wordmark {
    height: 36px;
    margin-bottom: 12px;
  }
  .home-hero-actions {
    margin-top: 12px;
  }
  .home-app {
    padding: 14px;
  }
  .home-app-icon {
    width: 36px;
    height: 36px;
  }
  .home-app-description {
    margin-block: 8px;
  }
  .home-module-link {
    padding: 6px 8px;
  }
  .home-quick-actions {
    margin-top: 12px;
  }
  .home-quick-actions button {
    padding: 10px;
  }
  .home-workspace {
    padding-block: 8px;
  }
  .home-workspace-list {
    --home-workspace-row-height: 48px;
  }
  .home-caption {
    margin-bottom: 10px;
  }
  .home-open-folder {
    margin-top: 8px;
    padding-block: 8px;
  }
  .home-overview {
    margin-top: 10px;
    padding-top: 8px;
  }
  .home-stat-grid button {
    padding-block: 6px;
  }
  .home-stat-grid strong {
    font-size: 24px;
  }
  .home-process-link {
    padding-top: 8px;
  }
  .home-footer {
    padding-top: 8px;
  }
}
/* A modest scale keeps the desktop overview together on laptop-height
   stages. Narrow windows reflow below; very short ones can still scroll. */
@container (min-width: 1051px) and (max-height: 850px) {
  .home-inner {
    --home-scale: 0.9;
  }
}
@container (min-width: 1051px) and (max-height: 750px) {
  .home-inner {
    --home-scale: 0.85;
  }
}
@container (max-width: 1600px) {
  .home-stat-grid {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
}
@container (max-width: 1050px) {
  .home-content {
    grid-template-columns: minmax(0, 1fr);
    gap: 20px;
  }
  .home-sidebar {
    border-left: none;
    padding-left: 0;
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 24px;
  }
  .home-overview {
    margin-top: 0;
    padding-top: 0;
    border-top: none;
  }
  .home-inner {
    padding-inline: 28px;
  }
  .home-hero-art {
    right: -85px;
    opacity: 0.65;
  }
  .home-hero {
    padding: 30px;
  }
  .home-quick-actions small {
    display: none;
  }
}
@container (max-width: 800px) {
  .home-hero-art {
    opacity: 0.2;
    right: -100px;
  }
  .home-hero-copy {
    max-width: 100%;
  }
  .home-hero {
    padding: 30px 25px 45px;
  }
}
@container (max-width: 520px) {
  .home-inner {
    padding: 18px;
  }
  .home-search-trigger span,
  .home-search-trigger kbd {
    display: none;
  }
  .home-app-grid,
  .home-sidebar {
    grid-template-columns: 1fr;
  }
  .home-section-head {
    flex-wrap: wrap;
  }
  .home-filter {
    width: 100%;
  }
  .home-wordmark {
    height: 29px;
  }
  .home-hero {
    padding-inline: 18px;
  }
  .home-hero-art {
    display: none;
  }
  .home h1 span {
    display: block;
  }
  .home-quick-actions {
    grid-template-columns: 1fr;
  }
  .home-quick-actions small {
    display: block;
  }
  .home-footer button {
    display: none;
  }
  .home-primary span {
    max-width: 170px;
  }
}
@media (prefers-reduced-motion: reduce) {
  .home button {
    transition: none;
  }
  .home-spinning {
    animation: none;
  }
}
</style>
