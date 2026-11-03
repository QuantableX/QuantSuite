<script setup lang="ts">
/**
 * A browser window on the canvas (rework 2026-11-03).
 *
 * Every tab is a native webview (made and watched by the canvas crate,
 * commands/browser.rs). A native webview is drawn above all HTML, so the
 * window shows the LIVE page only while nothing lies on top of it and the
 * canvas is at rest; otherwise a PICTURE of the page (WebView2's capture)
 * stands in its place in the DOM, below every menu, dialog, drawer and
 * window like any other element. Higher canvas windows, the minimap and what
 * lies outside the visible canvas are cut out of a live page exactly (a
 * window region); anything else on top is found by hit-testing the page's
 * area (utils/browserSurface.ts).
 *
 * Nothing runs per frame: the page follows the canvas on events — the
 * canvas moving (pan, zoom, a window dragged or resized: the picture moves
 * with it and the live page returns ~160 ms after the motion ends), DOM
 * changes and the user's clicks and keys (something may have opened on top),
 * and the tab's own events (address, title, loading, history).
 */
import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { useCanvasStore } from '../../../stores/canvas'
import { useWorkspacesStore } from '../../../stores/workspaces'
import { useBrowserStore } from '../../../stores/browser'
import type { CanvasWindow, BrowserTab } from '../../../shared/types'
import { clipRects, covered, intersect, normalizeAddress, overlaps, probePoints, tabTitle, zoomStep, type Box } from '../../utils/browserSurface'
import BrowserTabBar from './Browser/BrowserTabBar.vue'
import BrowserOmnibox from './Browser/BrowserOmnibox.vue'
import BrowserNavButtons from './Browser/BrowserNavButtons.vue'
import BrowserProgressBar from './Browser/BrowserProgressBar.vue'
import BrowserFindBar from './Browser/BrowserFindBar.vue'
import BrowserBookmarkBar from './Browser/BrowserBookmarkBar.vue'
import BrowserHistoryPanel from './Browser/BrowserHistoryPanel.vue'
import BrowserBookmarkPanel from './Browser/BrowserBookmarkPanel.vue'

const props = defineProps<{
  window: CanvasWindow
}>()

interface TabStatus {
  label: string
  url: string
  title: string
  loading: boolean
  canGoBack: boolean
  canGoForward: boolean
}

const canvasStore = useCanvasStore()
const workspacesStore = useWorkspacesStore()
const browserStore = useBrowserStore()
const workspaceId = computed(() => workspacesStore.contentWorkspaceId!)

/** The page's area: the live page is placed on it, the picture lives in it. */
const contentRef = ref<HTMLElement | null>(null)
const omniboxRef = ref<InstanceType<typeof BrowserOmnibox> | null>(null)

// Sidebar widths inside the canvas (index.vue), the canvas context menu and
// the canvas element (InfinityCanvas).
const viewportInsets = inject<ComputedRef<{ left: number; right: number }>>('viewportInsets', computed(() => ({ left: 0, right: 0 })))
const contextMenuVisible = inject<ComputedRef<boolean>>('contextMenuVisible', computed(() => false))
const canvasEl = inject<Ref<HTMLElement | null>>('canvasRef', ref(null))

// ---- Tabs ----

const tabs = ref<BrowserTab[]>([])
const activeTabId = ref('')
/** What each tab's page reports, by tab id. */
const status = reactive<Record<string, TabStatus>>({})
/** Tabs whose page exists (a tab is opened when it is first shown). */
const opened = reactive(new Set<string>())
const openError = ref('')

const activeTab = computed(() => tabs.value.find((t) => t.id === activeTabId.value))
const activeStatus = computed(() => status[activeTabId.value])
const canGoBack = computed(() => activeStatus.value?.canGoBack ?? false)
const canGoForward = computed(() => activeStatus.value?.canGoForward ?? false)
const loading = computed(() => activeStatus.value?.loading ?? !!activeTab.value?.isLoading)
const currentUrl = computed(() => activeStatus.value?.url || activeTab.value?.url || '')
const urlInput = ref('')

