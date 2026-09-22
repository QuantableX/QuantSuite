<script setup lang="ts">
import { invoke } from '@tauri-apps/api/core'
import { Webview } from '@tauri-apps/api/webview'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { LogicalPosition, LogicalSize } from '@tauri-apps/api/dpi'
import { listen } from '@tauri-apps/api/event'
import { inActiveKeepAliveTree } from '@quantsuite/core'
import { useCanvasStore } from '../../../stores/canvas'
import { useWorkspacesStore } from '../../../stores/workspaces'
import { useBrowserStore } from '../../../stores/browser'
import type { CanvasWindow, BrowserTab } from '../../../shared/types'
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

const canvasStore = useCanvasStore()
const workspacesStore = useWorkspacesStore()
const browserStore = useBrowserStore()
const workspaceId = computed(() => workspacesStore.contentWorkspaceId!)

// Placeholder element whose screen rect drives webview positioning
const contentRef = ref<HTMLElement | null>(null)
const omniboxRef = ref<InstanceType<typeof BrowserOmnibox> | null>(null)

// Viewport insets (sidebar widths) for clipping — provided by index.vue
const viewportInsets = inject<ComputedRef<{ left: number; right: number }>>(
  'viewportInsets',
  computed(() => ({ left: 0, right: 0 })),
)

// Context menu visibility — hide webview when canvas context menu is open
const contextMenuVisible = inject<ComputedRef<boolean>>(
  'contextMenuVisible',
  computed(() => false),
)

// The canvas element, provided by InfinityCanvas. Its screen rect is the area
// this window is allowed to paint in — see computeObscuringRects.
const canvasEl = inject<Ref<HTMLElement | null>>('canvasRef', ref(null))

// Bookmarks panel state
const bookmarksPanelVisible = ref(false)

// ---- Overlay rects: screen-coordinate rects that punch through the webview clip ----
const overlayRects = ref<{ x: number; y: number; w: number; h: number }[]>([])
provide('browserOverlayRects', overlayRects)

// ---- Tab state ----
const tabs = ref<BrowserTab[]>([])
const activeTabId = ref('')

interface TabWebviewState {
  webview: Webview
  history: string[]
  historyIndex: number
}

const tabWebviews = new Map<string, TabWebviewState>()

const activeTab = computed(() => tabs.value.find(t => t.id === activeTabId.value))

const activeTabWebview = computed(() => {
  if (!activeTabId.value) return undefined
  return tabWebviews.get(activeTabId.value)
})

const canGoBack = computed(() => {
  const state = activeTabWebview.value
  return state ? state.historyIndex > 0 : false
})

const canGoForward = computed(() => {
  const state = activeTabWebview.value
  return state ? state.historyIndex < state.history.length - 1 : false
})

// URL bar state
const urlInput = ref('')
const currentUrl = ref('')
const webviewReady = ref(false)

// Progress bar state
const loadProgress = ref(0)
let progressInterval: ReturnType<typeof setInterval> | null = null

// Find in page state
const findBarVisible = ref(false)
const findQuery = ref('')

// History panel state
const historyPanelVisible = ref(false)

// Fullscreen state — when content goes fullscreen, we expand the webview to fill the window
const isContentFullscreen = ref(false)

// ---- Webview label helper ----

function webviewLabelForTab(tabId: string): string {
  return `browser-${props.window.id.replace(/[^a-zA-Z0-9_-]/g, '_')}-${tabId.replace(/[^a-zA-Z0-9_-]/g, '_')}`
}

/**
 * close() promises from unmounting instances, keyed by webview label.
 *
 * A remount (workspace switch, HMR) races the old instance's `close()` against
 * the new instance's `getByLabel`: the label still resolves while the webview
 * is being torn down, the new instance "reconnects" to it, and the browser
 * shows a black content area from then on. The registry lives on `globalThis`
 * deliberately — module replacement during HMR would reset a module-level map,
 * and the whole point is surviving into the next instance.
 */
const pendingWebviewCloses = ((globalThis as unknown as Record<string, unknown>)
  .__qcBrowserPendingCloses ??= new Map()) as Map<string, Promise<unknown>>

/** Close a native tab webview and register the teardown for remounts to await. */
function closeWebviewTracked(label: string, webview: Webview) {
  const closing: Promise<unknown> = webview
    .close()
    .catch(() => {})
    .finally(() => {
      if (pendingWebviewCloses.get(label) === closing) pendingWebviewCloses.delete(label)
    })
  pendingWebviewCloses.set(label, closing)
}

// ---- URL normalization with search detection ----