const pageZoom = ref(props.window.browserConfig?.pageZoom ?? 1)
const findBarVisible = ref(false)
const findQuery = ref('')
const historyPanelVisible = ref(false)
const bookmarksPanelVisible = ref(false)
/** A video in the page went fullscreen: the page covers the suite window. */
const isContentFullscreen = ref(false)

function labelOf(tabId: string): string {
  return `browser-${props.window.id.replace(/[^a-zA-Z0-9_-]/g, '_')}-${tabId.replace(/[^a-zA-Z0-9_-]/g, '_')}`
}

function tabOfLabel(label: string): string | undefined {
  return tabs.value.find((t) => labelOf(t.id) === label)?.id
}

const SEARCH: Record<string, (q: string) => string> = {
  google: (q) => `https://www.google.com/search?q=${encodeURIComponent(q)}`,
  duckduckgo: (q) => `https://duckduckgo.com/?q=${encodeURIComponent(q)}`,
  bing: (q) => `https://www.bing.com/search?q=${encodeURIComponent(q)}`,
  brave: (q) => `https://search.brave.com/search?q=${encodeURIComponent(q)}`,
}
function address(input: string): string {
  return normalizeAddress(input, SEARCH[browserStore.searchEngine] ?? SEARCH.google)
}

// ---- Saved with the canvas ----

let persistTimer: ReturnType<typeof setTimeout> | null = null
function persist(): void {
  if (persistTimer) clearTimeout(persistTimer)
  persistTimer = setTimeout(() => {
    persistTimer = null
    canvasStore.updateWindow(workspaceId.value, props.window.id, {
      browserConfig: {
        ...props.window.browserConfig,
        url: activeTab.value?.url ?? '',
        tabs: tabs.value.map((t) => ({ id: t.id, url: t.url, title: t.title, isLoading: false })),
        activeTabId: activeTabId.value,
        pageZoom: pageZoom.value,
      },
    })
  }, 400)
}

watch(currentUrl, (url) => {
  const omnibox = (omniboxRef.value?.$el as HTMLElement | undefined)?.querySelector('input')
  if (document.activeElement !== omnibox) urlInput.value = url
}, { immediate: true })

// The window's title: the page's.
watch(() => (activeTab.value ? tabTitle(activeStatus.value?.title ?? activeTab.value.title, currentUrl.value) : ''), (name) => {
  if (!name) return
  const title = `Browser - ${name}`
  if (props.window.title !== title) canvasStore.updateWindow(workspaceId.value, props.window.id, { title })
})

// ---- The tab's page: events from the canvas crate ----

function applyStatus(tabId: string, next: TabStatus): void {
  const before = status[tabId]
  status[tabId] = next
  const tab = tabs.value.find((t) => t.id === tabId)
  if (tab) {
    const url = next.url || tab.url
    const title = tabTitle(next.title, url)
    if (tab.url !== url || tab.title !== title) {
      tab.url = url
      tab.title = title
      persist()
    }
    tab.isLoading = next.loading
  }
  // A page finished loading: into the history, and a fresh picture of it.
  if (before?.loading && !next.loading && next.url && !next.url.startsWith('about:')) {
    browserStore.addHistoryEntry({ url: next.url, title: tabTitle(next.title, next.url), visitedAt: new Date().toISOString() })
    if (tabId === activeTabId.value) setTimeout(capture, 600)
  }
}

const pendingCloses = ((globalThis as unknown as Record<string, unknown>).__qcBrowserPendingCloses ??= new Map()) as Map<string, Promise<unknown>>

/** Make (or reconnect to) a tab's page, off screen; `sync` shows it. */
async function openTab(tabId: string, url: string): Promise<void> {
  const label = labelOf(tabId)
  const closing = pendingCloses.get(label)
  if (closing) await closing
  const r = contentRef.value?.getBoundingClientRect()
  const bounds = r && r.width > 0 ? { x: Math.round(r.left), y: Math.round(r.top), width: Math.round(r.width), height: Math.round(r.height) } : { x: 0, y: 0, width: 800, height: 600 }
  try {
    const st = await invoke<TabStatus>('plugin:canvas|browser_tab_open', { label, url, bounds, zoom: scale() * pageZoom.value, visible: false })
    if (!tabs.value.some((t) => t.id === tabId)) {
      // Closed while it was opening.
      closePage(tabId)
      return
    }
    opened.add(tabId)
    openError.value = ''
    // Events that came while it opened are newer than this answer.
    if (!status[tabId]) applyStatus(tabId, st)
  } catch (e) {
    if (tabId === activeTabId.value) openError.value = String(e)
  }
  requestSync()
}

/** The tab's page goes — its media and process with it. */
function closePage(tabId: string): void {
  const label = labelOf(tabId)
  opened.delete(tabId)
  delete status[tabId]
  const closing: Promise<unknown> = invoke('plugin:canvas|browser_tab_close', { label })
    .catch(() => {})
    .finally(() => {
      if (pendingCloses.get(label) === closing) pendingCloses.delete(label)
    })
  pendingCloses.set(label, closing)
}

/** Step the page aside (its picture stays). */
function hidePage(tabId: string): void {
  if (!opened.has(tabId)) return
  invoke('plugin:canvas|browser_tab_place', { label: labelOf(tabId), bounds: { x: 0, y: 0, width: 1, height: 1 }, zoom: 1, visible: false }).catch(() => {})
}

let nextTabNum = 1
async function addTab(url?: string): Promise<void> {
  const tabId = `tab-${Date.now()}-${nextTabNum++}`
  const target = address(url ?? 'https://www.google.com')
  tabs.value.push({ id: tabId, url: target, title: tabTitle('', target), isLoading: true })
  await switchTab(tabId)
  persist()
}

async function switchTab(tabId: string): Promise<void> {
  if (tabId === activeTabId.value && opened.has(tabId)) return
  const before = activeTabId.value
  if (before && before !== tabId) {
    hidePage(before)
    goStill()
  }
  activeTabId.value = tabId
  findBarVisible.value = false
  const tab = tabs.value.find((t) => t.id === tabId)
  if (tab && !opened.has(tabId)) await openTab(tabId, tab.url)
  persist()
  requestSync()
}

async function closeTab(tabId: string): Promise<void> {
  const idx = tabs.value.findIndex((t) => t.id === tabId)
  if (idx === -1 || tabs.value.length <= 1) return
  closePage(tabId)
  tabs.value.splice(idx, 1)
  if (activeTabId.value === tabId) {
    activeTabId.value = ''
    mode.value = 'still'
    placedKey = ''
    stopLiveTimers()
    const next = tabs.value[Math.min(idx, tabs.value.length - 1)]
    if (next) await switchTab(next.id)
  }
  persist()
}

// ---- Navigation ----

async function navigateTo(input: string): Promise<void> {
  const tabId = activeTabId.value
  const tab = activeTab.value
  if (!tabId || !tab) return
  const url = address(input)
  tab.url = url
  urlInput.value = url
  if (opened.has(tabId)) {
    await invoke('plugin:canvas|browser_tab_navigate', { label: labelOf(tabId), url }).catch((e) => { openError.value = String(e) })
  } else {
    await openTab(tabId, url)
  }
  persist()
}

function onUrlSubmit(value?: string): void {
  void navigateTo(value ?? urlInput.value)
}

function act(action: 'back' | 'forward' | 'reload' | 'stop'): void {
  const tabId = activeTabId.value
  if (!opened.has(tabId)) return
  invoke('plugin:canvas|browser_tab_action', { label: labelOf(tabId), action }).catch(() => {})
}

function toggleBookmark(): void {
  const url = currentUrl.value
  if (!url) return
  if (browserStore.isBookmarked(url)) {
    const bm = browserStore.bookmarks.find((b) => b.url === url)
    if (bm) browserStore.removeBookmark(bm.id)
  } else {
    browserStore.addBookmark({ id: `bm-${Date.now()}`, url, title: activeTab.value?.title ?? url, createdAt: new Date().toISOString() })
  }
}

function find(forward: boolean): void {
  const tabId = activeTabId.value
  if (!opened.has(tabId)) return
  invoke('plugin:canvas|find_in_browser', { label: labelOf(tabId), query: findQuery.value, forward }).catch(() => {})
}

function closeFind(): void {
  findBarVisible.value = false
  findQuery.value = ''
  find(true)
}

function setZoom(z: number): void {
  pageZoom.value = Math.round(z * 100) / 100
  placedKey = ''
  persist()
  requestSync()
}