function normalizeUrl(input: string): string {
  let u = input.trim()
  if (/^https?:\/\//i.test(u)) return u
  if (/^(localhost|127\.|192\.|10\.|0\.0\.0\.0)/.test(u)) return 'http://' + u
  if (/^[\w-]+(\.[\w-]+)+/.test(u) && !u.includes(' ')) return 'https://' + u
  // Search query
  const engines: Record<string, string> = {
    google: 'https://www.google.com/search?q=',
    duckduckgo: 'https://duckduckgo.com/?q=',
    bing: 'https://www.bing.com/search?q=',
    brave: 'https://search.brave.com/search?q=',
  }
  return (engines[browserStore.searchEngine] ?? engines.google) + encodeURIComponent(u)
}

// ---- Progress bar ----

function startProgress() {
  loadProgress.value = 10
  if (progressInterval) clearInterval(progressInterval)
  progressInterval = setInterval(() => {
    if (loadProgress.value < 90) {
      loadProgress.value += (90 - loadProgress.value) * 0.1
    }
  }, 200)
}

function finishProgress() {
  if (progressInterval) clearInterval(progressInterval)
  loadProgress.value = 100
  setTimeout(() => { loadProgress.value = 0 }, 300)
}

// ---- Persist tab state to canvas ----

function persistTabState() {
  canvasStore.updateWindow(workspaceId.value, props.window.id, {
    browserConfig: {
      url: activeTab.value?.url ?? '',
      tabs: tabs.value,
      activeTabId: activeTabId.value,
    },
  })
}

// ---- Tab lifecycle ----

let nextTabNum = 1

async function createWebviewForTab(tabId: string, url: string): Promise<Webview | null> {
  const label = webviewLabelForTab(tabId)

  // A previous mount may still be tearing this label down — wait it out, or
  // getByLabel below finds the dying webview and reconnects to a corpse.
  const pendingClose = pendingWebviewCloses.get(label)
  if (pendingClose) await pendingClose

  await nextTick()
  if (!contentRef.value) return null

  const rect = contentRef.value.getBoundingClientRect()
  const initX = Math.round(rect.left)
  const initY = Math.round(rect.top)
  const initW = Math.max(Math.round(rect.width), 100)
  const initH = Math.max(Math.round(rect.height), 100)

  const isActive = tabId === activeTabId.value

  try {
    // Try to reconnect to an existing webview (survives page reloads)
    let existing = await Webview.getByLabel(label)
    if (existing) {
      // The label can outlive its webview through teardown paths this
      // registry never sees (an old code version's unmount, the window
      // frame's own cleanup). Ping it: a webview that cannot report its URL
      // is not one worth reconnecting to — clear it and build fresh.
      try {
        await invoke<string>('plugin:canvas|get_browser_url', { label })
      } catch {
        await existing.close().catch(() => {})
        existing = null
      }
    }
    if (existing) {
      if (isActive) {
        await existing.setPosition(new LogicalPosition(initX, initY)).catch(() => {})
        await existing.setSize(new LogicalSize(initW, initH)).catch(() => {})
        await existing.show().catch(() => {})
      } else {
        await existing.hide().catch(() => {})
      }

      // Sync URL from the webview's actual state
      try {
        const actualUrl = await invoke<string>('plugin:canvas|get_browser_url', { label })
        if (actualUrl) {
          const tab = tabs.value.find(t => t.id === tabId)
          if (tab) {
            tab.url = actualUrl
            if (isActive) {
              urlInput.value = actualUrl
              currentUrl.value = actualUrl
            }
          }
          const tabState = tabWebviews.get(tabId)
          if (tabState) {
            tabState.history = [actualUrl]
            tabState.historyIndex = 0
          }
        }
      } catch { /* ignore */ }

      // Prevent WebView2 from making the Tauri window fullscreen
      invoke('plugin:canvas|browser_disable_fullscreen', { label }).catch(() => {})

      if (isActive) {
        injectBrowserScripts(label)
      }

      return existing
    }

    // No existing webview — create a new one
    const appWindow = getCurrentWindow()
    const wv = new Webview(appWindow, label, {
      url,
      x: isActive ? initX : -10000,
      y: isActive ? initY : -10000,
      width: initW,
      height: initH,
      zoomHotkeysEnabled: false,
    })

    return await new Promise<Webview | null>((resolve) => {
      wv.once('tauri://created', () => {
        if (!isActive) {
          wv.hide().catch(() => {})
        }
        if (isActive) {
          setTimeout(() => injectBrowserScripts(label), 1000)
        }
        // Prevent WebView2 from making the Tauri window fullscreen
        invoke('plugin:canvas|browser_disable_fullscreen', { label }).catch(() => {})
        resolve(wv)
      })

      wv.once('tauri://error', (e) => {
        // Resolve null, not the handle: a webview that failed to create shows
        // nothing, and handing it back flips webviewReady while the content
        // area stays black. Null keeps the "Loading browser..." placeholder
        // on screen, which is at least an honest state.
        console.error('Webview creation error:', e)
        resolve(null)
      })
    })
  } catch (err) {
    console.error('Failed to create/reconnect webview for tab:', tabId, err)
    return null
  }
}

async function addTab(url?: string, options?: { background?: boolean }) {
  const background = options?.background ?? false
  const tabId = `tab-${Date.now()}-${nextTabNum++}`
  const tabUrl = url ?? 'https://www.google.com'
  const normalizedUrl = normalizeUrl(tabUrl)

  const newTab: BrowserTab = {
    id: tabId,
    url: normalizedUrl,
    title: 'New Tab',
    isLoading: true,
  }

  tabs.value.push(newTab)

  if (!background) {
    activeTabId.value = tabId
    urlInput.value = normalizedUrl
    currentUrl.value = normalizedUrl

    // Hide the previously active webview
    for (const [id, state] of tabWebviews) {
      if (id !== tabId) {
        state.webview.hide().catch(() => {})
      }
    }
  }

  const wv = await createWebviewForTab(tabId, normalizedUrl)
  if (wv) {
    tabWebviews.set(tabId, {
      webview: wv,
      history: [normalizedUrl],
      historyIndex: 0,
    })
    if (!background) webviewReady.value = true
    newTab.isLoading = false
  } else {
    newTab.isLoading = false
  }

  // Update title from hostname
  try {
    const hostname = new URL(normalizedUrl).hostname
    newTab.title = hostname
    if (!background) {
      canvasStore.updateWindow(workspaceId.value, props.window.id, {
        title: `Browser - ${hostname}`,
      })
    }
  } catch { /* invalid URL */ }

  // Add to browser history store
  browserStore.addHistoryEntry({
    url: normalizedUrl,
    title: newTab.title,
    visitedAt: new Date().toISOString(),
  })

  // Start position sync if this is the first tab
  if (tabs.value.length === 1 && rafId === null) {
    rafId = requestAnimationFrame(syncWebviewPosition)
  }

  persistTabState()
}

async function closeTab(tabId: string) {
  const idx = tabs.value.findIndex(t => t.id === tabId)
  if (idx === -1) return

  // If it's the only tab, don't close
  if (tabs.value.length <= 1) return

  // Destroy the webview (close stops media playback, frees the OS process)
  const state = tabWebviews.get(tabId)
  if (state) {
    const label = webviewLabelForTab(tabId)
    invoke('plugin:canvas|set_browser_clip_region', {
      label,
      fullWidth: 0,
      fullHeight: 0,
      obscuringRects: [],
    }).catch(() => {})
    closeWebviewTracked(label, state.webview)
    tabWebviews.delete(tabId)
  }

  tabs.value.splice(idx, 1)

  // If we closed the active tab, switch to adjacent
  if (activeTabId.value === tabId) {
    const newIdx = Math.min(idx, tabs.value.length - 1)
    const newActiveTab = tabs.value[newIdx]
    if (newActiveTab) {
      await switchTab(newActiveTab.id)
    }
  }

  persistTabState()
}

async function switchTab(tabId: string) {
  if (tabId === activeTabId.value) return

  const oldTabId = activeTabId.value

  // Hide old webview
  const oldState = tabWebviews.get(oldTabId)
  if (oldState) {
    oldState.webview.hide().catch(() => {})
  }

  // Reset clip key so we re-apply on the new webview
  lastClipKey = null

  activeTabId.value = tabId

  // Show new webview
  const newState = tabWebviews.get(tabId)
  if (newState) {
    const rect = contentRef.value?.getBoundingClientRect()
    if (rect) {
      await newState.webview.setPosition(new LogicalPosition(Math.round(rect.left), Math.round(rect.top))).catch(() => {})
      await newState.webview.setSize(new LogicalSize(Math.max(Math.round(rect.width), 100), Math.max(Math.round(rect.height), 100))).catch(() => {})
    }
    await newState.webview.show().catch(() => {})
    injectCtrlWheelForwarder(webviewLabelForTab(tabId))
  }


  // Sync URL bar to the new tab
  const tab = tabs.value.find(t => t.id === tabId)
  if (tab) {
    urlInput.value = tab.url
    currentUrl.value = tab.url

    // Update window title
    try {
      const hostname = new URL(tab.url).hostname
      canvasStore.updateWindow(workspaceId.value, props.window.id, {
        title: `Browser - ${hostname}`,
      })
    } catch { /* invalid URL */ }
  }

  persistTabState()
}

// ---- Navigation ----

async function navigateTo(url: string) {
  const normalizedUrl = normalizeUrl(url)
  const tabId = activeTabId.value
  const label = webviewLabelForTab(tabId)
  const tab = tabs.value.find(t => t.id === tabId)
  const state = tabWebviews.get(tabId)

  urlInput.value = normalizedUrl
  currentUrl.value = normalizedUrl

  if (tab) {
    tab.url = normalizedUrl
    tab.isLoading = true
  }

  startProgress()

  // Update per-tab navigation history
  if (state) {
    if (state.historyIndex < state.history.length - 1) {
      state.history = state.history.slice(0, state.historyIndex + 1)
    }
    state.history.push(normalizedUrl)
    state.historyIndex = state.history.length - 1
  }

  // Navigate the native webview
  try {
    await invoke('plugin:canvas|navigate_browser', { label, url: normalizedUrl })
    if (tab) tab.isLoading = false
    finishProgress()
  } catch (err) {
    console.error('navigate_browser failed:', err)
    if (tab) tab.isLoading = false
    finishProgress()
  }

  // Update title with hostname
  try {
    const hostname = new URL(normalizedUrl).hostname
    if (tab) tab.title = hostname
    canvasStore.updateWindow(workspaceId.value, props.window.id, {
      title: `Browser - ${hostname}`,
    })
  } catch { /* invalid URL */ }

  // Add to browser history store
  browserStore.addHistoryEntry({
    url: normalizedUrl,
    title: tab?.title ?? '',
    visitedAt: new Date().toISOString(),
  })

  persistTabState()
}

function onUrlSubmit(value?: string) {
  navigateTo(value ?? urlInput.value)
}

async function goBack() {
  if (!canGoBack.value) return
  const state = activeTabWebview.value
  if (!state) return

  state.historyIndex--
  const url = state.history[state.historyIndex]
  const tab = activeTab.value
  const label = webviewLabelForTab(activeTabId.value)

  urlInput.value = url
  currentUrl.value = url
  if (tab) tab.url = url

  startProgress()
  try {
    await invoke('plugin:canvas|navigate_browser', { label, url })
    finishProgress()
  } catch {
    finishProgress()
  }

  persistTabState()
}

async function goForward() {
  if (!canGoForward.value) return
  const state = activeTabWebview.value
  if (!state) return

  state.historyIndex++
  const url = state.history[state.historyIndex]
  const tab = activeTab.value
  const label = webviewLabelForTab(activeTabId.value)

  urlInput.value = url
  currentUrl.value = url
  if (tab) tab.url = url

  startProgress()
  try {
    await invoke('plugin:canvas|navigate_browser', { label, url })
    finishProgress()
  } catch {
    finishProgress()
  }

  persistTabState()
}

async function reload() {
  const tab = activeTab.value
  const label = webviewLabelForTab(activeTabId.value)

  if (tab) tab.isLoading = true
  startProgress()
  try {
    await invoke('plugin:canvas|eval_browser', { label, js: 'location.reload()' })
  } catch { /* ignore */ }
  if (tab) tab.isLoading = false
  finishProgress()
}

// ---- Bookmark toggle ----

function toggleBookmark() {
  if (browserStore.isBookmarked(currentUrl.value)) {
    const bookmark = browserStore.getBookmarkByUrl(currentUrl.value)
    if (bookmark) {
      browserStore.removeBookmark(bookmark.id)
    }
  } else {
    browserStore.addBookmark({
      id: `bm-${Date.now()}`,
      url: currentUrl.value,
      title: activeTab.value?.title ?? currentUrl.value,
      createdAt: new Date().toISOString(),
    })
  }
}

// ---- Find in page ----

function findNext() {
  if (!findQuery.value) return
  const label = webviewLabelForTab(activeTabId.value)
  invoke('plugin:canvas|find_in_browser', { label, query: findQuery.value, forward: true }).catch(() => {})
}

function findPrev() {
  if (!findQuery.value) return
  const label = webviewLabelForTab(activeTabId.value)
  invoke('plugin:canvas|find_in_browser', { label, query: findQuery.value, forward: false }).catch(() => {})
}

// ---- Keyboard shortcuts ----

function onKeydown(e: KeyboardEvent) {
  if (e.ctrlKey && e.key === 't') {
    e.preventDefault()
    addTab()
  } else if (e.ctrlKey && e.key === 'w') {
    e.preventDefault()
    closeTab(activeTabId.value)
  } else if (e.ctrlKey && e.key === 'f') {
    e.preventDefault()
    findBarVisible.value = !findBarVisible.value
    if (!findBarVisible.value) {
      findQuery.value = ''
    }
  } else if (e.ctrlKey && e.key === 'l') {
    e.preventDefault()
    // Focus the omnibox input
    const omniboxEl = omniboxRef.value?.$el
    if (omniboxEl) {
      const input = omniboxEl.querySelector('input')
      if (input) {
        input.focus()
        input.select()
      }
    }
  } else if (e.key === 'Escape') {
    if (findBarVisible.value) {
      findBarVisible.value = false
      findQuery.value = ''
    } else if (historyPanelVisible.value) {
      historyPanelVisible.value = false
    } else if (bookmarksPanelVisible.value) {
      bookmarksPanelVisible.value = false
    }
  }
}

// ---- Webview position sync with clipping ----

let rafId: number | null = null
let lastRect = { x: 0, y: 0, w: 0, h: 0 }
let lastVisible = true
/**
 * The region currently on the native HWND, as a key — `null` means "unknown",
 * which is NOT the same as "nothing obscures the browser".
 *
 * A region set with SetWindowRgn belongs to the webview's HWND, and that HWND
 * outlives this component: a reload or a remount reconnects to the same
 * webview by label. Starting this at `''` — the key an empty clip list
 * produces — made "nothing obscures it" indistinguishable from "nothing has
 * changed since mount", so a region left over from a previous life was never
 * cleared: the page went on rendering through a hole the shape of a panel
 * that closed minutes ago, the rest of the content area black. The sentinel
 * makes the first frame push the truth, empty or not.
 */
let lastClipKey: string | null = null

interface LocalClipRect {
  x: number
  y: number
  w: number
  h: number
}

// Compute obscuring rectangles in webview-local coordinates (physical pixels).
// These rects are subtracted from the full webview area by SetWindowRgn on the
// native HWND, allowing the main webview's HTML content to show through.
function computeObscuringRects(
  wvX: number, wvY: number, wvW: number, wvH: number,
): LocalClipRect[] {
  const state = canvasStore.activeCanvasState
  if (!state) return []

  const dpr = window.devicePixelRatio || 1
  const my = props.window
  const clips: LocalClipRect[] = []

  /** Cut a screen rect (CSS px) out of the webview, if the two overlap. */
  function subtract(rect: { left: number; top: number; right: number; bottom: number }) {
    const iLeft = Math.max(wvX, rect.left)
    const iTop = Math.max(wvY, rect.top)
    const iRight = Math.min(wvX + wvW, rect.right)
    const iBottom = Math.min(wvY + wvH, rect.bottom)
    if (iRight <= iLeft || iBottom <= iTop) return
    clips.push({
      x: Math.round((iLeft - wvX) * dpr),
      y: Math.round((iTop - wvY) * dpr),
      w: Math.round((iRight - iLeft) * dpr),
      h: Math.round((iBottom - iTop) * dpr),
    })
  }

  // --- Higher-z canvas windows ---
  for (const win of state.windows) {
    if (win.id === my.id) continue
    if (win.minimized) continue
    if (win.zIndex <= my.zIndex) continue

    // Find the DOM element for this window and get its screen rect
    const el = document.querySelector(`[data-window-id="${win.id}"]`)
    if (!el) continue
    subtract(el.getBoundingClientRect())
  }

  // --- Canvas overlays that belong above every window (the minimap) ---
  // They are HTML and the webview is a native HWND, so z-index cannot reach
  // them: without cutting them out by hand a browser window parked in the
  // corner simply swallows the minimap.
  for (const el of canvasEl.value?.querySelectorAll('[data-canvas-overlay]') ?? []) {
    subtract(el.getBoundingClientRect())
  }

  // --- Everything outside the visible canvas (sidebars, and the suite chrome
  // around the module panel). The webview is a native HWND floating above all
  // HTML, so anything the canvas does not own has to be cut away by hand.
  //
  // Measured from the canvas element, NOT from the window: under V3 the module
  // is a panel inset by the shell's rail, titlebar, module header and drawer,
  // so window coordinates put the canvas' left edge at 0 and its right edge at
  // innerWidth. That let a browser window dragged to an edge paint straight
  // over the suite chrome, and put the sidebar bands off by the panel's own
  // offset. Top and bottom were never clipped at all.
  const canvasRect = canvasEl.value?.getBoundingClientRect()
  const insets = viewportInsets.value
  const vpLeft = (canvasRect?.left ?? 0) + insets.left
  const vpRight = (canvasRect?.right ?? window.innerWidth) - insets.right
  const vpTop = canvasRect?.top ?? 0
  const vpBottom = canvasRect?.bottom ?? window.innerHeight

  // Left of the canvas (file explorer, rail, panel edge)
  if (wvX < vpLeft) {
    const clipW = Math.min(vpLeft - wvX, wvW)
    clips.push({
      x: 0,
      y: 0,
      w: Math.round(clipW * dpr),
      h: Math.round(wvH * dpr),
    })
  }

  // Right of the canvas (editor panel, panel edge)
  if (wvX + wvW > vpRight) {
    const clipStart = Math.max(0, vpRight - wvX)
    clips.push({
      x: Math.round(clipStart * dpr),
      y: 0,
      w: Math.round((wvW - clipStart) * dpr),
      h: Math.round(wvH * dpr),
    })
  }

  // Above the canvas (module header, titlebar)
  if (wvY < vpTop) {
    const clipH = Math.min(vpTop - wvY, wvH)
    clips.push({
      x: 0,
      y: 0,
      w: Math.round(wvW * dpr),
      h: Math.round(clipH * dpr),
    })
  }

  // Below the canvas (notes bar, drawer, panel edge)
  if (wvY + wvH > vpBottom) {
    const clipStart = Math.max(0, vpBottom - wvY)
    clips.push({
      x: 0,
      y: Math.round(clipStart * dpr),
      w: Math.round(wvW * dpr),
      h: Math.round((wvH - clipStart) * dpr),
    })
  }

  // Overlay rects (popups/context menus that need to punch through the webview)
  for (const rect of overlayRects.value) {
    subtract({ left: rect.x, top: rect.y, right: rect.x + rect.w, bottom: rect.y + rect.h })
  }

  return clips
}

function syncWebviewPosition() {
  const state = tabWebviews.get(activeTabId.value)
  if (!state || !contentRef.value) {
    rafId = requestAnimationFrame(syncWebviewPosition)
    return
  }

  // When content is in fullscreen, the webview fills the entire window.
  // Skip normal positioning/clipping so we don't shrink it back.
  if (isContentFullscreen.value) {
    rafId = requestAnimationFrame(syncWebviewPosition)
    return
  }

  const webview = state.webview
  const rect = contentRef.value.getBoundingClientRect()
  const x = Math.round(rect.left)
  const y = Math.round(rect.top)
  const w = Math.round(rect.width)
  const h = Math.round(rect.height)

  const tooSmall = w <= 10 || h <= 10
  const visible = !tooSmall

  // Update position/size if changed
  if (x !== lastRect.x || y !== lastRect.y || w !== lastRect.w || h !== lastRect.h || visible !== lastVisible) {
    lastRect = { x, y, w, h }
    lastVisible = visible

    if (visible) {
      webview.setPosition(new LogicalPosition(x, y)).catch(() => {})
      webview.setSize(new LogicalSize(w, h)).catch(() => {})
      webview.show().catch(() => {})
    } else {
      webview.hide().catch(() => {})
    }
  }

  // Compute & apply clip region (only when visible)
  if (visible) {
    const clips = computeObscuringRects(rect.left, rect.top, rect.width, rect.height)
    const dpr = window.devicePixelRatio || 1
    const label = webviewLabelForTab(activeTabId.value)
    const fullWidth = Math.round(rect.width * dpr)
    const fullHeight = Math.round(rect.height * dpr)

    // The size is part of the key because the region is built FROM it: a
    // resize that leaves the obscuring rects unchanged still needs a new
    // region, or the window keeps the one cut for its old size and the area it
    // gained stays invisible.
    const clipKey = `${fullWidth}x${fullHeight}:` + clips.map(r => `${r.x},${r.y},${r.w},${r.h}`).join('|')
    if (clipKey !== lastClipKey) {
      lastClipKey = clipKey
      invoke('plugin:canvas|set_browser_clip_region', {
        label,
        fullWidth,
        fullHeight,
        obscuringRects: clips,
      }).catch(() => {})
    }
  }

  rafId = requestAnimationFrame(syncWebviewPosition)
}

// Hide webview when minimized and stop the loops behind it — the body stays
// mounted (v-show), so nothing else stops them. Same shape as onDeactivated.
watch(() => props.window.minimized, (minimized) => {
  const state = tabWebviews.get(activeTabId.value)
  if (minimized) {
    state?.webview.hide().catch(() => {})
    if (rafId !== null) {
      cancelAnimationFrame(rafId)
      rafId = null
    }
    stopUrlPolling()
  } else {
    state?.webview.show().catch(() => {})
    if (rafId === null) {
      rafId = requestAnimationFrame(syncWebviewPosition)
    }
    startUrlPolling()
  }
})

// Force clip region recalculation when overlay rects change
watch(overlayRects, () => { lastClipKey = null }, { deep: true })

// Computed: should the webview be hidden? (context menu, panels open, etc.)
const shouldHideWebview = computed(() =>
  contextMenuVisible.value || historyPanelVisible.value || bookmarksPanelVisible.value
)

// Hide/show webview when overlays toggle (native webviews render above HTML)
watch(shouldHideWebview, (hide) => {
  if (props.window.minimized) return
  const state = tabWebviews.get(activeTabId.value)
  if (!state) return
  if (hide) {
    state.webview.hide().catch(() => {})
  } else {
    state.webview.show().catch(() => {})
  }
})

// ---- URL polling: detect in-webview navigation (link clicks, redirects) ----

let urlPollInterval: ReturnType<typeof setInterval> | null = null

/** Ticks since the last safety re-injection — see startUrlPolling. */
let injectTicks = 0

function startUrlPolling() {
  if (urlPollInterval) return
  urlPollInterval = setInterval(async () => {
    const tabId = activeTabId.value
    if (!tabId) return
    const label = webviewLabelForTab(tabId)
    try {
      const actualUrl = await invoke<string>('plugin:canvas|get_browser_url', { label })

      // This used to re-inject on EVERY tick — one eval_browser per second per
      // browser window, forever, for a script that no-ops after the first run.
      // Injection belongs on navigation (a new JS context loses the guards),
      // which is the branch below. The slow tick here is the safety net for
      // the one case a URL comparison cannot see: a reload of the same URL.
      injectTicks += 1
      if (injectTicks >= 5) {
        injectTicks = 0
        injectBrowserScripts(label)
      }

      if (actualUrl && actualUrl !== currentUrl.value) {
        injectBrowserScripts(label)
        injectTicks = 0

        currentUrl.value = actualUrl
        urlInput.value = actualUrl

        const tab = tabs.value.find(t => t.id === tabId)
        if (tab) {
          tab.url = actualUrl
          try {
            const hostname = new URL(actualUrl).hostname
            tab.title = hostname
            canvasStore.updateWindow(workspaceId.value, props.window.id, {
              title: `Browser - ${hostname}`,
            })
          } catch { /* invalid URL */ }
        }

        // Update per-tab history
        const state = tabWebviews.get(tabId)
        if (state) {
          const lastUrl = state.history[state.historyIndex]
          if (actualUrl !== lastUrl) {
            // Trim forward history and push new URL
            state.history = state.history.slice(0, state.historyIndex + 1)
            state.history.push(actualUrl)
            state.historyIndex = state.history.length - 1
          }
        }

        // Add to browsing history store
        browserStore.addHistoryEntry({
          url: actualUrl,
          title: tab?.title ?? '',
          visitedAt: new Date().toISOString(),
        })

        persistTabState()
      }
    } catch { /* webview may not exist yet */ }
  }, 1000)
}

function stopUrlPolling() {
  if (urlPollInterval) {
    clearInterval(urlPollInterval)
    urlPollInterval = null
  }
}

// ---- Inject Ctrl+Wheel forwarding into the child webview ----

async function injectBrowserScripts(label: string) {
  try {
    await invoke('plugin:canvas|eval_browser', {
      label,
      js: `
        // ---- Ctrl+Wheel forwarding (canvas zoom) ----
        if (!window.__qcWheelInjected) {
          window.__qcWheelInjected = true;
          document.addEventListener('wheel', function(e) {
            if (e.ctrlKey) {
              e.preventDefault();
              e.stopPropagation();
              if (window.__TAURI_INTERNALS__) {
                window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {
                  event: 'browser-ctrl-wheel',
                  payload: { deltaY: e.deltaY, clientX: e.clientX, clientY: e.clientY }
                }).catch(function(){});
              }
            }
          }, { passive: false, capture: true });
        }

        // ---- Middle-click on links → open in new tab ----
        if (!window.__qcMiddleClickInjected) {
          window.__qcMiddleClickInjected = true;
          document.addEventListener('auxclick', function(e) {
            if (e.button === 1) {
              var target = e.target;
              while (target && target.tagName !== 'A') {
                target = target.parentElement;
              }
              if (target && target.href && target.href.startsWith('http')) {
                e.preventDefault();
                e.stopPropagation();
                if (window.__TAURI_INTERNALS__) {
                  window.__TAURI_INTERNALS__.invoke('plugin:canvas|browser_request_new_tab', { url: target.href }).catch(function(){});
                }
              }
            }
          }, { capture: true });
        }

      `,
    })
  } catch {
    // External pages may not have __TAURI_INTERNALS__
  }
}

// Listen for fullscreen state changes from the Rust handler
let unlistenFullscreen: (() => void) | null = null

async function setupFullscreenListener() {
  unlistenFullscreen = await listen<{ label: string; isFullscreen: boolean }>(
    'browser-fullscreen-changed',
    (event) => {
      // Only handle events for webviews belonging to this browser window
      const activeLabel = webviewLabelForTab(activeTabId.value)
      if (event.payload.label !== activeLabel) return

      isContentFullscreen.value = event.payload.isFullscreen

      if (event.payload.isFullscreen) {
        // Entering fullscreen: expand webview to fill the entire app window,
        // remove clip region so the video surface renders properly
        const state = tabWebviews.get(activeTabId.value)
        if (state) {
          const w = window.innerWidth
          const h = window.innerHeight
          state.webview.setPosition(new LogicalPosition(0, 0)).catch(() => {})
          state.webview.setSize(new LogicalSize(w, h)).catch(() => {})
          // Remove clip region
          invoke('plugin:canvas|set_browser_clip_region', {
            label: activeLabel,
            fullWidth: 0,
            fullHeight: 0,
            obscuringRects: [],
          }).catch(() => {})
        }
        // Reset sync so it doesn't override our fullscreen positioning
        lastRect = { x: 0, y: 0, w: window.innerWidth, h: window.innerHeight }
        lastClipKey = null
      } else {
        // Leaving fullscreen: force a position re-sync on next frame
        lastRect = { x: 0, y: 0, w: 0, h: 0 }
        lastClipKey = null
      }
    },
  )
}

// Listen for forwarded Ctrl+Wheel events from child webviews
let unlistenWheel: (() => void) | null = null
let unlistenMiddleClick: (() => void) | null = null

async function setupWheelListener() {
  unlistenWheel = await listen<{ deltaY: number; clientX: number; clientY: number }>(
    'browser-ctrl-wheel',
    (event) => {
      // Dispatch a synthetic wheel event on the document to trigger canvas zoom
      const syntheticEvent = new WheelEvent('wheel', {
        deltaY: event.payload.deltaY,
        clientX: event.payload.clientX,
        clientY: event.payload.clientY,
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      })
      document.dispatchEvent(syntheticEvent)
    },
  )
}

async function setupMiddleClickListener() {
  unlistenMiddleClick = await listen<string>(
    'browser-new-tab-request',
    (event) => {
      if (event.payload) {
        addTab(event.payload, { background: true })
      }
    },
  )
}

// ---- Lifecycle ----

onMounted(async () => {
  // Read before the first await: Vue only keeps the current instance set while
  // the hook body runs synchronously, and the helper needs it for its walk.
  const stageWasActive = inActiveKeepAliveTree()

  canvasStore.updateWindow(workspaceId.value, props.window.id, { status: 'live' })

  // Initialize the browser store
  await browserStore.loadFromDisk()

  // Wait for the placeholder to be in the DOM
  await nextTick()
  if (!contentRef.value) return

  try {
    // Restore tabs from saved state, or create initial tab
    const config = props.window.browserConfig
    const savedTabs = config?.tabs
    const savedActiveTabId = config?.activeTabId

    if (savedTabs && savedTabs.length > 0) {
      // Restore saved tabs
      tabs.value = savedTabs.map(t => ({ ...t, isLoading: true }))
      activeTabId.value = savedActiveTabId && savedTabs.some(t => t.id === savedActiveTabId)
        ? savedActiveTabId
        : savedTabs[0].id

      // Create webviews for all saved tabs
      for (const tab of tabs.value) {
        const wv = await createWebviewForTab(tab.id, tab.url)
        if (wv) {
          tabWebviews.set(tab.id, {
            webview: wv,
            history: [tab.url],
            historyIndex: 0,
          })
          tab.isLoading = false
        } else {
          tab.isLoading = false
        }
      }

      // Only ready when something actually exists to show — flipping this
      // with every creation failed used to hide the placeholder and leave a
      // silent black pane (the missing-permission bug looked exactly so).
      webviewReady.value = tabWebviews.size > 0

      // Sync URL bar to active tab
      const active = tabs.value.find(t => t.id === activeTabId.value)
      if (active) {
        urlInput.value = active.url
        currentUrl.value = active.url
      }
    } else {
      // No saved tabs — create a single initial tab
      const initialUrl = config?.url ?? 'https://www.google.com'
      await addTab(initialUrl)
    }

    // Update title with hostname
    try {
      const hostname = new URL(currentUrl.value).hostname
      canvasStore.updateWindow(workspaceId.value, props.window.id, {
        title: `Browser - ${hostname}`,
      })
    } catch { /* invalid URL */ }

    // Start position sync loop
    if (rafId === null) {
      rafId = requestAnimationFrame(syncWebviewPosition)
    }

    // Set up listeners for events forwarded from child webview
    await setupFullscreenListener()
    await setupWheelListener()
    await setupMiddleClickListener()

    // Start polling for URL changes from in-webview navigation
    startUrlPolling()

    // Mounted into a stage the user had already left: the webviews had to be
    // created (onActivated bails until they exist), but they must not be shown
    // over whatever module is on screen. The stage's own onDeactivated is long
    // past, so stand down here — onActivated brings everything back.
    if (!stageWasActive) goDormant()

  } catch (err) {
    console.error('Failed to initialize browser tabs:', err)
  }
})

/**
 * V3 warm cache: the canvas stage keeps this window mounted while the module
 * is hidden — but the tab webviews are NATIVE child webviews that float above
 * all HTML, including other modules. So hide them and stop the position/url
 * loops whenever the stage is not on screen; on reactivation force a full
 * re-sync (the loop's change detection would otherwise see "nothing changed"
 * and never re-show). Called from onDeactivated and, for a window that mounted
 * into an already-abandoned stage, from the tail of onMounted.
 */
function goDormant() {
  if (rafId !== null) {
    cancelAnimationFrame(rafId)
    rafId = null
  }
  stopUrlPolling()
  for (const [, state] of tabWebviews) {
    state.webview.hide().catch(() => {})
  }
}

onDeactivated(goDormant)

onActivated(() => {
  // Initial mount: onMounted owns startup (webviews don't exist yet).
  if (!webviewReady.value) return
  if (props.window.minimized) return
  if (!shouldHideWebview.value) {
    // Force the sync loop past its change detection so it re-shows the
    // active tab; with an overlay panel open, leave it hidden — the panel
    // watcher shows it again when the panel closes.
    lastRect = { x: 0, y: 0, w: 0, h: 0 }
    lastVisible = false
    lastClipKey = null
  }
  if (rafId === null) {
    rafId = requestAnimationFrame(syncWebviewPosition)
  }
  startUrlPolling()
})

onUnmounted(() => {
  // Stop position sync
  if (rafId !== null) {
    cancelAnimationFrame(rafId)
    rafId = null
  }

  // Clean up progress interval
  if (progressInterval) {
    clearInterval(progressInterval)
    progressInterval = null
  }

  // Stop URL polling
  stopUrlPolling()


  // Clean up event listeners
  unlistenFullscreen?.()
  unlistenFullscreen = null
  unlistenWheel?.()
  unlistenWheel = null
  unlistenMiddleClick?.()
  unlistenMiddleClick = null

  // Destroy all tab webviews (close stops media, frees OS processes). Tracked,
  // so the instance a workspace switch mounts next can await the teardown
  // instead of reconnecting to a half-closed webview.
  for (const [tabId, state] of tabWebviews) {
    const label = webviewLabelForTab(tabId)
    invoke('plugin:canvas|set_browser_clip_region', {
      label,
      fullWidth: 0,
      fullHeight: 0,
      obscuringRects: [],
    }).catch(() => {})
    closeWebviewTracked(label, state.webview)
  }
  tabWebviews.clear()

  lastClipKey = null
})

/**
 * PRE-EXISTING BUG, carried over unchanged in behaviour.
 *
 * This is called in `switchTab` but was never defined - not here and not in the
 * standalone QuantCode either. At runtime it throws a ReferenceError, which
 * aborts `switchTab` before the URL bar and window title are synced, so
 * switching browser tabs is half-broken today.
 *
 * Defined here as a no-op so the module type-checks and so the surrounding
 * best-effort code (every other webview call is `.catch(() => {})`) behaves the
 * way it was evidently meant to. Whatever ctrl+wheel zoom forwarding was
 * intended is NOT implemented - inventing it would be a behaviour change the
 * migration has no business making.
 */
function injectCtrlWheelForwarder(_webviewLabel: string): void {
  // intentionally empty - see above
}
</script>

<template>
  <div
    class="w-full h-full flex flex-col overflow-hidden"
    :style="{ background: 'var(--qc-bg)' }"
    tabindex="0"
    @keydown="onKeydown"
  >
    <!-- Tab Bar -->
    <BrowserTabBar
      :tabs="tabs"
      :active-tab-id="activeTabId"
      @switch="switchTab"
      @close="closeTab"
      @add="addTab()"
    />

    <!-- Navigation Bar -->
    <div
      class="flex items-center gap-1 px-2 py-1.5 flex-shrink-0"
      :style="{ background: 'var(--qc-bg-header)', borderBottom: '1px solid var(--qc-border)' }"
    >
      <BrowserNavButtons
        :can-go-back="canGoBack"
        :can-go-forward="canGoForward"
        :is-loading="activeTab?.isLoading ?? false"
        @back="goBack"
        @forward="goForward"
        @reload="reload"
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
      <button
        class="flex-shrink-0 w-7 h-7 flex items-center justify-center rounded hover:opacity-80"
        :style="{ color: bookmarksPanelVisible ? 'var(--qc-text)' : 'var(--qc-text-dim)' }"
        title="Bookmarks"
        @click="bookmarksPanelVisible = !bookmarksPanelVisible; historyPanelVisible = false"
        @mousedown.stop
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z" />
        </svg>
      </button>
      <button
        class="flex-shrink-0 w-7 h-7 flex items-center justify-center rounded hover:opacity-80"
        :style="{ color: historyPanelVisible ? 'var(--qc-text)' : 'var(--qc-text-dim)' }"
        title="History"
        @click="historyPanelVisible = !historyPanelVisible; bookmarksPanelVisible = false"
        @mousedown.stop
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="12" cy="12" r="10" />
          <polyline points="12 6 12 12 16 14" />
        </svg>
      </button>
    </div>

    <!-- Bookmark Bar -->
    <BrowserBookmarkBar
      :bookmarks="browserStore.bookmarks"
      @navigate="navigateTo"
      @remove="(id: string) => browserStore.removeBookmark(id)"
    />

    <!-- Progress Bar -->
    <BrowserProgressBar :progress="loadProgress" />

    <!-- Find Bar + Webview area + overlay panels -->
    <div class="relative flex-1 min-h-0">
      <BrowserFindBar
        v-if="findBarVisible"
        v-model="findQuery"
        @find-next="findNext"
        @find-prev="findPrev"
        @close="findBarVisible = false"
      />

      <!-- Webview placeholder (inset so native webview doesn't cover resize handles) -->
      <div ref="contentRef" class="absolute inset-0" style="left: 4px; right: 4px; bottom: 4px;">
        <div
          v-if="!webviewReady"
          class="w-full h-full flex items-center justify-center"
          :style="{ color: 'var(--qc-text-dim)' }"
        >
          <span class="text-xs">Loading browser...</span>
        </div>
      </div>

      <!-- History Panel (overlay inside webview area) -->
      <BrowserHistoryPanel
        v-if="historyPanelVisible"
        :history="browserStore.history"
        @navigate="(url: string) => { navigateTo(url); historyPanelVisible = false }"
        @close="historyPanelVisible = false"
        @clear="browserStore.clearHistory()"
      />

      <!-- Bookmarks Panel (overlay inside webview area) -->
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
@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}

.animate-pulse {
  animation: pulse 1.5s ease-in-out infinite;
}
</style>