// ---- Keyboard (while the browser's own bar has the focus) ----

function focusOmnibox(): void {
  const input = (omniboxRef.value?.$el as HTMLElement | undefined)?.querySelector('input')
  input?.focus()
  input?.select()
}

function onKeydown(e: KeyboardEvent): void {
  const ctrl = e.ctrlKey || e.metaKey
  if (ctrl && e.key === 't') addTab()
  else if (ctrl && e.key === 'w') closeTab(activeTabId.value)
  else if (ctrl && e.key === 'f') {
    if (findBarVisible.value) closeFind()
    else findBarVisible.value = true
  }
  else if (ctrl && e.key === 'l') focusOmnibox()
  else if ((ctrl && e.key === 'r') || e.key === 'F5') act('reload')
  else if (e.altKey && e.key === 'ArrowLeft') act('back')
  else if (e.altKey && e.key === 'ArrowRight') act('forward')
  else if (ctrl && (e.key === '=' || e.key === '+')) setZoom(zoomStep(pageZoom.value, 1))
  else if (ctrl && e.key === '-') setZoom(zoomStep(pageZoom.value, -1))
  else if (ctrl && e.key === '0') setZoom(1)
  else if (e.key === 'Escape') {
    if (findBarVisible.value) closeFind()
    else if (historyPanelVisible.value) historyPanelVisible.value = false
    else if (bookmarksPanelVisible.value) bookmarksPanelVisible.value = false
    else return
  } else return
  e.preventDefault()
}

// ---- Live page or picture ----

const mode = ref<'live' | 'still'>('still')
const picture = ref<{ src: string; w: number; h: number; tabId: string } | null>(null)
/** The stage this window lives in is on screen (KeepAlive). */
let stageActive = false
let started = false
let moving = false
let settleTimer: ReturnType<typeof setTimeout> | null = null
let placedKey = ''
let clipKey = ''
let syncQueued = false
let lastProbe = 0
let probeTimer: ReturnType<typeof setTimeout> | null = null

function scale(): number {
  return canvasStore.activeCanvasState?.transform.scale ?? 1
}

function box(r: DOMRect): Box {
  return { left: r.left, top: r.top, right: r.right, bottom: r.bottom }
}

/** The visible canvas on screen (CSS px), inside its sidebars. */
function viewportBox(): Box {
  const c = canvasEl.value?.getBoundingClientRect()
  const insets = viewportInsets.value
  return {
    left: (c?.left ?? 0) + insets.left,
    right: (c?.right ?? window.innerWidth) - insets.right,
    top: c?.top ?? 0,
    bottom: c?.bottom ?? window.innerHeight,
  }
}

/** What is cut out of the live page exactly: higher windows and the minimap. */
function cutBoxes(): Box[] {
  const cuts: Box[] = []
  const me = props.window
  for (const w of canvasStore.activeCanvasState?.windows ?? []) {
    // A folded window stays on the canvas as its title bar: cut that.
    if (w.id === me.id || w.zIndex <= me.zIndex) continue
    const el = document.querySelector(`[data-window-id="${w.id}"]`)
    if (!el) continue
    cuts.push(box(el.getBoundingClientRect()))
    // Its resize handles and port straddle its edges.
    for (const child of el.children) cuts.push(box(child.getBoundingClientRect()))
  }
  for (const el of canvasEl.value?.querySelectorAll('[data-canvas-overlay]') ?? []) cuts.push(box(el.getBoundingClientRect()))
  // This window's own frame — its resize handles straddle the edges and its
  // connection port sits on one: cut out, so they stay usable over a live page.
  const root = contentRef.value?.closest('.qcb')
  const frame = root?.closest('[data-window-id]')
  if (root && frame) {
    for (const child of frame.children) if (!child.contains(root)) cuts.push(box(child.getBoundingClientRect()))
  }
  return cuts
}

function wantLive(): boolean {
  return stageActive
    && !props.window.minimized
    && document.visibilityState === 'visible'
    && !moving
    && !contextMenuVisible.value
    && !historyPanelVisible.value
    && !bookmarksPanelVisible.value
    && opened.has(activeTabId.value)
}

function goStill(): void {
  if (mode.value === 'live') hidePage(activeTabId.value)
  mode.value = 'still'
  placedKey = ''
  stopLiveTimers()
}

/** Show the live page where it belongs, or step it aside for its picture. */
function sync(): void {
  const tabId = activeTabId.value
  const el = contentRef.value
  if (!tabId || !el || !opened.has(tabId) || isContentFullscreen.value) return
  const r = el.getBoundingClientRect()
  const page = box(r)
  const viewport = viewportBox()
  let live = wantLive() && r.width >= 40 && r.height >= 30 && !!intersect(page, viewport)
  let cuts: Box[] = []
  if (live) {
    cuts = cutBoxes()
    lastProbe = performance.now()
    if (covered(probePoints(page, viewport, cuts), (x, y) => document.elementFromPoint(x, y), (e) => el.contains(e))) live = false
  }
  if (!live) {
    if (mode.value === 'live') goStill()
    return
  }
  const label = labelOf(tabId)
  const bounds = { x: Math.round(r.left), y: Math.round(r.top), width: Math.round(r.width), height: Math.round(r.height) }
  const zoom = scale() * pageZoom.value
  const key = `${label}|${bounds.x},${bounds.y},${bounds.width},${bounds.height}|${zoom.toFixed(4)}`
  const dpr = window.devicePixelRatio || 1
  const clips = clipRects(page, cuts, viewport, dpr)
  const fullWidth = Math.round(r.width * dpr)
  const fullHeight = Math.round(r.height * dpr)
  const ck = `${label}|${fullWidth}x${fullHeight}:` + clips.map((c) => `${c.x},${c.y},${c.w},${c.h}`).join('|')
  // The region first, so the page never shows uncut for a frame.
  if (ck !== clipKey || mode.value !== 'live') {
    clipKey = ck
    invoke('plugin:canvas|set_browser_clip_region', { label, fullWidth, fullHeight, obscuringRects: clips }).catch(() => {})
  }
  if (key !== placedKey || mode.value !== 'live') {
    placedKey = key
    invoke('plugin:canvas|browser_tab_place', { label, bounds, zoom, visible: true }).catch(() => {})
  }
  if (mode.value !== 'live') {
    mode.value = 'live'
    startLiveTimers()
    setTimeout(capture, 700)
  }
}

function requestSync(): void {
  if (syncQueued || !started) return
  syncQueued = true
  requestAnimationFrame(() => {
    syncQueued = false
    sync()
  })
}

/** DOM changes: something may have opened on top. Probed at most 5× a second. */
function onMutations(): void {
  if (!stageActive || probeTimer) return
  const wait = Math.max(0, 200 - (performance.now() - lastProbe))
  if (wait === 0) requestSync()
  else probeTimer = setTimeout(() => { probeTimer = null; requestSync() }, wait)
}

/** A click or key in the suite: menus open on those — look now and once more after their entrance. */
function onUserInput(): void {
  if (!stageActive) return
  requestSync()
  setTimeout(requestSync, 140)
}

/** The canvas or this window moves: the picture moves with it, the page waits. */
function markMoving(): void {
  if (!stageActive) return
  if (!moving) {
    moving = true
    if (mode.value === 'live') sync()
  }
  if (settleTimer) clearTimeout(settleTimer)
  settleTimer = setTimeout(() => {
    settleTimer = null
    moving = false
    clipKey = ''
    requestSync()
    setTimeout(requestSync, 260)
  }, 160)
}

watch(() => {
  const t = canvasStore.activeCanvasState?.transform
  return t ? `${t.x},${t.y},${t.scale}` : ''
}, markMoving)
watch(() => {
  const p = props.window.position
  return `${p.x},${p.y},${p.width},${p.height}`
}, markMoving)
watch(() => props.window.zIndex, requestSync)
watch(() => props.window.minimized, (min) => (min ? goStill() : requestSync()))
watch([contextMenuVisible, historyPanelVisible, bookmarksPanelVisible, findBarVisible], requestSync)

// Other windows: one that moves over or off this one is motion too (its
// picture moves with it cleanly); anything else only re-cuts the page.
let others = new Map<string, string>()
watch(() => (canvasStore.activeCanvasState?.windows ?? [])
  .filter((w) => w.id !== props.window.id)
  .map((w) => `${w.id}:${w.zIndex}:${w.minimized ? 1 : 0}:${w.position.x},${w.position.y},${w.position.width},${w.position.height}`)
  .join('|'), () => {
  const me = props.window.position
  const now = new Map<string, string>()
  let touched = false
  for (const w of canvasStore.activeCanvasState?.windows ?? []) {
    if (w.id === props.window.id) continue
    const key = `${w.zIndex}:${w.minimized ? 1 : 0}:${w.position.x},${w.position.y},${w.position.width},${w.position.height}`
    now.set(w.id, key)
    const before = others.get(w.id)
    if (before !== undefined && before !== key && (w.zIndex > props.window.zIndex || Number(before.split(':')[0]) > props.window.zIndex)) {
      const [bx, by, bw, bh] = before.split(':')[2]!.split(',').map(Number)
      if (overlaps(me, w.position) || overlaps(me, { x: bx!, y: by!, width: bw!, height: bh! })) touched = true
    }
  }
  others = now
  if (touched) markMoving()
  else requestSync()
  // A window that opens or folds animates its size: cut again once it has.
  setTimeout(requestSync, 260)
}, { immediate: true })

// ---- The picture ----

let captureBusy = false
let lastCapture = 0
let liveTimer: ReturnType<typeof setInterval> | null = null
let lastPointer = 0

/** A picture of the live page, for whenever it has to step aside. */
async function capture(): Promise<void> {
  const tabId = activeTabId.value
  const el = contentRef.value
  if (mode.value !== 'live' || captureBusy || !el || !opened.has(tabId)) return
  captureBusy = true
  // Layout px inside the (scaled) canvas: the picture keeps the size the page had.
  const w = el.offsetWidth
  const h = el.offsetHeight
  try {
    const bytes = await invoke<ArrayBuffer>('plugin:canvas|browser_tab_capture', { label: labelOf(tabId) })
    if (tabId !== activeTabId.value) return
    const src = URL.createObjectURL(new Blob([bytes], { type: 'image/jpeg' }))
    const old = picture.value?.src
    picture.value = { src, w, h, tabId }
    if (old) setTimeout(() => URL.revokeObjectURL(old), 1000)
  } catch {
    // No picture (another platform, a page still starting): the placeholder stays.
  } finally {
    captureBusy = false
    lastCapture = performance.now()
  }
}

/** The pointer comes back from the page (the suite sees it again): what it did there is worth a picture. */
function onPointerMove(): void {
  const now = performance.now()
  if (mode.value === 'live' && now - lastPointer > 250 && now - lastCapture > 1000) void capture()
  lastPointer = now
}

function startLiveTimers(): void {
  if (liveTimer) return
  // Moving pages (video, animations): a fresh picture now and then.
  liveTimer = setInterval(() => {
    if (document.visibilityState === 'visible' && performance.now() - lastCapture > 4000) void capture()
  }, 5000)
}

function stopLiveTimers(): void {
  if (liveTimer) clearInterval(liveTimer)
  liveTimer = null
}

// ---- Watching the suite while on screen ----

let mutations: MutationObserver | null = null
let resizes: ResizeObserver | null = null

function startWatching(): void {
  if (mutations) return
  mutations = new MutationObserver(onMutations)
  mutations.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ['class', 'style', 'open', 'hidden', 'data-state'] })
  resizes = new ResizeObserver(markMoving)
  if (contentRef.value) resizes.observe(contentRef.value)
  if (canvasEl.value) resizes.observe(canvasEl.value)
  document.addEventListener('pointerdown', onUserInput, true)
  document.addEventListener('keydown', onUserInput, true)
  document.addEventListener('contextmenu', onUserInput, true)
  document.addEventListener('pointermove', onPointerMove, { capture: true, passive: true })
  document.addEventListener('visibilitychange', requestSync)
  window.addEventListener('resize', markMoving)
}

function stopWatching(): void {
  mutations?.disconnect()
  mutations = null
  resizes?.disconnect()
  resizes = null
  document.removeEventListener('pointerdown', onUserInput, true)
  document.removeEventListener('keydown', onUserInput, true)
  document.removeEventListener('contextmenu', onUserInput, true)
  document.removeEventListener('pointermove', onPointerMove, true)
  document.removeEventListener('visibilitychange', requestSync)
  window.removeEventListener('resize', markMoving)
  if (probeTimer) clearTimeout(probeTimer)
  probeTimer = null
  if (settleTimer) clearTimeout(settleTimer)
  settleTimer = null
  moving = false
}

// ---- Lifecycle ----

const unlisten: UnlistenFn[] = []

async function listenToPages(): Promise<void> {
  unlisten.push(await listen<TabStatus>('canvas-browser-state', (e) => {
    const tabId = tabOfLabel(e.payload.label)
    if (tabId) applyStatus(tabId, e.payload)
  }))
  unlisten.push(await listen<{ label: string; url: string }>('canvas-browser-new-tab', (e) => {
    if (tabOfLabel(e.payload.label)) void addTab(e.payload.url)
  }))
  unlisten.push(await listen<{ label: string; isFullscreen: boolean }>('browser-fullscreen-changed', (e) => {
    const tabId = activeTabId.value
    if (!tabId || e.payload.label !== labelOf(tabId)) return
    isContentFullscreen.value = e.payload.isFullscreen
    if (e.payload.isFullscreen) {
      // The page over the whole suite window, uncut.
      invoke('plugin:canvas|set_browser_clip_region', { label: e.payload.label, fullWidth: 0, fullHeight: 0, obscuringRects: [] }).catch(() => {})
      invoke('plugin:canvas|browser_tab_place', { label: e.payload.label, bounds: { x: 0, y: 0, width: window.innerWidth, height: window.innerHeight }, zoom: pageZoom.value, visible: true }).catch(() => {})
      mode.value = 'live'
    } else {
      placedKey = ''
      clipKey = ''
      requestSync()
    }
  }))
}

/** The stage left the screen: the page steps aside, nothing keeps watching. */
function goDormant(): void {
  stageActive = false
  stopWatching()
  goStill()
}

onMounted(async () => {
  // Read before the first await: Vue keeps the current instance only while
  // the hook body runs synchronously.
  stageActive = inActiveKeepAliveTree()
  canvasStore.updateWindow(workspaceId.value, props.window.id, { status: 'live' })
  await browserStore.loadFromDisk()
  await nextTick()
  await listenToPages()

  const config = props.window.browserConfig
  const saved = config?.tabs ?? []
  if (saved.length) {
    tabs.value = saved.map((t) => ({ ...t, title: t.title || tabTitle('', t.url), isLoading: false }))
    const active = saved.some((t) => t.id === config?.activeTabId) ? config!.activeTabId! : saved[0]!.id
    started = true
    if (stageActive) startWatching()
    await switchTab(active)
  } else {
    started = true
    if (stageActive) startWatching()
    await addTab(config?.url || 'https://www.google.com')
  }
  if (!stageActive) goDormant()
})

onDeactivated(goDormant)

onActivated(() => {
  // The first mount starts everything itself.
  if (!started) return
  stageActive = true
  startWatching()
  placedKey = ''
  clipKey = ''
  requestSync()
})

onUnmounted(() => {
  started = false
  stopWatching()
  stopLiveTimers()
  if (persistTimer) {
    clearTimeout(persistTimer)
    persistTimer = null
  }
  for (const fn of unlisten.splice(0)) fn()
  // Every tab, not only those opened since this mount: a reload of the suite
  // page keeps the pages of tabs it never showed again, hidden but running.
  for (const t of tabs.value) closePage(t.id)
  if (picture.value) URL.revokeObjectURL(picture.value.src)
})
</script>

<template>
  <div class="qcb" tabindex="0" @keydown="onKeydown">
    <BrowserTabBar :tabs="tabs" :active-tab-id="activeTabId" @switch="switchTab" @close="closeTab" @add="addTab()" />

    <div class="qcb-bar">
      <BrowserNavButtons
        :can-go-back="canGoBack"
        :can-go-forward="canGoForward"
        :is-loading="loading"
        @back="act('back')"
        @forward="act('forward')"
        @reload="act('reload')"
        @stop="act('stop')"
      />
      <BrowserOmnibox
        ref="omniboxRef"
        v-model="urlInput"
        :current-url="currentUrl"
        :history="browserStore.history"
        :is-bookmarked="browserStore.isBookmarked(currentUrl)"
        @submit="onUrlSubmit"
        @toggle-bookmark="toggleBookmark"
      />
      <button v-if="pageZoom !== 1" class="qcb-zoom" title="Page zoom — reset to 100 % (Ctrl+0)" @mousedown.stop @click="setZoom(1)">
        {{ Math.round(pageZoom * 100) }}%
      </button>
      <button
        class="qcb-icon"
        :class="{ on: bookmarksPanelVisible }"
        title="Bookmarks"
        @mousedown.stop
        @click="bookmarksPanelVisible = !bookmarksPanelVisible; historyPanelVisible = false"
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" /></svg>
      </button>
      <button
        class="qcb-icon"
        :class="{ on: historyPanelVisible }"
        title="History"
        @mousedown.stop
        @click="historyPanelVisible = !historyPanelVisible; bookmarksPanelVisible = false"
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10" /><polyline points="12 6 12 12 16 14" /></svg>
      </button>
    </div>

    <BrowserBookmarkBar :bookmarks="browserStore.bookmarks" @navigate="navigateTo" @remove="(id: string) => browserStore.removeBookmark(id)" />
    <BrowserProgressBar :loading="loading" />
    <BrowserFindBar v-if="findBarVisible" v-model="findQuery" @find-next="find(true)" @find-prev="find(false)" @close="closeFind" />

    <div class="qcb-body">
      <!-- The page's area, inset so the live page never covers the window's resize edges. -->
      <div ref="contentRef" class="qcb-page" :data-mode="mode">
        <img
          v-if="picture && picture.tabId === activeTabId"
          class="qcb-picture"
          :src="picture.src"
          :style="{ width: picture.w + 'px', height: picture.h + 'px' }"
          alt=""
          draggable="false"
        >
        <div v-else class="qcb-placeholder">
          <span v-if="openError" class="qcb-error">{{ openError }}</span>
          <span v-else>{{ activeTab ? tabTitle(activeTab.title, activeTab.url) : '' }}</span>
        </div>
      </div>

      <BrowserHistoryPanel
        v-if="historyPanelVisible"
        :history="browserStore.history"
        @navigate="(url: string) => { navigateTo(url); historyPanelVisible = false }"
        @close="historyPanelVisible = false"
        @clear="browserStore.clearHistory()"
      />
      <BrowserBookmarkPanel
        v-if="bookmarksPanelVisible"
        :bookmarks="browserStore.bookmarks"
        @navigate="(url: string) => { navigateTo(url); bookmarksPanelVisible = false }"
        @close="bookmarksPanelVisible = false"
        @remove="(id: string) => browserStore.removeBookmark(id)"
      />
    </div>
  </div>
</template>

<style scoped>
.qcb {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--qc-bg);
  outline: none;
}
.qcb-bar {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 5px 8px;
  flex-shrink: 0;
  background: var(--qc-bg-header);
  border-bottom: 1px solid var(--qc-border);
}
.qcb-icon {
  flex-shrink: 0;
  width: 26px;
  height: 26px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 6px;
  color: var(--qc-text-dim);
}
.qcb-icon:hover,
.qcb-icon.on {
  color: var(--qc-text);
  background: color-mix(in srgb, var(--qc-text) 8%, transparent);
}
.qcb-zoom {
  flex-shrink: 0;
  height: 22px;
  padding: 0 7px;
  border-radius: 11px;
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  color: var(--qc-text);
  background: color-mix(in srgb, var(--qc-text) 10%, transparent);
}
.qcb-body {
  position: relative;
  flex: 1;
  min-height: 0;
}
.qcb-page {
  position: absolute;
  top: 0;
  left: 4px;
  right: 4px;
  bottom: 4px;
  overflow: hidden;
  background: var(--qc-bg-surface);
}
.qcb-picture {
  position: absolute;
  left: 0;
  top: 0;
  max-width: none;
  pointer-events: none;
  user-select: none;
}
.qcb-placeholder {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 16px;
  text-align: center;
  font-size: 12px;
  color: var(--qc-text-dim);
}
.qcb-error {
  color: #f87171;
}
</style>
